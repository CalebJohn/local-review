# Module Extraction Refactor — Implementation Plan

## Context

Several source files have grown past comfortable sizes (`app.rs` at 2740 lines, `git/mod.rs` at 1847). This is a pure structural refactor to split them into focused modules. No behavioral changes — every existing test must pass at every step.

The SPEC is at `SPEC.md`. This plan breaks it into 15 tasks across 5 phases, ordered to minimize risk and maximize independent verifiability.

---

## Phase 1: Extract tests to co-located files

### Task 1.1 — Extract tests from 6 smaller files

Move `#[cfg(test)] mod tests` blocks out of `undo.rs`, `diff/mod.rs`, `classify/mod.rs`, `ui.rs`, `git/mod.rs`, and `main.rs`.

| Source | Test starts at | New file | Stub |
|---|---|---|---|
| `undo.rs` | line 101 | `undo_tests.rs` | `#[cfg(test)] #[path = "undo_tests.rs"] mod tests;` |
| `diff/mod.rs` | line 216 | `diff/tests.rs` | `#[cfg(test)] mod tests;` |
| `classify/mod.rs` | line 172 | `classify/tests.rs` | `#[cfg(test)] mod tests;` |
| `ui.rs` | line 435 | `ui_tests.rs` | `#[cfg(test)] #[path = "ui_tests.rs"] mod tests;` |
| `git/mod.rs` | line 550 | `git/tests.rs` | `#[cfg(test)] mod tests;` |
| `main.rs` | line 384 | `main_tests.rs` | `#[cfg(test)] #[path = "main_tests.rs"] mod tests;` |

**Acceptance:** `cargo test` passes, `cargo clippy` clean. Each source file shrinks by its test count.

### Task 1.2 — Extract tests from app.rs

Move lines 1330–2740 to `app_tests.rs`. Consolidate any mid-module `use` statements at the top of the new file.

**Acceptance:** `cargo test` passes. `app.rs` ~1330 lines, `app_tests.rs` ~1410 lines.

**Checkpoint:** Commit "extract tests to co-located files". All tests green.

---

## Phase 2: Split app.rs into sub-dispatchers

### Task 2.1 — Convert app.rs → app/mod.rs

- `mkdir src/app && mv src/app.rs src/app/mod.rs && mv src/app_tests.rs src/app/mod_tests.rs`
- Update test path attribute to `#[path = "mod_tests.rs"]`

**Acceptance:** `cargo test` passes. `mod app;` in main.rs resolves correctly.

### Task 2.2 — Extract app/geometry.rs (~100 lines)

Pure coordinate-mapping functions. No `&self` or `&mut App`.

**Move out of `impl App`** (convert to free functions taking explicit params):
- `total_diff_lines(dc: &DiffContent) -> usize`
- `total_content_lines(dc: &DiffContent) -> usize`
- `cursor_row(dc: &DiffContent, diff_cursor: usize) -> usize`
- `row_to_cursor(dc: &DiffContent, row_offset: usize) -> usize`

**Move free functions directly:**
- `DiffLineKey` type alias
- `diff_line_at_row()`
- `row_for_diff_line()`
- `nearest_row_for_line()`

Keep thin wrapper methods on `impl App` for `total_diff_lines`, `cursor_row`, `row_to_cursor` (callers outside app use `app.row_to_cursor()`). Re-export `DiffLineKey`, `diff_line_at_row`, `row_for_diff_line` from `app/mod.rs`.

**Acceptance:** `cargo test` passes. `grep 'fn.*&self' src/app/geometry.rs` returns nothing.

### Task 2.3 — Extract app/comment.rs (~100 lines)

- `CommentContext` struct
- `format_comment()` free function
- Handler: `pub(crate) fn handle_comment(app: &mut App, msg: Message)` for `StartComment`, `CommentInputChar`, `CommentInputBackspace`, `CommentInputSubmit`, `CommentInputCancel`
- Re-export `CommentContext` from `app/mod.rs` (used by `ui.rs`)

**Acceptance:** `cargo test` passes. `update()` delegates all 5 comment messages.

### Task 2.4 — Extract app/visual.rs (~120 lines)

- `update_visual_selection(app: &mut App)`
- `local_selected_lines(app: &App, dc: &DiffContent, hunk_idx: usize, hunk: &DiffHunk) -> Vec<usize>`
- Handler: `pub(crate) fn handle_visual(app: &mut App, msg: Message)` for `EnterVisual`, `ExitVisual`, `ExtendSelectionUp`, `ExtendSelectionDown`, `MouseClickDiffLine`, `MouseDragDiff`

Keep forwarding method on `App` for `local_selected_lines` (used by staging handlers still in mod.rs at this point).

**Depends on:** 2.2 (visual handlers call `total_content_lines` via App wrapper)

**Acceptance:** `cargo test` passes. No `update_visual_selection` implementation in mod.rs.

### Task 2.5 — Extract app/navigation.rs (~300 lines)

