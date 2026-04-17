# Stack Research

**Domain:** Git Diff Review TUI
**Researched:** 2026-04-17
**Confidence:** HIGH

## Recommended Stack

### Core Technologies

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| Rust | 1.80+ (stable) | Language | Required by project constraints. Native tree-sitter/libgit2 bindings, single binary distribution, zero-cost abstractions for performant TUI rendering. |
| ratatui | 0.30.0 | TUI framework | The standard Rust TUI framework (fork of tui-rs). Mature widget system (List, Scrollbar, Table, Paragraph), modular workspace architecture since 0.30, active maintenance by Orhun and community. Re-exports crossterm for version alignment. No real competitor. |
| crossterm | 0.29.0 | Terminal backend | Ratatui's default backend. Cross-platform terminal manipulation (events, styling, alternate screen). Use via ratatui's re-export to avoid version mismatch. |
| git2 | 0.20.4 | Git operations | Rust bindings for libgit2. Structured access to repo state, index manipulation, diff generation, status queries. Avoids shelling out to git CLI. Actively maintained under rust-lang org. |
| tree-sitter | 0.25.x | Parsing engine | Incremental parsing for syntax-aware features. Use 0.25.x series (not 0.24 which is older, not 0.26 which is bleeding edge and language grammars lag behind). The 0.25 series has the best grammar ecosystem compatibility. |
| tree-sitter-highlight | 0.25.4 | Syntax highlighting | Official tree-sitter highlighting crate. Provides `Highlighter` and `HighlightConfiguration` for mapping parse tree nodes to highlight scopes. Must match tree-sitter major version (0.25.x). |

### Supporting Libraries

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| similar | 2.7.0 | Diff algorithm | Line-level and character-level diffing with Patience and Myers algorithms. Use for computing unified diffs from file content pairs. Dependency-free, battle-tested (powers insta snapshot testing). |
| arboard | latest | Clipboard access | Copy formatted comments to system clipboard. Prefer over copypasta -- more actively maintained, better Wayland support via wayland-data-control feature. |
| tui-scrollview | latest | Scrollable regions | Ratatui companion for scrollable content areas. Use for the diff view panels where content exceeds viewport. |
| tui-widget-list | latest | Virtual list widget | Efficient scrollable list for file sidebar. Handles large file lists without rendering all items. |
| unicode-width | 2.x | Text measurement | Correct terminal column width calculation for CJK, emoji, and other wide characters in diff content. |
| textwrap | latest | Text wrapping | Wrap long lines in inline diff view. Needed for side-by-side mode where each panel is half terminal width. |

### Tree-sitter Language Grammars

| Grammar | Crate | Notes |
|---------|-------|-------|
| Rust | tree-sitter-rust | Primary -- dogfooding language |
| TypeScript/JavaScript | tree-sitter-typescript, tree-sitter-javascript | High-priority targets |
| Python | tree-sitter-python | High-priority target |
| Go | tree-sitter-go | Common language |
| C/C++ | tree-sitter-c, tree-sitter-cpp | Common language |
| JSON/YAML/TOML | tree-sitter-json, tree-sitter-yaml, tree-sitter-toml | Config files appear in most diffs |

Grammars are compiled into the binary as static libraries. Add more as needed -- each is ~100-500KB in the binary. Match grammar versions to tree-sitter 0.25.x compatibility.

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| cargo | Build system | Standard Rust toolchain |
| cargo-watch | Dev iteration | `cargo watch -x run` for rapid development |
| cross | Cross-compilation | Build Linux/macOS/Windows binaries from one platform |
| cargo-release | Release management | Automate version bumps and crate publishing |
| bacon | Background checking | Continuous compilation checks, lighter than cargo-watch for large projects |

## Installation

```toml
[package]
name = "git-review-tui"
version = "0.1.0"
edition = "2021"
rust-version = "1.80"

[dependencies]
# TUI framework (includes crossterm backend by default)
ratatui = "0.30.0"

# Git operations
git2 = "0.20.4"

# Parsing and highlighting
tree-sitter = "0.25"
tree-sitter-highlight = "0.25.4"

# Language grammars (add as needed)
tree-sitter-rust = "0.24"
tree-sitter-javascript = "0.23"
tree-sitter-typescript = "0.23"
tree-sitter-python = "0.25"
tree-sitter-go = "0.23"
tree-sitter-json = "0.24"
tree-sitter-toml = "0.24"

# Diff computation
similar = "2.7"

# Clipboard
arboard = "3"

# Text handling
unicode-width = "0.2"

[dev-dependencies]
# None required initially

[profile.release]
lto = true
codegen-units = 1
strip = true
```

