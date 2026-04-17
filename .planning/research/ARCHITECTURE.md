# Architecture Patterns

**Domain:** Rust TUI Git Diff Review Tool
**Researched:** 2026-04-17

## System Overview

```
+-----------------------------------------------------------+
|                     Terminal (crossterm)                    |
+-----------------------------------------------------------+
|                                                             |
|  +------------------+    +-----------------------------+    |
|  |   Event Loop     |    |      Renderer (ratatui)     |    |
|  |  (crossterm +    |    |                             |    |
|  |   tick timer)    |    |  +----------+ +-----------+ |    |
|  +--------+---------+    |  | Sidebar  | | Diff View | |    |
|           |              |  | Widget   | | Widget    | |    |
|           v              |  +----------+ +-----------+ |    |
|  +------------------+    |  +-----------+ +----------+ |    |
|  |   App State      |    |  | Status    | | Comment  | |    |
|  |   (Model)        |    |  | Bar       | | Overlay  | |    |
|  |                  |<---+  +-----------+ +----------+ |    |
|  | - file_list      |    +-----------------------------+    |
|  | - selected_file  |                                       |
|  | - diff_data      |    +-----------------------------+    |
|  | - view_mode      |    |      Git Layer (git2)       |    |
|  | - staging_state   |    |                             |    |
|  | - comments       |    |  +----------+ +-----------+ |    |
|  +--------+---------+    |  | Diff     | | Staging   | |    |
|           |              |  | Engine   | | Engine    | |    |
|           v              |  +----------+ +-----------+ |    |
|  +------------------+    |  +-----------+              |    |
|  | Action Dispatch  |    |  | Repo     |              |    |
|  | (Message enum)   |--->|  | State    |              |    |
|  +------------------+    |  +-----------+              |    |
|                          +-----------------------------+    |
|                                                             |
|                          +-----------------------------+    |
|                          |   Syntax Layer (tree-sitter) |    |
|                          |                             |    |
|                          |  +----------+ +-----------+ |    |
|                          |  | Highlighter| Semantic  | |    |
|                          |  |          | | Diff Aligner| |   |
|                          |  +----------+ +-----------+ |    |
|                          +-----------------------------+    |
+-------------------------------------------------------------+
```

## Recommended Architecture: Modified Elm Architecture (TEA)

Use the Elm Architecture (Model-Update-View) pattern. This is the recommended pattern from ratatui's official documentation for applications of moderate complexity. The Component Architecture pattern (trait-based, OOP-style) is better for very large applications with many independent panels, but TEA gives cleaner data flow for a focused tool like this.

**Why TEA over Component Architecture:**
- This app has tightly coupled panels (sidebar selection drives diff view content) -- shared state is the norm, not the exception
- TEA makes state transitions explicit via a Message enum, which is critical when staging operations modify git state that multiple views depend on
- Easier to reason about and test: pure functions from (Model, Message) -> Model

**Why not full Component Architecture:**
- Components encapsulate private state, but this app's panels share heavy state (selected file, diff data, staging state)
- Inter-component communication adds boilerplate for a tool with 3-4 panels

### Core Loop

```rust
fn main() -> Result<()> {
    let mut terminal = setup_terminal()?;
    let mut model = Model::new(repo_path)?;

    loop {
        // View: render current state
        terminal.draw(|frame| view(&model, frame))?;

        // Handle input: map raw events to messages
        if let Some(msg) = handle_event(&model)? {
            // Update: process message, mutate model
            let should_quit = update(&mut model, msg)?;
            if should_quit {
                break;
            }
        }
    }

    restore_terminal(terminal)?;
    Ok(())
}
```

## Component Responsibilities

