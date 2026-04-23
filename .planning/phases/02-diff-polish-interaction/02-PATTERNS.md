# Phase 2: Diff Polish + Interaction - Pattern Map

**Mapped:** 2026-04-23
**Files analyzed:** 9 (4 new, 4 modified, 1 Cargo.toml)
**Analogs found:** 8 / 9 (the cpp/tree-sitter crate mechanics have no Phase 1 analog — follow RESEARCH.md verbatim for those internals)

Phase 1 is the only codebase present. All analog references below are to Phase 1 files. The overarching idiom is:
- **Module = `mod.rs` exposing functions + constants; `types.rs` holding owned data structs** (matches `src/git/mod.rs` + `src/git/types.rs`; matches `src/diff/mod.rs` + `src/diff/types.rs`).
- **View/render = pure function of `(&mut Frame, &App)`** (matches `src/ui.rs::view`).
- **State mutation = single `App::update(Message)` match** with one case per `Message` variant (matches `src/app.rs::update`).
- **Tests = `#[cfg(test)] mod tests { use super::*; ... }` at the bottom of each module**, many small focused `#[test] fn test_<behaviour>()` functions (matches every Phase 1 file).

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `src/syntax/mod.rs` (NEW) | module root / public API + types | transform (source text -> styled spans) | `src/diff/mod.rs` + `src/diff/types.rs` | exact (new isolated subsystem w/ public transform fns) |
| `src/syntax/registry.rs` (NEW) | config-cache service | init-once data bundle | no exact analog — closest is the stateless function style in `src/git/mod.rs` | role-match |
| `src/syntax/scope.rs` (NEW) | pure mapping util | lookup / transform | `src/ui.rs::status_style` (scope/key -> `Style`) | exact |
| `src/syntax/mapping.rs` (NEW) | highlight event -> per-line-span transform | stream transform | `src/diff/mod.rs::compute_hunks` (iter events -> structured `Vec<DiffHunk>`) | exact (iterator-of-events folding into parallel per-line structure) |
| `src/app.rs` (MODIFY) | TEA state + update | state machine / request-response | itself (Phase 1) | self |
| `src/ui.rs` (MODIFY) | rendering | pull (read `app` -> emit `Line`s) | itself (Phase 1) | self |
| `src/main.rs` (MODIFY) | event loop + terminal lifecycle | event-driven | itself (Phase 1) | self |
| `src/diff/types.rs` (MODIFY — optional) | owned data | data | itself (Phase 1) | self |
| `Cargo.toml` (MODIFY) | config | — | itself | self |

## Pattern Assignments

### `src/syntax/mod.rs` (module root, transform)

**Analog:** `src/diff/mod.rs` (lines 1-4) — declares `pub mod types`, imports from it, re-exports free functions. Same shape here: `pub mod registry; pub mod scope; pub mod mapping;` plus the public `StyledSpan` / `StyledLine` types and the `highlight_source` free function.

**Analog for types-next-to-module pattern** — `src/diff/types.rs` (full file, 28 lines):
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChangeKind { Equal, Insert, Delete }

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: ChangeKind,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone)]
pub struct DiffContent {
    pub path: String,
    pub hunks: Vec<DiffHunk>,
    pub is_binary: bool,
}
```

**Copy idioms:**
- Plain `pub` fields (no getters) — same as every type in Phase 1.
- `#[derive(Debug, Clone)]` on data structs.
- Owned `String` only (no `&str` / lifetimes leak into the public API — mirrors the STATE.md locked decision that Phase 1 already honours in `ContentResult::Text(String)`).
- No re-exports of external crate types. `StyledSpan { pub text: String, pub style: ratatui::style::Style }` follows the same shape as `DiffLine`.

**Free-function signature pattern** from `src/diff/mod.rs` lines 75-84:
```rust
pub fn compute_diff_content(path: &str, old_content: Option<&str>, new_content: Option<&str>) -> DiffContent {
    let old = old_content.unwrap_or("");
    let new = new_content.unwrap_or("");
    let hunks = compute_hunks(old, new, 3);
    DiffContent { path: path.to_string(), hunks, is_binary: false }
}
```
Mirror for `syntax::highlight_source(source: &str, extension: Option<&str>) -> Option<Vec<StyledLine>>` — borrowed `&str` in, owned result out, `Option` for missing inputs.

---

### `src/syntax/registry.rs` (service, init-once data)

