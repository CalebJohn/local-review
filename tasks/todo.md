# Todo: Cursor Position Preservation

## Phase 1: Core Plumbing
- [ ] Task 1: Rename `scroll_positions` → `cursor_positions` (type u16→usize) in App struct, new(), test_with_files
- [ ] Task 2: Rename `save_scroll_position` → `save_cursor_position`, store diff_cursor
- [ ] Task 3: Rewrite `restore_scroll_for_selected` → `restore_cursor_for_selected` (restore cursor, clamp, derive scroll)
- [ ] Task 4: Remove `self.diff_cursor = 0` from `reset_diff_view_state`
- [ ] Task 5: Reorder `load_diff_for_selected` to: reset → load → restore cursor
- [ ] Task 6: Rename all callers in navigation.rs and search.rs
- [ ] **Checkpoint: `cargo build` succeeds**

## Phase 2: Audit Cursor-Moving Handlers
- [ ] Task 7: Fix `handle_next_hunk`/`handle_prev_hunk` to derive scroll from cursor
- [ ] Task 7b: Verify `handle_mouse_click_diff_line` scroll behavior
- [ ] Task 7c: Verify search navigation scroll behavior
- [ ] **Checkpoint: `cargo build && cargo test` pass**

## Phase 3: Tests
- [ ] Task 8a: Update existing scroll_positions tests for cursor_positions model
- [ ] Task 8b: Add test — cursor preserved across file switch
- [ ] Task 8c: Add test — cursor clamped on return to shorter file
- [ ] Task 8d: Add test — scroll derived from cursor position
- [ ] Task 8e: Add test — mouse wheel does not move cursor
- [ ] **Checkpoint: `cargo test && cargo clippy` clean**
