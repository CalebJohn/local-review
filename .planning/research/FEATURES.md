# Feature Landscape: Git Diff Review TUI

**Domain:** Standalone terminal-based git diff review tool
**Researched:** 2026-04-17
**Competitors analyzed:** lazygit, gitui, tig, delta, difftastic, diffsitter, critique, revdiff, deff, diffity, magit (Emacs)

---

## Table Stakes

Features users expect. Missing any of these and the tool feels broken or incomplete compared to existing options.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Unified (inline) diff view | Every diff tool has this; it's the baseline format | Low | Standard unified diff format with +/- markers, colored |
| Syntax highlighting in diffs | delta, critique, deff all do this; unhighlighted diffs look primitive | Medium | Tree-sitter is the project's chosen engine; bat/syntect is the other common path |
| File list / sidebar navigation | lazygit, gitui, revdiff, deff all show a navigable file list | Medium | Must show git status indicators (M/A/D/R) per file |
| Keyboard navigation (vim-style) | j/k/h/l is universal in terminal tools; lazygit, gitui, tig, revdiff all use it | Low | Minimum: j/k for lines, file-to-file jumping, hunk-to-hunk jumping |
| Hunk-to-hunk jumping | delta (n/N), revdiff, lazygit all support jumping between change hunks | Low | Essential for large diffs; users skip unchanged context constantly |
| Git status awareness | Showing which files are staged, unstaged, untracked is baseline for any git TUI | Low | Color-coded or icon-coded status in file list |
| Working tree diff (unstaged changes) | git diff HEAD equivalent; the primary use case | Low | This is the default view users expect |
| Staged diff view | git diff --cached equivalent; users need to see what's already staged | Low | lazygit, gitui, magit all distinguish staged vs unstaged |
| Color-coded additions/deletions | Green for added, red for removed lines is universal convention | Low | Background tinting (not just +/- markers) is the modern expectation |
| Responsive terminal handling | Resize gracefully, adapt to terminal width | Medium | Ratatui handles much of this, but layout logic still needs care |
| Search within diffs | / search with n/N navigation; revdiff, deff, delta all have this | Low | At minimum: text search within current diff view |

## Differentiators

Features that set the product apart. Not universally expected, but valued. These are where the project's identity lives.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Side-by-side diff view** | critique, deff, delta offer this; many TUIs don't. Toggle between inline and split is the real differentiator | High | Must handle terminal width constraints; critique auto-switches on narrow terminals. Synchronized scrolling required |
| **Syntax-aware diffing (structural)** | Only difftastic and diffsitter do this today. Reduces noise from reformats, indentation, bracket moves. This is the project's core value proposition | High | Uses tree-sitter ASTs to compare structure, not lines. Known limitations: multi-mapping, refactoring awareness. No existing TUI combines this with interactive staging |
| **Semantic hunk boundaries** | Standard diff uses arbitrary line-based context boundaries. Aligning hunks to function/block boundaries makes diffs dramatically more readable | High | Requires tree-sitter parsing to identify function/block boundaries, then adjusting hunk splitting. Novel -- no existing tool does this well interactively |
| **Line-level staging** | magit's "killer feature" per HN. lazygit and gitui support it. Staging arbitrary line selections (not just hunks) gives surgical precision | High | Requires constructing partial patches from selected lines. git2 supports index manipulation but building correct patches from arbitrary line selections is tricky |
| **Hunk-level staging** | lazygit, gitui, magit all have this. Table stakes for a staging-focused tool, but differentiating for a diff *review* tool | Medium | Simpler than line-level; apply/reverse individual hunks to/from index |
| **Inline annotations/comments** | revdiff has this as core feature; diffity integrates with AI agents. This is the project's agent-feedback feature | Medium | Comments attached to diff lines, with context. The clipboard-export aspect is the real differentiator |
| **Comments-to-clipboard export** | diffity has "copy prompt" for AI agents. revdiff pipes annotations to stdout. This specific feature -- formatted comments with diff context for AI coding agents -- is the project's unique angle | Medium | Format: file path + line number + diff context + comment text. Must be paste-ready for Claude Code / Cursor / similar |
| **Word-level diff highlighting** | delta uses Levenshtein edit inference; critique has word-level diff. Shows exactly which words changed within a line | Medium | Significantly improves readability for small changes on long lines. Can be done at character or word granularity |
| **Mouse support** | Click to select files, click to position cursor in diffs. lazygit has full mouse support | Medium | Ratatui supports mouse events. Important for accessibility but secondary to keyboard-first design |
| **File review tracking** | deff has per-file "reviewed" toggles. Useful for large changesets -- track which files you've looked at | Low | Simple boolean state per file; visual indicator in sidebar. Ephemeral (not persisted) |
| **Watch mode / live updates** | critique has live file watching. Useful when editing alongside review | Medium | File system watcher that re-reads git state on changes. Nice for iterative review-edit cycles |