**Analog:** None in Phase 1 exactly — Phase 1 has no long-lived init-once caches. Closest stylistic match is the small-helper cluster at the top of `src/git/mod.rs` (free `fn is_binary_content`, `fn map_index_status`, `fn map_workdir_status` — module-private helpers), plus `GitRepo::open` (lines 46-49) as the "constructor" pattern:
```rust
impl GitRepo {
    pub fn open(path: &str) -> Result<Self, git2::Error> {
        let repo = git2::Repository::discover(path)?;
        Ok(Self { repo })
    }
```

**Copy idioms:**
- `pub struct HighlightRegistry { by_lang: HashMap<&'static str, HighlightConfiguration> }` — private field (follows `GitRepo { repo: git2::Repository }` line 6-8 of `src/git/mod.rs`: one private field wrapping the external handle).
- A private `fn build(...) -> HighlightConfiguration { ... .expect(...) }` helper at module bottom mirrors the private helper block in `src/git/mod.rs` lines 10-43.
- `.expect("rust highlight config")` style for programmer-guaranteed init matches the test-setup pattern in `src/app.rs` line 190 (`GitRepo::open("/workspace").expect("workspace repo should open")`).

**Per-RESEARCH.md Code Examples block (lines 669-733)** — this file's internals (HIGHLIGHT_NAMES const, `OnceLock`, `get_or_init`) are new-to-codebase and come verbatim from the research. Use that skeleton — Phase 1 has nothing to copy here.

---

### `src/syntax/scope.rs` (pure mapping util)

**Analog:** `src/ui.rs::status_style` (lines 15-28). This is the canonical shape for "categorical input -> `Style`" in this codebase:
```rust
fn status_style(entry: &FileEntry) -> Style {
    // Prefer workdir_status for coloring, fall back to index_status.
    let status = entry
        .workdir_status
        .or(entry.index_status);
    match status {
        Some(FileStatus::Modified) => Style::default().fg(Color::Yellow),
        Some(FileStatus::Added)    => Style::default().fg(Color::Green),
        Some(FileStatus::Deleted)  => Style::default().fg(Color::Red),
        Some(FileStatus::Renamed)  => Style::default().fg(Color::Cyan),
        Some(FileStatus::Untracked)=> Style::default().fg(Color::DarkGray),
        None => Style::default(),
    }
}
```

**Copy idioms:**
- Return `ratatui::style::Style` (use `use ratatui::prelude::*;` like `ui.rs` line 1).
- `match` arms with `Style::default().fg(Color::X)` — **do not** reach for `Modifier::BOLD` or `bg()` per RESEARCH.md Pattern 4 (avoid mixing with diff +/- fg).
- Catch-all arm returns `Style::default()` (matches `None => Style::default()` at line 26).
- Pure function; no allocation, no error paths. Keep it a free `pub fn scope_to_style(h: Highlight) -> Style`.

---

### `src/syntax/mapping.rs` (stream transform)

**Analog:** `src/diff/mod.rs::compute_hunks` (lines 11-69). Same shape: pre-seeded output buffer, loop over an external iterator of events, fold into structured per-line output.

**Pre-computation then event-loop pattern** from `src/diff/mod.rs` lines 11-66:
```rust
pub fn compute_hunks(old: &str, new: &str, context_lines: usize) -> Vec<DiffHunk> {
    let diff = TextDiff::from_lines(old, new);
    let grouped = diff.grouped_ops(context_lines);

    let mut hunks = Vec::new();

    for group in grouped {
        if group.is_empty() { continue; }

        let old_start = group[0].old_range().start as u32 + 1;
        let new_start = group[0].new_range().start as u32 + 1;

        let mut lines = Vec::new();

        for op in &group {
            for change in diff.iter_changes(op) {
                let old_idx = change.old_index();
                let new_idx = change.new_index();
                let content = change.value().to_string();

                let (kind, old_lineno, new_lineno) = match change.tag() {
                    ChangeTag::Equal  => (ChangeKind::Equal,  Some(old_idx.unwrap() as u32 + 1), Some(new_idx.unwrap() as u32 + 1)),
                    ChangeTag::Delete => (ChangeKind::Delete, Some(old_idx.unwrap() as u32 + 1), None),
                    ChangeTag::Insert => (ChangeKind::Insert, None, Some(new_idx.unwrap() as u32 + 1)),
                };

                lines.push(DiffLine { kind, old_lineno, new_lineno, content });
            }
        }

        hunks.push(DiffHunk { old_start, new_start, lines });
    }

    hunks
}
```

