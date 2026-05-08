# Semantic Diff Boundaries

Align hunk boundaries to meaningful code structures (functions, blocks, classes) rather than arbitrary line ranges. When a change falls within a function, the hunk context should expand to include the function signature so the reviewer sees which function was changed.

This requires using tree-sitter to identify enclosing scope boundaries and adjusting hunk grouping after the initial diff computation. The `compute_hunks` function in `diff/mod.rs` currently uses `similar`'s `grouped_ops(3)` which groups by a fixed context line count with no structural awareness.

**Ref:** PROJECT.md requirement "Semantic diff boundaries -- hunks aligned to functions/blocks, not arbitrary lines"