| Component | Responsibility | Communicates With |
|-----------|---------------|-------------------|
| **Event Loop** | Poll crossterm events, map to Messages, drive tick timer for periodic refresh | App State (reads for context-sensitive keybinds), Action Dispatch (emits Messages) |
| **App State (Model)** | Central state: file list, selected file, parsed diff, view mode, comments, staging selections | Read by all renderers, mutated by update() |
| **Action Dispatch (update)** | Process Messages, mutate Model, trigger side effects (git operations) | Git Layer (calls staging/diff ops), Syntax Layer (requests highlighting) |
| **Sidebar Widget** | Render file tree with git status icons, handle selection | Reads file_list and selected_file from Model |
| **Diff View Widget** | Render inline or side-by-side diff with syntax highlighting | Reads diff_data, highlighted_lines, staging selections from Model |
| **Status Bar** | Show current mode, file path, help hints | Reads view_mode, keybind context from Model |
| **Comment Overlay** | Inline comment editor on diff lines, clipboard export | Reads/writes comments in Model |
| **Git Layer** | All git2 operations: repo status, diff computation, staging/unstaging | Accessed by update() only -- never by renderers |
| **Syntax Layer** | tree-sitter parsing, highlight token generation, semantic diff alignment | Called during diff computation, results cached in Model |

## Recommended Project Structure

```
src/
  main.rs              # Terminal setup, main loop, signal handling
  app.rs               # Model struct, Message enum, update() function
  event.rs             # Event polling, key/mouse mapping to Messages
  
  ui/
    mod.rs             # Top-level view() function, layout splits
    sidebar.rs         # File tree rendering widget
    diff_view.rs       # Diff rendering (inline + side-by-side)
    diff_line.rs       # Single diff line rendering with highlighting
    status_bar.rs      # Bottom status bar
    comment.rs         # Comment overlay widget
    theme.rs           # Color scheme, style constants
    
  git/
    mod.rs             # Public API for git operations
    repo.rs            # Repository wrapper, file status enumeration
    diff.rs            # Diff computation (worktree vs HEAD, index vs HEAD)
    staging.rs         # Stage/unstage files, hunks, and lines
    
  syntax/
    mod.rs             # Public API for syntax operations
    highlight.rs       # tree-sitter highlighting, token spans
    language.rs        # Language detection, grammar loading
    semantic_diff.rs   # Syntax-aware hunk alignment (Phase 2+)
    
  model/
    mod.rs             # Core data types
    file_entry.rs      # FileEntry with git status
    diff_data.rs       # Parsed diff: hunks, lines, line types
    selection.rs       # Selection state (file, hunk, line range)
    comment.rs         # Comment data model
```

## Architectural Patterns

### Pattern 1: Message-Driven State Transitions

**What:** All state changes flow through a central Message enum and update() function. No widget directly mutates state.

**When:** Always -- this is the core architectural pattern.

**Example:**
```rust
#[derive(Debug)]
enum Message {
    // Navigation
    SelectFile(usize),
    ScrollDiff(i32),
    
    // View
    ToggleViewMode,          // inline <-> side-by-side
    
    // Staging
    StageFile(PathBuf),
    StageHunk(PathBuf, usize),
    StageLines(PathBuf, usize, Range<usize>),
    UnstageFile(PathBuf),
    UnstageHunk(PathBuf, usize),
    UnstageLines(PathBuf, usize, Range<usize>),
    
    // Comments
    AddComment(PathBuf, usize, String),
    DeleteComment(PathBuf, usize),
    CopyCommentsToClipboard,
    
    // System
    Resize(u16, u16),
    Tick,                    // periodic refresh to detect external git changes
    Quit,
}

fn update(model: &mut Model, msg: Message) -> Result<bool> {
    match msg {
        Message::SelectFile(idx) => {
            model.selected_index = idx;
            // Recompute diff for newly selected file
            let file = &model.file_list[idx];
            model.diff_data = model.git.compute_diff(file)?;
            model.highlighted_diff = model.syntax.highlight_diff(&model.diff_data)?;
            Ok(false)
        }
        Message::StageHunk(path, hunk_idx) => {
            model.git.stage_hunk(&path, hunk_idx)?;
            // Refresh file list and diff after staging
            model.file_list = model.git.list_changed_files()?;
            model.diff_data = model.git.compute_diff_for_current(model)?;
            Ok(false)
        }
        Message::Quit => Ok(true),
        // ...
    }
}
```

