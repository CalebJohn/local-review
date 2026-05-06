# Task List: Vim-style Visual Line Selection

## Task 1: Add line-filtered `apply_hunk_to_content_with_lines`

**Description:** Add a variant of `apply_hunk_to_content` in `git/mod.rs` that accepts a `&[usize]` of hunk-relative line indices. Only lines whose hunk index appears in this slice are applied; other change lines are skipped.

**Acceptance criteria:**
- [ ] Function `apply_hunk_to_content_with_lines(old: &str, hunk: &DiffHunk, selected: &[usize]) -> String`
- [ ] When `selected` is empty, returns `old_content` unchanged
- [ ] When `selected` contains all +/- lines, behaves identically to `apply_hunk_to_content`
- [ ] Only Insert lines at selected indices appear in output; Delete lines at selected indices are skipped
- [ ] Equal context lines at selected indices are preserved; unselected Equal lines are also preserved (context)

**Verification:**
- [ ] `cargo test --lib -- git::tests::test_apply_hunk_to_content_with_lines_*`
- [ ] Edge cases: empty selection, single line, non-contiguous

**Dependencies:** None

**Files likely touched:**
- `src/git/mod.rs`

**Estimated scope:** S (1 function, ~40 lines)

---

## Task 2: Add line-filtered `reverse_apply_hunk_to_content_with_lines`

**Description:** Mirror of Task 1 for reverse apply. When unstaging, we need to apply the hunk's changes to the index content, keeping only selected lines.

**Acceptance criteria:**
- [ ] Function `reverse_apply_hunk_to_content_with_lines(new: &str, hunk: &DiffHunk, selected: &[usize]) -> String`
- [ ] Delete lines at selected indices become real Delete output (restored to output)
- [ ] Insert lines at selected indices are removed from output
- [ ] Equal lines at selected indices are preserved; unselected Equal lines also preserved

**Verification:**
- [ ] `cargo test --lib -- git::tests::test_reverse_apply_hunk_to_content_with_lines_*`
- [ ] Verified inverse property: `apply_hunk_to_content_with_lines(old, hunk, selected)` + `reverse_apply_hunk_to_content_with_lines(new, hunk, selected)` roundtrip correctly

**Dependencies:** Task 1

**Files likely touched:**
- `src/git/mod.rs`

**Estimated scope:** S (1 function, ~40 lines)

---

## Task 3: Unit tests for line-filtered apply/unstage

**Description:** Comprehensive test suite for the line-filtered apply functions covering edge cases, non-contiguous selection, and roundtrip properties.

**Acceptance criteria:**
- [ ] Tests for: empty selection, all +/- lines selected, non-contiguous selection
- [ ] Tests for: only Insert lines selected, only Delete lines selected, mixed
- [ ] Tests for: single-line hunk, multi-line hunk, hunk with only Equal context
- [ ] Integration test: apply + reverse_apply roundtrip preserves original

**Verification:**
- [ ] `cargo test --lib -- git::tests::test_apply_hunk_to_content_with_lines`
- [ ] `cargo test --lib -- git::tests::test_reverse_apply_hunk_to_content_with_lines`

**Dependencies:** Tasks 1, 2

**Files likely touched:**
- `src/git/mod.rs` (test module)

**Estimated scope:** S (3-5 test functions)

---

## Task 4: Add `AppMode` enum and visual selection state to `App`

**Description:** Add the mode state machine to `App` in `app.rs`. The app can be in `Normal` or `Visual` mode. Visual mode carries a line selection (Vec of hunk-relative indices) and an anchor for vim-style range extension.

**Acceptance criteria:**
- [ ] `AppMode` enum: `Normal`, `Visual`
- [ ] `visual_selection: Vec<usize>` — hunk-relative line indices (within current hunk)
- [ ] `visual_selection_anchor: Option<usize>` — anchor point for j/k extension (start of selection)
- [ ] `visual_mode_hunk_index: Option<usize>` — which hunk the selection belongs to (cleared on hunk change)
- [ ] `mode: AppMode` field in `App` struct
- [ ] `App::new()` initializes `mode: AppMode::Normal`, `visual_selection: Vec::new()`, etc.

**Verification:**
- [ ] `cargo test --lib -- app::tests::test_app_mode_*`
- [ ] App constructs without panic, mode defaults to Normal

**Dependencies:** None

**Files likely touched:**
- `src/app.rs`

**Estimated scope:** S (enum + 3 fields)

---

## Task 5: Add visual mode Messages to `app.rs`

