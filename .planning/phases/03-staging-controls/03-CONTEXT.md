# Phase 3 Context: Staging Controls

**Created:** 2026-04-23

## Phase Overview
- **Goal:** Stage/unstage at file and hunk granularity with live sidebar refresh
- **Requirements:** STAG-01, STAG-02, STAG-03, STAG-04, VIEW-09
- **Depends on:** Phase 2

## Implementation Decisions

### File-Level Staging
- **Stage (STAG-01):** Use `git2::Index.add_path(path)` — adds file to index
- **Unstage (STAG-02):** Use `git2::Index.remove_from_head(path)` if HEAD contains file, otherwise use worktree write to reset

### Hunk-Level Staging (STAG-03, STAG-04)
- **Approach:** Partial by re-appplying diff
- **Stage a hunk:**
  1. Get HEAD content for file
  2. Extract the diff hunk lines (Delete + Insert from similar output)
  3. Apply only those changes to HEAD content in memory
  4. Write the modified content to the index via patch-style apply (`git2::Index.add_from_buffer`)
- **Unstage a hunk:**
  1. Get current staged content
  2. Remove the hunk's changes to get "what was before"
  3. Write new staged content to index

### Live Sidebar Refresh (VIEW-09)
- After any staging operation, call `repo.refresh_changed_files()` (existing method)
- Update `app.files` with fresh `changed_files()`
- Reset `selected_index` if the previously selected file is no longer in the list
- Preserve scroll and focus if file still exists

## Key Methods to Add (git module)
- `GitRepo::stage_file(path)` → stage entire file
- `GitRepo::unstage_file(path)` → unstage entire file
- `GitRepo::stage_hunk(path, hunk_idx)` → stage specific hunk
- `GitRepo::unstage_hunk(path, hunk_idx)` → unstage specific hunk

## Keybindings (Plan)
- `s` in sidebar → stage selected file
- `u` in sidebar → unstage selected file
- `s` in diff view → stage current hunk (under cursor/selection)
- `u` in diff view → unstage current hunk

## Success Criteria (from REQ)
1. Stage file → sidebar updates to show staged status
2. Unstage file → sidebar shows unstaged
3. Stage hunk → diff refreshes showing remaining unstaged
4. Unstage hunk → diff refreshes showing unstaged
5. All operations reflect in sidebar immediately