**Note on tree-sitter grammar versions:** Grammar crate versions do NOT match the tree-sitter core version. Each grammar has its own versioning. The key constraint is that grammars must be compiled against a compatible tree-sitter version -- use grammars that depend on tree-sitter ~0.25. Check each grammar's Cargo.toml `[dependencies]` section to verify compatibility before pinning versions.

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| ratatui 0.30 | cursive | Never for this project. Cursive is higher-level with built-in event loop but less control over rendering, weaker widget customization, smaller ecosystem. |
| git2 | gix (gitoxide) | If you need pure-Rust git (no C dependency). gix is newer, async-first, but API is less stable and documentation is thinner. git2 is proven and the diff/staging APIs are well-documented. Consider gix if libgit2 linking causes build issues. |
| similar | imara-diff | If diffing becomes a performance bottleneck. imara-diff uses histogram algorithm (10-100% faster than Myers on some workloads) but has a smaller API surface -- no built-in unified diff output. Use similar first, swap to imara-diff only if profiling shows diff computation is slow. |
| tree-sitter-highlight | syntect | If tree-sitter grammar availability is a problem. syntect uses TextMate grammars (broader language coverage) but lacks AST awareness needed for semantic diffing. Since tree-sitter is also needed for syntax-aware diff boundaries, using syntect for highlighting would mean maintaining two parsing systems. |
| arboard | copypasta | If arboard has platform issues. copypasta (by Alacritty team) is mature but less actively maintained. arboard has better Wayland support. |
| crossterm (via ratatui) | termion | Never. termion is Unix-only, crossterm is cross-platform. ratatui defaults to crossterm. |

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| tui-rs | Abandoned since 2023. Ratatui is the maintained fork. | ratatui |
| syntect (for highlighting) | Would create dual parsing pipeline -- tree-sitter for AST diffing + syntect for highlighting. Wasteful and complex. | tree-sitter-highlight |
| git CLI (shelling out) | Fragile parsing of text output, PATH dependency, no structured error handling, slower for batch operations. | git2 |
| tree-sitter 0.26.x | Too new (released Feb 2026). Language grammars lag behind -- many still target 0.24/0.25. Risk of incompatible grammar versions. | tree-sitter 0.25.x |
| tree-sitter 0.24.x | Older series. 0.25 has important improvements and better highlight crate support. | tree-sitter 0.25.x |
| diffsitter (as library) | It's a CLI tool, not a library. Architecture is not designed for embedding. Reimplement the AST-diff approach using tree-sitter + similar directly. | tree-sitter + similar |
| cursive | Different paradigm (dialog-based). Less control over custom rendering needed for side-by-side diff panels. Smaller widget ecosystem. | ratatui |
| termwiz | Facebook's terminal library. Less community adoption, fewer widgets, less documentation. | ratatui + crossterm |
| gix (for MVP) | API still stabilizing, documentation gaps. git2 is more predictable for index manipulation and diff generation. | git2 |

## Architecture Notes for Stack

### Tree-sitter Integration Pattern

Tree-sitter serves dual purposes in this project:

1. **Syntax highlighting** via tree-sitter-highlight -- map parse nodes to terminal colors
2. **Syntax-aware diff boundaries** -- use AST node boundaries (functions, blocks, structs) to align hunk boundaries instead of arbitrary line ranges

The approach: parse both old and new file versions with tree-sitter, compute diff with `similar`, then use tree-sitter node ranges to snap hunk boundaries to meaningful AST positions.

### git2 Integration Pattern

Key git2 APIs needed:
- `Repository::open()` -- open the repo
- `Repository::statuses()` -- file status for sidebar
- `Repository::diff_index_to_workdir()` -- unstaged changes
- `Repository::diff_tree_to_index()` -- staged changes  
- `Index::add_path()` / `Index::remove_path()` -- stage/unstage files
- `DiffOptions` for controlling diff context lines
- `Diff::foreach()` / `Diff::print()` for iterating hunks

For hunk-level and line-level staging: reconstruct patches manually by selectively applying hunk deltas to the index. This is the most complex git2 interaction -- gitui (the TUI git client) has battle-tested code for this pattern.

### Binary Size Considerations

Each tree-sitter grammar adds ~100-500KB to the binary. With 10 languages, expect ~2-5MB total from grammars. libgit2 static linking adds ~2-3MB. Total binary should be ~8-15MB with LTO and stripping, which is reasonable for a self-contained tool.

## Sources

- [Ratatui official site](https://ratatui.rs/) - framework docs, installation guide
- [Ratatui GitHub](https://github.com/ratatui/ratatui) - v0.30.0 release, workspace architecture
- [git2 on crates.io](https://crates.io/crates/git2) - v0.20.4
- [tree-sitter releases](https://github.com/tree-sitter/tree-sitter/releases) - v0.25.x/0.26.x release timeline
- [tree-sitter-highlight on crates.io](https://crates.io/crates/tree-sitter-highlight) - v0.25.4
- [similar on GitHub](https://github.com/mitsuhiko/similar) - v2.7.0, diff algorithms
- [imara-diff on GitHub](https://github.com/pascalkuthe/imara-diff) - performance-focused alternative
- [Syntax highlighting with tree-sitter (2025-03-30)](https://dotat.at/@/2025-03-30-hilite.html) - practical integration guide
- [ratatui-code-editor](https://github.com/vipmax/ratatui-code-editor) - ratatui + tree-sitter integration example
- [flamestro/deff](https://github.com/flamestro/deff) - similar project (side-by-side git diff TUI in Rust)
- [gitui](https://github.com/gitui-org/gitui) - reference implementation for git2 staging patterns
- [arboard on GitHub](https://github.com/1Password/arboard) - clipboard library

---
*Stack research for: Git Diff Review TUI*
*Researched: 2026-04-17*
