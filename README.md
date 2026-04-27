# Git Diff TUI

A terminal tool for reviewing git diffs with syntax highlighting and hunk-level staging. Built in Rust with Ratatui.

## Limitations

- **Scroll overflow (u16 cap):** Internal scroll state uses `u16`, limiting diffs to ~65,000 rendered lines. Large files or diffs with many hunks may cause navigation issues. The ratatui `Paragraph::scroll()` API also uses `u16`, so this is a fundamental constraint.

## Development

```bash
cargo build          # Build debug
cargo run            # Run (must be inside a git repo with changes)
cargo test           # Run tests
cargo clippy         # Lint
```
