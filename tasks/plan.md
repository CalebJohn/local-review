# Plan: Cursor Position Preservation on File Switch

## Summary

Replace per-file scroll-position storage with per-file cursor-position storage. The cursor becomes the single source of truth — scroll is always derived from it. Mouse-wheel scrolling is the only way the viewport diverges from the cursor; any cursor action snaps the view back.

## Dependency Graph

```
[1. State field rename] ──┬──> [2. save_cursor_position]
                          ├──> [3. restore_cursor_for_selected]
                          │         │
                          │         ├──> [4. Remove diff_cursor=0 from reset]
                          │         │         │
                          │         └────┬────┘
                          │              v
                          │    [5. Reorder load_diff_for_selected]
                          │
                          └──> [6. Rename callers in navigation.rs + search.rs]
                                         │
                                         v
                               [7. Audit cursor-moving handlers]
                                         │
                                         v
                               [8. Update + add tests]
```

## Phase 1: Core Plumbing (Tasks 1–6)

All six tasks are tightly coupled — they form one atomic change that must compile together. The strategy is to make all renames and logic changes in a single pass, then verify with `cargo build`.

### Task 1: Rename state field

**File:** `src/app/mod.rs`

- `scroll_positions: HashMap<(String, SidebarSection, bool), u16>` → `cursor_positions: HashMap<(String, SidebarSection, bool), usize>`
- Update both `App::new()` (line 178) and `App::test_with_files()` (line 486)

**Acceptance:** Field compiles with new name and type.

### Task 2: Rename `save_scroll_position` → `save_cursor_position`

**File:** `src/app/mod.rs` (line 351)

- Rename method
- Change body: store `self.diff_cursor` into `self.cursor_positions` instead of `self.diff_scroll` into `self.scroll_positions`

**Acceptance:** Method stores cursor, not scroll.

### Task 3: Rewrite `restore_scroll_for_selected` → `restore_cursor_for_selected`

**File:** `src/app/mod.rs` (line 272)

- Rename method
- Restore `self.diff_cursor` from `cursor_positions` (default 0)
- Clamp to `total_content_lines().saturating_sub(1)` — but note: at call time the diff content may not be loaded yet. The clamping must happen after content is available.
- Derive `diff_scroll` to center cursor in viewport:
  ```
  let cursor_row = self.cursor_row();
  let half_vp = (self.diff_viewport_height as usize) / 2;
  let target_scroll = cursor_row.saturating_sub(half_vp);
  let max_scroll = self.total_diff_lines().saturating_sub(1);
  self.diff_scroll = target_scroll.min(max_scroll) as u16;
  ```
- **Critical subtlety:** `cursor_row()` and `total_content_lines()` depend on `self.diff_content` being populated. This method must be called AFTER diff content is loaded (see Task 5).

**Acceptance:** Cursor restored from map, scroll derived from cursor position.

### Task 4: Remove `diff_cursor = 0` from `reset_diff_view_state`

**File:** `src/app/mod.rs` (line 287)

- Delete `self.diff_cursor = 0;` — cursor is now set by `restore_cursor_for_selected`.

**Acceptance:** `reset_diff_view_state` no longer touches `diff_cursor`.

### Task 5: Reorder `load_diff_for_selected`

**File:** `src/app/mod.rs` (line 241)

Current order: `restore_scroll` → `reset_state` → load diff
New order: `reset_state` → load diff → `restore_cursor`

For the cache-hit path, `restore_cursor_for_selected` must also run after cache content is assigned.

New logic:
```
fn load_diff_for_selected(&mut self) {
    self.reset_diff_view_state();

    let Some(entry) = self.selected_entry().cloned() else {
        self.diff_content = None;
        return;
    };

    let cache_key = (entry.path.clone(), self.sidebar_section, self.show_full_file);
    if let Some((dc, styled)) = self.diff_cache.get(&cache_key) {
        self.diff_content = Some(dc.clone());
        self.styled_diff = styled.clone();
        self.restore_cursor_for_selected();
        self.update_hunk_from_cursor();
        return;
    }

    let Some((old, new)) = self.load_file_contents(&entry.path) else {
        self.diff_content = None;
        return;
    };

    self.compute_diff(&entry.path, old, new);

    if let Some(dc) = &self.diff_content {
        self.diff_cache.insert(cache_key, (dc.clone(), self.styled_diff.clone()));
    }

    self.restore_cursor_for_selected();
    self.update_hunk_from_cursor();
}
```

**Acceptance:** Cursor restored after content is available; scroll derived from cursor.

### Task 6: Rename callers

**Files:** `src/app/navigation.rs`, `src/app/search.rs`

All calls to `save_scroll_position()` → `save_cursor_position()`:
- `navigation.rs`: `handle_move_up` (3 sites), `handle_move_down` (3 sites), `handle_mouse_click_staged_sidebar`, `handle_mouse_click_unstaged_sidebar`
- `search.rs`: `navigate_to_sidebar_match`

