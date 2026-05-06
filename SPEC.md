# Spec: Vim-style Visual Line Selection in Diff View

## Objective

Add a visual mode to the diff view allowing users to select individual lines and stage/unstage only those lines (rather than full hunks). This gives general developers fine-grained control over what gets staged, with an intuitive vim-inspired interface.

**User stories:**
- As a developer, I can press `v` to enter visual mode and use `j/k` to extend a line selection range
- As a developer, I can press `s` with a line selection to stage only the selected lines
- As a developer, I can press `u` with a line selection to unstage only the selected lines
- As a developer, I can press `Esc` to exit visual mode and clear selection

## Tech Stack

- **Language:** Rust
- **Framework:** ratatui (TUI framework)
- **Architecture:** MVC with `App` state in `app.rs`, UI rendering in `ui.rs`, git operations in `git/mod.rs`
- **Key types:** `DiffHunk`, `DiffLine` (from `diff/types.rs`)

## Commands

No new CLI commands. Features accessed via keyboard in TUI.

## Project Structure

```
src/
  app.rs          → App state, focus management, mode state
  ui.rs           → Diff view rendering, footer key hints
  git/
    mod.rs        → stage_hunk, unstage_hunk, line-filtered variants
    types.rs      → FileEntry, FileStatus, ContentResult
  diff/
    mod.rs        → DiffContent, compute_hunks
    types.rs      → DiffHunk, DiffLine, ChangeKind
```

## Code Style

- State machine for modes: `Normal` | `Visual` (with line range)
- Visual mode indicator in footer: `[VISUAL] j/k extend  s stage  u unstage  Esc cancel`
- Selection highlighting: cyan background on selected line prefixes
- Line selection stored as `Vec<usize>` of hunk-relative line indices
- Line-filtered apply functions accept additional `&[usize]` parameter to filter lines

## Testing Strategy

- Unit tests for `apply_hunk_to_content` variants with line filters
- Unit tests for `reverse_apply_hunk_to_content` variants with line filters
- Integration tests verifying staged content matches selected lines only

## Boundaries

**Always:**
- Preserve workdir content when staging partial hunks
- Run `cargo test` before commits
- Update SPEC.md if requirements change

**Ask first:**
- Changing keybindings (existing `s` for hunk staging should remain functional)
- Modifying `DiffLine` or `DiffHunk` structs

**Never:**
- Stage lines outside the current hunk's range
- Apply partial hunk that corrupts file state

## Success Criteria

1. `v` enters visual mode from Normal mode in diff view
2. `j/k` extends/shrinks line selection visually
3. `s` stages only selected +/- lines (error if no +/- lines selected)
4. `u` unstages only selected +/- lines (error if no +/- lines selected)
5. `Esc` exits visual mode, clears selection, returns to Normal mode
6. Selection is cleared after stage/unstage operation
7. Existing hunk-level `s`/`u` bindings continue to work for full hunk staging
8. Visual mode shown in footer when active
9. `c` in visual mode comments on the selected lines (copies selection detail to clipboard)

## Visual Mode Keybindings

| Key | Normal Mode | Visual Mode |
|-----|-------------|-------------|
| `v` | Enter visual mode | - |
| `j/k` | Scroll diff | Extend selection down/up |
| `s` | Stage hunk | Stage selected lines |
| `u` | Unstage hunk | Unstage selected lines |
| `c` | Comment on hunk | Comment on selected lines |
| `Esc` | - | Exit visual, clear selection |

## Comment Integration

When `c` is pressed in visual mode with a line selection:
1. Capture the selected line range (start/end indices within the hunk)
2. Format the comment to include the specific line numbers selected
3. Copy to clipboard in the same format as hunk-level comments
4. Exit visual mode after comment is copied

Comment format for selected lines:
```
File: {path}
Section: {staged/unstaged}
Hunk: @{hunk_start},{hunk_end}
Selected lines ({count}): {line_nums}
--- (optional: show first few lines of selection) ---
{line_preview}
Comment: {user_input}
```