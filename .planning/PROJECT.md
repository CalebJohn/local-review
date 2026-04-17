# Git Diff Review TUI

## What This Is

A standalone terminal-based tool for reviewing git diffs. It provides a navigable file sidebar, syntax-highlighted diff views (side-by-side and inline), and uses git staging as the review mechanism — staging a change means you've reviewed it. Built in Rust with Ratatui for a fast, self-contained single binary.

## Core Value

Make reviewing local git changes fast, clear, and precise — with syntax-aware diffs that cut through noise and staging controls that let you approve at any granularity.

## Requirements

### Validated

(None yet — ship to validate)

### Active

- [ ] File sidebar showing git status (modified, added, deleted, renamed)
- [ ] Vim-style keybindings for sidebar navigation
- [ ] Mouse/click support for sidebar
- [ ] Inline diff view with syntax highlighting via tree-sitter
- [ ] Side-by-side diff view with syntax highlighting via tree-sitter
- [ ] Toggle between inline and side-by-side views
- [ ] Working tree vs HEAD diff for unstaged files
- [ ] Index vs HEAD diff for staged files (context-driven by sidebar selection)
- [ ] Stage entire file from the review UI
- [ ] Stage individual hunks with toggle controls
- [ ] Stage arbitrary line selections (vim visual-select style)
- [ ] Unstage files/hunks/lines (reverse of staging)
- [ ] Syntax-aware diffing — reduce noise from reformats, indentation changes
- [ ] Semantic diff boundaries — hunks aligned to functions/blocks, not arbitrary lines
- [ ] Inline comments on diff lines
- [ ] Copy all comments to clipboard as formatted text (with diff context, for coding agents)
- [ ] Quit exits cleanly — no explicit "done" step, staging is the review artifact

### Out of Scope

- Git difftool integration — this is a standalone workflow, not a git plugin
- Persistent state or database — stateless, git is the source of truth
- Remote/PR review — this is for local changes only
- Commit creation — user commits separately after reviewing

## Context

- Built for personal use by the author, a developer who reviews diffs frequently and wants better signal-to-noise than standard git diff
- The comment-to-clipboard feature is specifically designed for feeding review notes to AI coding agents
- Tree-sitter is both the syntax highlighter and the foundation for syntax-aware diffing
- Key Rust libraries: ratatui (TUI), git2 (libgit2 bindings), tree-sitter (parsing/highlighting)

## Constraints

- **Language**: Rust — for performance, single-binary distribution, and native tree-sitter/git2 support
- **TUI Framework**: Ratatui — mature, well-maintained Rust TUI framework
- **Distribution**: Single binary with no runtime dependencies
- **Git integration**: Via git2 (libgit2), not shelling out to git CLI

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Rust + Ratatui | Native tree-sitter bindings, single binary distribution, performance | -- Pending |
| git2 over CLI shelling | Reliable, structured access to git state; no PATH dependency | -- Pending |
| Staging as review state | Stateless design; git index is the review artifact | -- Pending |
| Sidebar drives diff context | Unstaged files show worktree diff, staged files show index diff — no manual mode switching | -- Pending |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? -> Move to Out of Scope with reason
2. Requirements validated? -> Move to Validated with phase reference
3. New requirements emerged? -> Add to Active
4. Decisions to log? -> Add to Key Decisions
5. "What This Is" still accurate? -> Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-04-17 after initialization*
