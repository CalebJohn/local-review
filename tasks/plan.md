# Plan: Vim-Style Search (`/`, `?`, `n`, `N`)

## Summary

Add vim-style search to the TUI. `/`/`?` opens a search prompt in the footer;
`n`/`N` cycle through matches. Works in both Sidebar (file path search) and
DiffView (line content search). Smart case, highlight matches in diff view
and sidebar.

## Dependency Graph

```
Phase 1: State & Types
    |
Phase 2: Search Module (handlers + pure functions + tests)
    |
Phase 3: Input Mapping (wire keys in main.rs / input.rs)
    |          \
Phase 4: Footer UI    Phase 5: Diff View Highlighting
    |                           |
Phase 6: Sidebar Highlighting   |
    \                          /
     Phase 7: Lifecycle (clear/recompute on file switch, reload, staging)
```

Phases 4/5/6 are independent of each other (only depend on Phase 3).
Phase 7 touches handlers from Phase 2 and rendering from 4/5/6, so it goes last.

## Phases & Tasks

### Phase 1: State & Types

**Files:** `app/mod.rs`

1. **Add `SearchDirection` enum** — `Forward | Backward`, derives matching `Focus`.
2. **Add `Focus::SearchInput` variant** — extends existing `Focus` enum.
3. **Add search Message variants** — `SearchForward`, `SearchBackward`,
   `SearchInputChar(char)`, `SearchInputBackspace`, `SearchInputSubmit`,
   `SearchInputCancel`, `NextMatch`, `PrevMatch`.
4. **Add search fields to `App` struct** — `search_query`, `search_direction`,
   `search_origin`, `search_pattern`, `search_case_sensitive`, `search_matches`,
   `search_sidebar_matches`, `search_match_cursor`.
5. **Initialize new fields in `App::new()`** — all to empty/default values.
6. **Add dispatch arms in `App::update()`** — route to `self.handle_*` methods
   (bodies will be in Phase 2).

**Acceptance:** `cargo check` passes. No behavioral changes yet.

---

### Phase 2: Search Module

**Files:** `app/search.rs` (new), `app/mod.rs` (add `mod search;`)

7. **Create `app/search.rs`** with pure functions:
   - `compute_diff_matches(diff_content, pattern, case_sensitive) -> Vec<usize>` —
     returns sorted content-line indices where the line content contains the pattern.
   - `compute_sidebar_matches(staged, unstaged, pattern, case_sensitive) -> Vec<(SidebarSection, usize)>` —
     returns matches in sidebar display order (staged first, then unstaged).
   - `find_next_match(matches_len, current_index, direction) -> Option<usize>` —
     wrapping navigation through a match list.
   - `is_case_sensitive(pattern) -> bool` — smart-case: any uppercase char -> sensitive.
8. **Implement search message handlers on App:**
   - `handle_search_forward` / `handle_search_backward` — set direction, origin, clear
     query buffer, switch focus to SearchInput.
   - `handle_search_input_char` / `handle_search_input_backspace` — edit query buffer.
   - `handle_search_input_submit` — commit pattern, compute matches for origin pane,
     jump to first match, return focus.
   - `handle_search_input_cancel` — clear query, return focus to origin, preserve
     existing committed pattern.
   - `handle_next_match` / `handle_prev_match` — navigate matches in the *current*
     focus pane (not origin). Recompute matches if empty but pattern exists.
     Set status_message on wrap.
9. **Unit tests for pure functions:**
   - `compute_diff_matches`: basic match, case-insensitive, case-sensitive, no matches,
     multiple matches per line (single entry), across multiple hunks.
   - `compute_sidebar_matches`: matches file paths, smart case, staged-then-unstaged order.
   - `find_next_match`: forward/backward from middle, wrap both directions, empty list,
     single match.
   - `is_case_sensitive`: all-lower -> false, mixed case -> true, all-upper -> true.

**Acceptance:** `cargo test` passes all new tests. `cargo check` passes.

**Checkpoint: commit Phase 1 + Phase 2.**

---

### Phase 3: Input Mapping

**Files:** `main.rs`, `input.rs`

10. **Add `Focus::SearchInput` handling in `main.rs` event loop** — same pattern as
    `CommentInput` (char/backspace/enter/esc), mapping to `SearchInputChar`,
    `SearchInputBackspace`, `SearchInputSubmit`, `SearchInputCancel`.
