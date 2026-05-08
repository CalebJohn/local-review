# Module Extraction — Task List

## Phase 1: Extract tests
- [x] 1.1 Extract tests from undo.rs, diff/mod.rs, classify/mod.rs, ui.rs, git/mod.rs, main.rs
- [x] 1.2 Extract tests from app.rs → app_tests.rs
- [x] **Checkpoint:** commit, all tests green

## Phase 2: Split app.rs
- [x] 2.1 Convert app.rs → app/mod.rs (rename + move tests)
- [x] 2.2 Extract app/geometry.rs (DiffLineKey, coordinate functions)
- [x] 2.3 Extract app/comment.rs (CommentContext, format_comment, 5 handlers)
- [x] 2.4 Extract app/visual.rs (visual selection, 6 handlers) — depends: 2.2
- [x] 2.5 Extract app/navigation.rs (nav helpers, 20 handlers) — depends: 2.2, 2.4
- [x] 2.6 Extract app/staging.rs (staging/undo, 12 handlers) — depends: 2.4, 2.5
- [x] **Checkpoint:** commit, update() is thin dispatcher, all tests green

## Phase 3: Split git/mod.rs
- [x] 3.1 Extract git/hunk.rs (apply_hunk_to_content, reverse_apply_hunk_to_content)
- [x] 3.2 Extract git/status.rs (is_binary_content, status mappers, changed_files helpers)
- [x] 3.3 Extract git/staging.rs (Snapshot types, staging impl methods) — depends: 3.1, 3.2
- [x] **Checkpoint:** commit, all tests green

## Phase 4: Extract input handling
- [x] 4.1 Extract input.rs (translate_* functions, rect_contains)
- [x] **Checkpoint:** commit, all tests green

## Phase 5: Split ui.rs
- [x] 5.1 Convert ui.rs → ui/mod.rs + extract ui/sidebar.rs
- [x] 5.2 Extract ui/diff_view.rs (render_diff_view, diff_lines)
- [x] 5.3 Extract ui/footer.rs (render_footer + footer_line + 12 tests)
- [x] 5.4 Update CLAUDE.md with new architecture
- [x] **Checkpoint:** commit, all tests green
