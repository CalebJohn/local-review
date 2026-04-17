# Requirements: Git Diff Review TUI

**Defined:** 2026-04-17
**Core Value:** Make reviewing local git changes fast, clear, and precise — with syntax-highlighted diffs and staging controls at any granularity.

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### Core Viewing

- [ ] **VIEW-01**: File sidebar showing git status indicators (M/A/D/R/U)
- [ ] **VIEW-02**: Vim-style keybindings for sidebar navigation (j/k to move, Enter to select)
- [ ] **VIEW-03**: Inline (unified) diff view with line numbers
- [ ] **VIEW-04**: Syntax highlighting in diff view via tree-sitter
- [ ] **VIEW-05**: Working tree vs HEAD diff for unstaged files
- [ ] **VIEW-06**: Index vs HEAD diff when a staged file is selected in sidebar
- [ ] **VIEW-07**: Color-coded additions/deletions (green/red)
- [ ] **VIEW-08**: Hunk-to-hunk jumping (n/N or similar keybind)
- [ ] **VIEW-09**: Sidebar refreshes to reflect current git status after staging operations

### Staging

- [ ] **STAG-01**: Stage entire file from the review UI
- [ ] **STAG-02**: Unstage entire file from the review UI
- [ ] **STAG-03**: Stage individual hunks within a diff view
- [ ] **STAG-04**: Unstage individual hunks within a diff view

### Interaction

- [ ] **INTR-01**: Mouse click support for sidebar file selection
- [ ] **INTR-02**: Mouse click support for diff view interaction
- [ ] **INTR-03**: Clean quit with no explicit "done" step

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Advanced Viewing

- **ADVV-01**: Side-by-side diff view with toggle
- **ADVV-02**: Word-level diff highlighting within lines
- **ADVV-03**: Search within diffs (/ with n/N)

### Advanced Staging

- **ADVS-01**: Line-level staging via visual selection

### Syntax Intelligence

- **SYNT-01**: Syntax-aware diffing (AST comparison, reduces reformat noise)
- **SYNT-02**: Semantic hunk boundaries (aligned to functions/blocks)

### Review Workflow

- **REVW-01**: Inline annotations/comments on diff lines
- **REVW-02**: Copy all comments to clipboard formatted with diff context for AI agents

## Out of Scope

| Feature | Reason |
|---------|--------|
| Commit creation | User commits separately after reviewing; staging is the review artifact |
| Branch management | lazygit/gitui own this space; this tool focuses on diff review only |
| Remote operations (push/pull/fetch) | Not a full git client |
| PR/remote review | This is for local changes only |
| Git difftool integration | Standalone workflow, not a git plugin |
| Persistent state / database | Stateless; git is the source of truth |
| AI-powered diff explanation | AI angle is output (comments for agents), not input |
| Configuration file system | Hardcode good defaults for v1 |
| Watch mode / live file updates | Defer indefinitely |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| VIEW-01 | — | Pending |
| VIEW-02 | — | Pending |
| VIEW-03 | — | Pending |
| VIEW-04 | — | Pending |
| VIEW-05 | — | Pending |
| VIEW-06 | — | Pending |
| VIEW-07 | — | Pending |
| VIEW-08 | — | Pending |
| VIEW-09 | — | Pending |
| STAG-01 | — | Pending |
| STAG-02 | — | Pending |
| STAG-03 | — | Pending |
| STAG-04 | — | Pending |
| INTR-01 | — | Pending |
| INTR-02 | — | Pending |
| INTR-03 | — | Pending |

**Coverage:**
- v1 requirements: 16 total
- Mapped to phases: 0
- Unmapped: 16

---
*Requirements defined: 2026-04-17*
*Last updated: 2026-04-17 after initial definition*
