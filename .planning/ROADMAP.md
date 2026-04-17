# Roadmap: Git Diff Review TUI

**Created:** 2026-04-17
**Granularity:** Coarse
**Phases:** 3
**Coverage:** 16/16 v1 requirements mapped

## Phases

- [ ] **Phase 1: Foundation + File Navigation** - TEA architecture, git layer, file sidebar with status, inline diff view with color coding, vim keybindings, and clean exit
- [ ] **Phase 2: Diff Polish + Interaction** - Syntax highlighting via tree-sitter, hunk-to-hunk jumping, and mouse support across sidebar and diff view
- [ ] **Phase 3: Staging Controls** - Stage/unstage at file and hunk granularity with live sidebar refresh after operations

## Phase Details

### Phase 1: Foundation + File Navigation
**Goal**: User can launch the TUI in a git repo, browse changed files in a sidebar, and view color-coded inline diffs with keyboard navigation
**Depends on**: Nothing (first phase)
**Requirements**: VIEW-01, VIEW-02, VIEW-03, VIEW-05, VIEW-06, VIEW-07, INTR-03
**Success Criteria** (what must be TRUE):
  1. User sees a sidebar listing all changed files with status indicators (M/A/D/R) when launching in a git repo
  2. User can navigate the sidebar with j/k and press Enter to view a file's diff in the main panel
  3. User sees an inline diff with line numbers, green for additions, red for deletions
  4. User sees the working tree diff for unstaged files and the index diff for staged files, determined automatically by sidebar selection
  5. User can press q to exit cleanly with terminal state fully restored
**Plans:** 2 plans
Plans:
- [x] 01-01-PLAN.md — Project scaffold, git service layer (owned types), and diff computation module
- [ ] 01-02-PLAN.md — TEA app model, event loop, sidebar + diff view UI rendering
**UI hint**: yes

### Phase 2: Diff Polish + Interaction
**Goal**: Diffs are syntax-highlighted and easily navigable via hunk jumping and mouse clicks
**Depends on**: Phase 1
**Requirements**: VIEW-04, VIEW-08, INTR-01, INTR-02
**Success Criteria** (what must be TRUE):
  1. Diff view displays syntax-highlighted code using tree-sitter, with language auto-detected from file extension
  2. User can jump between hunks in the diff view using keyboard shortcuts (n/N or similar)
  3. User can click files in the sidebar with the mouse to select them
  4. User can click in the diff view area to interact (scroll, select position)
**Plans**: TBD
**UI hint**: yes

### Phase 3: Staging Controls
**Goal**: User can stage and unstage changes at file and hunk granularity directly from the review UI, with immediate visual feedback
**Depends on**: Phase 2
**Requirements**: STAG-01, STAG-02, STAG-03, STAG-04, VIEW-09
**Success Criteria** (what must be TRUE):
  1. User can stage an entire file from the diff view and see the sidebar update to reflect the new status
  2. User can unstage an entire file and see the sidebar revert to show it as unstaged
  3. User can stage individual hunks within a diff and see the diff refresh to show remaining unstaged changes
  4. User can unstage individual hunks and see the diff refresh accordingly
  5. Sidebar file status indicators update immediately after any staging operation without manual refresh
**Plans**: TBD
**UI hint**: yes

## Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Foundation + File Navigation | 0/2 | Planned | - |
| 2. Diff Polish + Interaction | 0/? | Not started | - |
| 3. Staging Controls | 0/? | Not started | - |

---
*Roadmap created: 2026-04-17*
*Last updated: 2026-04-17*
