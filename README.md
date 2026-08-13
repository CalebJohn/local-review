# local-review

A terminal-native git diff review tool. Navigate, stage, and discard changes without leaving your keyboard.

## Features

- **Hunk & line staging** — Stage individual hunks or select arbitrary line ranges.
- **Branch & commit review** — Diff against a base branch or between two commits, read-only.
- **Syntax highlighting** — Powered by tree-sitter. Supports Rust, TypeScript, JavaScript, Python, Go, C, C++, JSON, YAML, and TOML.
- **Live reload** — Automatically refreshes when files or the index change. No manual refresh needed.
- **Search** — Find text across the current diff. Navigate matches forward and backward.
- **Semantic filtering** — Hide formatting-only changes to focus on meaningful diffs.
- **Undo/redo** — Mistakes happen. Undo and redo staging, unstaging, and discard operations.
- **Editor integration** — Press `e` to edit the selected file in your `$EDITOR` or `$VISUAL`.
- **Mouse support** — Click to navigate, drag to select lines.
- **Vim-like keybindings** — `j`/`k` to move, `]`/`[` for hunks, `s`/`u` to stage/unstage.

## Disclaimer

This was a project for me to:

    1. Build something for my personal use

    2. Experiment with agentic develepment

You can see this reflected in the choice of features, as well as the messy git history where I experimented with "vibe coding" and other techniques. I'm putting this out there in case anywhere feels like they need a similar tool, but I will not be adding features or requests from the public. Feel free to fork and modify!

## Installation

```bash
cargo install --path .
```

Requires Rust 1.75+.

## Usage

Run local-review from within a git repository, invoked with `re`.

```
re                  Review staged and unstaged changes (default)
re <base>           Review all changes since diverging from <base> (includes uncommitted)
re <A>..<B>         Review changes between commits A and B
re <A> <B>          Same as A..B
re <A>...<B>        Review changes on B since it diverged from A
```

Review mode is read-only: the sidebar becomes a single file list (no Staged/Unstaged
sections) and staging, unstaging, and discard are disabled. `<base>` may be any
revspec — a branch name, tag, `HEAD~1`, or SHA. For a PR-style review of committed
changes only (no uncommitted or untracked files), use `re <base>...HEAD`, e.g.
`re main...HEAD`.

### Keybindings

| Key | Action |
|-----|--------|
| `j` / `k` | Move down / up |
| `]` / `[` | Next / previous hunk |
| `s` / `u` | Stage / unstage hunk |
| `S` / `U` | Stage / unstage file |
| `d` | Discard hunk or file |
| `v` | Enter visual mode (line selection) |
| `/` | Search forward |
| `w` | Toggle semantic filter |
| `e` | Open file in editor |
| `z` / `Z` | Undo / redo |
| `b` | Toggle sidebar |
| `f` | Toggle full file view |
| `q` | Quit |

## Limitations

- **Scroll overflow (u16 cap):** Internal scroll state uses `u16`, limiting diffs to ~65,000 rendered lines. Large files or diffs with many hunks may cause navigation issues. The ratatui `Paragraph::scroll()` API also uses `u16`, so this is a fundamental constraint.
