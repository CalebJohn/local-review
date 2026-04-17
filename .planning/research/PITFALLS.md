# Domain Pitfalls

**Domain:** Rust TUI Git Diff Review Tool
**Researched:** 2026-04-17

## Critical Pitfalls

Mistakes that cause rewrites or major issues.

### Pitfall 1: Line-Level Staging Requires Custom Blob Manipulation

**What goes wrong:** git2-rs (libgit2 bindings) does not natively support staging individual lines -- only whole files or applying complete diffs/patches. Developers assume `Repository::apply()` handles arbitrary line selection and discover mid-implementation that it only applies complete diff hunks.

**Why it happens:** libgit2's apply API works at the hunk level. Git's own `git add -p` line-editing is implemented in the CLI layer, not in libgit2. The git2-rs issue #589 confirms this gap remains open upstream.

**Consequences:** If you build hunk-level staging first and assume line-level is a simple extension, you'll need to rewrite the staging pipeline. The approaches are fundamentally different.

**Prevention:**
- Implement line-level staging from the start using the blob-replacement approach: take the working tree file, construct a synthetic version containing only the selected lines' changes (keeping other lines as HEAD), write that as a new blob, and update the index entry to point to it.
- Study gitui's implementation -- they solved this exact problem using the "replace selected lines in blob, calculate new diff" pattern from nodegit.
- Build a `StagingEngine` abstraction that handles file/hunk/line uniformly from day one.

**Detection:** If your staging code calls `Repository::apply()` directly with unmodified diffs, you're heading toward this problem.

**Phase:** Must be addressed in the core staging architecture (Phase 1-2). Do not defer line-level staging to a later phase thinking it's incremental.

---

### Pitfall 2: Side-by-Side Diff View Synchronized Scrolling

**What goes wrong:** Side-by-side diff views require two panels that scroll in sync, but added/deleted lines mean the two sides have different line counts. Naive synchronized scrolling (same offset on both sides) produces misaligned diffs where corresponding lines don't appear on the same row.

