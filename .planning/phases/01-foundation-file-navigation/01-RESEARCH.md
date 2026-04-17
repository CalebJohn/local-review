# Phase 1: Foundation + File Navigation - Research

**Researched:** 2026-04-17
**Domain:** Rust TUI application architecture (ratatui + git2 + similar)
**Confidence:** HIGH

## Summary

Phase 1 establishes the complete application skeleton: TEA (Elm Architecture) event loop, git repository integration for file status and diff computation, a navigable file sidebar, and an inline diff view with color coding. This is a greenfield Rust project -- no existing code.

The stack is fully locked by CLAUDE.md: ratatui 0.30 for TUI, crossterm 0.29 for terminal backend, git2 0.20 for git operations, and similar 2.7 for diff computation. The TEA architecture pattern is a locked decision from STATE.md. The key architectural insight is that git2 types have complex lifetimes and are not `Send`, so the git layer must return owned types -- also a locked decision.

**Primary recommendation:** Structure as a TEA app with three clean layers: (1) a `git` module returning owned data types, (2) an `App` model struct with a central `Message` enum + `update()`, and (3) a `ui` module with pure `view()` functions. Use `similar::TextDiff` to compute diffs from file content (not git2's built-in diff), giving full control over the diff data model for later phases.

## Project Constraints (from CLAUDE.md)

- **Language:** Rust (mandatory)
- **TUI Framework:** ratatui (mandatory)
- **Distribution:** Single binary, no runtime dependencies
- **Git integration:** Via git2 (libgit2), NOT shelling out to git CLI
- **Tree-sitter version:** 0.25.x series (not relevant for Phase 1 but noted)
- **ratatui version:** 0.30.0 with crossterm 0.29.0 via re-export
- **git2 version:** 0.20.4
- **similar version:** 2.7.0

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| VIEW-01 | File sidebar showing git status indicators (M/A/D/R/U) | git2 `Repository::statuses()` with `StatusOptions`; map `Status` flags to single-char indicators |
| VIEW-02 | Vim-style keybindings for sidebar navigation (j/k, Enter) | crossterm `KeyCode::Char('j'/'k')` + `KeyCode::Enter`; `ListState` for selection tracking |
| VIEW-03 | Inline (unified) diff view with line numbers | `similar::TextDiff::from_lines()` with `iter_all_changes()`; `Change::old_index()`/`new_index()` for line numbers |
| VIEW-05 | Working tree vs HEAD diff for unstaged files | git2 `diff_index_to_workdir()` for file list; read workdir file + index blob via git2 for content |
| VIEW-06 | Index vs HEAD diff when staged file selected | git2 `diff_tree_to_index()` with HEAD tree; read HEAD blob + index blob for content |
| VIEW-07 | Color-coded additions/deletions (green/red) | ratatui `Style::new().fg(Color::Green/Red)` on `Span`s within diff `Line`s |
| INTR-03 | Clean quit with no explicit "done" step | `ratatui::init()` handles alternate screen + panic hook; `ratatui::restore()` on quit; `q` key mapped to `Message::Quit` |
</phase_requirements>

## Standard Stack

### Core (Phase 1 only)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| ratatui | 0.30.0 | TUI framework | Locked by CLAUDE.md. Workspace architecture, mature widget system (List, Paragraph, Block, Scrollbar). [VERIFIED: CLAUDE.md] |
| crossterm | 0.29.0 | Terminal backend | Use via ratatui re-export to avoid version mismatch. Handles raw mode, alternate screen, key events. [VERIFIED: CLAUDE.md] |
| git2 | 0.20.4 | Git operations | Locked by CLAUDE.md. Repository status, diff generation, blob reading. [VERIFIED: CLAUDE.md] |
| similar | 2.7.0 | Diff algorithm | Locked by CLAUDE.md. `TextDiff::from_lines()` for unified diff computation. [VERIFIED: CLAUDE.md] |

### Supporting (Phase 1)
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| unicode-width | 2.x | Text measurement | Line number gutter width, sidebar truncation of long paths. [ASSUMED] |

### Not Needed in Phase 1
| Library | Why Deferred |
|---------|-------------|
| tree-sitter, tree-sitter-highlight | Syntax highlighting is Phase 2 (VIEW-04) |
| arboard | Clipboard is v2 (REVW-02) |
| tui-scrollview | Evaluate need -- ratatui's built-in scroll offset may suffice for Phase 1 |
| tui-widget-list | Evaluate need -- ratatui's built-in `List` + `ListState` handles sidebar selection |
| textwrap | Side-by-side is v2 (ADVV-01) |

**Cargo.toml dependencies for Phase 1:**
```toml
[dependencies]
ratatui = "0.30.0"
git2 = "0.20.4"
similar = "2.7.0"
unicode-width = "0.2"
```

Note: crossterm is accessed via `ratatui::crossterm` re-export -- do not add as a direct dependency.

## Architecture Patterns

### Recommended Project Structure
```
src/
  main.rs          # Entry point: init terminal, run app, restore terminal
  app.rs           # App struct (model), Message enum, update() function
  ui.rs            # view() function: renders App state to Frame
  git/
    mod.rs         # Public API for git operations
    types.rs       # Owned data types (FileEntry, DiffContent, HunkData)
  diff/
    mod.rs         # Diff computation using `similar`
    types.rs       # DiffLine, DiffHunk, ChangeKind enum
```

### Pattern 1: TEA (The Elm Architecture)
**What:** Central Message enum + update() for all state transitions. View is a pure function of model state. [VERIFIED: STATE.md locked decision + ratatui official docs]
**When to use:** Always -- this is the locked architecture decision.

```rust
// Source: https://ratatui.rs/concepts/application-patterns/the-elm-architecture/

// --- Model ---
struct App {
    files: Vec<FileEntry>,       // From git status
    selected_index: usize,       // Sidebar cursor position
    diff_content: Option<DiffContent>, // Currently displayed diff
    diff_scroll: u16,            // Vertical scroll offset in diff view
    focus: Focus,                // Which panel has focus
    should_quit: bool,
}

enum Focus {
    Sidebar,
    DiffView,
}

// --- Messages ---
enum Message {
    MoveUp,
    MoveDown,
    SelectFile,
    ScrollDiffUp,
    ScrollDiffDown,
    Quit,
}

// --- Update ---
fn update(app: &mut App, msg: Message) {
    match msg {
        Message::MoveUp => {
            if app.selected_index > 0 {
                app.selected_index -= 1;
            }
        }
        Message::SelectFile => {
            // Load diff for selected file
            if let Some(entry) = app.files.get(app.selected_index) {
                app.diff_content = Some(compute_diff(entry));
            }
        }
        Message::Quit => app.should_quit = true,
        // ...
    }
}

// --- View (pure rendering) ---
fn view(frame: &mut Frame, app: &App) {
    // Layout, widgets, render -- no side effects
}
```

### Pattern 2: Git Service Layer (Owned Types)
**What:** A `GitRepo` wrapper that converts git2 types into owned structs before returning. [VERIFIED: STATE.md locked decision]
**Why:** git2 types have complex lifetimes tied to `Repository` and are not `Send`. Returning owned types prevents lifetime spaghetti and makes the model clean.

```rust
// Source: Architectural decision from STATE.md + git2 API patterns

// Owned types -- no git2 lifetimes leak out
pub struct FileEntry {
    pub path: String,
    pub status: FileStatus,
}

pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

pub struct GitRepo {
    repo: git2::Repository,
}

impl GitRepo {
    pub fn open(path: &str) -> Result<Self, git2::Error> {
        let repo = git2::Repository::discover(path)?;
        Ok(Self { repo })
    }

    pub fn changed_files(&self) -> Result<Vec<FileEntry>, git2::Error> {
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true);
        let statuses = self.repo.statuses(Some(&mut opts))?;

        let files = statuses.iter()
            .filter(|e| e.status() != git2::Status::CURRENT)
            .map(|entry| {
                let path = entry.path().unwrap_or("").to_string();
                let status = map_status(entry.status());
                FileEntry { path, status }
            })
            .collect();
        Ok(files)
    }

    /// Read file content from workdir (for unstaged diff)
    pub fn workdir_content(&self, path: &str) -> Result<String, ...> { ... }

    /// Read file content from index (staging area)
    pub fn index_content(&self, path: &str) -> Result<String, ...> { ... }

    /// Read file content from HEAD commit tree
    pub fn head_content(&self, path: &str) -> Result<String, ...> { ... }
}
```

### Pattern 3: Diff Data Model (Forward-Compatible)
**What:** Design the diff row model now to support both inline and side-by-side views later. [VERIFIED: STATE.md locked decision]

```rust
// Each line in the diff view
pub enum ChangeKind {
    Equal,
    Insert,
    Delete,
}

pub struct DiffLine {
    pub kind: ChangeKind,
    pub old_lineno: Option<u32>,  // None for insertions
    pub new_lineno: Option<u32>,  // None for deletions
    pub content: String,
}

pub struct DiffHunk {
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<DiffLine>,
}

pub struct DiffContent {
    pub path: String,
    pub hunks: Vec<DiffHunk>,
}
```

### Pattern 4: Terminal Init/Restore with Panic Safety
**What:** Use `ratatui::init()` for automatic alternate screen, raw mode, and panic hook setup. [CITED: https://ratatui.rs/recipes/apps/panic-hooks/]

```rust
// Source: https://ratatui.rs/recipes/apps/panic-hooks/ + docs.rs/ratatui

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // init() enters alternate screen, raw mode, sets up panic hook
    let mut terminal = ratatui::init();

    let result = run(&mut terminal);

    // restore() exits alternate screen, disables raw mode
    ratatui::restore();

    result
}

fn run(terminal: &mut ratatui::DefaultTerminal) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new()?;
    loop {
        terminal.draw(|frame| view(frame, &app))?;

        // Must filter for KeyEventKind::Press to avoid duplicate events
        if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
            if key.kind == crossterm::event::KeyEventKind::Press {
                if let Some(msg) = handle_key(key, &app) {
                    update(&mut app, msg);
                }
            }
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}
```

### Pattern 5: Layout Structure
**What:** Horizontal split -- fixed-width sidebar on the left, flexible diff view on the right. [CITED: https://ratatui.rs/concepts/layout/]

```rust
// Source: https://ratatui.rs/concepts/layout/

use ratatui::layout::{Layout, Constraint, Direction};

fn view(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(30),  // Sidebar: fixed 30 columns
            Constraint::Min(1),     // Diff view: fills remaining space
        ])
        .split(frame.area());

    render_sidebar(frame, app, chunks[0]);
    render_diff_view(frame, app, chunks[1]);
}
```

### Anti-Patterns to Avoid
- **Leaking git2 lifetimes into App model:** git2 types like `StatusEntry`, `DiffDelta` have lifetimes tied to `Repository`. Never store them in App. Always convert to owned types. [VERIFIED: STATE.md decision]
- **Using git2's built-in diff for display:** git2's `Diff::print()` is callback-based and produces text output. Use `similar::TextDiff` on file content pairs for structured diff data you control. Reserve git2 diffs for file-list discovery only.
- **Handling KeyDown/KeyUp as keypresses:** crossterm sends KeyDown, KeyRepeat, and KeyUp events. You MUST check `key.kind == KeyEventKind::Press` to avoid processing each keypress 2-3 times. [CITED: https://ratatui.rs/concepts/event-handling/]
- **Forgetting `ratatui::restore()` on error paths:** If you use `?` in main and it errors before restore, terminal is corrupted. Use `ratatui::init()` which sets up a panic hook, but also ensure normal error paths call `restore()`.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Diff computation | Custom diff algorithm | `similar::TextDiff::from_lines()` | Myers/Patience algorithms are complex; `similar` handles edge cases (empty files, binary, unicode) |
| Terminal management | Raw mode / alternate screen toggling | `ratatui::init()` / `ratatui::restore()` | Panic hooks, signal handling, platform differences are handled |
| Git status parsing | Shell out to `git status --porcelain` | `git2::Repository::statuses()` | Structured data, no PATH dependency, proper error types |
| Key event debouncing | Manual KeyDown/KeyUp tracking | `KeyEventKind::Press` filter | crossterm already distinguishes press/repeat/release |
| Scroll state management | Custom scroll tracking | ratatui's `ListState` (sidebar) + manual offset (diff) | ListState handles wrap-around, selection highlighting |

## Common Pitfalls

### Pitfall 1: git2 Lifetime Complexity
**What goes wrong:** Storing `StatusEntry<'_>` or `DiffDelta<'_>` in your App struct causes borrow checker fights because they borrow from `Repository`.
**Why it happens:** libgit2 returns views into internal data structures; the Rust bindings enforce this with lifetimes.
**How to avoid:** Convert to owned types immediately in the git service layer. Never let git2 types cross module boundaries.
**Warning signs:** Lifetime annotations spreading to App, Message, or UI types.

### Pitfall 2: Duplicate Key Events
**What goes wrong:** Every keypress triggers the action 2-3 times.
**Why it happens:** crossterm emits KeyDown, KeyRepeat, and KeyUp events. Without filtering, all are processed.
**How to avoid:** Always check `key.kind == KeyEventKind::Press` before processing. [CITED: https://ratatui.rs/concepts/event-handling/]
**Warning signs:** Actions happening multiple times per keypress, especially on Windows.

### Pitfall 3: Terminal Corruption on Panic/Error
**What goes wrong:** Application crashes and leaves terminal in raw mode / alternate screen.
**Why it happens:** Cleanup code in normal exit path is skipped on panic.
**How to avoid:** Use `ratatui::init()` which installs a panic hook that calls `ratatui::restore()`. For non-panic errors, structure main so `restore()` is always called. [CITED: https://ratatui.rs/recipes/apps/panic-hooks/]
**Warning signs:** Having to type `reset` after a crash.

### Pitfall 4: Confusing Staged vs Unstaged Diff Sources
**What goes wrong:** Showing the wrong diff -- e.g., showing workdir diff for a staged file.
**Why it happens:** git has three states: HEAD tree, index (staging area), workdir. "Unstaged changes" = index vs workdir. "Staged changes" = HEAD vs index.
**How to avoid:** For VIEW-05 (unstaged): compare index content vs workdir content. For VIEW-06 (staged): compare HEAD content vs index content. The git service layer should expose both content sources clearly.
**Warning signs:** Diff not matching what `git diff` or `git diff --cached` shows.

### Pitfall 5: Empty/New/Deleted File Edge Cases
**What goes wrong:** Crash or empty diff view for newly added files (no HEAD version) or deleted files (no workdir version).
**Why it happens:** `head_content()` returns error for new files; `workdir_content()` returns error for deleted files.
**How to avoid:** Treat missing content as empty string for diff purposes. New file = diff empty string vs full content. Deleted file = diff full content vs empty string.
**Warning signs:** Panics on `unwrap()` when viewing added/deleted files.

### Pitfall 6: Repository Discovery
**What goes wrong:** App fails to open when launched from a subdirectory of a git repo.
**Why it happens:** Using `Repository::open(".")` only works if CWD is the repo root. 
**How to avoid:** Use `Repository::discover(".")` which searches upward for `.git`. [CITED: https://docs.rs/git2/latest/git2/struct.Repository.html]
**Warning signs:** "Not a git repository" error when in a subdirectory.

## Code Examples

### Computing a Diff with `similar`
```rust
// Source: https://docs.rs/similar/latest/similar/struct.TextDiff.html

use similar::{TextDiff, ChangeTag};

fn compute_diff(old: &str, new: &str) -> Vec<DiffLine> {
    let diff = TextDiff::from_lines(old, new);
    let mut lines = Vec::new();

    for change in diff.iter_all_changes() {
        let kind = match change.tag() {
            ChangeTag::Equal => ChangeKind::Equal,
            ChangeTag::Delete => ChangeKind::Delete,
            ChangeTag::Insert => ChangeKind::Insert,
        };
        lines.push(DiffLine {
            kind,
            old_lineno: change.old_index().map(|i| i as u32 + 1),
            new_lineno: change.new_index().map(|i| i as u32 + 1),
            content: change.value().to_string(),
        });
    }
    lines
}
```

### Grouping Changes into Hunks
```rust
// Source: https://docs.rs/similar/latest/similar/struct.TextDiff.html

use similar::{TextDiff, DiffOp};

fn compute_hunks(old: &str, new: &str, context: usize) -> Vec<DiffHunk> {
    let diff = TextDiff::from_lines(old, new);
    let mut hunks = Vec::new();

    for group in diff.grouped_ops(context) {
        let mut hunk_lines = Vec::new();
        let mut old_start = 0;
        let mut new_start = 0;
        let mut first = true;

        for op in &group {
            match op {
                DiffOp::Equal { old_index, new_index, .. } => {
                    if first { old_start = *old_index as u32 + 1; new_start = *new_index as u32 + 1; first = false; }
                    // Add equal lines with both line numbers
                }
                DiffOp::Delete { old_index, .. } => {
                    if first { old_start = *old_index as u32 + 1; first = false; }
                    // Add delete lines with old line numbers only
                }
                DiffOp::Insert { new_index, .. } => {
                    if first { new_start = *new_index as u32 + 1; first = false; }
                    // Add insert lines with new line numbers only
                }
                DiffOp::Replace { old_index, new_index, .. } => {
                    if first { old_start = *old_index as u32 + 1; new_start = *new_index as u32 + 1; first = false; }
                    // Add deletes then inserts
                }
            }
        }
        hunks.push(DiffHunk { old_start, new_start, lines: hunk_lines });
    }
    hunks
}
```

### Reading File Content from git2 (All Three Sources)
```rust
// Source: https://docs.rs/git2/latest/git2/struct.Repository.html

impl GitRepo {
    /// Content from HEAD commit tree
    pub fn head_content(&self, path: &str) -> Result<Option<String>> {
        let head = self.repo.head()?;
        let tree = head.peel_to_tree()?;
        match tree.get_path(std::path::Path::new(path)) {
            Ok(entry) => {
                let blob = self.repo.find_blob(entry.id())?;
                Ok(Some(String::from_utf8_lossy(blob.content()).to_string()))
            }
            Err(_) => Ok(None), // File doesn't exist in HEAD (new file)
        }
    }

    /// Content from the staging area (index)
    pub fn index_content(&self, path: &str) -> Result<Option<String>> {
        let index = self.repo.index()?;
        match index.get_path(std::path::Path::new(path), 0) {
            Some(entry) => {
                let blob = self.repo.find_blob(entry.id)?;
                Ok(Some(String::from_utf8_lossy(blob.content()).to_string()))
            }
            None => Ok(None), // File not in index
        }
    }

    /// Content from working directory
    pub fn workdir_content(&self, path: &str) -> Result<Option<String>> {
        let workdir = self.repo.workdir()
            .ok_or_else(|| anyhow::anyhow!("bare repository"))?;
        let full_path = workdir.join(path);
        match std::fs::read_to_string(&full_path) {
            Ok(content) => Ok(Some(content)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}
```

### Mapping git2 Status Flags
```rust
// Source: https://docs.rs/git2/latest/git2/struct.Status.html + git2-rs/examples/status.rs

fn map_status(status: git2::Status) -> FileStatus {
    if status.contains(git2::Status::INDEX_NEW) || status.contains(git2::Status::WT_NEW) {
        FileStatus::Added
    } else if status.contains(git2::Status::INDEX_DELETED) || status.contains(git2::Status::WT_DELETED) {
        FileStatus::Deleted
    } else if status.contains(git2::Status::INDEX_RENAMED) || status.contains(git2::Status::WT_RENAMED) {
        FileStatus::Renamed
    } else if status.contains(git2::Status::INDEX_MODIFIED) || status.contains(git2::Status::WT_MODIFIED) {
        FileStatus::Modified
    } else {
        FileStatus::Modified // fallback
    }
}
```

### Determining Whether to Show Staged or Unstaged Diff
```rust
// For VIEW-05 and VIEW-06: auto-detect based on file status

impl FileEntry {
    /// Whether this file has staged changes (index differs from HEAD)
    pub fn has_staged_changes(&self) -> bool { /* INDEX_* flags */ }
    /// Whether this file has unstaged changes (workdir differs from index)
    pub fn has_unstaged_changes(&self) -> bool { /* WT_* flags */ }
}

// In the git service, expose richer status info:
pub struct FileEntry {
    pub path: String,
    pub index_status: Option<FileStatus>,  // HEAD->Index changes
    pub workdir_status: Option<FileStatus>, // Index->Workdir changes
}

// The sidebar shows the "dominant" status; selecting the file shows
// the appropriate diff automatically:
// - If file has unstaged changes -> show workdir diff (index vs workdir)
// - If file has only staged changes -> show staged diff (HEAD vs index)
```

### Rendering a Diff Line with Color
```rust
// Source: ratatui widget patterns

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

fn render_diff_view(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(app.diff_content.as_ref().map(|d| d.path.as_str()).unwrap_or(""))
        .borders(Borders::ALL);

    if let Some(diff) = &app.diff_content {
        let lines: Vec<Line> = diff.hunks.iter()
            .flat_map(|hunk| &hunk.lines)
            .skip(app.diff_scroll as usize)
            .map(|dl| {
                let (prefix, style) = match dl.kind {
                    ChangeKind::Insert => ("+", Style::default().fg(Color::Green)),
                    ChangeKind::Delete => ("-", Style::default().fg(Color::Red)),
                    ChangeKind::Equal => (" ", Style::default()),
                };
                let old_num = dl.old_lineno.map(|n| format!("{:4}", n)).unwrap_or("    ".into());
                let new_num = dl.new_lineno.map(|n| format!("{:4}", n)).unwrap_or("    ".into());
                Line::from(vec![
                    Span::styled(format!("{} {} ", old_num, new_num), Style::default().fg(Color::DarkGray)),
                    Span::styled(format!("{}{}", prefix, dl.content.trim_end_matches('\n')), style),
                ])
            })
            .collect();

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
    } else {
        let paragraph = Paragraph::new("Select a file to view diff")
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(paragraph, area);
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| tui-rs | ratatui (maintained fork) | 2023 | tui-rs abandoned; ratatui is the only maintained option |
| `ratatui::Terminal::new()` + manual panic hooks | `ratatui::init()` / `ratatui::restore()` | ratatui 0.28+ | Automatic alternate screen, raw mode, panic hook setup |
| Separate crossterm dependency | `ratatui::crossterm` re-export | ratatui 0.30 | Avoids version mismatch between ratatui and crossterm |
| `Layout::default().direction().constraints()` | `Layout::horizontal([...])` shorthand | ratatui 0.26+ | Cleaner layout syntax |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `unicode-width` 2.x is the correct version for terminal column width | Standard Stack | Low -- easy to adjust version |
| A2 | `similar::Change` has `old_index()` and `new_index()` methods returning `Option<usize>` | Code Examples | Medium -- line number tracking approach depends on this API |
| A3 | `ratatui::init()` is available in 0.30.0 (introduced ~0.28) | Architecture Patterns | Low -- fallback is manual Terminal::new() + panic hook |
| A4 | `Layout::horizontal()` shorthand available in 0.30.0 | State of the Art | Low -- fallback is Layout::default().direction(Direction::Horizontal) |

## Open Questions (RESOLVED)

1. **Scroll behavior for diff view**
   - What we know: ratatui `Paragraph` can be scrolled with `.scroll((y_offset, 0))`
   - What's unclear: Whether `Paragraph::scroll()` is sufficient or if `tui-scrollview` provides better UX (scrollbar widget, etc.)
   - Recommendation: Start with `Paragraph::scroll()` for Phase 1. Add `tui-scrollview` later if scrollbar visualization is needed.

2. **File entry deduplication for files with both staged and unstaged changes**
   - What we know: A file can have both INDEX_MODIFIED and WT_MODIFIED simultaneously
   - What's unclear: Should the sidebar show one entry or two (one staged, one unstaged)?
   - Recommendation: Show one entry per file path. Store both index_status and workdir_status. Default to showing unstaged diff; Phase 3 adds staging toggle.

3. **Binary file handling**
   - What we know: `similar` works on text; binary files produce garbage diffs
   - What's unclear: How to detect binary files reliably
   - Recommendation: Check for null bytes in first 8KB of content (same heuristic as git). Show "Binary file" placeholder instead of diff.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain (rustc + cargo) | Build system | NOT FOUND | -- | Must install via rustup |
| libgit2 (C library) | git2 crate build | Unknown | -- | git2 bundles libgit2 source and builds from source by default (vendored feature) |
| C compiler (cc) | git2 vendored build | Unknown | -- | Required for git2; typically available on Linux |

**Missing dependencies with no fallback:**
- Rust toolchain (rustc, cargo) -- MUST be installed before any work begins. Install via `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`

**Missing dependencies with fallback:**
- libgit2: git2 crate has `vendored` feature (enabled by default) that builds libgit2 from bundled source. Requires a C compiler (gcc/clang).

## Security Domain

Not applicable for Phase 1. This is a local-only TUI tool that reads from a local git repository. No network access, no authentication, no user input beyond keyboard navigation. The only file I/O is reading from the working directory of a git repo the user has already opened.

## Sources

### Primary (HIGH confidence)
- [Ratatui TEA Architecture](https://ratatui.rs/concepts/application-patterns/the-elm-architecture/) - TEA pattern, Model/Message/Update/View structure
- [Ratatui Event Handling](https://ratatui.rs/concepts/event-handling/) - KeyEventKind::Press filtering, centralized event handling
- [Ratatui Panic Hooks](https://ratatui.rs/recipes/apps/panic-hooks/) - Terminal restoration, init()/restore() pattern
- [Ratatui Layout](https://ratatui.rs/concepts/layout/) - Horizontal split, Constraint types
- [git2-rs examples/status.rs](https://github.com/rust-lang/git2-rs/blob/master/examples/status.rs) - StatusOptions, Status flags, iteration
- [git2-rs examples/diff.rs](https://github.com/rust-lang/git2-rs/blob/master/examples/diff.rs) - DiffOptions, diff_index_to_workdir, diff_tree_to_index, Diff::print callback
- [git2 Repository docs](https://docs.rs/git2/latest/git2/struct.Repository.html) - discover(), head(), statuses()
- [similar TextDiff docs](https://docs.rs/similar/latest/similar/struct.TextDiff.html) - from_lines, iter_all_changes, grouped_ops
- [similar Change docs](https://docs.rs/similar/latest/similar/struct.Change.html) - old_index, new_index, tag, value

### Secondary (MEDIUM confidence)
- [Ratatui List widget](https://docs.rs/ratatui/latest/ratatui/widgets/struct.List.html) - List + ListState for sidebar
- [git2 DiffOptions docs](https://docs.rs/git2/latest/git2/struct.DiffOptions.html) - Context lines, include_untracked

### Tertiary (LOW confidence)
- None -- all critical claims verified against official documentation

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All libraries locked by CLAUDE.md, versions specified
- Architecture: HIGH - TEA pattern documented by ratatui, git owned types decided in STATE.md
- Pitfalls: HIGH - Verified against official docs (key events, panic hooks, git2 lifetimes)
- Code examples: MEDIUM - API patterns verified but exact method signatures for similar::Change::old_index need runtime confirmation

**Research date:** 2026-04-17
**Valid until:** 2026-05-17 (stable libraries, 30 days)