**Copy idioms:**
- Imperative `for event in events { match event { ... } }` — **not** iterator combinators. The match arms here should mirror RESEARCH.md Pattern 3 (`HighlightStart` / `HighlightEnd` / `Source`) with the same flat style.
- Name the owned output `result: Vec<StyledLine>` (pre-allocated `(0..num_lines).map(|_| Vec::new()).collect()`), push onto it as events arrive — matches the `let mut lines = Vec::new(); ... lines.push(...);` rhythm above.
- Convert borrowed slice fragments to owned with `.to_string()` (matches line 32: `let content = change.value().to_string();`).
- On malformed / missing input, return a sentinel value (`None` for `highlight_source`) — mirrors `binary_diff_content` returning a sentinel `DiffContent` (src/diff/mod.rs lines 90-96).

**Tests next to the module** — follow `src/diff/mod.rs` lines 98-227. Small focused tests: `test_single_line_source`, `test_multiline_all_equal_lines`, `test_byte_range_spanning_two_lines`, `test_unknown_extension_returns_none`, `test_oversize_source_returns_none`. Same `#[cfg(test)] mod tests { use super::*; #[test] fn test_...() { ... } }` layout.

---

### `src/app.rs` (MODIFY) — TEA state + update

**Analog:** itself (lines 1-290). Copy the idioms verbatim for the new additions.

**Message enum extension** — current `Message` enum (lines 12-21):
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Message {
    MoveUp,
    MoveDown,
    SelectFile,
    ScrollDiffUp,
    ScrollDiffDown,
    SwitchFocus,
    Quit,
}
```
Add new variants in the same style. Unit variants stay unit; tuple variants for payload-bearing messages — keep `Copy` **only** if all variants are `Copy`. `MouseClickSidebar(usize)` is `Copy`, `FocusDiff` is unit, `NextHunk`/`PrevHunk` are unit. Keep `#[derive(Debug, Clone, Copy, PartialEq)]`.

Planner note: if you later add a variant that carries `String` (e.g., a filter query), drop `Copy` and keep `Clone, PartialEq`. Not needed for Phase 2.

**App struct extension** — current (lines 23-31):
```rust
pub struct App {
    pub repo: GitRepo,
    pub files: Vec<FileEntry>,
    pub selected_index: usize,
    pub diff_content: Option<DiffContent>,
    pub diff_scroll: u16,
    pub focus: Focus,
    pub should_quit: bool,
}
```
Add per RESEARCH.md Impact section:
```rust
// NEW FIELDS
pub hunk_line_starts: Vec<u16>,
pub styled_diff: Option<...>,  // type defined in src/syntax/mod.rs
```
All `pub`, no getters — matches the Phase 1 convention exactly. Initialize in `App::new()` alongside existing fields (lines 37-45):
```rust
let mut app = App {
    repo,
    files,
    selected_index: 0,
    diff_content: None,
    diff_scroll: 0,
    focus: Focus::Sidebar,
    should_quit: false,
    // NEW:
    hunk_line_starts: Vec::new(),
    styled_diff: None,
};
```

**`update()` match arm style** — current arms (lines 108-150):
```rust
pub fn update(&mut self, msg: Message) {
    match msg {
        Message::MoveUp => {
            if self.selected_index > 0 {
                self.selected_index -= 1;
                if self.focus == Focus::Sidebar {
                    self.load_diff_for_selected();
                }
            }
        }
        Message::ScrollDiffUp => {
            if self.diff_scroll > 0 {
                self.diff_scroll -= 1;
            }
        }
        Message::ScrollDiffDown => {
            let max_scroll = self.total_diff_lines().saturating_sub(1) as u16;
            if self.diff_scroll < max_scroll {
                self.diff_scroll = self.diff_scroll.saturating_add(1);
            }
        }
        Message::SwitchFocus => {
            self.focus = match self.focus {
                Focus::Sidebar => Focus::DiffView,
                Focus::DiffView => Focus::Sidebar,
            };
        }
        Message::Quit => { self.should_quit = true; }
        // ... etc
    }
}
```
**Copy idioms for new arms:**
- Guard every state mutation (e.g., `if self.diff_scroll > 0` before decrement; matches `Message::ScrollDiffUp`).
- Use `saturating_*` arithmetic on scroll (matches `saturating_add(1)` on line 138, `saturating_sub(1)` on line 136).
- Write the bounds check inline; do not factor out until duplication appears.
- `Message::NextHunk` / `PrevHunk` should use `.iter().find()` / `.iter().rev().find()` as in RESEARCH.md Pattern 8 — no panics on empty `hunk_line_starts`.
- `Message::MouseClickSidebar(idx)` must bounds-check `idx < self.files.len()` before assigning `self.selected_index = idx` (matches the `if !self.files.is_empty() && self.selected_index < self.files.len() - 1` style on line 119). Then set `self.focus = Focus::Sidebar` and call `self.load_diff_for_selected()`.
- `Message::FocusDiff` is a 2-liner: `self.focus = Focus::DiffView;` — mirrors the shape of `Message::Quit` (line 147-149).