## Anti-Features

Features to explicitly NOT build. These are commonly requested but conflict with the project's design philosophy.

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| **Commit creation** | PROJECT.md explicitly scopes this out. User commits separately after reviewing. Adding commit UI bloats scope toward lazygit/gitui territory | Quit cleanly; staging IS the review artifact. User runs `git commit` after |
| **Branch management** | lazygit/gitui own this space completely. Adding branch switching, merging, rebasing turns this into a general git TUI | Stay focused on diff review. User switches branches with git CLI |
| **Remote operations (push/pull/fetch)** | Same as above -- scope creep toward full git client | Out of scope per PROJECT.md |
| **PR/remote review** | PROJECT.md: "this is for local changes only." PR review is a different product (GitHub CLI, diffity, etc.) | Focus on local working tree and index |
| **Persistent state / database** | PROJECT.md: "stateless, git is the source of truth." Adding review history, saved comments, etc. adds complexity and state management | Git index is the review artifact. Comments are ephemeral (copied to clipboard, then gone) |
| **Git difftool integration** | PROJECT.md: "this is a standalone workflow, not a git plugin." critique supports difftool mode but this tool should be opinionated about its own workflow | Run standalone: `diffr` (or whatever the binary is called) in a git repo |
| **AI-powered diff explanation** | critique has this. Adding LLM integration couples to external services and bloats scope. The project's AI angle is *output* (comments for agents), not *input* (AI analyzing diffs) | The comment-to-clipboard feature serves the AI workflow without requiring AI integration |
| **Web preview / HTML export** | critique generates shareable HTML. This is a TUI tool; web output is a different product | Clipboard export covers the sharing use case for this tool's audience |
| **PDF generation** | critique does this for Kindle reading. Way out of scope | N/A |
| **Configuration file system** | revdiff has extensive theming/config. For a personal-use tool, hardcode good defaults | Minimal config (if any). Good defaults > configurability for v1 |

## Feature Dependencies

```
Syntax Highlighting (tree-sitter) ──> Syntax-Aware Diffing (AST comparison)
                                  └──> Semantic Hunk Boundaries (function/block detection)

File Sidebar ──> Git Status Awareness
             └──> File Review Tracking

Inline Diff View ──> Side-by-Side Diff View (side-by-side builds on inline)
               └──> Hunk-to-Hunk Jumping
               └──> Search Within Diffs
               └──> Word-Level Diff Highlighting

Working Tree Diff ──> Staged Diff View (same rendering, different git source)
                  └──> Hunk Staging ──> Line-Level Staging (line staging extends hunk staging)
                                    └──> Unstaging (reverse of staging operations)

Inline Annotations ──> Comments-to-Clipboard Export
```

## MVP Recommendation

**Phase 1 -- Core diff viewing (table stakes):**
1. File sidebar with git status indicators (M/A/D/R)
2. Inline diff view with syntax highlighting (tree-sitter)
3. Vim-style keyboard navigation (j/k, hunk jumping, file switching)
4. Working tree diff (unstaged) and staged diff views
5. Color-coded additions/deletions with line numbers

**Phase 2 -- Staging controls (primary differentiator from delta/difftastic):**
6. Stage/unstage entire files from sidebar
7. Hunk-level staging within diff view
8. Line-level staging with visual selection

**Phase 3 -- Advanced diff quality (core value proposition):**
9. Side-by-side diff view with toggle
10. Word-level diff highlighting
11. Syntax-aware diffing (tree-sitter AST comparison)
12. Semantic hunk boundaries

**Phase 4 -- Review workflow (unique features):**
13. Inline annotations/comments on diff lines
14. Comments-to-clipboard export (formatted for AI agents)
15. Mouse support
16. File review tracking

**Defer indefinitely:** Watch mode, config system, any anti-features listed above.

**Rationale:** The MVP must first be a competent diff viewer (Phase 1) before adding staging (Phase 2), because staging controls are meaningless if the diff display is poor. Advanced diff quality (Phase 3) is the long-term differentiator but requires Phase 1's tree-sitter foundation. Review workflow features (Phase 4) are the project's unique angle but require all prior phases.

## Feature Prioritization Matrix