**Why it happens:** Ratatui has no built-in synchronized scrolling primitive. Each panel is an independent widget with its own scroll state. The RFC for scrollable widgets (ratatui issue #174) has been open since early in the project's life, and scroll behavior remains manual.

**Consequences:** Users see misaligned diffs that are harder to read than inline mode, defeating the purpose of side-by-side view. Fixing this after building both panels independently requires rethinking the data model.

**Prevention:**
- Model the diff as a single unified data structure of "diff rows" where each row contains (left_line | gap, right_line | gap). Gap entries represent insertions/deletions with no counterpart.
- Scroll state operates on diff rows, not individual panel lines. Both panels render from the same row index.
- Handle word-wrapped lines carefully: if one side wraps to more visual lines than the other, pad the shorter side to maintain alignment.
- Build the diff-row model before building the side-by-side renderer.

**Detection:** If your left and right panels have independent scroll offsets, you're heading toward misalignment.

**Phase:** Must be designed into the diff data model (Phase 1 architecture). The side-by-side renderer depends on this model.

---

### Pitfall 3: Tree-Sitter on Diff Fragments Produces Broken Syntax Trees

**What goes wrong:** Tree-sitter expects complete, valid (or at least structurally plausible) source files. Diff hunks are code fragments -- they may start mid-function, lack closing braces, or contain interleaved +/- lines. Feeding raw diff content to tree-sitter produces ERROR nodes and broken highlighting.

**Why it happens:** Tree-sitter's error recovery is designed for incomplete code during editing (missing a closing bracket as you type), not for arbitrary code slices extracted from the middle of a file. Diff context lines don't form a parseable unit.

**Consequences:** Syntax highlighting in diff view is inconsistent, broken, or absent. Users see unhighlighted code or wrong colors on most hunks.

**Prevention:**
- Highlight the full source files (both the old version from HEAD and the new version from the working tree), not the diff hunks.
- Map diff line numbers back to source file line numbers. Extract highlight spans from the full-file parse and apply them to the corresponding diff lines.
- Cache the full-file syntax trees. Tree-sitter's incremental parsing means updating the new-version tree after edits is sub-millisecond.
- For renamed/moved files, parse both the old and new paths.

**Detection:** If your highlighting pipeline takes diff hunk text as input to tree-sitter, this is wrong.

**Phase:** Must be designed correctly from the first syntax highlighting implementation. Retrofitting full-file parsing onto a hunk-based highlighter is a significant rearchitecture.

---

### Pitfall 4: Terminal State Corruption on Panic or Unexpected Exit

**What goes wrong:** If the application panics, receives SIGTERM, or crashes while in alternate screen + raw mode, the user's terminal is left in a broken state: no echo, no line buffering, cursor hidden, stuck on alternate screen. Users must run `reset` or `tput reset` to recover.

**Why it happens:** Raw mode and alternate screen are terminal state changes that must be explicitly reversed. Rust's default panic handler doesn't know about terminal state.

**Consequences:** Users lose trust in the tool after a single crash leaves their terminal broken. They may lose unsaved work in other terminal tabs if they force-quit.

**Prevention:**
- Use `ratatui::init()` (v0.28.1+) which installs a panic hook that restores terminal state automatically.
- Also install a custom panic hook that: (1) restores terminal state, (2) prints the panic info to stdout (not the alternate screen), (3) calls the original panic hook.
- Handle SIGINT/SIGTERM with a signal handler that restores terminal state before exiting.
- Use `scopeguard` or `Drop` implementations as a safety net for terminal restoration.
- Never panic in the terminal restoration code itself -- use `let _ = ...` for all restoration calls.

**Detection:** Try `panic!("test")` early in development. If your terminal breaks, your cleanup is missing.

**Phase:** Must be the very first thing set up when initializing the TUI (Phase 1, first task).

---

### Pitfall 5: Index Lock Contention with External Git Operations

**What goes wrong:** While the user has the diff review tool open and is staging changes, they (or a hook, or their IDE) run `git add`, `git stash`, or another git operation that locks the index. The tool's next staging operation fails with a lock error, or worse, the tool holds a stale index snapshot and writes over the external changes.

**Why it happens:** libgit2 uses file-based locking (`index.lock`). The index is loaded into memory on open; changes from other processes aren't reflected. Writing the index back can silently discard changes made by other tools between the read and write.

**Consequences:** Silent data loss (staged changes from IDE overwritten), confusing lock errors, or corrupted index state.

**Prevention:**
- Re-read the index immediately before each write operation. Don't cache the index across user interactions.
- Use `Index::read(force: true)` to reload from disk before staging operations.
- Handle lock errors gracefully with a user-visible message ("Another git operation is in progress, try again").
- Consider advisory locking: hold the lock briefly during write, not for the lifetime of the application.
- Watch for `index.lock` file existence before attempting writes.

**Detection:** Open the tool, then in another terminal run `git add .`. If the tool doesn't notice or overwrites changes, this is broken.

**Phase:** Must be addressed in the staging engine design (Phase 1-2).

---

## Technical Debt Patterns

### Debt 1: Monolithic Render Function

**What goes wrong:** All rendering logic ends up in a single `fn ui(frame: &mut Frame, app: &App)` function that grows to hundreds of lines. Ratatui's immediate-mode rendering encourages putting everything in the draw callback.

**Prevention:**
- Split rendering into composable widget structs from the start: `FileTreeWidget`, `DiffViewWidget`, `StatusBarWidget`.
- Each widget implements ratatui's `Widget` or `StatefulWidget` trait.
- The top-level render function only does layout calculation and delegates to child widgets.

**Phase:** Architecture decision in Phase 1.

### Debt 2: Event Handling Spaghetti

**What goes wrong:** Key handling starts as a simple `match` on key events, then grows to a nested nightmare as modes multiply (sidebar focused, diff focused, visual selection mode, search mode, comment editing). Adding a new mode requires touching every existing match arm.

**Prevention:**
- Implement a modal input handler from the start. Each mode owns its key bindings.
- Use an enum for application mode with a dispatch layer: `Mode::Sidebar`, `Mode::DiffView`, `Mode::VisualSelect`, `Mode::CommentEdit`.
- Each mode struct handles its own input and returns an `Action` enum that the app processes.

**Phase:** Architecture decision in Phase 1.

### Debt 3: Tight Coupling Between Diff Computation and Display

**What goes wrong:** Diff data structures are designed around rendering needs (storing ANSI colors, terminal coordinates) rather than semantic content. When you need to add side-by-side view alongside inline, or export comments with context, the diff model can't serve both purposes.

**Prevention:**
- Three-layer architecture: (1) Git layer produces raw diffs, (2) Diff model layer normalizes into a semantic representation (files, hunks, lines with change type, source locations), (3) Rendering layer maps semantic model to styled terminal output.
- The diff model should be renderable into both inline and side-by-side views.

**Phase:** Core architecture in Phase 1.

---

## Performance Traps

### Trap 1: Re-Parsing Entire Repository on Every Keystroke

**What goes wrong:** Calling `Repository::statuses()` or re-diffing all files on every input event. `git_status_list` in libgit2 is notably slower than CLI `git status` (documented in libgit2 issue #4230), and doing this on every render frame makes the UI sluggish.

**Prevention:**
- Compute file statuses once on startup and on explicit refresh (e.g., user presses `r`).
- Use filesystem watching (notify crate) to detect changes and refresh only affected files.
- Debounce refreshes -- don't react to every filesystem event individually.
- For individual file diffs, compute lazily when the user selects a file, not eagerly for all files.

**Phase:** Phase 1-2 (initial implementation should be lazy, optimization with file watching in later phase).

### Trap 2: Full Buffer Redraw When Only One Widget Changed

**What goes wrong:** Ratatui uses double-buffering and only sends diffs to the terminal. But if your app state changes cause all widgets to re-render with different content, the terminal diff is large and rendering is slow. This is especially bad with syntax-highlighted diffs that produce many styled spans.

**Prevention:**
- Ratatui's double-buffer diffing handles this automatically for unchanged regions. The real issue is constructing styled content every frame.
- Cache the `Vec<Line>` (styled text) for the diff view. Only reconstruct when the selected file changes or the diff content changes, not on every frame.
- For syntax highlighting, cache highlight results per file. Tree-sitter parse + highlight for a 2000-line file takes ~6ms, which is fine once but not 60 times per second.

**Phase:** Phase 2-3 (initial implementation can be naive, optimize when profiling shows rendering is slow).

### Trap 3: Tree-Sitter Grammar Bloat in Binary

**What goes wrong:** Each tree-sitter grammar is a compiled C file that adds 200KB-1MB+ to the binary. Supporting 20+ languages can add 10-20MB to binary size. The C compilation also significantly increases build times (each grammar requires cc compilation).

**Prevention:**
- Start with a curated set of grammars: Rust, TypeScript/JavaScript, Python, Go, C/C++, JSON, YAML, Markdown, TOML. These cover the vast majority of use cases.
- Consider loading grammars as WASM modules at runtime for extensibility (tree-sitter supports this), but this adds complexity. For a personal tool, compiled-in grammars are fine.
- Feature-gate grammars behind Cargo features so users can build with only what they need.
- Each grammar's C code compiles independently -- parallel builds help with build time.

**Phase:** Phase 2 (when adding syntax highlighting). Start small, expand later.

### Trap 4: Allocating Styled Text for Offscreen Lines

**What goes wrong:** For a 10,000-line diff, constructing `ratatui::text::Line` objects with `Span` styling for all 10,000 lines when only 40 are visible. Each Line/Span allocation is small, but 10K of them per frame adds up.

**Prevention:**
- Only construct styled Line objects for the visible window plus a small buffer (viewport + 10 lines above/below).
- Store the diff model as a compact representation (line text + highlight ranges), not as pre-styled ratatui types.
- The ratatui-code-editor crate demonstrates this pattern: "Highlighting results are cached per visible region."

**Phase:** Phase 2-3 (optimize when profiling shows allocation pressure).

---

## UX Pitfalls

### UX 1: Vim Keybindings That Conflict with Git Semantics

**What goes wrong:** `s` is a natural key for "stage" in a git context, but it's also a vim command (substitute). `d` means "delete" in vim but could mean "diff" or "discard" in git context. Users with vim muscle memory will accidentally trigger destructive operations.

**Prevention:**
- Map staging to `<Space>` or `<Enter>` (non-conflicting, intuitive "toggle/confirm").
- Use `s` for stage only in sidebar mode where vim's substitute doesn't apply.
- For destructive operations (unstage, discard changes), require a confirmation or use capital letters / key chords.
- Document the keybinding philosophy clearly and provide a help overlay (`?`).

**Phase:** Phase 1 (keybinding design). Get this right early -- changing keybindings after users develop muscle memory is painful.

### UX 2: No Visual Feedback During Staging Operations

**What goes wrong:** User stages a hunk and nothing visibly changes. The staging succeeded but the diff view still shows the same content. The user stages it again (no-op) or loses confidence that it worked.

**Prevention:**
- After staging, immediately refresh the diff for the current file. Staged hunks should disappear from the unstaged diff (or the file should move to "staged" status in the sidebar).
- Provide a transient status message: "Staged 3 lines in src/main.rs".
- If the sidebar shows staged/unstaged status, update it immediately.
- Consider a brief highlight flash on successfully staged lines before they disappear.

**Phase:** Phase 2 (staging UI). The feedback loop is critical to the core value proposition.

### UX 3: Side-by-Side View Unusable in Narrow Terminals

**What goes wrong:** Side-by-side view splits terminal width in half. In an 80-column terminal, each side gets ~38 columns (minus borders/gutters). Code with 4-level indentation is immediately truncated, making the diff unreadable.

**Prevention:**
- Auto-detect terminal width and disable side-by-side below a threshold (e.g., < 120 columns). Show inline by default in narrow terminals.
- In side-by-side mode, support horizontal scrolling for long lines rather than wrapping.
- Show a "terminal too narrow for side-by-side view" message rather than rendering garbage.
- Allow the user to adjust the split ratio (60/40, 70/30) for asymmetric diffs.

**Phase:** Phase 2-3 (when implementing side-by-side view).

### UX 4: Mouse Support Feels Broken Across Terminal Emulators

**What goes wrong:** Mouse click events work in one terminal (e.g., iTerm2) but not another (e.g., tmux, older xterm). Scroll events behave differently. Right-click context menus conflict with terminal's own right-click behavior.

**Prevention:**
- Make the tool fully keyboard-navigable first. Mouse is enhancement, not primary input.
- Test mouse in: iTerm2, Alacritty, WezTerm, Kitty, Terminal.app, Windows Terminal, and tmux.
- Use crossterm's mouse capture which handles terminal differences, but expect edge cases.
- Detect when running inside tmux (check `$TMUX` env var) and adjust mouse mode accordingly.
- Avoid right-click context menus entirely -- they conflict with terminal emulator behavior.

**Phase:** Phase 2-3 (after keyboard navigation is solid).

---

## "Looks Done But Isn't" Checklist

These features appear complete in demos but break in real-world usage:

| Feature | Looks Done When... | Actually Done When... |
|---------|-------------------|----------------------|
| File sidebar | Shows modified files | Handles renames, copies, untracked files, submodules, .gitignore changes, files in nested directories |
| Inline diff | Shows +/- lines with colors | Handles binary files (shows "binary file differs"), empty files, files with no newline at EOF, very long lines, files with mixed encodings |
| Syntax highlighting | Works on a .rs file | Handles files with no grammar (falls back to plain text), files tree-sitter can't parse (doesn't crash), shebangs that indicate language, files with mixed content (e.g., HTML with embedded JS/CSS) |
| Hunk staging | Stages one hunk | Handles the first hunk, last hunk, only hunk, adjacent hunks, hunks that change after staging a prior hunk (line offsets shift), staging in non-sequential order |
| Line staging | Stages selected lines | Handles lines at hunk boundaries, deletion-only selections, addition-only selections, mixed selections, context lines adjacent to selections, selections that span multiple hunks |
| Side-by-side view | Two panels with colored lines | Handles files with only additions (no left side), files with only deletions (no right side), files longer than terminal height, lines longer than panel width, synchronized scrolling with unequal-length sides |
| Vim navigation | j/k moves cursor | Handles counts (5j), gg/G for top/bottom, Ctrl-d/Ctrl-u for half-page scroll, search with /, n/N for next/prev match, visual mode for line selection |
| Comments | Can type a comment | Comments survive file re-selection, comments are associated with correct lines after staging changes line numbers, comments on deleted lines are preserved, copy-to-clipboard formats context correctly |

---

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| Git integration setup | libgit2 version mismatch with system git config | Pin git2 crate version, test with both `vendored` and system libgit2 features. Default to `vendored` for single-binary distribution. |
| Diff computation | Diff algorithm doesn't match `git diff` output | Use git2's diff functions (which use libgit2's Myers by default) rather than implementing your own diff algorithm. Match git's default context of 3 lines. |
| Tree-sitter setup | Grammar version conflicts between tree-sitter-* crates | Pin all tree-sitter-* crates to versions that use the same tree-sitter ABI version. Check tree-sitter issue #3095 for known conflicts. |
| Staging engine | Hunk line offsets after partial staging | After staging hunk N, the line numbers for subsequent hunks change. Re-diff the file after each staging operation rather than trying to adjust offsets manually. |
| Visual line selection | Off-by-one errors in selection ranges | Use inclusive ranges consistently. Write edge-case tests: select first line only, select last line only, select all lines, select across hunk boundaries. |
| Comment system | Comments anchored to line numbers become invalid after staging | Anchor comments to content (line content hash + surrounding context) rather than absolute line numbers. When the diff changes, fuzzy-match comments to their new positions. |
| Clipboard export | Clipboard access varies across OS/terminal | Use the `arboard` crate for cross-platform clipboard. Also support writing to stdout (pipe-friendly) and OSC 52 escape sequence for remote/tmux sessions. |

## Sources

- [ratatui rendering discussion #579](https://github.com/ratatui/ratatui/discussions/579)
- [ratatui scrollable widgets RFC #174](https://github.com/ratatui/ratatui/issues/174)
- [ratatui panic hooks recipe](https://ratatui.rs/recipes/apps/panic-hooks/)
- [ratatui stderr performance issue #1348](https://github.com/ratatui/ratatui/issues/1348)
- [git2-rs line staging issue #589](https://github.com/rust-lang/git2-rs/issues/589)
- [libgit2 index locking #809](https://github.com/libgit2/libgit2/issues/809)
- [libgit2 threading docs](https://github.com/libgit2/libgit2/blob/main/docs/threading.md)
- [libgit2 status performance #4230](https://github.com/libgit2/libgit2/issues/4230)
- [tree-sitter syntax highlighting docs](https://tree-sitter.github.io/tree-sitter/3-syntax-highlighting.html)
- [tree-sitter versioning conflicts #3095](https://github.com/tree-sitter/tree-sitter/issues/3095)
- [GitHub Desktop git concurrency](https://github.blog/2015-10-20-git-concurrency-in-github-desktop/)
- [gitui repository](https://github.com/gitui-org/gitui)
- [crossterm unicode issues #561](https://github.com/crossterm-rs/crossterm/issues/561)
