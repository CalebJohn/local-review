---
status: complete
phase: 01-foundation-file-navigation
source: [01-01-SUMMARY.md, 01-02-SUMMARY.md]
started: 2026-04-23T05:19:11Z
updated: 2026-04-23T05:23:30Z
---

## Current Test

[testing complete]

## Tests

### 1. Launch TUI in a Git Repo
expected: |
  Run `cargo run` from the project root. The TUI takes over the terminal with a two-panel layout:
  a narrow sidebar on the left (about 30 columns wide) listing the repo's changed files, and a wider
  diff panel on the right. The first file is auto-selected (highlighted in the sidebar) and its diff
  is already rendered in the right panel without needing to press anything.
result: pass

### 2. File Status Indicators in Sidebar
expected: |
  Each sidebar row shows a status letter + file path (e.g. `M Dockerfile.claude`, `? .claude/...`).
  Status letters are color-coded: M = yellow, A = green, D = red, R = cyan, ? = dark gray.
  The currently-selected row has a darker background and bold text.
result: pass

### 3. Navigate Sidebar with j/k
expected: |
  With focus on the sidebar (blue border around it), press `j` to move the selection down and `k`
  to move it up. As you move, the right panel updates to show the diff for the newly selected file.
  Selection does not go past the last or before the first file.
result: pass

### 4. Color-Coded Inline Diff
expected: |
  The diff panel shows hunk headers like `@@ -12,7 +12,9 @@` in cyan. Each content line is prefixed
  with ` ` (context), `+` (added, green), or `-` (removed, red). To the left of each line are two
  right-aligned columns of line numbers (old and new) in dark gray; blank where a number doesn't apply.
result: pass

### 5. Focus Switching and Diff Scrolling
expected: |
  Press `Tab` to move focus from the sidebar to the diff view — the sidebar border turns gray and
  the diff panel border turns blue. With diff view focused, press `j`/`k` to scroll the diff down/up
  by one line. Press `Tab` or `Esc` to return focus to the sidebar.
result: pass

### 6. Staged vs Unstaged Diff Auto-Selection
expected: |
  Quit the app. Stage one of the currently-modified files with `git add <file>`, then run `cargo run`
  again. Select the staged file — its diff shows only what was staged (HEAD vs index). Select a file
  that is still unstaged — its diff shows only the remaining unstaged changes (index vs working tree).
  You do not have to toggle anything; the app picks the right view automatically based on status.
result: pass

### 7. Binary File Handling
expected: |
  If any changed file in this repo is binary (e.g. an image, a compiled artifact), select it in the
  sidebar. The diff panel shows a centered yellow message "Binary file (not shown)" rather than
  garbled content. If no binary file is currently changed, you can skip this test.
result: skipped
reason: no binary file in the currently changed set

### 8. Clean Exit with q
expected: |
  Press `q` (from either the sidebar or diff-view focus). The TUI exits immediately, the terminal
  returns to your shell prompt with the cursor visible, scroll-back is intact, and there are no
  leftover rendering artifacts or a stuck alternate screen.
result: pass

## Summary

total: 8
passed: 7
issues: 0
pending: 0
skipped: 1
blocked: 0

## Gaps

[none yet]
