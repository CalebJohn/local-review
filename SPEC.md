# Spec: Cursor Position Preservation on File Switch

## Objective

Replace scroll-position preservation with cursor-position preservation when switching between files. The cursor is now the primary anchor for the diff view — restoring the cursor naturally restores the scroll position around it. Mouse-wheel scrolling is the only way the viewport should diverge from the cursor; any subsequent cursor action snaps the view back.

## Current Behavior

- `scroll_positions: HashMap<(String, SidebarSection, bool), u16>` saves/restores `diff_scroll` per file.
- `diff_cursor` is reset to 0 on every file switch (`reset_diff_view_state`).
- Mouse-wheel scroll (`handle_scroll_diff_up/down`) moves `diff_scroll` without touching `diff_cursor`.
- Keyboard cursor movement (`handle_move_cursor_up/down`) moves `diff_cursor` and adjusts `diff_scroll` to keep the cursor visible.

## Target Behavior

### B1 — Save and restore cursor position per file

Replace `scroll_positions: HashMap<…, u16>` with `cursor_positions: HashMap<(String, SidebarSection, bool), usize>`. On file switch:

1. **Save:** Before loading the new file, store `diff_cursor` into `cursor_positions` under the current file's key.
2. **Load:** After loading the new file's diff content, restore `diff_cursor` from `cursor_positions` (default 0). Clamp to `total_content_lines().saturating_sub(1)` so the cursor can't point past the end if the file changed.
3. **Scroll derivation:** After restoring the cursor, compute `diff_scroll` to center the cursor in the viewport (or as close as possible given bounds). No separate scroll storage.

### B2 — Mouse-wheel scroll diverges from cursor

`handle_scroll_diff_up` and `handle_scroll_diff_down` continue to adjust only `diff_scroll` — they do **not** move `diff_cursor`. This lets the user peek at other parts of the diff without losing their place.

### B3 — Cursor action snaps scroll back

Any action that moves `diff_cursor` (j/k, G/gg, n/N search nav, hunk jump, mouse click on a diff line, entering visual mode) must re-derive `diff_scroll` from the cursor position using the existing margin logic. This is already how `handle_move_cursor_up/down` work — the requirement is that **all** cursor-moving code paths apply the same scroll-follows-cursor adjustment, including `handle_scroll_to_top`, `handle_scroll_to_bottom`, hunk jumps, and search navigation.

No new behavior is needed here if all cursor-moving handlers already call the scroll-adjustment logic. Verify each one; fix any that set `diff_scroll` independently without going through the cursor-relative calculation.

### B4 — Full-file toggle preserves cursor identity

The existing logic in `handle_toggle_full_file` (lines 210-237 of navigation.rs) already saves the logical diff line at the cursor, reloads, and restores. This should continue to work unchanged — it operates on `diff_cursor` and derives scroll from it, which aligns with the new model.

## Changes

### 1. `app/mod.rs` — State fields

- Rename `scroll_positions` to `cursor_positions`, change value type from `u16` to `usize`.
- Update `Default` / `new()` initialization accordingly.

### 2. `app/mod.rs` — `save_scroll_position` → `save_cursor_position`

Rename. Store `self.diff_cursor` instead of `self.diff_scroll`.

### 3. `app/mod.rs` — `restore_scroll_for_selected` → `restore_cursor_for_selected`

Rename. Restore `self.diff_cursor` from `cursor_positions` (default 0), clamp to content bounds, then derive `diff_scroll` to center the cursor:

```
let cursor_row = self.cursor_row();
let half_vp = (self.diff_viewport_height as usize) / 2;
let target_scroll = cursor_row.saturating_sub(half_vp);
let max_scroll = self.total_diff_lines().saturating_sub(1);
self.diff_scroll = target_scroll.min(max_scroll) as u16;
```

### 4. `app/mod.rs` — `reset_diff_view_state`

Remove the line `self.diff_cursor = 0;`. Cursor is now set by `restore_cursor_for_selected` before this is called, or immediately after. Adjust call ordering in `load_diff_for_selected` so that:
1. `reset_diff_view_state` runs first (clears stale styled_diff, mode, visual selection, search).
2. Diff content is loaded and assigned.
3. `restore_cursor_for_selected` runs after content is available (needs `total_content_lines()` for clamping).

### 5. `app/mod.rs` — `load_diff_for_selected`

Reorder to: reset state → load diff → restore cursor. Currently the order is restore scroll → reset state → load diff. The new order ensures cursor clamping has valid content to clamp against.

### 6. `app/navigation.rs` — All callers of `save_scroll_position`

Rename calls to `save_cursor_position`. These are in:
- `handle_move_up` (3 sites)
- `handle_move_down` (3 sites)
- `handle_mouse_click_staged_sidebar`
- `handle_mouse_click_unstaged_sidebar`

### 7. `app/navigation.rs` — Verify scroll-follows-cursor in all cursor movers

Audit each handler that sets `diff_cursor`. Confirm it derives `diff_scroll` from the cursor afterward. Handlers to check:
- `handle_scroll_to_top` — already sets both; OK.
- `handle_scroll_to_bottom` — already sets both; OK.
- `handle_next_hunk` / `handle_prev_hunk` — verify they adjust scroll after setting cursor.
- Search navigation (`handle_next_search_match`, `handle_prev_search_match`) — verify scroll adjustment.
- `handle_mouse_click_diff_line` — verify scroll adjustment.

If any handler sets `diff_scroll` to an absolute value without going through cursor-relative logic, refactor it to derive scroll from cursor.

### 8. No changes needed

- `handle_scroll_diff_up` / `handle_scroll_diff_down` — keep as-is, scroll-only.
- `handle_toggle_full_file` — already cursor-identity-preserving.
- `diff_cache` — unrelated, no changes.

## Testing

- **Unit test: cursor preserved across file switch.** Set up App with two files, navigate cursor to line N in file A, switch to file B, switch back to file A, assert `diff_cursor == N`.
- **Unit test: cursor clamped on return.** Set cursor to line 50 in file A, simulate file A shrinking to 10 lines, switch away and back, assert `diff_cursor == 9`.
- **Unit test: scroll derived from cursor.** Restore cursor to a mid-file position, assert `diff_scroll` places the cursor within the viewport (not at row 0).
- **Manual test: mouse wheel divergence.** Open a long diff, scroll with mouse wheel, press j/k, confirm viewport snaps back to cursor.

## Boundaries

- **Always:** Clamp restored cursor to valid range.
- **Always:** Derive scroll from cursor, never store scroll independently per file.
- **Never:** Move `diff_cursor` on mouse wheel scroll.
- **Never:** Store both scroll and cursor per file — cursor is the single source of truth.