**Description:** Add the new `Message` variants for visual mode interactions.

**Acceptance criteria:**
- [ ] `EnterVisual` — enter visual mode from normal diff view
- [ ] `ExitVisual` — exit visual mode, clear selection, return to normal
- [ ] `ExtendSelectionDown` — extend selection anchor downward (visual j)
- [ ] `ExtendSelectionUp` — extend selection anchor upward (visual k)
- [ ] `StageSelectedLines` — stage only the selected lines in current hunk
- [ ] `UnstageSelectedLines` — unstage only the selected lines in current hunk
- [ ] `CommentSelectedLines` — copy selection comment to clipboard

**Verification:**
- [ ] `cargo build` succeeds with new messages
- [ ] All existing tests pass

**Dependencies:** Task 4

**Files likely touched:**
- `src/app.rs`

**Estimated scope:** S (6-7 message variants)

---

## Task 6: Implement update() handlers for visual mode

**Description:** Implement the message handlers in `App::update()` for visual mode transitions and operations.

**Acceptance criteria:**
- [ ] `EnterVisual`: only works when `focus == Focus::DiffView && sidebar_section == Unstaged`; initializes selection to current line
- [ ] `ExitVisual`: clears `visual_selection`, `visual_selection_anchor`, sets `mode = Normal`
- [ ] `ExtendSelectionDown`: if in visual mode, adds next line to selection (if within same hunk); otherwise enters visual mode with single line
- [ ] `ExtendSelectionUp`: adds previous line to selection (if within same hunk)
- [ ] `StageSelectedLines`: only works in visual mode with unstaged hunk; requires at least one Insert line selected; error message if no +/- lines selected
- [ ] `UnstageSelectedLines`: only works in visual mode with staged hunk; requires at least one Delete line selected
- [ ] `CommentSelectedLines`: formats comment for selected lines and copies to clipboard
- [ ] `Esc` in DiffView enters `ExitVisual` when in visual mode

**Verification:**
- [ ] `cargo test --lib -- app::tests::test_update_visual_*`
- [ ] Mode transitions: Normal→Visual (v), Visual→Normal (Esc)
- [ ] `j/k` in visual mode extends selection; `j/k` in normal mode scrolls diff

**Dependencies:** Tasks 4, 5

**Files likely touched:**
- `src/app.rs`

**Estimated scope:** M (6 handlers, ~100 lines)

---

## Task 7: Add visual mode indicator to footer (ui.rs)

**Description:** Update `render_footer()` in `ui.rs` to show `[VISUAL]` indicator and visual-mode key hints when `app.mode == AppMode::Visual`.

**Acceptance criteria:**
- [ ] Footer shows `[VISUAL] j/k extend  s stage  u unstage  Esc cancel` when in visual mode
- [ ] Visual mode key hints replace normal diff key hints (but `n`, `N`, `Tab`, `q` remain)
- [ ] In staged section, `u` for unstage shown; in unstaged, `s` for stage shown
- [ ] Normal mode footer unchanged

**Verification:**
- [ ] `cargo build` succeeds
- [ ] Visual mode footer renders without panic

**Dependencies:** Task 4

**Files likely touched:**
- `src/ui.rs`

**Estimated scope:** S (footer branch for visual mode)

---

## Task 8: Add cyan selection highlighting to diff line gutter

**Description:** Update `diff_lines()` and `diff_lines_styled()` in `ui.rs` to render a cyan background on the gutter character (`│`) for lines that are part of the visual selection.

**Acceptance criteria:**
- [ ] When `app.mode == AppMode::Visual` and a line is in `visual_selection`, the gutter character has cyan background
- [ ] Uses `Style::default().bg(Color::Cyan)` on the gutter `Span`
- [ ] Selection highlight applies to both styled and non-styled diff rendering paths
- [ ] Hunk header line does not get selection highlighting (only content lines)

**Verification:**
- [ ] Manual test: enter visual mode, extend selection, verify cyan background on selected lines
- [ ] `cargo test --lib -- ui` still passes

**Dependencies:** Tasks 4, 6

**Files likely touched:**
- `src/ui.rs`

**Estimated scope:** S (2 functions, gutter span styling)

---

## Task 9: Implement StageSelectedLines handler

**Description:** Implement the `StageSelectedLines` case in `App::update()`. This stages only the selected +/- lines from the current hunk using the line-filtered apply function.

**Acceptance criteria:**
- [ ] Uses `apply_hunk_to_content_with_lines` with the selected line indices
- [ ] Records undo action before making changes
- [ ] Returns error status message if no Insert lines are selected
- [ ] After stage, clears selection and exits visual mode
- [ ] After stage, calls `refresh_files()` and restores hunk position