**`load_diff_for_selected` extension** — current helper (lines 52-106):
```rust
fn load_diff_for_selected(&mut self) {
    self.diff_scroll = 0;

    if self.selected_index >= self.files.len() {
        self.diff_content = None;
        return;
    }
    // ... determines old/new content, branches on is_staged_only, handles Binary ...
    self.diff_content = Some(compute_diff_content(path, old_text, new_text));
}
```
Mirror the same early-return + sentinel pattern for the new state:
1. At the very top, reset `self.hunk_line_starts = Vec::new();` and `self.styled_diff = None;` alongside `self.diff_scroll = 0;` — matches the "reset on entry" convention on line 53.
2. At every early-return (lines 55-58, 82-87, 91-94), the new fields are already cleared by the top reset, so no extra cleanup needed.
3. After `self.diff_content = Some(...)` on line 105, compute `self.hunk_line_starts = compute_hunk_line_starts(...);` and optionally `self.styled_diff = build_styled_diff(...);` — matches RESEARCH.md Code Examples lines 744-772.

**Tests module pattern** — current (lines 166-290). Key idioms to copy:

Builders-at-top:
```rust
fn staged_only_entry() -> FileEntry {
    FileEntry {
        path: "staged.rs".to_string(),
        index_status: Some(FileStatus::Modified),
        workdir_status: None,
    }
}

fn test_app_with_files(files: Vec<FileEntry>) -> App {
    let repo = GitRepo::open("/workspace").expect("workspace repo should open");
    App {
        repo,
        files,
        selected_index: 0,
        diff_content: None,
        diff_scroll: 0,
        focus: Focus::Sidebar,
        should_quit: false,
    }
}
```
For Phase 2: extend `test_app_with_files` to initialize the new fields (`hunk_line_starts: Vec::new()`, `styled_diff: None`). Add a builder like `diff_content_with_hunk_sizes(&[lines_per_hunk]) -> DiffContent` to make `NextHunk`/`PrevHunk` tests readable.

Test function shape (lines 202-289):
```rust
#[test]
fn test_update_move_down() {
    let mut app = test_app_with_files(vec![
        staged_only_entry(),
        unstaged_entry(),
        staged_only_entry(),
    ]);
    app.focus = Focus::DiffView; // prevent load_diff_for_selected from running
    assert_eq!(app.selected_index, 0);
    app.update(Message::MoveDown);
    assert_eq!(app.selected_index, 1);
}

#[test]
fn test_update_scroll_diff_up_at_zero() {
    let mut app = test_app_with_files(vec![]);
    assert_eq!(app.diff_scroll, 0);
    app.update(Message::ScrollDiffUp);
    assert_eq!(app.diff_scroll, 0);
}
```
**Copy idioms:**
- One behaviour per test, descriptive name `test_update_<scenario>_<expected>`.
- `app.focus = Focus::DiffView;` sentinel to suppress `load_diff_for_selected` side effects when only testing state transitions (line 232, 241, 250, 259). Use the same trick for `NextHunk`/`PrevHunk` tests.
- `assert_eq!` on specific fields before and after `app.update(...)`.

New tests to add (matching this style): `test_next_hunk_no_op_on_empty`, `test_next_hunk_advances`, `test_prev_hunk_no_op_at_first`, `test_mouse_click_sidebar_selects_file`, `test_mouse_click_sidebar_out_of_bounds_noop`, `test_compute_hunk_line_starts_empty`, `test_compute_hunk_line_starts_three_hunks`, `test_focus_diff`.

---

### `src/ui.rs` (MODIFY) — rendering

**Analog:** itself (lines 1-151).