**Acceptance:** No references to `save_scroll_position` or `scroll_positions` remain.

### Phase 1 Checkpoint

```bash
cargo build   # Must compile
```

---

## Phase 2: Audit Cursor-Moving Handlers (Task 7)

### Task 7: Verify scroll-follows-cursor in all cursor movers

**File:** `src/app/navigation.rs`

Audit each handler that sets `diff_cursor`. Confirm it derives `diff_scroll` from the cursor afterward:

| Handler | Status | Notes |
|---------|--------|-------|
| `handle_move_cursor_up` | OK | Already derives scroll from cursor |
| `handle_move_cursor_down` | OK | Already derives scroll from cursor |
| `handle_scroll_to_top` | OK | Sets both cursor=0 and scroll=0 |
| `handle_scroll_to_bottom` | OK | Sets cursor to max, derives scroll |
| `handle_next_hunk` | **FIX** | Sets `diff_scroll` to absolute value from `change_hunk_starts()`, then calls `move_cursor_to_hunk` which does NOT adjust scroll — need to add scroll derivation after cursor set |
| `handle_prev_hunk` | **FIX** | Same issue as next_hunk |
| `handle_mouse_click_diff_line` | **CHECK** | Sets `diff_cursor` but does not adjust scroll — OK for click since viewport is already showing the line, but should be verified |
| `scroll_to_diff_match` (search) | OK | Derives scroll from cursor_row |
| `handle_enter_visual` | OK | Does not move cursor |

**Fix for hunk jumps:** After `move_cursor_to_hunk`, the scroll should be derived from cursor rather than set from `change_hunk_starts()`. Refactor `handle_next_hunk`/`handle_prev_hunk` to:
1. Find the target hunk index
2. Call `move_cursor_to_hunk(idx)` to set cursor
3. Derive scroll from cursor (center or top-of-hunk)

**Acceptance:** All handlers that move `diff_cursor` also derive `diff_scroll` from it. `handle_scroll_diff_up/down` remain scroll-only (no cursor movement).

### Phase 2 Checkpoint

```bash
cargo build && cargo test
```

---

## Phase 3: Tests (Task 8)

### Task 8: Update and add tests

**File:** `src/app/mod_tests.rs`

**Update existing tests:**
- `test_circular_wrap_saves_scroll_position` — now saves cursor_positions, assert on cursor_positions map
- `test_scroll_position_saved_and_restored_on_navigation` — rewrite to test cursor preservation
- `test_scroll_positions_are_per_mode` — rewrite for cursor_positions
- `test_move_down_resets_scroll_for_new_file` / `test_move_up_resets_scroll_for_new_file` — update expectations (scroll is derived from cursor=0)
- `test_cross_section_resets_scroll_for_new_file` — update for cursor model
- `test_mouse_click_resets_scroll_for_new_file` — update for cursor model
- `test_mouse_click_cross_section_saves_scroll` — rewrite as cursor test

**Add new tests:**

1. **`test_cursor_preserved_across_file_switch`**: Set up App with two files + diff content, navigate cursor to line N in file A, switch to file B, switch back to file A, assert `diff_cursor == N`.

2. **`test_cursor_clamped_on_return`**: Set cursor to line 50 in file A, assign shorter diff content (10 lines), trigger restore, assert `diff_cursor == 9`.

3. **`test_scroll_derived_from_cursor`**: Restore cursor to a mid-file position with known viewport height, assert `diff_scroll` places cursor within viewport.

4. **`test_mouse_wheel_does_not_move_cursor`**: Set cursor to N, scroll with `ScrollDiffUp`/`ScrollDiffDown`, assert `diff_cursor` unchanged.

**Acceptance:** All tests pass. No references to `scroll_positions` in test code.

### Phase 3 Checkpoint

```bash
cargo test && cargo clippy
```

---

## Boundaries / Invariants

- **Always:** Clamp restored cursor to valid range
- **Always:** Derive scroll from cursor, never store scroll independently per file
- **Never:** Move `diff_cursor` on mouse wheel scroll
- **Never:** Store both scroll and cursor per file — cursor is the single source of truth

## Risk Areas

1. **`load_diff_for_selected` ordering** — The cursor restore MUST happen after content is loaded, otherwise `total_content_lines()` returns 0 and clamping breaks. The cache-hit path needs special attention.
2. **Hunk jump scroll** — Currently `handle_next_hunk`/`handle_prev_hunk` set `diff_scroll` from precomputed row positions. After this change, they should derive scroll from cursor. This is a behavior change that affects navigation feel.
3. **Test breakage** — Many existing tests assert on `diff_scroll` values. These assertions need updating since scroll is now derived from cursor + viewport height.
