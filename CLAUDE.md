# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Git Diff Review TUI -- a terminal tool for reviewing git diffs with syntax highlighting and hunk-level staging. Built in Rust with Ratatui. Single binary, no runtime dependencies.

**Core concept:** Git staging IS the review mechanism. Staging a change means you've reviewed it.

### Constraints

- Rust only, single binary distribution
- Git integration via git2 (libgit2), never shell out to git CLI
- TUI via ratatui (re-exports crossterm)
- Syntax highlighting via tree-sitter 0.25.x (not 0.24, not 0.26)

## Build & Development Commands

```bash
cargo build                  # Build debug
cargo run                    # Run (must be inside a git repo with changes)
cargo test                   # Run all tests
cargo test test_name         # Run a single test
cargo test -- --nocapture    # Run tests with stdout
cargo clippy                 # Lint
cargo watch -x run           # Auto-rebuild on changes (requires cargo-watch)
```

The justfile contains Docker-based development environments (`just code`, `just opencode`) but local `cargo` commands are the primary workflow.

## Architecture

Elm architecture: events are translated to `Message` variants, dispatched through `App::update()`, and the UI re-renders from state.

```
main.rs              Event loop: keyboard/mouse -> Message -> app.update() -> ui::view()
cli.rs               CLI arg parsing for review mode (ReviewArgs, pure, no repo access)
input.rs             Translate raw crossterm events to Message variants

app/mod.rs           App state + update() thin dispatcher
app/navigation.rs    Navigation handlers (move, scroll, focus, hunk jump)
app/staging.rs       Stage/unstage/discard handlers + undo integration
app/visual.rs        Visual-mode selection handlers
app/comment.rs       CommentContext, format_comment, comment handlers
app/geometry.rs      DiffLineKey, coordinate helper functions

ui/mod.rs            Top-level view() layout + diff_lines()
ui/sidebar.rs        Sidebar render (staged/unstaged file lists)
ui/diff_view.rs      Diff view render (hunk headers, change lines, highlighting)
ui/footer.rs         Context-sensitive footer bar + footer_line() tests

git/mod.rs           GitRepo struct, content retrieval (head/index/workdir), content_in_tree helper
git/review.rs        ReviewTarget/ReviewHead, resolve_review (merge-base), review_files (tree diffs), tree_content
git/hunk.rs          apply_hunk_to_content, reverse_apply_hunk_to_content
git/staging.rs       Snapshot types, stage/unstage/discard/snapshot impl methods
git/status.rs        Binary detection, git2::Status -> FileStatus mappers
git/types.rs         FileEntry, FileStatus, ContentResult

diff/mod.rs          Diff computation via `similar` crate. compute_hunks(), compute_diff_content()
diff/types.rs        DiffContent, DiffHunk, DiffLine, ChangeKind

classify/mod.rs      Hunk classification (formatting-only vs semantic changes)
classify/canonical.rs  Token normalization for formatting detection

syntax/mod.rs        build_styled_diff(), lang_for_extension(), StyledDiffContent
syntax/registry.rs   Lazy-init HighlightConfiguration for all 11 languages (OnceLock)
syntax/mapping.rs    Tree-sitter highlight events -> per-line StyledSpan vectors
syntax/scope.rs      Maps highlight scope names to ratatui Style (colors)

undo.rs              UndoManager with snapshot-based undo/redo
```

### Key data flow

1. **Sidebar:** `GitRepo::changed_files()` -> partition into `staged_files` / `unstaged_files` by index vs workdir status
2. **Diff loading:** For staged section: HEAD content vs index content. For unstaged section: index content vs workdir content. Content retrieved as `ContentResult` (Text/Binary/NotFound).
3. **Diff computation:** `similar::TextDiff::from_lines` with `grouped_ops(3)` produces `Vec<DiffHunk>` with 1-based line numbers.
4. **Syntax highlighting:** Tree-sitter parses the full old/new files, producing `HashMap<u32, Vec<StyledSpan>>` keyed by line number. Applied only to Equal (context) lines in the diff view; Insert/Delete lines keep their green/red coloring.
5. **Hunk staging:** Write-stage-restore pattern -- temporarily writes hunk-applied content to workdir, `git add`s it, then restores the original workdir content.
6. **Review mode (`re <base>`, `re A..B`):** `App::new(review: Option<ReviewArgs>)` resolves a `ReviewTarget` (base tree oid + `ReviewHead::Workdir` or head tree oid) via `cli.rs` -> `resolve_review`. `sidebar_section` is `SidebarSection::Review`; `current_section_files()` reads `review_files` (from `review_files()`, untracked sorted last); `load_file_contents` reads `tree_content(base)` vs `workdir_content`/`tree_content(head)`. All 8 staging/discard handlers early-return with "Staging unavailable in review mode"; undo/redo stay no-ops (stack never populates).

### Navigation model

Two focus modes: `Sidebar` and `DiffView`. Sidebar has two sections (Staged/Unstaged) with cross-section navigation (j/k wraps between them). Review mode renders a single `SidebarSection::Review` list and j/k wraps within it. Mouse support for clicking files and scrolling diffs.

## Conventions

- Tests use `#[cfg(test)] mod tests` either inline or as a co-located file (e.g. `git/tests.rs` included via `mod tests;`).
- Content from git is always returned as `ContentResult` enum (Text/Binary/NotFound), with binary detection via null-byte-in-first-8KB heuristic.
- `DiffHunk.old_start` / `new_start` and line numbers are 1-based (matching git convention).
- The `apply_hunk_to_content` function uses `old_start` + `old_lineno` for positioning (not `new_lineno`), which is critical for applying individual hunks from multi-hunk diffs.
- Syntax highlighting has a 256KB size cap (`MAX_HIGHLIGHT_BYTES`). Files larger than this get no highlighting.
- The highlight registry is built once (OnceLock) and shared across all calls.

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| ratatui 0.30 | TUI framework (re-exports crossterm 0.29) |
| git2 0.20 | Git operations (libgit2 bindings) |
| similar 2.7 | Line-level diffing (Patience/Myers) |
| tree-sitter 0.25 + tree-sitter-highlight 0.25 | Syntax highlighting engine |
| tree-sitter-{rust,typescript,javascript,python,go,c,cpp,json,yaml,toml-ng} | Language grammars |
