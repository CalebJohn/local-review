# Search Feature — Task List

## Phase 1: State & Types
- [ ] 1. Add `SearchDirection` enum to `app/mod.rs`
- [ ] 2. Add `Focus::SearchInput` variant
- [ ] 3. Add search `Message` variants (8 new variants)
- [ ] 4. Add search fields to `App` struct (8 new fields)
- [ ] 5. Initialize search fields in `App::new()`
- [ ] 6. Add dispatch arms in `App::update()`

## Phase 2: Search Module
- [ ] 7. Create `app/search.rs` with pure functions (`compute_diff_matches`, `compute_sidebar_matches`, `find_next_match`, `is_case_sensitive`)
- [ ] 8. Implement search message handlers on `App` (forward/backward, input char/backspace/submit/cancel, next/prev match)
- [ ] 9. Unit tests for pure functions

**Checkpoint: commit Phases 1+2**

## Phase 3: Input Mapping
- [ ] 10. Add `Focus::SearchInput` handling in `main.rs` event loop
- [ ] 11. Add `/` and `?` keybindings in Sidebar
- [ ] 12. Add `/` and `?` keybindings in DiffView Normal mode
- [ ] 13. Add `n` and `N` keybindings in DiffView Normal mode
- [ ] 14. Add `n` and `N` keybindings in Sidebar
- [ ] 15. Update `handle_switch_focus` for SearchInput
- [ ] 16. Verify inert in Visual mode and CommentInput

## Phase 4: Footer UI
- [ ] 17. Add `Focus::SearchInput` arm in `footer_line()`
- [ ] 18. Add footer tests for search input rendering

## Phase 5: Diff View Search Highlighting
- [ ] 19. Add `search_pattern` parameter to `diff_lines()`
- [ ] 20. Implement span splitting for search matches
- [ ] 21. Update `render_diff_view()` call site
- [ ] 22. Unit tests for highlighted spans

## Phase 6: Sidebar Search Highlighting
- [ ] 23. Add search pattern to `FileListProps`
- [ ] 24. Split file name spans at match boundaries
- [ ] 25. Pass search pattern from `render_sidebar()`

**Checkpoint: commit Phases 3-6**

## Phase 7: Lifecycle & Cleanup
- [ ] 26. Clear `search_matches` on file switch (in `load_diff_for_selected`)
- [ ] 27. Clear `search_sidebar_matches` on staging/unstaging
- [ ] 28. Recompute matches lazily on `n`/`N` when empty but pattern exists
- [ ] 29. Integration test: file switch clears matches, preserves pattern
- [ ] 30. Integration test: `n`/`N` recompute after clear

**Checkpoint: commit Phase 7**
