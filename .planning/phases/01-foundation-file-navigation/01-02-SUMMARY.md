---
plan: 01-02
phase: 01-foundation-file-navigation
status: complete
tasks_completed: 2
tasks_total: 2
---
# Plan 01-02 Summary

## Objective
Build the complete TUI application: TEA architecture (App model + Message + update), keyboard-driven UI with sidebar file list and color-coded inline diff view, and clean terminal management. After this plan the binary is usable end-to-end: launch in a git repo, browse files, view diffs with colors and line numbers, quit cleanly.

## Tasks Completed

### Task 1: TEA App Model + Event Loop
- Created `src/app.rs` with `Focus` (Sidebar/DiffView), `Message` (MoveUp/MoveDown/SelectFile/ScrollDiffUp/ScrollDiffDown/SwitchFocus/Quit), and `App { repo, files, selected_index, diff_content, diff_scroll, focus, should_quit }`
- `App::new()` opens repo at `.`, loads `changed_files()`, auto-selects first file and loads its diff
- `load_diff_for_selected()` branches on `FileEntry::is_staged_only()`:
  - staged-only -> `head_content` vs `index_content`
  - unstaged    -> `index_content` vs `workdir_content`
- Handles `ContentResult::Binary` on either side by producing `binary_diff_content(path)` sentinel; errors produce `None`; `NotFound` -> empty string for diff computation
- `update()` dispatches all messages; Sidebar moves auto-load a diff preview; scroll clamped via `total_diff_lines`
- Rewrote `src/main.rs` to use `ratatui::init()` / `ratatui::restore()` and an event loop filtering on `KeyEventKind::Press` (avoids 2-3x duplicate events on some terminals). Key maps differ per focus (q/j/k/Enter/Tab for Sidebar, q/j/k/Tab/Esc for DiffView)
- 10 unit tests: staged/unstaged branching, binary sentinel, move down/clamp/up, quit, focus toggle, scroll-up-at-zero

### Task 2: UI Rendering
- Created `src/ui.rs` with `pub fn view(frame, app)` using `Layout::horizontal([Length(30), Min(1)])`
- Sidebar: `List` + `ListState` for selection; each row is `"{status} {path}"` with per-status fg color (M=Yellow, A=Green, D=Red, R=Cyan, ?=DarkGray); highlight style uses DarkGray bg + BOLD
- Diff view:
  - Hunk headers `@@ -old +new @@` in Cyan
  - Per-line prefix `+`/`-`/` ` with fg Green/Red/default; line numbers formatted `{old:>4} {new:>4} ` in DarkGray (blanks when missing)
  - Trailing newline stripped from `DiffLine.content`
  - `Paragraph::new(lines).block(block).scroll((app.diff_scroll, 0))`
  - Binary guard: renders "Binary file (not shown)" (Yellow, centered) when `DiffContent.is_binary` is true -- never attempts to render hunks
  - None content: centered "Select a file to view diff"
- Border color reflects focus (Blue focused, DarkGray unfocused) on both panels

## Key Files
- created: src/app.rs
- created: src/ui.rs
- modified: src/main.rs

## Self-Check
- `cargo test` -> 46 passed, 0 failed (36 pre-existing + 10 new in `app::tests`)
- `cargo build` -> completes cleanly (one dead-code warning on `FileStatus` fields, pre-existing behaviour)
- Event loop filters `KeyEventKind::Press`; uses `ratatui::init`/`restore`; uses `ratatui::crossterm::event` re-export (no version drift)
- Acceptance criteria for both tasks met: App struct/enums/update; Message variants; Focus enum; ContentResult::Binary handling; KeyEventKind::Press filter; two-panel layout with Length(30) sidebar; Green/Red additions/deletions; `{old:>4} {new:>4}` line numbers; `@@` hunk header in Cyan; binary "Binary file (not shown)" render; `Paragraph::scroll((diff_scroll, 0))`; border color varies by focus; status indicator coloring

## Deviations
- `cargo build --release` fails in this sandbox with an openssl-sys / pkg-config missing error when compiling the release profile's git2 transitive deps. This is an environment issue (no `pkg-config` or `libssl-dev` and no sudo/apt access in the sandbox), not a code issue -- debug build and all tests succeed. No code-level deviation from the plan.