| Feature | User Impact | Implementation Effort | Risk | Priority |
|---------|-------------|----------------------|------|----------|
| File sidebar + navigation | Critical | Medium | Low | P0 |
| Inline diff + syntax highlighting | Critical | Medium | Low | P0 |
| Working tree / staged diffs | Critical | Low | Low | P0 |
| Vim keybindings | Critical | Low | Low | P0 |
| Hunk jumping | High | Low | Low | P0 |
| Search in diffs | High | Low | Low | P1 |
| Stage/unstage files | High | Low | Low | P1 |
| Hunk staging | High | Medium | Medium | P1 |
| Line-level staging | High | High | High | P2 |
| Side-by-side view | High | High | Medium | P2 |
| Word-level diff highlighting | Medium | Medium | Low | P2 |
| Syntax-aware diffing | High | Very High | High | P2 |
| Semantic hunk boundaries | Medium | Very High | High | P3 |
| Inline annotations | Medium | Medium | Low | P3 |
| Clipboard export | Medium | Low | Low | P3 |
| Mouse support | Medium | Medium | Low | P3 |
| File review tracking | Low | Low | Low | P3 |
| Watch mode | Low | Medium | Low | Defer |

## Competitor Feature Analysis

| Feature | lazygit | gitui | tig | delta | difftastic | critique | revdiff | deff | **This Project** |
|---------|---------|-------|-----|-------|------------|----------|---------|------|-------------------|
| File list/sidebar | Yes | Yes | Yes | No (pager) | No (CLI) | Yes | Yes | Yes | **Yes** |
| Inline diff | Yes | Yes | Yes | Yes | Yes | Yes | Yes | No | **Yes** |
| Side-by-side diff | No | No | No | Yes | Yes | Yes | No | Yes | **Yes** |
| Syntax highlighting | Limited | Limited | No | Yes (bat) | No | Yes (tree-sitter) | Yes | Yes | **Yes (tree-sitter)** |
| Syntax-aware diff | No | No | No | No | **Yes** | No | No | No | **Yes** |
| Word-level diff | No | No | No | Yes | N/A (structural) | Yes | Yes | No | **Yes** |
| Hunk staging | Yes | Yes | Yes | No | No | Yes | No | No | **Yes** |
| Line staging | Yes | Yes | No | No | No | No | No | No | **Yes** |
| Inline comments | No | No | No | No | No | No | **Yes** | No | **Yes** |
| Clipboard for AI | No | No | No | No | No | No | Yes (stdout) | No | **Yes** |
| Mouse support | Yes | Yes | No | N/A | N/A | No | No | Yes | **Yes** |
| Vim keybindings | Yes | Partial | Yes | N/A | N/A | No | Yes | Yes | **Yes** |
| File review tracking | No | No | No | No | No | No | No | Yes | **Yes** |
| Git operations | Full | Full | Browse | None | None | Limited | None | None | **Stage only** |
| Watch mode | No | No | No | No | No | Yes | No | No | **No (defer)** |
| Standalone binary | Yes (Go) | Yes (Rust) | Yes (C) | Yes (Rust) | Yes (Rust) | No (Node) | Yes (Go) | Yes (Rust) | **Yes (Rust)** |

### Competitive Positioning

**The gap this project fills:** No existing tool combines all three of:
1. Syntax-aware diffing (difftastic has this but is CLI-only, no staging)
2. Interactive staging controls (lazygit/gitui have this but no syntax-aware diffs)
3. Review annotations for AI agent feedback (revdiff has annotations but no staging or syntax-aware diffs)

The closest competitor is **critique** (TUI, syntax highlighting, staging, split view) but it lacks syntax-aware diffing and the AI-output annotation workflow. **Revdiff** has the annotation/AI angle but lacks staging and syntax-aware diffs. **Deff** is the most similar in architecture (Rust TUI, side-by-side, file tracking) but lacks staging, annotations, and syntax-aware diffs.

## Sources

- [lazygit - GitHub](https://github.com/jesseduffield/lazygit)
- [gitui - GitHub](https://github.com/gitui-org/gitui)
- [tig - GitHub](https://github.com/jonas/tig)
- [delta - GitHub](https://github.com/dandavison/delta)
- [difftastic - GitHub](https://github.com/Wilfred/difftastic)
- [critique - GitHub](https://github.com/remorses/critique)
- [revdiff - GitHub](https://github.com/umputun/revdiff)
- [deff - Hacker News Show HN](https://news.ycombinator.com/item?id=47169518)
- [diffity - GitHub](https://github.com/kamranahmedse/diffity)
- [diffsitter - GitHub](https://github.com/afnanenayet/diffsitter)
- [Magit - Official Site](https://magit.vc/)
- [lazygit 2026 guide](https://www.heyuan110.com/posts/ai/2026-04-10-lazygit-guide/)
- [Practical lazygit walkthrough](https://www.bwplotka.dev/2025/lazygit/)
