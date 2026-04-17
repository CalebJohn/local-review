# Project Research Summary

**Project:** Git Diff Review TUI
**Domain:** Terminal-based git diff review and staging tool
**Researched:** 2026-04-17
**Confidence:** HIGH

---

## Executive Summary

This project is a Rust TUI tool for reviewing and staging local git changes. The product niche is well-defined: no existing tool combines syntax-aware diffing (only difftastic/diffsitter, both CLI-only), interactive staging at hunk/line granularity (lazygit/gitui have this but no syntax-aware diffs), and review annotations formatted for AI coding agents (revdiff has annotations but lacks staging). The gap is real and the implementation path is clear because gitui (Rust, ratatui, git2) is an open-source near-reference implementation for the git and staging layers.

The recommended stack is highly constrained by the project's own requirements (Rust, ratatui, git2, tree-sitter) and research confirms these are the right choices — ratatui 0.30 is the sole maintained Rust TUI framework, git2 0.20.4 is the only practical choice for programmatic git operations in Rust, and tree-sitter 0.25.x is the correct series (0.24 is older, 0.26 is too new with grammar compatibility lag). The Modified Elm Architecture (TEA) pattern is the recommended architecture, giving clean testable state transitions via a central Message enum and update() function — critical when staging operations mutate git state that multiple UI panels depend on.

The primary technical risks are well-understood: line-level staging requires custom blob reconstruction (not a simple extension of hunk-level staging), side-by-side synchronized scrolling requires a unified diff-row data model designed upfront, and tree-sitter must parse complete source files rather than diff fragments. All three pitfalls are architectural — they must be addressed in the initial design, not retrofitted. The build order maps cleanly: git layer and domain types first, basic rendering second, staging controls third, syntax highlighting fourth, advanced features (syntax-aware diffs, comments) fifth.

---

## Key Findings

### Recommended Stack

| Technology | Version | Role |
|------------|---------|------|
| Rust | 1.80+ | Language — required by constraints, single-binary distribution |
| ratatui | 0.30.0 | TUI framework — sole maintained Rust TUI, active ecosystem |
| crossterm | 0.29.0 | Terminal backend — use via ratatui re-export to avoid version mismatch |
| git2 | 0.20.4 | Git operations — libgit2 bindings, proven staging/diff APIs |
| tree-sitter | 0.25.x | Parsing engine — 0.25 series has best grammar ecosystem compatibility |
| tree-sitter-highlight | 0.25.4 | Syntax highlighting — must match tree-sitter major version |
| similar | 2.7.0 | Diff computation — line-level and character-level, Patience + Myers algorithms |
| arboard | latest | Clipboard — better Wayland support than alternatives |

Critical version notes: avoid tree-sitter 0.26.x (grammar lag) and 0.24.x (older). Use tui-scrollview and tui-widget-list as ratatui companions for scrollable diff and file list regions. Grammar crates (tree-sitter-rust, tree-sitter-python, etc.) have independent versioning — verify each targets tree-sitter 0.25.x ABI before pinning.

### Expected Features

**Table stakes (P0 — must ship or the tool feels broken):**
- File sidebar with git status indicators (M/A/D/R)
- Inline unified diff view with color-coded additions/deletions
- Syntax highlighting via tree-sitter
- Vim-style keyboard navigation (j/k, hunk jumping, file switching)
- Working tree diff (unstaged) and staged diff views
- Hunk-to-hunk jumping
- Search within diffs

**Differentiators (P1-P2 — where the product identity lives):**
- Hunk-level and line-level staging (interactive staging controls separate this from delta/difftastic)
- Side-by-side diff view with synchronized scrolling
- Word-level diff highlighting
- Syntax-aware diffing using tree-sitter ASTs (only difftastic does this; it is CLI-only with no staging)
- Semantic hunk boundaries aligned to function/block AST nodes