### Pattern 2: Git Layer as Opaque Service

**What:** All git2 access is wrapped behind a clean Rust API. No git2 types leak into the Model or UI layers.

**When:** Always. git2 types are complex, have lifetime constraints, and the Repository handle is not Send.

**Example:**
```rust
// git/mod.rs - public interface
pub struct GitRepo {
    repo: git2::Repository,
}

// Return owned data, not git2 references
pub struct FileEntry {
    pub path: PathBuf,
    pub status: FileStatus,
}

pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed { from: PathBuf },
}

pub struct DiffResult {
    pub hunks: Vec<Hunk>,
}

pub struct Hunk {
    pub header: String,
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<DiffLine>,
}

pub struct DiffLine {
    pub kind: LineKind,       // Added, Removed, Context
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub content: String,
}
```

### Pattern 3: Lazy Diff Computation

**What:** Compute diffs only for the currently selected file, not all files at once. Cache results and invalidate on staging operations.

**When:** Always. Diff computation is expensive, especially with syntax highlighting.

**Why:** A repository might have 50 modified files. Computing tree-sitter-highlighted diffs for all of them upfront wastes time and memory. Compute on selection, cache until invalidated.

### Pattern 4: Highlight Overlay, Not Embedded

**What:** Syntax highlighting produces a parallel data structure of styled spans, overlaid onto diff lines during rendering. Diff data and highlight data are separate.

**When:** Always. Separates concerns and allows diff data to exist without highlighting (fallback for unknown languages).

**Example:**
```rust
// Diff line is plain text
pub struct DiffLine {
    pub content: String,
    pub kind: LineKind,
}

// Highlighting is a separate layer
pub struct HighlightedLine {
    pub spans: Vec<StyledSpan>,
}

pub struct StyledSpan {
    pub text: String,
    pub style: Style,  // ratatui::style::Style
}
```

### Pattern 5: Line-Level Staging via Blob Reconstruction

**What:** To stage individual lines (not full hunks), reconstruct the target blob by applying only selected lines, write the new blob to the git object store, then update the index entry to point to the new blob.

**When:** For line-granularity staging/unstaging.

