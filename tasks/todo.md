# Search Feature — Task List

## Phase 1: State & Types
- [x] 1. Add `SearchDirection` enum to `app/mod.rs`
- [x] 2. Add `Focus::SearchInput` variant
- [x] 3. Add search `Message` variants (8 new variants)
- [x] 4. Add search fields to `App` struct (8 new fields)
- [x] 5. Initialize search fields in `App::new()`
- [x] 6. Add dispatch arms in `App::update()`

## Phase 2: Search Module
- [x] 7. Create `app/search.rs` with pure functions (`compute_diff_matches`, `compute_sidebar_matches`, `find_next_match`, `is_case_sensitive`)
- [x] 8. Implement search message handlers on `App` (forward/backward, input char/backspace/submit/cancel, next/prev match)
- [x] 9. Unit tests for pure functions

## Phase 3: Input Mapping
- [x] 10. Add `Focus::SearchInput` handling in `main.rs` event loop
- [x] 11. Add `/` and `?` keybindings in Sidebar
- [x] 12. Add `/` and `?` keybindings in DiffView Normal mode
- [x] 13. Add `n` and `N` keybindings in DiffView Normal mode
- [x] 14. Add `n` and `N` keybindings in Sidebar
- [x] 15. Update `handle_switch_focus` for SearchInput
- [x] 16. Verify inert in Visual mode and CommentInput

## Phase 4: Footer UI
- [x] 17. Add `Focus::SearchInput` arm in `footer_line()`
- [x] 18. Add footer tests for search input rendering

## Phase 5: Diff View Search Highlighting
- [x] 19. Add `search_pattern` parameter to `diff_lines()`
- [x] 20. Implement span splitting for search matches
- [x] 21. Update `render_diff_view()` call site
- [x] 22. Unit tests for highlighted spans

## Phase 6: Sidebar Search Highlighting
- [x] 23. Add search pattern to `FileListProps`
- [x] 24. Split file name spans at match boundaries
- [x] 25. Pass search pattern from `render_sidebar()`

## Phase 7: Lifecycle & Cleanup
- [x] 26. Clear `search_matches` on file switch (in `load_diff_for_selected`)
- [x] 27. Clear `search_sidebar_matches` on staging/unstaging
- [x] 28. Recompute matches lazily on `n`/`N` when empty but pattern exists
- [x] 29. Integration test: file switch clears matches, preserves pattern
- [x] 30. Integration test: `n`/`N` recompute after clear