**`Line`/`Span` construction pattern** for Equal lines with styled spans — extends `diff_lines()` (lines 87-109):
```rust
fn diff_lines(diff: &DiffContent) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    for hunk in &diff.hunks {
        lines.push(hunk_header_line(hunk.old_start, hunk.new_start));
        for dl in &hunk.lines {
            let content = dl.content.trim_end_matches('\n').to_string();
            let (prefix, content_style) = match dl.kind {
                ChangeKind::Equal  => (" ", Style::default()),
                ChangeKind::Insert => ("+", Style::default().fg(Color::Green)),
                ChangeKind::Delete => ("-", Style::default().fg(Color::Red)),
            };

            let lineno_span = Span::styled(
                format!("{} {} ", format_lineno(dl.old_lineno), format_lineno(dl.new_lineno)),
                Style::default().fg(Color::DarkGray),
            );
            let body_span = Span::styled(format!("{}{}", prefix, content), content_style);

            lines.push(Line::from(vec![lineno_span, body_span]));
        }
    }
    lines
}
```

**Copy idioms for the styled-spans extension:**
- **Only** replace the single `body_span` for `ChangeKind::Equal` lines when styled spans are available. For `Insert` / `Delete`, keep the current single-span full-line green/red (per RESEARCH.md Pattern 4 and Pitfall 5).
- When styled: keep emitting the prefix (`" "`) as its own unstyled `Span`, then emit one `Span::styled(span.text.clone(), span.style)` per `StyledSpan` in the `StyledLine`, all collected into the `Line::from(vec![lineno_span, prefix_span, span1, span2, ...])`.
- `trim_end_matches('\n')` stripping (line 92) moves into the highlighter's `emit_spans` helper (RESEARCH.md Pattern 3 — `slice_end -= 1` when the chunk ends at `\n`). Keep the Phase 1 trim as a fallback for the non-highlighted path.
- Every new `Span` must own its text (`.to_string()` / `.clone()`) — `Line<'static>` is the return type (line 87), so no borrowed `&str` from the source string. Matches current `format!(...)` allocations on lines 100, 103.

**Signature decision:** keep `fn diff_lines(diff: &DiffContent)` as-is; add a sibling `fn diff_lines_styled(diff: &DiffContent, styled: &StyledDiffContent) -> Vec<Line<'static>>` or thread an `Option<&StyledDiffContent>` parameter through the existing function. Planner picks. Either way, signature stays non-generic and takes owned-or-borrowed refs only — matches Phase 1's non-generic function style (no trait bounds anywhere in `ui.rs`).

**Render-dispatch pattern** — current `render_diff_view` match (lines 124-150):
```rust
match &app.diff_content {
    None => { /* centered placeholder */ }
    Some(dc) if dc.is_binary => { /* Binary file placeholder */ }
    Some(dc) => {
        let lines = diff_lines(dc);
        let paragraph = Paragraph::new(lines)
            .block(block)
            .scroll((app.diff_scroll, 0));
        frame.render_widget(paragraph, area);
    }
}
```
Extend the third arm with a nested `match app.styled_diff { Some(sd) => diff_lines_styled(dc, sd), None => diff_lines(dc) }` — same flat match style, no early returns added.

**No panel-rect storage.** Per RESEARCH.md Open Question 1 recommendation, the event handler recomputes `Layout::horizontal(...)`. Phase 1's `view(frame, &app)` signature stays `&App`, not `&mut App`. This is the lower-risk choice and keeps Phase 1's pure-view contract intact.

---

### `src/main.rs` (MODIFY) — event loop + terminal lifecycle

**Analog:** itself (lines 1-53).

**Init/restore pattern** — current (lines 9-14):
```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = ratatui::init();
    let result = run(&mut terminal);
    ratatui::restore();
    result
}
```
**Extension (per RESEARCH.md Pattern 6):**
```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = ratatui::init();
    ratatui::crossterm::execute!(std::io::stdout(), ratatui::crossterm::event::EnableMouseCapture)?;

    let result = run(&mut terminal);

    // tolerate errors on shutdown — matches the forgiving shutdown style of the existing restore
    let _ = ratatui::crossterm::execute!(std::io::stdout(), ratatui::crossterm::event::DisableMouseCapture);
    ratatui::restore();
    result
}
```
**Copy idioms:**
- Use `ratatui::crossterm::...` re-exports — matches line 7 (`ratatui::crossterm::event::{...}`) and the Phase 1 convention of never depending on `crossterm` directly (no version drift).
- Tolerate errors on shutdown with `let _ = ...` — consistent with `ratatui::restore()` being infallible on line 12.