**Why:** git2/libgit2 does not natively support line-level staging. GitUI implements this approach (based on nodegit's pattern). This is the established technique in the Rust/git2 ecosystem.

**Approach:**
1. Get the HEAD version of the file (or index version for unstaging)
2. Parse the diff into hunks and lines
3. Apply only the user-selected lines to produce a new file content
4. Write the new content as a blob: `repo.blob(new_content)`
5. Update the index entry to point to the new blob OID
6. Write the index to disk

## Data Flow

### Primary Flow: File Selection -> Diff Display

```
User selects file in sidebar
    |
    v
Message::SelectFile(idx) emitted
    |
    v
update() calls git.compute_diff(file)
    |
    +--> git2: repo.diff_index_to_workdir() or repo.diff_tree_to_index()
    |         depending on whether file is staged or unstaged
    |
    v
update() calls syntax.highlight_diff(diff_data)
    |
    +--> tree-sitter: parse file content, generate highlight spans
    |
    v
Model.diff_data and Model.highlighted_diff updated
    |
    v
Next render cycle: diff_view widget reads Model and renders
```

### Staging Flow: User Stages a Hunk

```
User presses 's' on a hunk
    |
    v
Message::StageHunk(path, hunk_idx) emitted
    |
    v
update() calls git.stage_hunk(path, hunk_idx)
    |
    +--> git2: Reconstruct partial patch, apply to index
    |
    v
update() refreshes file list and current diff
    |
    +--> git.list_changed_files() -- file might move from
    |    "modified" to "staged" or disappear if fully staged
    |
    v
Model updated with new file list + new diff
    |
    v
Next render cycle reflects changes
```

### Context-Driven Diff Mode

```
Sidebar file selected
    |
    +-- File is unstaged? --> diff worktree vs HEAD
    |                         (repo.diff_index_to_workdir)
    |
    +-- File is staged?   --> diff index vs HEAD
    |                         (repo.diff_tree_to_index)
    |
    +-- File is partially --> Show combined view:
        staged?               unstaged portion as worktree diff
                              (user sees what's not yet staged)
```

### Syntax Highlighting Pipeline

```
File content (String)
    |
    v
Language detection (file extension -> tree-sitter grammar)
    |
    v
tree-sitter parse -> Syntax tree
    |
    v
tree-sitter-highlight -> HighlightEvent stream
    |
    v
Map highlight names to ratatui Styles (via theme)
    |
    v
Vec<HighlightedLine> stored in Model
    |
    v
Diff renderer merges DiffLine + HighlightedLine during render
```

## Anti-Patterns to Avoid

### Anti-Pattern 1: Exposing git2 Types to the UI Layer

**What:** Passing `git2::Diff`, `git2::DiffHunk`, or `git2::Repository` references to rendering code.

**Why bad:** git2 types have complex lifetimes tied to the Repository handle. They are not `Send`. Leaking them into the UI creates lifetime spaghetti and makes testing impossible (can't mock git2 in UI tests).

**Instead:** Convert to owned domain types (DiffResult, Hunk, DiffLine) at the git layer boundary.

### Anti-Pattern 2: Rendering in the Update Function

**What:** Mixing state mutation with terminal drawing calls.

**Why bad:** Breaks TEA's core invariant. Makes state transitions untestable. Causes flickering if rendering is interleaved with state changes.

**Instead:** update() only mutates Model. view() only reads Model and draws.

### Anti-Pattern 3: Computing Diffs for All Files Eagerly

**What:** On startup or after any staging operation, computing and highlighting diffs for every modified file.

**Why bad:** O(files * file_size) work upfront. Tree-sitter parsing is fast but not free. 50 files with 1000 lines each = noticeable startup lag.

**Instead:** Compute on demand when a file is selected. Cache with a dirty flag that invalidates on staging operations.

### Anti-Pattern 4: Blocking the Event Loop on Git Operations

**What:** Running `repo.diff_index_to_workdir()` synchronously in the main loop for large repos.

**Why bad:** Large diffs can take 100ms+. UI freezes during computation. No way to show loading state.

**Instead:** For initial file listing and small diffs, synchronous is fine. For large diffs or staging operations, consider spawning onto a thread with a channel back (GitUI's asyncgit pattern). However, for an initial version targeting personal use, synchronous is acceptable -- optimize to async only if latency becomes noticeable.

### Anti-Pattern 5: Monolithic Render Function

**What:** One giant `view()` function with all layout and widget rendering inline.

**Why bad:** Unreadable past 200 lines. Impossible to work on sidebar and diff view independently.

**Instead:** Split into widget functions: `render_sidebar()`, `render_diff_view()`, `render_status_bar()`, each taking `&Model` and a `Rect` area.

## Suggested Build Order

Dependencies between components dictate the build order. Each layer depends on the one below it.

### Layer 1: Foundation (no dependencies between these)
1. **Git Layer (basic)** -- `git/repo.rs`, `git/diff.rs`: Open repo, list files, compute diffs. This is the data source for everything.
2. **Model types** -- `model/`: FileEntry, DiffResult, Hunk, DiffLine. Pure data, no behavior.
3. **Event handling** -- `event.rs`: crossterm event polling, key mapping.

### Layer 2: Basic UI (depends on Layer 1)
4. **Layout shell** -- `ui/mod.rs`: Split terminal into sidebar + diff pane + status bar regions.
5. **Sidebar** -- `ui/sidebar.rs`: Render file list with status icons, handle selection.
6. **Inline diff view** -- `ui/diff_view.rs`: Render diff hunks without highlighting (plain text first).
7. **Status bar** -- `ui/status_bar.rs`: Current file, mode indicator.

### Layer 3: Core Features (depends on Layer 2)
8. **File-level staging** -- `git/staging.rs`: Stage/unstage entire files. Simplest staging operation.
9. **Hunk-level staging** -- Extend staging.rs with hunk-level operations.
10. **Side-by-side view** -- Extend diff_view.rs with dual-pane rendering.

### Layer 4: Syntax Intelligence (depends on Layer 3)
11. **Syntax highlighting** -- `syntax/highlight.rs`, `syntax/language.rs`: tree-sitter parsing, highlight span generation.
12. **Highlighted diff rendering** -- Integrate highlight spans into diff_view rendering.

### Layer 5: Advanced Features (depends on Layer 4)
13. **Line-level staging** -- Blob reconstruction approach in staging.rs.
14. **Semantic diff alignment** -- `syntax/semantic_diff.rs`: Use tree-sitter AST to align hunk boundaries to function/block boundaries.
15. **Comments system** -- `model/comment.rs`, `ui/comment.rs`: Inline comments, clipboard export.

### Build Order Rationale

- Git layer first because it provides the data everything else displays
- Plain diff rendering before highlighting because it validates the data model
- File staging before hunk staging before line staging (increasing complexity, each builds on prior)
- Syntax highlighting after basic diff works because it's an overlay, not structural
- Semantic diffing last because it's the most complex and least proven (Dijkstra graph algorithm from difftastic is expensive; may need simplification)
- Comments are independent of staging and can be built whenever, but placed last because they're lower priority than core diff/staging

## Scalability Considerations

| Concern | Personal Use (target) | 100+ files changed | 10K+ line files |
|---------|----------------------|--------------------|--------------------|
| File listing | Synchronous, instant | Synchronous, acceptable | N/A |
| Diff computation | Synchronous per file | Cache aggressively, compute on selection | May need streaming/chunked rendering |
| Syntax highlighting | Parse full file | Parse full file | Parse full file (tree-sitter handles this fast) |
| Rendering | Full redraw each frame | Full redraw (ratatui buffer diff handles efficiency) | Virtual scrolling needed for diff view |
| Staging operations | Synchronous | Synchronous (single file at a time) | Synchronous (blob operations are fast) |

## Key Technical Decisions

| Decision | Recommendation | Rationale |
|----------|---------------|-----------|
| Architecture pattern | TEA (Elm Architecture) | Clean data flow, testable, recommended by ratatui docs |
| Async git operations | Synchronous initially | Personal use tool; avoid async complexity until needed |
| Diff computation | On-demand per file, cached | Avoids O(n) upfront cost; invalidate cache on staging |
| Line-level staging | Blob reconstruction | Only proven approach in git2 ecosystem (GitUI's pattern) |
| Syntax highlighting | tree-sitter-highlight crate | Standard Rust crate for this; overlay approach keeps concerns separate |
| Semantic diffing | Study difftastic's approach | Dijkstra-based AST diff is state of the art, but expensive; may simplify to just hunk boundary alignment |
| Terminal backend | crossterm | Cross-platform (Windows+Unix), most popular ratatui backend |

## Sources

- [Ratatui Component Architecture](https://ratatui.rs/concepts/application-patterns/component-architecture/) (HIGH confidence)
- [Ratatui Elm Architecture](https://ratatui.rs/concepts/application-patterns/the-elm-architecture/) (HIGH confidence)
- [Ratatui Event Handling](https://ratatui.rs/concepts/event-handling/) (HIGH confidence)
- [GitUI Repository](https://github.com/gitui-org/gitui) (HIGH confidence - production Rust git TUI)
- [GitUI asyncgit module](https://github.com/gitui-org/gitui/blob/master/asyncgit/README.md) (HIGH confidence)
- [git2-rs Line Staging Issue #589](https://github.com/rust-lang/git2-rs/issues/589) (HIGH confidence - discusses blob reconstruction)
- [Difftastic](https://github.com/Wilfred/difftastic) (HIGH confidence - structural diff approach)
- [tree-sitter-highlight crate](https://crates.io/crates/tree-sitter-highlight) (HIGH confidence)
- [git2 DiffOptions docs](https://docs.rs/git2/latest/git2/struct.DiffOptions.html) (HIGH confidence)
