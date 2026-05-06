# Implementation Plan: Vim-style Visual Line Selection

## Overview

Add a visual mode to the diff view allowing users to select individual lines and stage/unstage only those lines (rather than full hunks). This gives general developers fine-grained control over what gets staged, with an intuitive vim-inspired interface.

## Architecture Decisions

- **State machine approach**: Add `Visual` mode alongside existing `Normal` mode, stored in `App` struct
- **Line selection stored as `Vec<usize>`**: Indices relative to current hunk's lines (not absolute file line numbers)
- **Selection highlighting**: Cyan background on line prefixes in the diff gutter
- **Line-filtered apply functions**: New variants of `apply_hunk_to_content` and `reverse_apply_hunk_to_content` accept `&[usize]` to filter which lines to apply
- **No new hunk-level struct changes**: All selection state lives in `App.visual_selection`

## Dependency Graph

```
app.rs (App struct, Message enum, update logic)
    │
    ├── ui.rs (footer hints, diff line rendering with selection highlight)
    │       │
    │       └── main.rs (keybinding dispatch for 'v', 'j', 'k', 'Esc')
    │
    ├── git/mod.rs (line-filtered stage/unstage functions)
    │
    └── diff/types.rs (DiffHunk, DiffLine - minimal, used for line iteration)
```

**Build order**: git/mod.rs (foundation) → app.rs (state + logic) → ui.rs (rendering) → main.rs (keybinding)

## Task List

### Phase 1: Foundation — Git layer with line-filtered apply

- [ ] Task 1: Add line-filtered apply functions in git/mod.rs
- [ ] Task 2: Add line-filtered unstage functions in git/mod.rs
- [ ] Task 3: Unit tests for line-filtered apply/unstage

### Checkpoint: Foundation
- [ ] `cargo test` passes — git module tests green
- [ ] New functions handle: empty selection, all lines selected, non-contiguous selection

### Phase 2: App State — Mode state, selection state, Messages

- [ ] Task 4: Add `AppMode` enum and visual selection state to `App`
- [ ] Task 5: Add `EnterVisual`, `ExtendSelection`, `ExitVisual` Messages
- [ ] Task 6: Implement `update()` handlers for visual mode transitions

### Checkpoint: App State
- [ ] `cargo test` passes — app module tests green
- [ ] Mode transitions work: Normal→Visual (v), Visual→Normal (Esc)

### Phase 3: UI — Selection rendering and footer hints

- [ ] Task 7: Add visual mode indicator to footer in ui.rs
- [ ] Task 8: Add cyan selection highlighting to diff line gutter

### Checkpoint: UI
- [ ] Visual mode shows `[VISUAL]` indicator in footer
- [ ] Selected lines have cyan gutter indicator

### Phase 4: Line Stage/Unstage — The complete feature path

- [ ] Task 9: Implement `StageSelectedLines` message and handler
- [ ] Task 10: Implement `UnstageSelectedLines` message and handler
- [ ] Task 11: Add `c` keybinding for comment on selected lines (clipboard integration)

### Checkpoint: Feature Complete
- [ ] `v` enters visual mode from Normal mode in diff view
- [ ] `j/k` extends/shrinks line selection
- [ ] `s` stages only selected +/- lines (error if none selected)
- [ ] `u` unstages only selected +/- lines (error if none selected)
- [ ] `Esc` exits visual mode, clears selection, returns to Normal
- [ ] Selection cleared after stage/unstage
- [ ] Existing hunk-level `s`/`u` bindings continue to work
- [ ] Visual mode shown in footer when active
- [ ] `c` in visual mode comments on selected lines

### Phase 5: Integration & Polish

- [ ] Task 12: End-to-end test: stage partial hunk and verify index content
- [ ] Task 13: Verify all existing tests still pass

### Checkpoint: Complete
- [ ] All acceptance criteria from SPEC.md met
- [ ] Ready for human review

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Line indices become invalid after partial hunk staging | Med | Selection cleared after every stage/unstage; reload diff and reset cursor |
| Complex state interaction between visual mode and focus | Med | Strict mode transitions — visual mode only entered when focus is DiffView |
| Line-filtered apply edge cases (empty selection, all selected) | Low | Unit tests cover edge cases before integration |

## Open Questions

- Should visual mode persist when switching files? (Decision: No — selection is hunk-local, cleared on file change)
- Should `v` work from sidebar? (Decision: No — spec says "in diff view")