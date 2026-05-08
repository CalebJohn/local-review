# SPEC: Module Extraction Refactor

## Objective

Split oversized files (`app.rs`, `git/mod.rs`, `main.rs`, `ui.rs`) into focused modules. The goal is comfortable code navigation and easier future feature additions. No behavioral changes — this is a pure structural refactor.

**Target file size guideline:** No logic file over ~400 lines (excluding tests). Tests move to co-located `*_tests.rs` files.

## Current State

| File | Total | Logic | Tests | Problem |
|------|-------|-------|-------|---------|
| `app.rs` | 2740 | 1330 | 1410 | 634-line `update()` match, 7+ responsibilities |
| `git/mod.rs` | 1847 | 550 | 1297 | Pure functions mixed with repo operations |
| `main.rs` | 474 | 382 | 92 | 150 lines of keyboard routing inline |
| `ui.rs` | 698 | 434 | 264 | `diff_lines()` too complex, layout math exported |

## Plan

### Phase 1: Extract tests to co-located files

Move `#[cfg(test)] mod tests` blocks to separate files using the Rust convention of `mod tests` in a co-located file.

For a file `foo.rs`, tests move to `foo_tests.rs` and the original file gets:

```rust
#[cfg(test)]
#[path = "foo_tests.rs"]
mod tests;
```

For submodule files like `git/mod.rs`, tests move to `git/tests.rs` and the module file gets:

```rust
#[cfg(test)]
mod tests;
```

This is a mechanical step that immediately reduces visual noise in every file and makes the logic-only line counts accurate for planning the next phases.

**Files affected:** `app.rs`, `git/mod.rs`, `ui.rs`, `diff/mod.rs`, `undo.rs`, `classify/mod.rs`

### Phase 2: Split `app.rs` into sub-dispatchers

Keep in `app.rs`:
- `App` struct, `Message` enum, all supporting enums (`Focus`, `AppMode`, `SidebarSection`, `PendingDiscard`)
- `App::new()`, `partition_files()`, accessors (`current_section_files`, `selected_entry`, `selected_file_path`)
- `App::update()` — but reduced to a thin dispatcher that routes message groups to handler modules
- `load_diff_for_selected()` — orchestration stays here since it touches many fields
- `refresh_file_list()`, `refresh_files()` — file list management

Extract these modules under `src/app/` (converting `app.rs` to `app/mod.rs`):

| New module | Responsibilities | Source lines |
|---|---|---|
| `app/navigation.rs` | `MoveUp`, `MoveDown`, `SelectFile`, `SelectSidebar`, `SwitchFocus`, `ToggleSidebar` message handlers. Scroll/cursor movement: `MoveCursorUp/Down`, `ScrollDiffUp/Down`, `ScrollToTop/Bottom`, `NextHunk`, `PrevHunk`. Helpers: `restore_hunk_position`, `update_hunk_from_cursor`, `move_cursor_to_hunk`, `change_hunk_starts`, `hunk_counts`, `toggle_full_file`, scroll position save/restore. | ~300 lines |
| `app/staging.rs` | `StageFile`, `UnstageFile`, `StageHunk`, `UnstageHunk`, `StageSelectedLines`, `UnstageSelectedLines` message handlers. Discard operations: `DiscardFile`, `DiscardHunk` with two-press confirmation. Undo/redo dispatch: `Undo`, `Redo`. `WorkdirChanged`, `IndexChanged`, `ReloadDiff`. | ~250 lines |
| `app/visual.rs` | `EnterVisual`, `ExitVisual`, `ExtendSelectionUp/Down`, `MouseClickDiffLine`, `MouseDragDiff` message handlers. Helpers: `update_visual_selection`, `local_selected_lines`. | ~120 lines |
| `app/comment.rs` | `CommentContext` struct. `StartComment`, `CommentInputChar`, `CommentInputBackspace`, `CommentInputSubmit`, `CommentInputCancel` message handlers. `format_comment()`. | ~100 lines |
| `app/geometry.rs` | Pure functions for diff coordinate mapping: `total_diff_lines`, `total_content_lines`, `cursor_row`, `row_to_cursor`, `diff_line_at_row`, `row_for_diff_line`, `nearest_row_for_line`. | ~100 lines |