**Event dispatch extension** — current (lines 21-46):
```rust
if let Event::Key(key) = event::read()? {
    if key.kind == KeyEventKind::Press {
        let msg = match app.focus {
            Focus::Sidebar => match key.code {
                KeyCode::Char('q') => Some(Message::Quit),
                KeyCode::Char('j') | KeyCode::Down => Some(Message::MoveDown),
                KeyCode::Char('k') | KeyCode::Up => Some(Message::MoveUp),
                KeyCode::Enter => Some(Message::SelectFile),
                KeyCode::Tab => Some(Message::SwitchFocus),
                _ => None,
            },
            Focus::DiffView => match key.code {
                KeyCode::Char('q') => Some(Message::Quit),
                KeyCode::Char('j') | KeyCode::Down => Some(Message::ScrollDiffDown),
                KeyCode::Char('k') | KeyCode::Up => Some(Message::ScrollDiffUp),
                KeyCode::Tab => Some(Message::SwitchFocus),
                KeyCode::Esc => Some(Message::SwitchFocus),
                _ => None,
            },
        };
        if let Some(msg) = msg {
            app.update(msg);
        }
    }
}
```
**Extension — add sibling `if let Event::Mouse(...)` arm**, following the same `let msg = match { ... }; if let Some(msg) = msg { app.update(msg); }` shape. The existing Key branch stays unchanged except for the `n` / `N` additions inside `Focus::DiffView`:

```rust
Focus::DiffView => match key.code {
    KeyCode::Char('q') => Some(Message::Quit),
    KeyCode::Char('j') | KeyCode::Down => Some(Message::ScrollDiffDown),
    KeyCode::Char('k') | KeyCode::Up   => Some(Message::ScrollDiffUp),
    KeyCode::Char('n') => Some(Message::NextHunk),   // NEW
    KeyCode::Char('N') => Some(Message::PrevHunk),   // NEW
    KeyCode::Tab => Some(Message::SwitchFocus),
    KeyCode::Esc => Some(Message::SwitchFocus),
    _ => None,
},
```

**Mouse branch structure** (new): reads current terminal size / recomputed layout per event for hit-testing — see RESEARCH.md Pattern 7. Dispatch at most one `Message` per mouse event. Keep the flat `match mev.kind { ... }` style — same as the key-code match above. Do **not** early-return or `?`-propagate from inside the branch; `_ => None,` on unrecognized kinds mirrors line 39.

**Event-read single-path discipline** — the current `if let Event::Key(key) = event::read()? { ... }` on line 21 becomes a `match event::read()? { Event::Key(key) => { ... }, Event::Mouse(mev) => { ... }, _ => {} }`. Keep one `event::read()?` call per loop iteration — matches the Phase 1 discipline and avoids event drift.

**Keep `KeyEventKind::Press` filter** — line 23 comment (`CRITICAL: Filter for KeyEventKind::Press to avoid duplicate events`) is load-bearing. Do not remove, do not attempt to generalize to "all kinds," do not expand to `MouseEvent` (mouse events do not have a `Press/Release/Repeat` enum — only `kind: MouseEventKind`).

---

### `src/diff/types.rs` (MODIFY — likely no change)

**Analog:** itself (28 lines).

Per RESEARCH.md Impact section line 873: **"No changes. `DiffLine.content: String` stays. Styled overlays live in a parallel structure."** The `StyledDiffContent` (or `Vec<Vec<StyledSpan>>` parallel structure) lives in `src/syntax/`, not here.

**Only** modify this file if the planner decides to cache a per-line span reference directly on `DiffLine`. Current leaning per research: don't. Keep the Phase 1 types pristine.

---

### `Cargo.toml` (MODIFY)

**Analog:** itself (9 lines).

Current:
```toml
[package]
name = "git-diff-tui"
version = "0.1.0"
edition = "2021"

[dependencies]
ratatui = "0.30.0"
git2 = { version = "0.20.4", default-features = false }
similar = "2.7.0"
```

**Copy idioms:**
- One dep per line, alphabetic-ish grouping acceptable.
- Prefer plain `"x.y"` version strings unless a feature flag is needed — matches `ratatui = "0.30.0"` / `similar = "2.7.0"`. Use `{ version = "...", default-features = false }` only when default features are unwanted (matches `git2` line 8).
- No `crossterm` entry — always accessed via `ratatui::crossterm::...` re-export.

Append per RESEARCH.md Installation section (lines 88-103):
```toml
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
tree-sitter-yaml       = "0.7"
tree-sitter-toml-ng    = "0.7"
```
Do NOT add `crossterm` directly. Do NOT pin `tree-sitter-toml` (unqualified — see RESEARCH.md Pitfall 2).