11. **Add `/` and `?` keybindings in Sidebar** — map to `SearchForward` / `SearchBackward`.
12. **Add `/` and `?` keybindings in DiffView Normal mode** — same messages.
13. **Add `n` and `N` keybindings in DiffView Normal mode** — map to `NextMatch` / `PrevMatch`.
14. **Add `n` and `N` keybindings in Sidebar** — same messages.
15. **Update `handle_switch_focus`** to handle `SearchInput` -> `Sidebar` (same as CommentInput).
16. **Verify `/`, `?`, `n`, `N` are NOT mapped in Visual mode or CommentInput** — they should
    be inert (CommentInput is already handled before the keybinding dispatch; Visual mode
    uses `translate_visual_key` which doesn't include these).

**Acceptance:** `cargo check` passes. Can type `/foo` in the TUI, press Enter to search,
`n`/`N` to navigate. Manual smoke test.

---

### Phase 4: Footer UI

**Files:** `ui/footer.rs`

17. **Add `Focus::SearchInput` arm in `footer_line()`** — render
    `/ {search_query}█` or `? {search_query}█` depending on `search_direction`.
    No other keybinding hints. Same pattern as CommentInput.
18. **Update footer tests** — add test for search input rendering.

**Acceptance:** `cargo test` passes. Footer displays search prompt correctly.

---

### Phase 5: Diff View Search Highlighting

**Files:** `ui/diff_view.rs`

19. **Add `search_pattern: Option<(&str, bool)>` parameter to `diff_lines()`** —
    `(pattern, case_sensitive)`.
20. **Implement span splitting for search matches** — for each content line, split
    the text at match boundaries and apply
    `Style::new().bg(Color::Yellow).fg(Color::Black)` to matched substrings.
    This interleaves with existing syntax highlighting and change-kind coloring.
21. **Update `render_diff_view()` call to `diff_lines()`** — pass the search pattern
    from `app.search_pattern`.
22. **Unit tests** — test that `diff_lines` with a search pattern produces the expected
    highlighted spans.

**Acceptance:** `cargo test` passes. Matching text in diff view is highlighted yellow.

---

### Phase 6: Sidebar Search Highlighting

**Files:** `ui/sidebar.rs`

23. **Add search pattern parameter to `FileListProps`** — `search_pattern: Option<(&str, bool)>`.
24. **Split file name spans at match boundaries** — apply the same yellow highlight style
    to matched substrings in file paths.
25. **Pass search pattern from `render_sidebar()`** — thread through from App state.

**Acceptance:** Matching file name substrings are highlighted in the sidebar.

**Checkpoint: commit Phases 3-6.**

---

### Phase 7: Lifecycle & Cleanup

**Files:** `app/mod.rs`, `app/search.rs`, `app/navigation.rs`, `app/staging.rs`

26. **Clear `search_matches` on file switch** — in `load_diff_for_selected()`, clear
    `search_matches` and `search_match_cursor`. Preserve `search_pattern`.
27. **Clear `search_matches` on `ReloadDiff`** — same clearing in `load_diff_for_selected`
    handles this since `ReloadDiff` calls it.
28. **Clear `search_sidebar_matches` on staging/unstaging** — after `refresh_file_list()`.
29. **Recompute matches lazily on `n`/`N`** — if `search_matches` is empty but pattern
    exists, recompute before navigating. Same for sidebar matches.
30. **Integration-style test: file switch clears diff matches but preserves pattern.**
31. **Integration-style test: `n`/`N` recompute after clear.**

**Acceptance:** All tests pass. No stale highlights after file switch or staging.

**Checkpoint: commit Phase 7.**

## Risks & Mitigations

- **`n` key conflict:** Currently `n` is mentioned in `NO_ACTIVE_HUNK_MSG` ("press n to
  navigate to a hunk") but is NOT actually mapped. Safe to claim.
- **Span splitting complexity:** Need to handle UTF-8 correctly for case-insensitive
  matching. Use `str::to_lowercase()` for comparison, byte-offset-based splitting.
- **Performance:** Match computation is O(lines * pattern_len) — fine for typical diffs.
  Sidebar match is O(files * pattern_len) — negligible.

## Out of Scope

- Regex support
- Cross-file search
- Filtering (hiding non-matching lines)
- Persistent search history
- Live/incremental filtering as you type
