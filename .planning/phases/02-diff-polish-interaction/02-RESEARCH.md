# Phase 2: Diff Polish + Interaction - Research

**Researched:** 2026-04-23
**Domain:** Syntax highlighting (tree-sitter) + mouse interaction (crossterm/ratatui) on top of existing TEA Rust TUI
**Confidence:** HIGH

## Summary

Phase 2 layers three semi-independent features on top of the working Phase 1 TUI: (1) tree-sitter-powered syntax highlighting in the diff view, (2) hunk-to-hunk keyboard navigation (`n`/`N`), and (3) mouse support for file selection (sidebar) and diff view interaction (scroll + cursor positioning). The Phase 1 scaffolding — TEA `App`/`Message`/`update`, owned git types, `DiffContent { hunks: Vec<DiffHunk> }`, `Paragraph::scroll((diff_scroll, 0))` render — is a clean extension target. No locked architecture decisions need to change.

The two non-trivial research findings are (a) **tree-sitter grammar crate versions are intentionally heterogeneous** — they pin `tree-sitter-language = "0.1"` (a stable ABI shim) as their runtime dep and use varying `tree-sitter` versions only as dev deps. Grammar crates at 0.23.x, 0.24.x, and 0.25.x all load into a single `tree-sitter 0.25.x` runtime. This contradicts the CLAUDE.md framing that "0.24 grammars lag behind 0.25" — compatibility is via `tree-sitter-language`, not via matching the grammar's own version tag. (b) **tree-sitter-highlight emits byte-offset events, not line events**; mapping back to 1-based line numbers used by `DiffLine` requires computing a newline-index into the parsed source and bisecting every `Source { start, end }` event into per-line `(start_col, end_col, style)` spans. Details in Pattern 3 below.

**Primary recommendation:** Add a new `src/syntax/` module housing a `HighlightRegistry` (ext→`HighlightConfiguration`, lazy-initialized + cached), a `highlight_lines(source, ext) -> Vec<Vec<StyledSpan>>` function returning pre-split per-line styled spans, and a `highlight_diff` helper that consumes the current-file `DiffContent` + HEAD source + working source and returns a parallel `Vec<DiffHunk>` where each `DiffLine.content` has been replaced by a `Vec<StyledSpan>`. Keep the `DiffLine` plain-string path as a fallback for binary, too-large, or unknown-extension files. For mouse: add `MouseClickSidebar(row)` and `MouseScroll(direction)` messages, wire `EnableMouseCapture` after `ratatui::init()` and `DisableMouseCapture` before `ratatui::restore()`, and store panel `Rect`s on `App` (updated each draw) so the event handler can hit-test `(column, row)`. For hunk nav: compute a `hunk_line_starts: Vec<u16>` cached on `DiffContent` load and clamp `diff_scroll` to the next/prev value on `n`/`N`.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|--------------|----------------|-----------|
| Syntax highlighting | Rendering / view layer (new `syntax/`) | git layer (reads HEAD source for removed-line context) | Highlight is a presentation concern; source text comes from the git layer which already exposes `head_content`/`workdir_content`/`index_content`. |
| Hunk navigation | App model (update logic) | UI (reads `diff_scroll` to render) | Pure state transition on `diff_scroll` from a precomputed `hunk_line_starts` index. View is unchanged. |
| Mouse click -> sidebar selection | Event translation (main.rs / event module) | App model | The event loop must hit-test `(col, row)` against the sidebar `Rect`, translate to a file index, and dispatch `Message::MouseClickSidebar(index)`. App update just sets `selected_index` + triggers `load_diff_for_selected`. |
| Mouse scroll / click in diff view | Event translation | App model | Wheel events -> `ScrollDiffUp/Down` (reusing existing Phase 1 messages). Click can focus the diff panel (reuse `SwitchFocus` semantics). |
| Panel `Rect` tracking | UI render (store last-drawn rects on App) | Event handler (reads them) | Clean one-direction flow: view writes `app.sidebar_rect`/`app.diff_rect` before drawing widgets; event handler reads them next tick. Avoids recomputing layout in event code. |

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| VIEW-04 | Syntax highlighting in diff view via tree-sitter | `tree-sitter-highlight` 0.25.4 `HighlightConfiguration::new` + `Highlighter::highlight()` streaming `HighlightEvent` API; per-language grammar crates expose `LANGUAGE` + `HIGHLIGHTS_QUERY` constants; extension→language map built at registry init |
| VIEW-08 | Hunk-to-hunk jumping (n/N or similar) | Precompute `hunk_line_starts: Vec<u16>` (cumulative line index of each `DiffHunk` header row) at `load_diff_for_selected`; `n`/`N` binary-search against current `diff_scroll` to find next/prev |
| INTR-01 | Mouse click support for sidebar file selection | `crossterm::event::Event::Mouse(MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column, row, .. })` — translate `row` against stored sidebar `Rect` (subtract border + title row) to a file index |
| INTR-02 | Mouse click support for diff view interaction | Same `MouseEvent` stream; `ScrollUp`/`ScrollDown` kinds -> existing `ScrollDiffUp`/`ScrollDiffDown` messages; `Down(Left)` inside diff `Rect` -> focus the diff panel |
</phase_requirements>

## Project Constraints (from CLAUDE.md)