---

## Shared Patterns

### Owned-types-at-module-boundary
**Source:** `src/git/types.rs` (the whole file), `src/diff/types.rs` (the whole file)
**Apply to:** `src/syntax/mod.rs` (public types), `src/syntax/registry.rs` (cached config), any new `StyledDiffContent`
**Rule:** No lifetimes on public types. Strings are owned. External crate types (`git2::Repository`, `HighlightConfiguration`) live behind a private field of a struct defined in this codebase. Seen in `src/git/mod.rs` line 6-8 (`pub struct GitRepo { repo: git2::Repository }`) — replicate exactly for `pub struct HighlightRegistry { by_lang: HashMap<&'static str, HighlightConfiguration> }`.

### `#[derive(Debug, Clone)]` (plus `Copy, PartialEq` where cheap)
**Source:** every public struct/enum in `src/diff/types.rs` (lines 1, 8, 16, 23), `src/git/types.rs` (lines 3, 24, 31), `src/app.rs` (lines 6, 12)
**Apply to:** `StyledSpan`, any new `Message` variants
**Rule:** Minimum `Debug, Clone`. Add `Copy, PartialEq` for small cheaply-copyable enums (matches `Focus`, `Message`, `ChangeKind`, `FileStatus`). Data-bearing structs that hold `String` get only `Debug, Clone` (matches `DiffLine`, `FileEntry`, `DiffContent`).

### Sentinel values over Result for "recoverable absence"
**Source:**
- `src/diff/mod.rs::binary_diff_content` lines 90-96 — returns a `DiffContent` with `is_binary: true` instead of bubbling an error.
- `src/git/types.rs::ContentResult` lines 24-29 — `Text / Binary / NotFound` enum instead of `Result<String, NotFoundError>`.

**Apply to:** `syntax::highlight_source` returns `Option<Vec<StyledLine>>` (None = unknown ext / too large / parse failure); `syntax::registry().get(lang)` returns `Option`; `App::load_diff_for_selected`'s new styled-diff computation sets `self.styled_diff = None` on any failure — never panics, never propagates.

### Early-return + reset-at-top in stateful methods
**Source:** `src/app.rs::load_diff_for_selected` (lines 52-106):
```rust
fn load_diff_for_selected(&mut self) {
    self.diff_scroll = 0;                       // reset at top

    if self.selected_index >= self.files.len() {
        self.diff_content = None;
        return;                                  // early return 1
    }
    // ... Ok(o), Ok(n) match with early return on Err ...
    if matches!(old_result, ContentResult::Binary) || matches!(new_result, ContentResult::Binary) {
        self.diff_content = Some(binary_diff_content(path));
        return;                                  // early return 2
    }
    self.diff_content = Some(compute_diff_content(path, old_text, new_text));
}
```
**Apply to:** the extended `load_diff_for_selected` — reset `hunk_line_starts`, `styled_diff` alongside `diff_scroll` at the top; no cleanup needed at early returns.

### Test module layout
**Source:** `src/app.rs` lines 166-290, `src/diff/mod.rs` lines 98-227, `src/git/mod.rs` lines 138-367
**Apply to:** every new or modified file
**Skeleton:**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    // Optional: use crate::x::y::Type; for cross-module helpers

    // Builders at top
    fn some_fixture() -> SomeType { ... }

    // One behaviour per test
    #[test]
    fn test_<unit>_<scenario>_<expected>() {
        // arrange
        let mut x = some_fixture();
        // act
        let got = x.do_thing();
        // assert
        assert_eq!(got, expected);
    }
}
```
- No `#[cfg(not(...))] mod integration_tests` — Phase 1 has none, stay consistent.
- No `mockall` / test doubles — Phase 1 opens a real `GitRepo::open("/workspace")` for tests that need one (line 190). `syntax::highlight_source` can be tested with tiny literal source strings — no fixtures or mocks needed.
- Test names use `test_` prefix — every Phase 1 test does (58 examples).

### Rendering = owned `Line<'static>` / `Span::styled(String, Style)`
**Source:** `src/ui.rs::diff_lines` (lines 87-109) and `hunk_header_line` (lines 73-78)
**Apply to:** any new rendering (including styled-span rendering)
**Rule:**
- `Line::from(vec![span1, span2, ...])` with explicit span vector — no `Line::raw` short-cuts for anything that carries style.
- `Span::styled(format!("..."), Style::default().fg(Color::X))` — `format!` (not borrowed `&str`) because the return type is `Line<'static>`.
- Never compose styles by `.patch()` / `.merge()` — Phase 1 always constructs the full `Style` inline. Avoids the "+/- color merged with syntax fg" footgun (RESEARCH.md Pitfall 5).