Each handler module exposes functions like:

```rust
pub(crate) fn handle_stage_file(app: &mut App) { ... }
pub(crate) fn handle_move_up(app: &mut App) { ... }
```

And `update()` becomes:

```rust
pub fn update(&mut self, msg: Message) {
    match msg {
        Message::MoveUp => navigation::handle_move_up(self),
        Message::StageFile => staging::handle_stage_file(self),
        // ...
    }
}
```

### Phase 3: Split `git/mod.rs`

Keep in `git/mod.rs`:
- `GitRepo` struct, `open()`, path accessors
- Re-exports from submodules

Extract:

| New module | Responsibilities |
|---|---|
| `git/hunk.rs` | `apply_hunk_to_content()`, `reverse_apply_hunk_to_content()` — pure functions, zero git2 dependency |
| `git/staging.rs` | `stage_file()`, `unstage_file()`, `stage_hunk()`, `unstage_hunk()`, `index_entry_for_path()`, `discard_file()`, `discard_hunk()`, `snapshot_path()`, `restore_snapshot()`, `WorkdirSnapshot`, `Snapshot` |
| `git/status.rs` | `is_binary_content()`, `is_executable()`, `create_symlink()`, `map_index_status()`, `map_workdir_status()`, `changed_files()` |

Content retrieval (`head_content`, `index_content`, `workdir_content`) stays in `mod.rs` — only 70 lines and tightly coupled to `GitRepo`.

### Phase 4: Extract input handling from `main.rs`

| New module | Responsibilities |
|---|---|
| `input.rs` | `translate_key()` (new top-level router), `translate_diff_common_key()`, `translate_visual_key()`, `translate_mouse()`, `translate_diff_mouse()`, `rect_contains()` |

`main.rs` keeps: terminal setup, event loop, file watcher, editor invocation. The event loop calls `input::translate_key()` and `input::translate_mouse()` instead of inline match trees.

### Phase 5: Split `ui.rs` rendering

| New module | Responsibilities |
|---|---|
| `ui/sidebar.rs` | `render_sidebar()`, `render_file_list()`, `sidebar_section_areas()`, `status_style()` |
| `ui/diff_view.rs` | `render_diff_view()`, `diff_lines()`, `hunk_header_line()`, `format_lineno()`, `apply_cursor_selection_style()` |
| `ui/footer.rs` | `render_footer()`, `border_color()` |

`ui/mod.rs` keeps: `view()` entry point that delegates to the three submodules.

## Ordering and Dependencies

Phases are ordered by risk and independence:

1. **Phase 1** (test extraction) is purely mechanical and risk-free. Do it first to reduce noise for all subsequent phases.
2. **Phase 2** (app.rs split) is the highest-value change. Depends on Phase 1 completing for app.rs.
3. **Phase 3** (git split) is independent of Phase 2. Can be done in parallel or after.
4. **Phase 4** (input extraction) is small and independent.
5. **Phase 5** (ui split) is lowest priority — `ui.rs` at 434 logic lines is close to acceptable.

## Conventions

- Handler functions take `&mut App` (not `&mut self`) so they can live outside the impl block. Use `pub(crate)` visibility.
- Pure functions (geometry, hunk application, comment formatting) take only the data they need, not `&App`.
- No new crates or dependencies.
- All existing tests must pass after each phase (`cargo test`).
- No behavioral changes. The refactor is complete when `cargo test` passes and no file exceeds ~400 logic lines.

## Boundaries

**Always:**
- Run `cargo test` after each phase
- Run `cargo clippy` after each phase
- Preserve all existing public API surface (the binary entry point doesn't change)

**Ask first:**
- Before changing any function signatures (should not be necessary)
- Before moving types that are used across many modules (e.g., `Message`, `App`)

**Never:**
- Change behavior or fix bugs during this refactor
- Add new features or abstractions beyond what's needed for the split
- Delete or skip any existing tests