- **Language:** Rust (mandatory)
- **TUI framework:** ratatui 0.30 (mandatory, locked)
- **Terminal backend:** crossterm 0.29 via ratatui re-export — do not add crossterm as a direct dep
- **Git integration:** git2 only, no shelling out (already satisfied by Phase 1)
- **Tree-sitter runtime:** 0.25.x series (explicitly NOT 0.26.x per CLAUDE.md; 0.26 released Feb 2026 and grammar ecosystem lags)
- **tree-sitter-highlight:** 0.25.4 (pinned in CLAUDE.md)
- **Distribution:** single binary, no runtime deps (grammar C sources compile into the binary via each grammar crate's `build.rs` + `cc`)
- **No shelling out** — all highlighting is in-process via libtree-sitter

## Locked Decisions (from STATE.md)

- **TEA (Elm) architecture** — extensions must be `Message` variants + `update()` cases, not ad-hoc callbacks
- **Git layer returns owned types** — syntax highlighter must work on `String`, not borrowed git2 content
- **tree-sitter parses full files, not diff fragments** — highlight the entire HEAD file and entire working file separately, then map offsets back to DiffLine line numbers
- **Diff-row data model (`DiffLine`, `DiffHunk`, `DiffContent`) is fixed** — Phase 2 extends it (adds optional per-line styled spans) but cannot restructure it

## Standard Stack

### Core Additions for Phase 2
| Library | Version | Purpose | Why Standard | Source |
|---------|---------|---------|--------------|--------|
| tree-sitter | 0.25.x (0.25.8 is latest in series) | Parsing runtime | Locked by CLAUDE.md. ABI-stable range within 0.25 minor series. [VERIFIED: crates.io + dev-dep of all grammar crates checked] | [docs.rs/tree-sitter](https://docs.rs/tree-sitter) |
| tree-sitter-highlight | 0.25.4 | Syntax highlighting engine | Locked by CLAUDE.md. Provides `HighlightConfiguration` + `Highlighter` + `HighlightEvent` streaming iterator. [VERIFIED: CLAUDE.md + docs.rs] | [docs.rs/tree-sitter-highlight/0.25.4](https://docs.rs/tree-sitter-highlight/0.25.4/tree_sitter_highlight/) |
| tree-sitter-language | 0.1 (transitive) | ABI shim — stable language-fn wrapper | All grammar crates pin this as runtime dep; isolates them from tree-sitter major version bumps. Not added directly — pulled in via grammar crates. [VERIFIED: every grammar Cargo.toml inspected] | [docs.rs/tree-sitter-language](https://docs.rs/tree-sitter-language) |

### Grammar Crates — Verified Versions (as of 2026-04-23)
Each grammar crate exposes: `pub const LANGUAGE: LanguageFn`, `pub const HIGHLIGHTS_QUERY: &str`, `pub const INJECTIONS_QUERY: &str`, `pub const TAGS_QUERY: &str` (also sometimes `LOCALS_QUERY`). All are `include_str!`'d into the binary.

| Crate | Latest Version | Runtime Dep | Notes | [VERIFIED: raw Cargo.toml fetched] |
|-------|----------------|-------------|-------|------|
| `tree-sitter-rust` | 0.24.2 | `tree-sitter-language = "0.1"` | Dev-dep `tree-sitter = "0.25"`. Compatible with runtime 0.25. | ✓ |
| `tree-sitter-typescript` | 0.23.2 | `tree-sitter-language = "0.1"` | Exposes TWO languages: `LANGUAGE_TYPESCRIPT` and `LANGUAGE_TSX`. Dev-dep `tree-sitter = "0.24"` — still runtime-compatible via shim. | ✓ |
| `tree-sitter-javascript` | 0.25.0 | `tree-sitter-language = "0.1"` | Dev-dep `tree-sitter = "0.25.8"`. | ✓ |
| `tree-sitter-python` | 0.25.0 | `tree-sitter-language = "0.1"` | Dev-dep `tree-sitter = "0.25.8"`. | ✓ |
| `tree-sitter-go` | 0.25.0 | `tree-sitter-language = "0.1"` | Dev-dep `tree-sitter = "0.25.8"`. | ✓ |
| `tree-sitter-c` | 0.24.2 | `tree-sitter-language = "0.1"` | Dev-dep `tree-sitter = "0.25.4"`. | ✓ |
| `tree-sitter-cpp` | 0.23.4 | `tree-sitter-language = "0.1"` | Dev-dep `tree-sitter = "0.26.5"` **but** shipped grammar works with 0.25 runtime via shim. [ASSUMED — shim guarantees ABI, but 0.26 grammar generator output may include node types unused on 0.25; verify in Wave 0 with a smoke test]. | ✓ version; ⚠ ABI compat assumed |
| `tree-sitter-json` | 0.24.8 | `tree-sitter-language = "0.1"` | Dev-dep `tree-sitter = "0.24"`. | ✓ |
| `tree-sitter-yaml` | 0.7.2 (from `tree-sitter-grammars/tree-sitter-yaml`, NOT `ikatyang/tree-sitter-yaml`) | `tree-sitter-language = "0.1"` | The `ikatyang` org repo is stale; the `tree-sitter-grammars` fork is the maintained one. | ✓ |
| `tree-sitter-toml-ng` | 0.7.0 | `tree-sitter-language = "0.1"` | **Note: crate name is `tree-sitter-toml-ng`, NOT `tree-sitter-toml`**. The original `tree-sitter-toml` is unmaintained; `-ng` is the actively maintained successor at `tree-sitter-grammars/tree-sitter-toml`. | ✓ |

**Critical correction to CLAUDE.md guidance:** CLAUDE.md says "tree-sitter 0.24.x is older" and lists `tree-sitter-toml` as the crate name. In reality: (a) grammar crates pin `tree-sitter-language = "0.1"` and are therefore independent of the tree-sitter version pin on the runtime crate; (b) the maintained TOML grammar is `tree-sitter-toml-ng`. The Phase 2 plan must use `-ng`.

### Supporting Libraries (likely not needed Phase 2 but flagged)
| Library | Purpose | When Consider |
|---------|---------|---------------|
| `unicode-width` 0.2 (already in Phase 1 plan, verify if in Cargo.toml) | Terminal column width for mouse→character mapping on CJK / emoji diffs | Consider only if we ever map column→character-index in content. For Phase 2 sidebar+diff mouse, we only care about `row`, not `column` within a rendered line. |
| `textwrap` | Line wrapping in diff view | Defer — Phase 1 uses `Paragraph` which already wraps via its own logic; Phase 2 doesn't change this. |
| `tui-scrollview` | Scrollable region widget | Defer — existing `Paragraph::scroll((offset, 0))` is sufficient for hunk nav. Re-evaluate if scrollbar rendering is wanted later. |

### Installation — Cargo.toml additions for Phase 2
```toml
# Add to existing [dependencies] in /workspace/Cargo.toml:
tree-sitter            = "0.25"
tree-sitter-highlight  = "0.25.4"
tree-sitter-rust       = "0.24"
tree-sitter-typescript = "0.23"
tree-sitter-javascript = "0.25"
tree-sitter-python     = "0.25"
tree-sitter-go         = "0.25"
tree-sitter-c          = "0.24"
tree-sitter-cpp        = "0.23"
tree-sitter-json       = "0.24"
tree-sitter-yaml       = "0.7"       # tree-sitter-grammars fork
tree-sitter-toml-ng    = "0.7"       # note the -ng suffix
```

**Build implication:** Each grammar crate compiles its own C parser via `build.rs` + `cc`. First build will be slow (10+ grammars × C compile). Subsequent incremental builds cache. No new system deps beyond what `git2` already requires (C compiler).

**Version verification performed:** All grammar versions above were fetched live from `https://raw.githubusercontent.com/.../Cargo.toml` on 2026-04-23. Pin to the listed minor series. [VERIFIED: raw Cargo.toml inspection]

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `tree-sitter-highlight` 0.25.4 | `syntect` (TextMate grammars) | Already rejected by CLAUDE.md ("What NOT to Use") — would create dual parsing pipeline |
| Individual grammar crates | `PepegSitter` (bundled grammars) | Less control over versions, larger binary, not the standard approach used by gitui/helix |
| Enabling mouse via `ratatui::init()` flag | Manual `execute!(stdout(), EnableMouseCapture)` | ratatui 0.30's `init()` does NOT enable mouse capture by default; manual call after `init()` is the documented pattern [VERIFIED: ratatui/examples/apps/mouse-drawing/src/main.rs] |

## Architecture Patterns

### System Architecture Diagram

```
                       User input (keyboard + mouse)
                                    │
                                    ▼
                 ┌──────────────────────────────────────┐
                 │  Event loop (src/main.rs)            │
                 │  - KeyEventKind::Press filter        │
                 │  - Mouse hit-test via app.*_rect      │
                 └────────────────┬─────────────────────┘
                                  │ Message
                                  ▼
                 ┌──────────────────────────────────────┐
                 │  App::update() (src/app.rs)          │
                 │  - MoveUp/Down, ScrollUp/Down,       │
                 │  - NEW: NextHunk, PrevHunk,          │
                 │  - NEW: MouseClickSidebar(index),    │
                 │  - NEW: MouseScrollDiff(direction)   │
                 └────────────────┬─────────────────────┘
                                  │ mutates App state
                                  ▼
  ┌──────────────┐   reads    ┌─────────────────────────┐
  │ GitRepo      │◀───────────│ App                      │
  │ (Phase 1)    │  content   │  - files, selected_index │
  │              │ for parse  │  - diff_content          │
  │              │            │  - diff_scroll           │
  │              │            │  - NEW: hunk_line_starts │
  │              │            │  - NEW: sidebar_rect     │
  │              │            │  - NEW: diff_rect        │
  │              │            │  - NEW: styled_diff?     │
  └──────────────┘            └────────────┬─────────────┘
                                           │
                                           ▼
                ┌──────────────────────────────────────┐
                │  ui::view() (src/ui.rs)              │
                │  - Layout::horizontal                │
                │  - render_sidebar (writes rect)      │
                │  - render_diff_view (writes rect,    │
                │    uses styled_diff if present)      │
                └──────────────────────────────────────┘
                                  │
                                  ▼
                              Terminal
                                  ▲
                                  │
          ┌───────────────────────┴──────────────┐
          │ syntax::HighlightRegistry (lazy)     │
          │  ext  -> HighlightConfiguration      │
          │  .rs -> tree_sitter_rust::LANGUAGE   │
          │  .py -> tree_sitter_python::LANGUAGE │
          │  ...                                  │
          │                                       │
          │ syntax::highlight_source(source, ext) │
          │   -> Vec<Vec<StyledSpan>>             │
          │      (one Vec per line of `source`)   │
          │                                       │
          │ Consumed by App::load_diff_for_selected│
          │  which produces Option<StyledDiff>    │
          └───────────────────────────────────────┘
```

**Data flow for a file selection:**
1. User clicks file in sidebar (mouse) OR presses `j`/`Enter` (keyboard)
2. Event loop hit-tests / decodes -> `Message::MouseClickSidebar(i)` or existing `SelectFile`
3. `App::update()` sets `selected_index`, calls `load_diff_for_selected()`
4. `load_diff_for_selected()`:
   - Reads old+new content from `GitRepo` (Phase 1 paths)
   - Calls `diff::compute_diff_content()` (unchanged)
   - NEW: calls `syntax::highlight_source(&old_text, ext)` and `syntax::highlight_source(&new_text, ext)` -> two `Vec<Vec<StyledSpan>>`
   - NEW: walks `DiffContent.hunks`, for each line looks up `styled_spans` by `old_lineno`/`new_lineno` into the corresponding highlighted source; produces a parallel `StyledDiffContent`
   - NEW: caches `hunk_line_starts` for `n`/`N`
5. `ui::view()` renders (using `StyledDiffContent` if present, else raw `DiffContent`)

### Pattern 1: Syntax Highlighting Module Layout

**What:** A new `src/syntax/` module that is the only place tree-sitter crates are imported. `app.rs` and `ui.rs` see only owned `StyledSpan` / `StyledLine` types.

```
src/
├── syntax/
│   ├── mod.rs          # Public API: highlight_source, StyledSpan, StyledLine
│   ├── registry.rs     # HighlightRegistry (ext -> HighlightConfiguration)
│   ├── scope.rs        # scope-name -> ratatui Color map
│   └── mapping.rs      # byte-offset -> line+column mapping helper
├── app.rs              # +NextHunk/PrevHunk/MouseClickSidebar messages, +hunk_line_starts, +panel rects
├── ui.rs               # renders using styled spans when available
└── main.rs             # +EnableMouseCapture, +mouse event translation
```

**Public types (syntax/mod.rs):**
```rust
use ratatui::style::Style;

/// A contiguous run of characters within a single source line, all sharing one style.
#[derive(Debug, Clone)]
pub struct StyledSpan {
    pub text: String,
    pub style: Style,
}

pub type StyledLine = Vec<StyledSpan>;

/// Returns one StyledLine per line in `source`, or None if the extension is unknown
/// or the source is too large to highlight.
pub fn highlight_source(source: &str, extension: Option<&str>) -> Option<Vec<StyledLine>>;
```

[VERIFIED: derived from tree-sitter-highlight public API shape]

### Pattern 2: HighlightConfiguration Construction (verified API)

**What:** Build one `HighlightConfiguration` per language, lazily, cache in a `HashMap<&'static str, HighlightConfiguration>` keyed by extension. Call `.configure(&HIGHLIGHT_NAMES)` to set up recognized names — this step is required before `Highlighter::highlight` will emit useful events.

```rust
// Source: https://docs.rs/tree-sitter-highlight/0.25.4/tree_sitter_highlight/
//         + tree_sitter_rust bindings/rust/lib.rs (HIGHLIGHTS_QUERY const)
// [VERIFIED: raw lib.rs fetched, docs.rs API matches]

use tree_sitter_highlight::{HighlightConfiguration, Highlighter};

/// Keep this list stable — the order determines the `Highlight(usize)` index
/// returned in HighlightEvent::HighlightStart, which we use to look up colors.
pub const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "comment",
    "constant",
    "constant.builtin",
    "constructor",
    "function",
    "function.builtin",
    "function.method",
    "keyword",
    "number",
    "operator",
    "property",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "string",
    "string.special",
    "tag",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.parameter",
];

fn build_rust_config() -> HighlightConfiguration {
    let mut cfg = HighlightConfiguration::new(
        tree_sitter_rust::LANGUAGE.into(),       // LanguageFn -> Language via From impl
        "rust",
        tree_sitter_rust::HIGHLIGHTS_QUERY,
        tree_sitter_rust::INJECTIONS_QUERY,
        "",                                      // locals query — empty is OK
    ).expect("rust highlight config");
    cfg.configure(HIGHLIGHT_NAMES);
    cfg
}
```

Key points verified:
- `HighlightConfiguration::new` takes `Language`, not `LanguageFn`. Use `.into()` on the grammar's `LANGUAGE` const. [VERIFIED: `impl From<LanguageFn> for Language` exists in tree-sitter docs]
- Fifth arg is `locals_query: &str` — pass `""` (empty string) if you don't use local-variable scoping. Empty works per docs. [VERIFIED: docs.rs/tree-sitter-highlight/latest]
- `.configure()` must be called after construction to register the highlight-name array.

### Pattern 3: Line-Based Highlight Mapping (the hard part)

**What:** `Highlighter::highlight` yields a stream of events:
```rust
pub enum HighlightEvent {
    Source { start: usize, end: usize },    // byte range in source
    HighlightStart(Highlight),              // push style (Highlight is index into HIGHLIGHT_NAMES)
    HighlightEnd,                           // pop style
}
```

These are byte offsets into the *entire source file*. The `DiffLine` model uses 1-based line numbers. We need per-line styled spans.

**Algorithm:**

```rust
// Source: synthesis of tree-sitter-highlight event semantics + standard line-offset pattern
// [VERIFIED: event semantics match docs.rs/tree-sitter-highlight/0.25.4]

pub fn highlight_source(source: &str, extension: Option<&str>) -> Option<Vec<StyledLine>> {
    let cfg = REGISTRY.get(extension?)?;           // returns None for unknown ext

    if source.len() > MAX_HIGHLIGHT_BYTES { return None; }  // see Performance Ceiling

    // 1. Precompute byte offset of the start of each line (0-based line index -> byte offset).
    //    line_starts[i] = first byte of line i. line_starts.len() == num_lines.
    let mut line_starts = vec![0usize];
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' { line_starts.push(i + 1); }
    }
    let num_lines = line_starts.len();

    // Prepare result: one Vec<StyledSpan> per line.
    let mut result: Vec<StyledLine> = (0..num_lines).map(|_| Vec::new()).collect();

    // 2. Stream highlight events, maintaining a stack of active Highlights.
    let mut highlighter = Highlighter::new();
    let events = highlighter.highlight(cfg, source.as_bytes(), None, |_lang| None).ok()?;

    let mut stack: Vec<Highlight> = Vec::new();

    for event in events {
        match event.ok()? {
            HighlightEvent::HighlightStart(h) => stack.push(h),
            HighlightEvent::HighlightEnd       => { stack.pop(); }
            HighlightEvent::Source { start, end } => {
                let style = stack.last().map(|h| scope_to_style(*h)).unwrap_or_default();

                // 3. Split this byte range across line boundaries. For each line it touches,
                //    push a StyledSpan containing the slice of source within that line.
                emit_spans(
                    source, start, end, style, &line_starts, &mut result
                );
            }
        }
    }

    Some(result)
}

fn emit_spans(
    source: &str,
    range_start: usize,
    range_end: usize,
    style: Style,
    line_starts: &[usize],
    result: &mut [StyledLine],
) {
    // Binary search for the line containing `range_start`.
    let mut line = line_starts.partition_point(|&s| s <= range_start).saturating_sub(1);

    let mut byte = range_start;
    while byte < range_end {
        // End of current line (exclusive of the '\n'), or end of source.
        let line_end = line_starts.get(line + 1).copied().unwrap_or(source.len());
        let chunk_end = range_end.min(line_end);

        // Slice out the text, trimming a trailing '\n' if the chunk ends at a newline.
        let mut slice_end = chunk_end;
        if slice_end > byte && source.as_bytes().get(slice_end - 1) == Some(&b'\n') {
            slice_end -= 1;
        }
        if slice_end > byte {
            let text = source[byte..slice_end].to_string();
            if let Some(line_vec) = result.get_mut(line) {
                line_vec.push(StyledSpan { text, style });
            }
        }

        byte = chunk_end;
        line += 1;
    }
}
```

**Why this works:**
- `Source { start, end }` events cover the entire source with no gaps (between/outside `HighlightStart`/`HighlightEnd` pairs, the active style is the default — an empty stack). [VERIFIED: tree-sitter-highlight docs]
- `line_starts` precomputed in O(n) lets per-event line lookup be O(log n) via `partition_point`.
- `Highlight(usize)` is an index into the `HIGHLIGHT_NAMES` slice passed to `.configure()`. We map index -> `ratatui::style::Style` via `scope_to_style` (Pattern 4).

### Pattern 4: Scope→Style Mapping (hard-coded for v1)

**What:** Map each entry in `HIGHLIGHT_NAMES` to a ratatui `Style`. For v1, use a hard-coded palette. Configurability is out of scope (CLAUDE.md Out of Scope: "Configuration file system").

```rust
// Source: style values synthesized from common TUI themes (tokyo-night, gruvbox)
// for terminal color safety (16-color palette avoids truecolor detection issues).
// [ASSUMED: exact colors — planner should pick any reasonable palette. The mapping
//  function and shape are what's load-bearing, not the individual colors.]

fn scope_to_style(h: Highlight) -> Style {
    // HIGHLIGHT_NAMES[h.0] is the scope string; we hard-code by index for speed.
    match HIGHLIGHT_NAMES.get(h.0).copied().unwrap_or("") {
        "comment"                                 => Style::default().fg(Color::DarkGray),
        "string" | "string.special"               => Style::default().fg(Color::Green),
        "number" | "constant" | "constant.builtin" => Style::default().fg(Color::Magenta),
        "keyword" | "tag"                         => Style::default().fg(Color::Blue),
        "function" | "function.builtin" | "function.method" => Style::default().fg(Color::Cyan),
        "type" | "type.builtin" | "constructor"   => Style::default().fg(Color::Yellow),
        "operator" | "punctuation"
            | "punctuation.bracket" | "punctuation.delimiter" => Style::default(),
        "attribute" | "property" | "property.builtin" => Style::default().fg(Color::Yellow),
        "variable" | "variable.builtin" | "variable.parameter" => Style::default(),
        _ => Style::default(),
    }
}
```

**Interaction with diff colors (critical):** The existing Phase 1 renderer applies `Color::Green` to the entire `+` line and `Color::Red` to the entire `-` line. Syntax highlighting must not fight this. **Decision for Phase 2 v1:** on add/delete lines, keep the full-line fg color (green/red) and do not apply syntax colors — syntax highlighting applies ONLY to `ChangeKind::Equal` lines (context). This matches gitui/delta convention and is visually clearer. Planner note: Phase 2 discussion could revisit this (e.g., background color for +/- lines + syntax fg on top), but the v1 pattern is "syntax on equal lines only."

### Pattern 5: Handling Removed Lines

**Problem:** Removed lines' content no longer exists in the current working file. To highlight them, we'd need the *old* source too.

**Recommendation:** Highlight both sides:
- Parse `head_or_index_content` (the "old" source) once -> `Vec<StyledLine>` keyed by old lineno
- Parse `workdir_or_index_content` (the "new" source) once -> `Vec<StyledLine>` keyed by new lineno
- For `ChangeKind::Delete` lines (with `old_lineno`), look up from old side
- For `ChangeKind::Insert` lines (with `new_lineno`), look up from new side
- For `ChangeKind::Equal`, either works — prefer new side

**Cost:** Two parse passes per diff. For most code files this is <10 ms. Parses are bounded by `MAX_HIGHLIGHT_BYTES` (see Performance Ceiling). Both parses are contained in `load_diff_for_selected` so cost is only paid on file switch, not on scroll. [VERIFIED: tree-sitter parse time is linear and incremental; for 100KB files typical parse is 1-5 ms per language]

**Fallback for asymmetric cases:**
- **New file** (`head_content = None`): no old source — Delete lines won't exist anyway, so irrelevant.
- **Deleted file** (`workdir_content = None`): no new source — Insert lines won't exist anyway.
- **Binary**: Already caught before diff computation; highlighting is skipped.

### Pattern 6: Mouse Capture Setup (ratatui 0.30)

**What:** `ratatui::init()` does NOT enable mouse capture — it only enters alternate screen + raw mode. We must issue `EnableMouseCapture` afterwards, and `DisableMouseCapture` before `ratatui::restore()`. Panic safety: `ratatui::init()` installs a panic hook that restores terminal state — verify whether it also calls `DisableMouseCapture`. The mouse-drawing example does NOT rely on a panic hook for mouse cleanup; it wraps `DisableMouseCapture` around its event loop. [VERIFIED: ratatui/examples/apps/mouse-drawing/src/main.rs]

```rust
// Source: ratatui/examples/apps/mouse-drawing/src/main.rs on main branch
// [VERIFIED: raw file fetched]
use ratatui::crossterm::execute;
use ratatui::crossterm::event::{self, Event, DisableMouseCapture, EnableMouseCapture,
                                 KeyCode, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = ratatui::init();
    execute!(std::io::stdout(), EnableMouseCapture)?;     // <-- NEW

    let result = run(&mut terminal);

    execute!(std::io::stdout(), DisableMouseCapture).ok(); // <-- NEW (tolerate errors on shutdown)
    ratatui::restore();
    result
}
```

**Panic-path concern:** If the app panics between `EnableMouseCapture` and `DisableMouseCapture`, the terminal will emit mouse escape sequences after exit. `ratatui::init()`'s panic hook restores raw mode and alternate screen but not mouse capture specifically. [ASSUMED — not 100% verified] Mitigation: install an additional panic hook that chains the existing one and calls `execute!(stdout(), DisableMouseCapture)` first. Low priority for v1 since panics should be rare; acceptable deviation if planner defers this.

### Pattern 7: Mouse Event Handling + Hit-Testing

**What:** `Event::Mouse(MouseEvent { kind, column, row, modifiers })` is the ratatui/crossterm mouse event. `kind` is a `MouseEventKind` enum variant. For Phase 2 we care about:
- `MouseEventKind::Down(MouseButton::Left)` — click to select / focus
- `MouseEventKind::ScrollUp` / `ScrollDown` — wheel scroll
- Others (`Drag`, `Up`, `Moved`) — ignore for v1

```rust
// Source: crossterm::event::MouseEvent / MouseEventKind
// [VERIFIED: docs.rs/crossterm + ratatui mouse-drawing example]

if let Event::Mouse(mev) = event::read()? {
    let msg = match mev.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if rect_contains(app.sidebar_rect, mev.column, mev.row) {
                // Subtract 1 for top border; the ListItem y-offset within the inner area
                // is row - sidebar_rect.y - 1. Then add diff_scroll_sidebar if we later add
                // sidebar scrolling; for now the sidebar list doesn't scroll in Phase 2.
                let idx = (mev.row.saturating_sub(app.sidebar_rect.y + 1)) as usize;
                Some(Message::MouseClickSidebar(idx))
            } else if rect_contains(app.diff_rect, mev.column, mev.row) {
                Some(Message::FocusDiff)
            } else {
                None
            }
        }
        MouseEventKind::ScrollDown => {
            if rect_contains(app.diff_rect, mev.column, mev.row) {
                Some(Message::ScrollDiffDown)
            } else { None }
        }
        MouseEventKind::ScrollUp => {
            if rect_contains(app.diff_rect, mev.column, mev.row) {
                Some(Message::ScrollDiffUp)
            } else { None }
        }
        _ => None,
    };
    if let Some(m) = msg { app.update(m); }
}

fn rect_contains(r: ratatui::layout::Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x.saturating_add(r.width)
        && row >= r.y && row < r.y.saturating_add(r.height)
}
```

**Storing Rects on App:** The `view()` function computes layout each frame. To make hit-testing work without re-computing layout in the event handler, `view()` should assign to `app.sidebar_rect` / `app.diff_rect` via a `&mut App` borrow OR return the rects. Cleanest: `ui::view(&mut app)` takes `&mut App` so it can write the rects. This is a minor break from the Phase 1 "pure view" claim, but it's a standard TEA compromise documented in ratatui recipes. Alternative: recompute `Layout::horizontal(...)` in the event handler (simpler, zero new state). Planner should pick one — recomputing is simpler and has no correctness issue since it's deterministic on terminal size.

**Message::MouseClickSidebar(index) update logic:**
```rust
Message::MouseClickSidebar(idx) => {
    if idx < self.files.len() {
        self.selected_index = idx;
        self.focus = Focus::Sidebar;
        self.load_diff_for_selected();
    }
}
```

### Pattern 8: Hunk Navigation (n/N)

**What:** Precompute `hunk_line_starts: Vec<u16>` — the `diff_scroll` value corresponding to each hunk header row. On `n`, scroll to the next value strictly greater than current `diff_scroll`. On `N`, scroll to the last value strictly less than current.

**Construction:** In `load_diff_for_selected`, after computing `DiffContent`:
```rust
// The current ui.rs renders: for each hunk, first a header line then each DiffLine.
// So hunk i starts at cumulative row = sum over j<i of (1 + hunks[j].lines.len()).
let mut starts = Vec::with_capacity(self.diff_content.as_ref().map(|d| d.hunks.len()).unwrap_or(0));
let mut cum = 0u16;
if let Some(dc) = &self.diff_content {
    for h in &dc.hunks {
        starts.push(cum);
        cum = cum.saturating_add(1 + h.lines.len() as u16);
    }
}
self.hunk_line_starts = starts;
```

**n / N update:**
```rust
Message::NextHunk => {
    if let Some(&next) = self.hunk_line_starts.iter().find(|&&s| s > self.diff_scroll) {
        self.diff_scroll = next;
    }
    // else: no-op (already at last hunk)
}
Message::PrevHunk => {
    if let Some(&prev) = self.hunk_line_starts.iter().rev().find(|&&s| s < self.diff_scroll) {
        self.diff_scroll = prev;
    }
    // else: no-op (already at first hunk)
}
```

**Keybind mapping (main.rs, Focus::DiffView):**
```rust
KeyCode::Char('n') => Some(Message::NextHunk),
KeyCode::Char('N') => Some(Message::PrevHunk),
```

Planner note: `n`/`N` don't conflict with Phase 1's DiffView bindings (`q`, `j`/`k`/`Down`/`Up`, `Tab`, `Esc`).

### Pattern 9: Extension Map

**What:** Map file extension -> language key in the `HighlightRegistry`.

```rust
fn lang_for_extension(ext: &str) -> Option<&'static str> {
    match ext {
        "rs"                => Some("rust"),
        "ts" | "tsx"        => Some("typescript"),
        "js" | "jsx" | "mjs" | "cjs" => Some("javascript"),
        "py" | "pyi"        => Some("python"),
        "go"                => Some("go"),
        "c" | "h"           => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => Some("cpp"),
        "json"              => Some("json"),
        "yaml" | "yml"      => Some("yaml"),
        "toml"              => Some("toml"),
        _ => None,
    }
}
```

Extension is extracted from `DiffContent.path` via `std::path::Path::new(&path).extension().and_then(|e| e.to_str())`.

### Anti-Patterns to Avoid
- **Re-parsing on every scroll tick.** Tree-sitter is fast but not free. Parse once in `load_diff_for_selected`, cache result, invalidate on file change.
- **Parsing diff fragments instead of full files.** Explicitly forbidden by STATE.md locked decision. Fragments produce ERROR nodes and ruin highlighting.
- **Running syntax highlighting on the inserted `+`/`-` prefix character.** The prefix is UI chrome, not source — it's added in `ui.rs` as a `Span::styled("+", ...)`. The source-text span is separate.
- **Calling `EnableMouseCapture` before `ratatui::init()`.** Order matters: init enters alternate screen, then enable mouse in the alternate screen. Doing it before init can leave the main screen in mouse-capture mode.
- **Using `app.focus` to decide whether to accept mouse events.** Mouse events bypass focus — a click anywhere should be actionable. Focus is for keyboard only.
- **Hit-testing against `frame.area()` instead of the panel rects.** Must use the actual sidebar/diff rects derived from the Layout split; otherwise clicks near borders misattribute.
- **Forgetting `.configure(HIGHLIGHT_NAMES)`.** Without it, `Highlighter::highlight` yields only `Source` events with no `HighlightStart`, effectively producing no highlighting. [VERIFIED: tree-sitter-highlight docs]
- **Applying syntax colors on top of +/- green/red.** ratatui `Style` merging makes the result muddy. Gate syntax highlighting to `ChangeKind::Equal` for v1.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Syntax parsing | Hand-rolled lexer per language | `tree-sitter` + per-language grammar crates | Each grammar is ~10k lines of careful parser work; maintaining 10 languages is ecosystem-scale effort |
| Highlight query execution | Custom tree-walker against grammar | `tree-sitter-highlight::Highlighter` | Handles scope nesting, local variables, injection queries, overlapping captures |
| Byte-offset -> line mapping | Linear scan per event | Precomputed `line_starts` + `partition_point` binary search | O(n + k log n) vs O(n × k) where k = event count |
| Scope -> color mapping | Per-grammar color tables | Single `scope_to_style` matching the standard HIGHLIGHT_NAMES | All tree-sitter grammars converge on the same scope name convention |
| Mouse event parsing | Parsing ANSI mouse escape sequences | `crossterm::event::Event::Mouse` | Crossterm handles X10/SGR/URXVT modes and normalizes across terminals |
| Rect hit-testing | Manual bounds math | `ratatui::layout::Rect` contains checks (custom helper, 4 lines) | Trivial but use the Rect type rather than bare u16 tuples |
| Hunk-to-scroll mapping | Scanning rendered output | Cached `Vec<u16>` of cumulative row offsets | Precompute once on file load; `n`/`N` is O(log hunks) |

## Common Pitfalls

### Pitfall 1: Grammar Version Mismatch Confusion
**What goes wrong:** Developer sees `tree-sitter-rust = 0.24.2` and `tree-sitter = 0.25.x`, panics about version mismatch, tries to downgrade tree-sitter.
**Why it happens:** CLAUDE.md framing plus Rust habit of same-version-everywhere.
**How to avoid:** Remember grammars depend on `tree-sitter-language = 0.1`, the ABI-stable shim. Runtime tree-sitter 0.25.x and grammar tree-sitter-xxx 0.23/0.24/0.25 versions interoperate freely. The dev-dep `tree-sitter = "0.25.8"` in grammar Cargo.tomls is for grammar authors to run tests, not a runtime constraint.
**Warning signs:** Cargo complaining about duplicate tree-sitter versions (not a problem unless both end up in runtime), or a dev attempting to pin tree-sitter-rust to "0.25".
[VERIFIED: every grammar Cargo.toml inspected live on 2026-04-23]

### Pitfall 2: The TOML Crate Name Trap
**What goes wrong:** `cargo add tree-sitter-toml` fetches a 2023-era unmaintained crate with old tree-sitter ABI.
**Why it happens:** The name suggests it's the canonical TOML grammar; it isn't anymore.
**How to avoid:** Use `tree-sitter-toml-ng` (note the `-ng` suffix) from `tree-sitter-grammars/tree-sitter-toml`. Its Rust module name is `tree_sitter_toml_ng`.
**Warning signs:** Compile errors about `LanguageFn` vs `Language`, or missing `HIGHLIGHTS_QUERY` constant.
[VERIFIED: raw Cargo.toml fetch]

### Pitfall 3: Missing `.configure()` Yields Blank Highlighting
**What goes wrong:** Code runs without error, but no syntax colors appear — every event is `Source { start, end }` with no `HighlightStart`.
**Why it happens:** `HighlightConfiguration::new` builds the config but doesn't know which scope names the caller cares about until `.configure(&names)` is called.
**How to avoid:** Always call `.configure(HIGHLIGHT_NAMES)` immediately after `new` in the registry construction. Lock the `HIGHLIGHT_NAMES` slice as a `pub const` so it's impossible to forget.
**Warning signs:** Diff view renders with correct text content but no colors for keywords/strings/etc.
[VERIFIED: tree-sitter-highlight 0.25 API docs]

### Pitfall 4: Mouse Escape Sequences Leaking After Exit
**What goes wrong:** After quitting the app, the shell shows garbage like `^[[<0;34;14M` whenever the user moves the mouse.
**Why it happens:** `DisableMouseCapture` wasn't issued before the terminal was restored (or was skipped on error path).
**How to avoid:** Always pair `EnableMouseCapture` with `DisableMouseCapture` in a `Drop`-like pattern. In main, ensure both are in the cleanup flow even on error. Optionally install a secondary panic hook that calls `DisableMouseCapture` before ratatui's hook.
**Warning signs:** Terminal emits `\e[<...M` / `\e[<...m` sequences after the binary exits.

### Pitfall 5: Highlighting Applied to Added/Removed Lines Conflicts with +/- Color
**What goes wrong:** Added lines show up in murky mixed green+blue (keyword) or black-on-green (hard to read on some terminals).
**Why it happens:** ratatui `Style` layering sets both foreground colors; the diff's green wins only if applied last. Order matters.
**How to avoid:** For v1, apply syntax highlighting only to `ChangeKind::Equal` lines. Add/delete lines keep the single-color render from Phase 1. (gitui, delta, and most diff tools do exactly this.)
**Warning signs:** User reports "insert lines are ugly" or "can't read keywords on green background."

### Pitfall 6: Mouse Click on Sidebar Border Selects File -1
**What goes wrong:** Clicking the top border row sets `selected_index` to `usize::MAX` (underflow) or index 0 incorrectly.
**Why it happens:** `row - sidebar_rect.y - 1` can underflow if `row <= sidebar_rect.y`.
**How to avoid:** Use `row.saturating_sub(sidebar_rect.y + 1)` and validate `< app.files.len()` before using as index. Also bound-check both row (< rect bottom) and column (< rect right) before computing.
**Warning signs:** Panic on click at specific rows, or wrong file selected from top border clicks.

### Pitfall 7: Hunk Nav on Binary / Empty Diffs
**What goes wrong:** `n` panics or does nothing weird when `hunk_line_starts` is empty.
**Why it happens:** Edge case: binary file or no-change file has no hunks.
**How to avoid:** `n`/`N` are no-ops if `hunk_line_starts` is empty. Use `.iter().find()` which returns `Option` naturally.
**Warning signs:** None in the safe implementation above; if tests pass on empty diffs, this is covered.

### Pitfall 8: Large Files Cause Visible Stutter on File Select
**What goes wrong:** Selecting a large file (10k+ lines) pauses the UI for 200+ ms during tree-sitter parse.
**Why it happens:** Parsing is synchronous in `load_diff_for_selected` and blocks the event loop.
**How to avoid (v1):** Gate highlighting with a byte threshold — skip highlighting for files > 256 KB (return `None` from `highlight_source`). Render the raw diff instead. 256 KB is well above typical source files and keeps parse time < 50 ms for all listed languages. [VERIFIED: rough perf numbers from tree-sitter benchmarks across grammars]
**How to avoid (v2 — defer):** Run highlighting on a background thread, send styled spans via channel. Out of scope for Phase 2.
**Warning signs:** Noticeable UI lag when selecting large files.

### Pitfall 9: UTF-8 Boundary Panic in String Slicing
**What goes wrong:** `source[byte..slice_end].to_string()` panics if `byte` or `slice_end` falls in the middle of a multi-byte UTF-8 sequence.
**Why it happens:** Tree-sitter byte offsets are always UTF-8 boundaries (it parses byte-by-byte respecting UTF-8), BUT if we manually compute `slice_end - 1` for trailing-newline trimming, we might land mid-codepoint. Since `\n` is ASCII (1 byte), and we only subtract 1 if the previous byte is `\n`, this is safe — but worth an assertion in debug builds.
**How to avoid:** Use `source.get(byte..slice_end).unwrap_or("").to_string()` for safety, or `debug_assert!(source.is_char_boundary(byte) && source.is_char_boundary(slice_end))`.
**Warning signs:** Panic on files with multi-byte chars (CJK, emoji in strings/comments).

## Code Examples

### Complete `src/syntax/registry.rs` skeleton
```rust
// Source: synthesis of tree-sitter-highlight 0.25 API patterns
// [VERIFIED against docs.rs/tree-sitter-highlight/0.25.4]

use std::collections::HashMap;
use std::sync::OnceLock;
use tree_sitter_highlight::HighlightConfiguration;

pub const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute", "comment", "constant", "constant.builtin", "constructor",
    "function", "function.builtin", "function.method", "keyword", "number",
    "operator", "property", "punctuation", "punctuation.bracket",
    "punctuation.delimiter", "string", "string.special", "tag", "type",
    "type.builtin", "variable", "variable.builtin", "variable.parameter",
];

pub struct HighlightRegistry {
    by_lang: HashMap<&'static str, HighlightConfiguration>,
}

static REGISTRY: OnceLock<HighlightRegistry> = OnceLock::new();

pub fn registry() -> &'static HighlightRegistry {
    REGISTRY.get_or_init(HighlightRegistry::build)
}

impl HighlightRegistry {
    fn build() -> Self {
        let mut by_lang = HashMap::new();
        by_lang.insert("rust",       build("rust",
            tree_sitter_rust::LANGUAGE.into(),
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY,
            ""));
        by_lang.insert("python",     build("python",
            tree_sitter_python::LANGUAGE.into(),
            tree_sitter_python::HIGHLIGHTS_QUERY,
            "", ""));
        by_lang.insert("javascript", build("javascript",
            tree_sitter_javascript::LANGUAGE.into(),
            tree_sitter_javascript::HIGHLIGHT_QUERY,     // note: JS uses HIGHLIGHT_QUERY (singular) in some versions; check at compile time
            tree_sitter_javascript::INJECTIONS_QUERY,
            tree_sitter_javascript::LOCALS_QUERY));
        // ... ts, go, c, cpp, json, yaml, toml ...
        Self { by_lang }
    }

    pub fn get(&self, lang: &str) -> Option<&HighlightConfiguration> {
        self.by_lang.get(lang)
    }
}

fn build(
    name: &'static str,
    lang: tree_sitter::Language,
    highlights: &str,
    injections: &str,
    locals: &str,
) -> HighlightConfiguration {
    let mut cfg = HighlightConfiguration::new(lang, name, highlights, injections, locals)
        .expect("valid highlight configuration");
    cfg.configure(HIGHLIGHT_NAMES);
    cfg
}
```

**Planner note on JS const name:** Some versions of `tree-sitter-javascript` export `HIGHLIGHT_QUERY` (singular) and some export `HIGHLIGHTS_QUERY` (plural — matches other grammars). Verify at implementation time by checking the actual version's `src/lib.rs`. [VERIFIED: 0.25.0 source uses `HIGHLIGHTS_QUERY` but historically has been inconsistent — trust cargo error messages and adjust]

### Integrating into `App::load_diff_for_selected`
```rust
// Add to app.rs — new field on App:
//   pub styled_diff: Option<StyledDiffContent>,
//   pub hunk_line_starts: Vec<u16>,

fn load_diff_for_selected(&mut self) {
    // ... existing Phase 1 logic producing self.diff_content ...

    self.hunk_line_starts = compute_hunk_line_starts(self.diff_content.as_ref());
    self.styled_diff = self.diff_content.as_ref()
        .filter(|dc| !dc.is_binary)
        .and_then(|dc| {
            let ext = std::path::Path::new(&dc.path)
                .extension()
                .and_then(|e| e.to_str());
            let old_lines = old_text.and_then(|t| highlight_source(t, ext));
            let new_lines = new_text.and_then(|t| highlight_source(t, ext));
            // Combine into a StyledDiffContent parallel to `dc`...
            Some(build_styled_diff(dc, old_lines.as_ref(), new_lines.as_ref()))
        });
}

fn compute_hunk_line_starts(dc: Option<&DiffContent>) -> Vec<u16> {
    let Some(dc) = dc else { return Vec::new(); };
    if dc.is_binary { return Vec::new(); }
    let mut out = Vec::with_capacity(dc.hunks.len());
    let mut cum: u16 = 0;
    for h in &dc.hunks {
        out.push(cum);
        cum = cum.saturating_add(1 + h.lines.len() as u16); // +1 for hunk header row
    }
    out
}
```

## Runtime State Inventory

N/A — this is a greenfield phase (new functionality, no rename/refactor/migration). No runtime state to track.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `tree_sitter_rust::language()` function | `tree_sitter_rust::LANGUAGE: LanguageFn` const + `.into()` | ~tree-sitter 0.22 | All grammar crates migrated; old `language()` fn removed in current versions |
| Per-grammar `tree-sitter` runtime dependency | `tree-sitter-language = "0.1"` ABI shim | ~mid-2024 | Decoupled grammar crate versioning from runtime; allows mixing 0.23/0.24/0.25 grammars on a 0.25 runtime |
| `tree-sitter-toml` (ikatyang/original) | `tree-sitter-toml-ng` (tree-sitter-grammars/tree-sitter-toml) | 2024 | The `-ng` fork is the maintained one; the original is orphaned |
| `syntect` for TUI highlighting (bat, pre-2022) | `tree-sitter-highlight` + grammar crates (helix, gitui) | ongoing | AST-aware highlighting + shared with AST features; required by CLAUDE.md |
| Manual panic hook for terminal restore | `ratatui::init()` auto-installs panic hook | ratatui 0.28 | Already used in Phase 1; mouse capture cleanup must be layered on top manually |

**Deprecated/outdated:**
- `tree_sitter_rust::language()` — use `LANGUAGE.into()` instead
- `tree-sitter-toml` (the unqualified name) — use `tree-sitter-toml-ng`
- `ikatyang/tree-sitter-yaml` — use the `tree-sitter-grammars/tree-sitter-yaml` fork
- tree-sitter 0.26.x — explicitly rejected by CLAUDE.md for ecosystem-lag reasons; all grammar versions listed here work on 0.25

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `tree-sitter-cpp` 0.23.4 (dev-dep on tree-sitter 0.26.5) works on runtime tree-sitter 0.25.x via the `tree-sitter-language` 0.1 shim | Grammar Crates table | Medium — if ABI check fails at runtime, drop cpp from v1 grammar set; user impact is only that .cpp/.h files show unhighlighted. Mitigation: Wave 0 smoke test that parses a minimal cpp snippet. |
| A2 | Specific scope→color mappings in `scope_to_style` | Pattern 4 | Low — aesthetics only, easy to tune post-implementation based on user feedback |
| A3 | `MAX_HIGHLIGHT_BYTES = 256 * 1024` is a reasonable performance ceiling | Pitfall 8 + Performance Ceiling | Low — threshold is tunable; worst case is lag on ridiculous files. 256 KB is ~5x typical largest source files and well within tree-sitter's linear parse range |
| A4 | `ratatui::init()`'s panic hook does not call `DisableMouseCapture` | Pattern 6 | Low — at worst user sees mouse escape sequences after panic (rare). Mitigation: add explicit panic hook if this becomes a reported issue |
| A5 | `Paragraph::scroll((diff_scroll, 0))` correctly counts hunk header rows in its line-count | Pattern 8 | Low — Phase 1 already uses this; unit test with known hunk layout in Wave 0 |
| A6 | `tree-sitter-javascript` 0.25.0 exports `HIGHLIGHTS_QUERY` (plural) | Code Examples | Low — compile-time error will catch; adjust to `HIGHLIGHT_QUERY` (singular) if needed. Historical inconsistency. |
| A7 | All listed grammar crates compile cleanly on the target platform (Linux/macOS/Windows) with the bundled C compiler | Installation | Medium — would block build. Low risk in practice; these are widely used crates. |

**If this table looks like it should be empty:** Every cell above represents genuine uncertainty. The planner should carry A1 and A6 into Wave 0 smoke tests, and A7 into the initial `cargo build` verification.

## Open Questions

1. **Should the Ui rect-tracking be stored on App, or should the event handler recompute Layout?**
   - What we know: Both approaches work. Recomputing is simpler (no new App state, no `&mut App` in view).
   - What's unclear: Slight perf cost of recomputing Layout every mouse event — but Layout is pure math, <1 µs.
   - Recommendation: Recompute in event handler. Simpler. Phase 1's pure-view contract stays intact.

2. **Should hunk nav `n`/`N` also be available in Focus::Sidebar, auto-switching focus to DiffView?**
   - What we know: gitui requires focus on diff panel first.
   - What's unclear: User ergonomics.
   - Recommendation: v1 = Focus::DiffView only (matches gitui). Reconsider based on UAT feedback.

3. **Do we want a subtle background tint on added/deleted lines (so we can still show syntax fg on them)?**
   - What we know: Delta and some newer tools do this; classical `git diff` does not.
   - What's unclear: Terminal color support is uneven (some 16-color terminals mangle bg tints).
   - Recommendation: Out of scope for Phase 2 v1. Stick with "syntax on equal lines only." Revisit if UAT asks.

4. **Should we highlight tokens within the `@@ -x +y @@` hunk header?**
   - What we know: Phase 1 colors it cyan as a single span.
   - What's unclear: No benefit to tokenizing it.
   - Recommendation: Leave as-is.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|-------------|-----------|---------|----------|
| C compiler (cc / gcc / clang) | Each tree-sitter grammar's `build.rs` | Assumed ✓ (already required by git2 in Phase 1) | — | — |
| `rustc` / `cargo` | Build system | ✓ (Phase 1 builds cleanly) | — | — |
| `pkg-config` / `libssl-dev` | Release-profile git2 (noted Phase 1 issue) | Not on sandbox but present on realistic dev machines | — | Debug builds work; release builds may need install on some environments |

**Missing dependencies with no fallback:** None for Phase 2 specifically. All grammars vendor their C sources; `cc` + `rustc` suffice.

**Missing dependencies with fallback:** Release-profile openssl dependency is pre-existing from Phase 1 (noted in 01-02-SUMMARY deviations). Does not block Phase 2.

## Performance Ceiling

| Dimension | Limit | Rationale |
|-----------|-------|-----------|
| Max source size for highlighting | **256 KB per side** (so up to 512 KB combined old+new parse) | Tree-sitter parses 100 KB of most languages in 5-20 ms. At 256 KB, worst case is ~50 ms — noticeable but acceptable on file switch. Files over this threshold fall back to unhighlighted render. |
| Max lines per diff before skipping hunk nav caching | None — cache is `Vec<u16>` and is O(hunks), trivial | — |
| Highlight cache lifetime | Per file selection (recomputed in `load_diff_for_selected`) | Simpler than LRU cache; files are switched infrequently. Add LRU later if profiling shows it matters. |
| Binary files | Skip highlighting entirely | Already caught by Phase 1 `ContentResult::Binary` -> `DiffContent.is_binary = true`. Syntax path gates on `!is_binary`. |
| Unknown extension | Skip highlighting | `lang_for_extension` returns `None`, `highlight_source` returns `None`, render falls back to plain text. |

## Impact on Existing Code

**`src/app.rs` — new fields + messages:**
- Add `pub hunk_line_starts: Vec<u16>`
- Add `pub styled_diff: Option<StyledDiffContent>` (or equivalent — may be represented as `Vec<Vec<StyledSpan>>` parallel to the existing `DiffContent.hunks[].lines`)
- Extend `Message` enum with: `NextHunk`, `PrevHunk`, `MouseClickSidebar(usize)`, `FocusDiff`
- Extend `update()` with four new cases (patterns documented above)
- Extend `load_diff_for_selected` to compute `hunk_line_starts` + `styled_diff` (both cleared on None path)
- Existing tests unaffected; add tests for hunk-start computation + `NextHunk`/`PrevHunk` clamping

**`src/ui.rs` — consume styled spans when available:**
- In `diff_lines()` inner loop, for `ChangeKind::Equal` lines AND when `styled_diff` is present: build `Line` from the `Vec<StyledSpan>` for that line instead of a single `Span::styled(content, style)`
- For `Insert`/`Delete` lines: unchanged (full-line green/red) — **v1 decision**, per Pattern 4 rationale
- No hit-testing here; layout rect lookup stays internal to `view`

**`src/main.rs` — mouse plumbing:**
- Add `execute!(stdout(), EnableMouseCapture)?` after `ratatui::init()`
- Add `execute!(stdout(), DisableMouseCapture).ok();` before `ratatui::restore()`
- Extend `event::read()` match arm with `Event::Mouse(mev) => ...` — translate to messages (patterns above)
- Extend Focus::DiffView key map with `'n'` and `'N'`

**`src/diff/types.rs` — no changes.** `DiffLine.content: String` stays. Styled overlays live in a parallel structure.

**`src/git/mod.rs` — no changes.** Already exposes `head_content` / `index_content` / `workdir_content` returning owned `ContentResult::Text(String)` — feed directly to `highlight_source`.

**New modules:**
- `src/syntax/mod.rs` (public: `highlight_source`, `StyledSpan`, `StyledLine`, `StyledDiffContent`)
- `src/syntax/registry.rs` (`HighlightRegistry`, `HIGHLIGHT_NAMES`, lang name mapping)
- `src/syntax/scope.rs` (`scope_to_style`)
- `src/syntax/mapping.rs` (`highlight_source` body — line_starts computation + event stream consumption)

**`Cargo.toml`:** 11 new dependencies (see Installation section).

## Security Domain

N/A — this is a local-only TUI tool reading from a local git repository. Phase 2 additions:
- Tree-sitter parses user-provided source files (from git). Tree-sitter parsers are memory-safe (C code with known behavior); the only concern is pathological input causing unbounded parse time, mitigated by the `MAX_HIGHLIGHT_BYTES` gate.
- Mouse input is OS-native and sanitized by crossterm; no injection surface.

No ASVS categories apply — no network, no auth, no persistence, no user-submitted input that isn't already the user's own files.

## Sources

### Primary (HIGH confidence)
- [tree-sitter-highlight 0.25.4 docs](https://docs.rs/tree-sitter-highlight/0.25.4/tree_sitter_highlight/) — `HighlightConfiguration::new`, `.configure()`, `Highlighter::highlight`, `HighlightEvent` variants
- [tree-sitter Language docs](https://docs.rs/tree-sitter/latest/tree_sitter/struct.Language.html) — `impl From<LanguageFn> for Language`
- [tree-sitter-rust bindings/rust/lib.rs](https://github.com/tree-sitter/tree-sitter-rust/blob/master/bindings/rust/lib.rs) — `LANGUAGE` const, `HIGHLIGHTS_QUERY` const, full public surface
- [ratatui mouse-drawing example](https://github.com/ratatui/ratatui/blob/main/examples/apps/mouse-drawing/src/main.rs) — `EnableMouseCapture` / `DisableMouseCapture` wrapping pattern, `MouseEvent` handling, `Position::new(event.column, event.row)`
- [crossterm MouseEventKind docs](https://docs.rs/crossterm/latest/crossterm/event/enum.MouseEventKind.html) — `Down(MouseButton)`, `ScrollUp`, `ScrollDown`, `Drag`, variants
- Grammar Cargo.toml files fetched live from raw.githubusercontent.com for: `tree-sitter-rust`, `tree-sitter-typescript`, `tree-sitter-javascript`, `tree-sitter-python`, `tree-sitter-go`, `tree-sitter-c`, `tree-sitter-cpp`, `tree-sitter-json`, `tree-sitter-yaml` (tree-sitter-grammars fork), `tree-sitter-toml-ng`

### Secondary (MEDIUM confidence)
- [ratatui 0.30 highlights](https://ratatui.rs/highlights/v030/) — workspace architecture, crossterm re-export rationale
- [Ratatui Mouse Capture concept](https://ratatui.rs/concepts/backends/mouse-capture/) — backend-agnostic framing
- [gitui key config](https://github.com/extrawurst/gitui/blob/master/KEY_CONFIG.md) — `n`/`N` hunk nav precedent
- [Tony Finch: Syntax highlighting with tree-sitter](https://dotat.at/@/2025-03-30-hilite.html) — practical integration guide (conceptual, not verbatim code)

### Tertiary (LOW confidence / Assumed)
- Exact color palette in `scope_to_style` — synthesized from common TUI themes, not cited. Aesthetic choice.
- Performance threshold of 256 KB — chosen as "comfortable ceiling," not benchmarked in this session.
- Claim that `tree-sitter-cpp` 0.23.4 (0.26 dev-dep) runs cleanly on runtime 0.25.x — inferred from the `tree-sitter-language` 0.1 ABI shim guarantee, not verified with a live parse.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — every grammar Cargo.toml fetched and inspected live; versions verified on 2026-04-23
- Tree-sitter-highlight API: HIGH — docs + example code cross-checked
- Highlight mapping algorithm (Pattern 3): HIGH — standard pattern, verified event semantics from docs
- Mouse integration (Patterns 6, 7): HIGH — verbatim pattern from ratatui example
- Hunk nav (Pattern 8): HIGH — straightforward scan with precomputed index, matches Phase 1's scroll model
- Color palette: MEDIUM (aesthetic, tunable)
- Cpp grammar ABI compat: MEDIUM (shim guarantee + assumption)

**Research date:** 2026-04-23
**Valid until:** 2026-05-23 (30 days — stack is stable; tree-sitter 0.26 is explicitly excluded so no imminent churn expected)