### Focus gating for interactive state changes
**Source:** `src/app.rs` lines 113-115, 121-123:
```rust
if self.focus == Focus::Sidebar {
    self.load_diff_for_selected();
}
```
**Apply to:** Do NOT gate `Message::NextHunk` / `PrevHunk` on focus inside `update()`. Focus gating happens in `main.rs` (the key is only bound when `Focus::DiffView`). `update()` trusts that valid messages were sent — same contract as `Message::ScrollDiffUp` (line 130-134 has no focus check). Keep `update()` pure state-transition, no environment checks.

**Mouse exception:** per RESEARCH.md Anti-Pattern "Using `app.focus` to decide whether to accept mouse events" — mouse events bypass focus. The event handler dispatches regardless of `app.focus`. This is explicitly a different rule from keys.

### `saturating_*` arithmetic on UI counters
**Source:** `src/app.rs` line 136 (`self.total_diff_lines().saturating_sub(1)`), line 138 (`self.diff_scroll.saturating_add(1)`)
**Apply to:** `hunk_line_starts` cumulative computation (RESEARCH.md Pattern 8 uses `cum.saturating_add(1 + h.lines.len() as u16)` — matches), any mouse row arithmetic (`mev.row.saturating_sub(rect.y + 1)` per RESEARCH.md Pattern 7 and Pitfall 6).
**Never:** raw `-` / `+` on `u16` fields in `App`. Phase 1 uses `self.diff_scroll -= 1` only inside an explicit `> 0` guard (line 131-132). Follow that template or use `saturating_sub`.

### Panic-free at the event-loop boundary
**Source:** `src/main.rs` line 21 (`if let Event::Key(key) = event::read()? { ... }`) — `?` propagates IO errors to `Result<(), Box<dyn Error>>`, the app never panics in the loop.
**Apply to:** Mouse branch returns `Option<Message>`; `None` is valid, never `unwrap`. `execute!(...DisableMouseCapture)` wrapped in `let _ = ...` on shutdown. Matches the forgiving shutdown discipline already in place.

### External-dep re-exports via ratatui
**Source:** `src/main.rs` line 7 (`use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};`)
**Apply to:** All crossterm uses — `ratatui::crossterm::event::{DisableMouseCapture, EnableMouseCapture, MouseButton, MouseEvent, MouseEventKind}`, `ratatui::crossterm::execute`. Never add `crossterm` to `Cargo.toml`. Prevents version drift (RESEARCH.md constraint — "crossterm 0.29 via ratatui re-export — do not add crossterm as a direct dep").

## No Analog Found

| File/concern | Reason | Guidance |
|---|---|---|
| tree-sitter-highlight event iteration | Phase 1 has no streaming parser integration | Follow RESEARCH.md Pattern 3 verbatim (line_starts + partition_point + emit_spans). Tests live in `src/syntax/mapping.rs` following the `src/diff/mod.rs` tests style (small literal input -> structured output assertions). |
| `HighlightConfiguration` lifecycle | Phase 1 has no init-once/`OnceLock` cache | Follow RESEARCH.md Code Examples `HighlightRegistry::build()` skeleton. No Phase 1 file to copy. |
| Mouse event hit-testing | No prior mouse code | Follow RESEARCH.md Pattern 7. Keep the `rect_contains` helper 4 lines; do not factor into a trait. |
| `EnableMouseCapture` macro invocation | No prior `execute!` calls | Follow RESEARCH.md Pattern 6 — use `ratatui::crossterm::execute!` (the re-export), not a direct crossterm import. |

## Metadata

**Analog search scope:** `/workspace/src/` (all 7 files), `/workspace/Cargo.toml`, both Phase 1 summaries.
**Files scanned:** 9 source files + 2 summary docs + 1 research doc.
**Key Phase 1 files referenced for pattern extraction:**
- `/workspace/src/main.rs`
- `/workspace/src/app.rs`
- `/workspace/src/ui.rs`
- `/workspace/src/diff/mod.rs`
- `/workspace/src/diff/types.rs`
- `/workspace/src/git/mod.rs`
- `/workspace/src/git/types.rs`
- `/workspace/Cargo.toml`

**Pattern extraction date:** 2026-04-23