**Unique angle (P3 — the review workflow features):**
- Inline annotations/comments on diff lines
- Comments-to-clipboard export formatted for AI coding agents (the project's specific differentiator)
- File review tracking (per-file "reviewed" toggle)
- Mouse support (enhancement, not primary)

**Anti-features (explicitly out of scope):**
- Commit creation, branch management, remote operations — scope creep toward general git TUI
- Persistent state/database — git index is the review artifact
- Watch mode, extensive configuration — good defaults over configurability for v1
- AI-powered diff explanation — output to agents (clipboard), not input from AI

**Competitive gap:** No tool combines all three of syntax-aware diffing + interactive staging + AI annotation output. The closest is critique (TUI, staging, split view) but lacks syntax-aware diffs and the AI output workflow.

### Architecture Approach

**Pattern:** Modified Elm Architecture (TEA) — Model, Message enum, update() function, view(). Component Architecture (trait-based OOP) is not recommended because the panels are tightly coupled: sidebar selection drives diff view content, staging operations update both the file list and the diff.

**Component map:**
- Event Loop: poll crossterm events, map to Messages, drive tick timer
- App State (Model): central state — file list, selected file, diff data, view mode, staging selections, comments
- update(): processes Messages, mutates Model, triggers git/syntax side effects
- Sidebar Widget: renders file tree with status icons
- Diff View Widget: renders inline or side-by-side with highlight overlay
- Status Bar: mode, file path, help hints
- Comment Overlay: inline comment editor, clipboard export
- Git Layer (git2): opaque service — no git2 types leak into Model or UI
- Syntax Layer (tree-sitter): highlight token generation, semantic diff alignment

**Key architectural decisions baked in by the research:**
1. Git Layer is an opaque service returning owned domain types (DiffResult, Hunk, DiffLine) — git2 types have complex lifetimes and are not Send; leaking them causes lifetime spaghetti
2. Diff computation is lazy and cached — compute on file selection, invalidate on staging operations; never eager for all files
3. Syntax highlighting is an overlay, not embedded in diff data — parse complete source files (not diff fragments), store highlight spans separately from DiffLine data
4. Side-by-side view uses a unified diff-row data model — each row contains (left_line | gap, right_line | gap); scroll operates on rows, not per-panel lines
5. Line-level staging uses blob reconstruction — write a synthetic blob with selected lines applied, update index entry OID

**Recommended project structure:**
```
src/
  main.rs, app.rs, event.rs
  ui/   (sidebar, diff_view, diff_line, status_bar, comment, theme)
  git/  (repo, diff, staging)
  syntax/ (highlight, language, semantic_diff)
  model/  (file_entry, diff_data, selection, comment)
```

### Critical Pitfalls

**Pitfall 1 — Line-level staging requires blob reconstruction, not apply() (Critical — Phase 1-2)**
git2-rs has no native line-level staging API. `Repository::apply()` operates at hunk level. Line-level staging requires constructing a synthetic blob from the selected lines and updating the index entry OID. Build a unified `StagingEngine` abstraction handling file/hunk/line from the start. Reference: gitui's implementation. This is not a safe deferral.

**Pitfall 2 — Side-by-side synchronized scrolling requires unified diff-row model (Critical — Phase 1 design)**
Ratatui has no synchronized scrolling primitive. Naive same-offset scrolling on two independent panels produces misaligned diffs. Design the diff-row data model (each row = left side + right side, with gap entries) before building any renderer. This is a Phase 1 architecture decision even though side-by-side ships later.

**Pitfall 3 — Tree-sitter must parse complete source files, not diff fragments (Critical — Phase 1)**
Diff hunks are code fragments that produce ERROR nodes in tree-sitter. The correct approach: parse full old and new file versions, map diff line numbers back to source line numbers, extract highlight spans from full-file parse. Must be designed correctly from the first syntax highlighting implementation.

**Pitfall 4 — Terminal state corruption on panic (High — Phase 1, first task)**
A panic while in raw mode + alternate screen leaves the terminal broken. Use `ratatui::init()` (v0.28.1+) which installs a panic hook that restores terminal state. Also handle SIGINT/SIGTERM.

**Pitfall 5 — Index lock contention with external git operations (High — Phase 2)**
Re-read the index immediately before each write (`Index::read(force: true)`). Handle lock errors with a user-visible message. Do not cache the index across interactions.

**Additional pitfalls:**
- Hunk line offsets shift after partial staging — re-diff after each op, don't adjust offsets manually
- Comments anchored to line numbers go stale after staging — anchor to content hash + context
- Grammar version conflicts — verify each tree-sitter-* grammar targets 0.25.x ABI
- Keybinding conflicts (vim `s`/`d` vs stage/discard) — map stage to Space/Enter, require confirmation for destructive ops
- Side-by-side unusable below ~120 columns — auto-detect and fall back to inline with a clear message

---

## Implications for Roadmap

### Phase Suggestions with Rationale

**Phase 1: Foundation + Core Diff Viewing**

Establishes the TEA architecture skeleton, terminal panic handling, git layer, domain types, and delivers a working TUI with file sidebar, inline syntax-highlighted diff rendering, and keyboard navigation. No staging yet — validate the data model first.

Features: file sidebar + git status, inline diff view, syntax highlighting (full-file parsing), vim keybindings, hunk jumping, working tree and staged diff views, search within diffs.

Architecture work required in this phase: TEA skeleton, terminal init with panic hook (Pitfall 4), Git Layer returning owned types, unified diff-row data model (designed here even though side-by-side ships in Phase 3), modal input dispatch (to prevent keybinding spaghetti as modes multiply), all rendering widgets (sidebar, inline diff view, status bar), full syntax layer (full-file parsing, language detection, highlight overlay).

**Phase 2: Staging Controls**

Delivers the core review mechanism — stage/unstage at file, hunk, and line granularity. This is what distinguishes the tool from pure diff viewers.

Features: stage/unstage entire files, hunk-level staging, line-level staging (visual select), unstage operations.

Architecture work: StagingEngine abstraction (file/hunk/line uniform interface), blob reconstruction for line-level staging (Pitfall 1), index refresh before writes (Pitfall 5), visual feedback after staging (transient status messages, diff refresh), visual line selection mode.

**Phase 3: Advanced Diff Quality**

Delivers the long-term differentiators — side-by-side view, word-level highlighting, and the novel syntax-aware diffing features. Tree-sitter foundation from Phase 1 makes this buildable.

Features: side-by-side diff view with toggle, word-level diff highlighting, syntax-aware diffing (tree-sitter AST comparison), semantic hunk boundaries aligned to functions/blocks.

Architecture work: side-by-side renderer using diff-row model from Phase 1, narrow terminal fallback (auto-switch below ~120 cols), word-level diff via similar at character granularity, semantic_diff.rs (AST comparison, hunk boundary alignment), viewport-only styled line construction (avoid 10K-line allocation for offscreen content).

**Phase 4: Review Workflow Features**

Delivers the unique annotation and AI output workflow, plus mouse support and file review tracking.

Features: inline comments on diff lines, comments-to-clipboard formatted for AI agents, file review tracking (ephemeral per-session), mouse support.

Architecture work: comment model (anchor to content hash, not line number), Comment Overlay widget, clipboard export (arboard + stdout fallback + OSC 52 for tmux), file review tracking state in Model, crossterm mouse event handling.

### Phase Ordering Rationale

- Foundation before staging: git layer and domain types are the data source for everything; staging controls are meaningless if diff display is wrong
- Diff-row data model in Phase 1: even though side-by-side ships in Phase 3, designing this model early means the inline renderer and all subsequent work uses the correct structure — retrofitting is expensive
- Syntax highlighting in Phase 1: the tree-sitter foundation is needed for Phase 3's syntax-aware diffing; getting the full-file parsing architecture right early avoids a rewrite
- Staging before advanced diff quality: staging is the core review mechanism and separates this tool from CLI alternatives; having it solid before adding AST-level features keeps scope manageable
- Comments last: depend on a stable diff model for content-hash anchoring; lower priority than staging and diff quality

### Research Flags

**Needs deeper research during planning (use /gsd-research-phase):**
- Phase 2 — Line-level staging edge cases: read gitui's staging.rs before writing milestones; edge cases (deletion-only selections, multi-hunk spanning selections, unstaging partial lines) need detailed investigation
- Phase 3 — Syntax-aware diffing scope: decide between full structural diff (difftastic's Dijkstra-based AST diff, expensive) vs. hunk-boundary snapping (snap existing boundaries to nearest AST node, simpler) before spec'ing milestones; this choice affects complexity significantly
- Phase 3 — Semantic hunk boundaries: novel feature with no existing interactive reference implementation; may need a research spike to validate the approach is achievable before committing to a milestone

**Standard patterns (skip research, well-documented):**
- Phase 1 git layer: well-documented git2 APIs, gitui as direct reference
- Phase 1 TEA architecture: documented in ratatui official docs with examples
- Phase 1 tree-sitter highlighting: tree-sitter-highlight crate well-documented; full-file parsing pattern is established
- Phase 2 hunk-level staging: well-documented in gitui
- Phase 4 clipboard/comments: standard arboard usage, low risk

---

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All libraries have clear recommended versions. ratatui/git2/tree-sitter are mature. The choices are obvious given project constraints; no real alternatives to evaluate. |
| Features | HIGH | Thorough competitive analysis across 9 tools. Table stakes and differentiators are grounded in what competitors do and don't do. The unique positioning (syntax-aware + staging + AI annotations) is validated by the gap analysis. |
| Architecture | HIGH | TEA pattern is recommended by ratatui's own docs. Key patterns (opaque git service, lazy diff computation, highlight overlay, blob reconstruction, diff-row model) are derived from production code (gitui) or well-understood constraints (ratatui scrolling primitives). |
| Pitfalls | HIGH | All critical pitfalls are grounded in specific library limitations with upstream issues cited (git2-rs #589, ratatui #174, libgit2 #4230, tree-sitter #3095). Not speculative — these are confirmed limitations. |

**Gaps to address during planning:**
- Line-level staging edge cases: read gitui staging.rs before Phase 2 milestone spec
- Syntax-aware diffing scope decision: full structural diff vs. boundary snapping — needs a decision before Phase 3 spec
- Grammar version pinning: verify each tree-sitter grammar crate targets 0.25.x ABI before finalizing Cargo.toml
- Clipboard in tmux: OSC 52 escape sequence support needs testing in Phase 4 spike

---

## Sources

**Stack research:**
- Ratatui official site and GitHub (v0.30.0)
- git2 on crates.io (v0.20.4)
- tree-sitter releases (v0.25.x/0.26.x timeline)
- tree-sitter-highlight on crates.io (v0.25.4)
- similar on GitHub (v2.7.0)
- gitui repository (staging patterns reference)
- arboard on GitHub
- Syntax highlighting with tree-sitter (2025-03-30, dotat.at)
- ratatui-code-editor (ratatui + tree-sitter integration example)

**Feature research:**
- lazygit, gitui, tig, delta, difftastic, critique, revdiff, deff, diffity, diffsitter, magit (competitive analysis)

**Architecture research:**
- Ratatui Component Architecture and Elm Architecture docs
- GitUI asyncgit module and staging implementation
- git2-rs Line Staging Issue #589
- Difftastic (structural diff approach)
- git2 DiffOptions docs

**Pitfalls research:**
- ratatui rendering discussion #579, scrollable widgets RFC #174, panic hooks recipe
- git2-rs line staging issue #589
- libgit2 index locking #809, threading docs, status performance #4230
- tree-sitter syntax highlighting docs, versioning conflicts #3095
- crossterm unicode issues #561