**Verification:**
- [ ] Can select lines in an unstaged hunk and stage only those lines
- [ ] Index content matches only the selected +/- lines applied
- [ ] Workdir is preserved
- [ ] `cargo test --lib -- app::tests::test_stage_selected_lines_*`

**Dependencies:** Tasks 1, 6, 8

**Files likely touched:**
- `src/app.rs`

**Estimated scope:** M (~40 lines)

---

## Task 10: Implement UnstageSelectedLines handler

**Description:** Implement the `UnstageSelectedLines` case in `App::update()`. This unstages only the selected +/- lines from the current staged hunk.

**Acceptance criteria:**
- [ ] Uses `reverse_apply_hunk_to_content_with_lines` with selected line indices
- [ ] Records undo action before making changes
- [ ] Returns error status message if no Delete lines are selected
- [ ] After unstage, clears selection and exits visual mode
- [ ] After unstage, calls `refresh_files()` and restores hunk position

**Verification:**
- [ ] Can select lines in a staged hunk and unstage only those lines
- [ ] Index content reflects only the selected lines removed
- [ ] Workdir is preserved
- [ ] `cargo test --lib -- app::tests::test_unstage_selected_lines_*`

**Dependencies:** Tasks 2, 6, 8

**Files likely touched:**
- `src/app.rs`

**Estimated scope:** M (~40 lines)

---

## Task 11: Add `c` keybinding for comment on selected lines

**Description:** When `c` is pressed in visual mode with a line selection, format and copy a comment to clipboard describing the selected lines. The comment format includes file path, section, hunk range, selected line numbers, and a preview.

**Acceptance criteria:**
- [ ] `c` in visual mode formats comment with: File path, section (staged/unstaged), hunk range, selected line indices, line preview
- [ ] Comment copied to clipboard via `arboard`
- [ ] After comment copy, exits visual mode and clears selection
- [ ] `c` in normal mode continues to work for hunk-level comments (existing behavior)

**Verification:**
- [ ] Press `v` to enter visual, select lines, press `c` — clipboard contains formatted comment
- [ ] Comment format matches SPEC.md example
- [ ] Visual mode exited after comment copy

**Dependencies:** Task 6

**Files likely touched:**
- `src/app.rs`
- `src/main.rs` (add `c` → `CommentSelectedLines` in DiffView visual mode branch)

**Estimated scope:** S (format function + keybinding)

---

## Task 12: End-to-end integration test

**Description:** Write an integration test that creates a temp git repo, makes two separate changes in one file, stages both hunks, then uses visual line selection to unstage only one hunk. Verify workdir and index content are correct.

**Acceptance criteria:**
- [ ] Test creates temp repo with initial commit
- [ ] Makes two changes in one file (separate hunks)
- [ ] Stages both hunks (full hunk staging)
- [ ] Uses visual line selection to unstage only hunk 2
- [ ] Verifies index has only hunk 1 staged, hunk 2 unstaged
- [ ] Verifies workdir has both changes (preserved)

**Verification:**
- [ ] `cargo test --lib -- test_visual_line_selection_e2e`
- [ ] All existing tests still pass

**Dependencies:** Tasks 9, 10

**Files likely touched:**
- `src/git/mod.rs` (test module)

**Estimated scope:** M (1 comprehensive test)

---

## Task 13: Verify existing tests pass

**Description:** Run the full test suite to ensure no regressions were introduced by the visual mode changes.

**Acceptance criteria:**
- [ ] `cargo test` passes with zero failures
- [ ] All app module tests pass
- [ ] All git module tests pass
- [ ] All ui module tests pass

**Verification:**
- [ ] `cargo test --lib` — all lib tests pass
- [ ] `cargo test` — full test suite passes

**Dependencies:** All previous tasks

**Files likely touched:** None (verification only)

**Estimated scope:** XS (test run)

---

## Task Sizing Summary

| Task | Size | Files |
|------|------|-------|
| 1 | S | 1 |
| 2 | S | 1 |
| 3 | S | 1 |
| 4 | S | 1 |
| 5 | S | 1 |
| 6 | M | 1 |
| 7 | S | 1 |
| 8 | S | 1 |
| 9 | M | 1 |
| 10 | M | 1 |
| 11 | S | 1-2 |
| 12 | M | 1 |
| 13 | XS | 0 |

**Total estimated scope:** M (13 tasks, ~300-400 lines of new code)