Handler: `pub(crate) fn handle_navigation(app: &mut App, msg: Message)` for: `MoveUp`, `MoveDown`, `SelectFile`, `SelectSidebar`, `SwitchFocus`, `ToggleSidebar`, `MoveCursorUp`, `MoveCursorDown`, `ScrollDiffUp`, `ScrollDiffDown`, `ScrollToTop`, `ScrollToBottom`, `NextHunk`, `PrevHunk`, `MouseClickStagedSidebar`, `MouseClickUnstagedSidebar`, `FocusDiff`, `ToggleFullFile`, `ToggleSemanticFilter`, `ReloadDiff`.

Also move navigation helpers: `change_hunk_starts`, `hunk_counts`, `move_cursor_to_hunk`, `update_hunk_from_cursor`, `toggle_full_file`.

**Keep in mod.rs** (shared infrastructure): `save_scroll_position`, `load_diff_for_selected`, `restore_hunk_position`, `refresh_file_list`, `refresh_files`, `partition_files`.

**Depends on:** 2.2, 2.4

**Acceptance:** `cargo test` passes. `hunk_counts` accessible via forwarding method for ui.rs.

### Task 2.6 — Extract app/staging.rs (~250 lines)

Handler: `pub(crate) fn handle_staging(app: &mut App, msg: Message)` for: `StageFile`, `UnstageFile`, `StageHunk`, `UnstageHunk`, `StageSelectedLines`, `UnstageSelectedLines`, `DiscardFile`, `DiscardHunk`, `Undo`, `Redo`, `WorkdirChanged`, `IndexChanged`.

Calls through `app.*` for shared methods (`save_scroll_position`, `refresh_files`, `restore_hunk_position`) and `visual::local_selected_lines`.

**Depends on:** 2.4, 2.5

**Acceptance:** `cargo test` passes. `update()` is now a thin dispatcher (~30-40 lines). `app/mod.rs` is ~400-500 lines.

**Checkpoint:** Commit "split app.rs into sub-dispatchers". All tests green.

---

## Phase 3: Split git/mod.rs (independent of Phase 2)

### Task 3.1 — Extract git/hunk.rs (~100 lines)

Pure functions: `apply_hunk_to_content()`, `reverse_apply_hunk_to_content()`. Re-export from `git/mod.rs`.

### Task 3.2 — Extract git/status.rs (~70 lines)

Utility functions: `is_binary_content()`, `is_executable()`, `create_symlink()`, `map_index_status()`, `map_workdir_status()`. Re-export with `pub(crate) use`.

### Task 3.3 — Extract git/staging.rs (~280 lines)

Move `WorkdirSnapshot`, `Snapshot`, and staging `impl GitRepo` methods: `stage_file`, `unstage_file`, `stage_hunk`, `unstage_hunk`, `discard_file`, `discard_hunk`, `snapshot_path`, `restore_snapshot`, `workdir_snapshot`, `index_entry_for_path`.

`impl GitRepo` blocks in the sub-module file work because private fields are accessible within the same module tree.

**Depends on:** 3.1, 3.2

**Acceptance:** `git/mod.rs` ~250 lines (struct, `open()`, content accessors, module decls).

**Checkpoint:** Commit "split git/mod.rs into sub-modules". All tests green.

---

## Phase 4: Extract input handling (independent)

### Task 4.1 — Extract input.rs (~90 lines)

Move from main.rs: `translate_diff_common_key()`, `translate_visual_key()`, `translate_mouse()`, `translate_diff_mouse()`, `rect_contains()`. Move `main_tests.rs` → `input_tests.rs`.

**Acceptance:** `main.rs` ~290 lines. `cargo test` passes.

**Checkpoint:** Commit "extract input handling to input.rs".

---

## Phase 5: Split ui.rs (lowest priority)

### Task 5.1 — Convert ui.rs → ui/mod.rs + extract ui/sidebar.rs (~110 lines)

Move: `render_sidebar()`, `render_file_list()`, `sidebar_section_areas()`, `status_style()`. Re-export `sidebar_section_areas` (used by main.rs). Keep `border_color()` in mod.rs (shared).

### Task 5.2 — Extract ui/diff_view.rs (~160 lines)

Move: `render_diff_view()`, `diff_lines()`, `hunk_header_line()`, `format_lineno()`, `apply_cursor_selection_style()`.

### Task 5.3 — Extract ui/footer.rs (~80 lines)

Move: `render_footer()`. `ui/mod.rs` retains only `view()`, `border_color()`, and module decls (~30 lines).

**Checkpoint:** Commit "split ui.rs into sub-modules". All tests green.

---

## Dependency graph

```
1.1 ──┬── 3.1 ── 3.2 ── 3.3
      ├── 4.1
      └── 5.1 ── 5.2 ── 5.3

1.2 ── 2.1 ── 2.2 ──┬── 2.3
                     ├── 2.4 ── 2.5 ── 2.6
                     └────────────────────┘
```

Phases 3, 4, 5 are independent of Phase 2. Recommended serial order:
`1.1 → 1.2 → 2.1 → 2.2 → 2.3 → 2.4 → 2.5 → 2.6 → 3.1 → 3.2 → 3.3 → 4.1 → 5.1 → 5.2 → 5.3`

## Verification (every task)

```bash
cargo test 2>&1 | tail -5         # all tests pass
cargo clippy 2>&1 | grep warning  # no new warnings
```
