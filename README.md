# local-review

A terminal-native git diff review tool. Navigate, stage, and discard changes without leaving your keyboard.

## Features

- **Hunk & line staging** — Stage individual hunks or select arbitrary line ranges.
- **Syntax highlighting** — Powered by tree-sitter. Supports Rust, TypeScript, JavaScript, Python, Go, C, C++, JSON, YAML, and TOML.
- **Live reload** — Automatically refreshes when files or the index change. No manual refresh needed.
- **Search** — Find text across the current diff. Navigate matches forward and backward.
- **Semantic filtering** — Hide formatting-only changes to focus on meaningful diffs.
- **Undo/redo** — Mistakes happen. Undo and redo staging, unstaging, and discard operations.
- **Editor integration** — Press `e` to edit the selected file in your `$EDITOR` or `$VISUAL`.
- **Mouse support** — Click to navigate, drag to select lines.
- **Vim-like keybindings** — `j`/`k` to move, `]`/`[` for hunks, `s`/`u` to stage/unstage.

## Installation

```bash
cargo install --path .
```

Requires Rust 1.75+.

## Usage

Run local-review from within a git repository, invoked with `re`.

```
re
```

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
