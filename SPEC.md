# Semantic Diff Context

## Objective

Replace the fixed 3-line context in diff hunks with AST-aware context that shows
enclosing code structures when they're small enough to display in full. Add
breadcrumb-style ancestry chains to hunk headers so reviewers always know where
a change lives in the code structure.

**Target user:** Developer reviewing diffs in the TUI.

**Problem solved:** Fixed 3-line context clips meaningful structure boundaries. A
one-line change inside a 10-line function should show the whole function. Bare
`@@ -28,8 +28,9 @@` headers give no structural context — the reviewer has to
mentally map line numbers back to the source.

## Behavior

### Context expansion

For each hunk, parse the **new** file with tree-sitter and walk the AST to find
enclosing nodes around the changed lines. Apply these rules (first match wins):

| Enclosing node | Threshold | Action |
|----------------|-----------|--------|
| Function/method | ≤15 lines | Expand hunk to show entire function |
| Block (if/for/match/etc.) | ≤10 lines | Expand hunk to show entire block |
| TOML table/array-of-tables | ≤10 lines | Expand hunk to show entire table |
| Anything larger or no AST | — | Keep standard 3-line context |

"Lines" = node end line − node start line + 1, counted in the new file.

When a node qualifies, expand the hunk by adjusting `old_start`/`new_start` and
prepending/appending Equal lines from the file content to cover the full node.

When expansion causes two adjacent hunks to overlap or become contiguous, merge
them into a single hunk.

### Hunk header ancestry

The `@@` header includes a breadcrumb chain of all named scopes enclosing the
change, outermost to innermost:

```
@@ -28,8 +28,9 @@ MyClass > process
@@ -42,5 +42,6 @@ impl Display for Config > fmt
@@ -10,3 +10,4 @@ [dependencies]
```

Rules:
- Walk from changed lines up through AST ancestors
- Collect every **class**, **container** (impl/module), and **function** node
- Deepest ancestor: show its signature (source text from node start to body/block start, trimmed)
- Higher ancestors: show just the name
- Separator: ` > `
- No named scope → bare `@@` line (current behavior)
- Truncate signature portion at 80 chars

### Name extraction

| Node kind | Name source | Signature source |
|-----------|------------|------------------|
| Function | `name` field child | Node start → body child start (gives `fn foo(x: i32) -> bool`, `def bar(self):`, etc.) |
| Class/struct/enum | `name` field child | Same approach |
| Impl block (Rust) | `type` field child | `impl Type` or `impl Trait for Type` |
| TOML table | Key text | `[section]` or `[[array]]` |
| Block (if/for/etc.) | First line of node, trimmed before `{`/`:` | Not used in header (blocks aren't "named scopes") |

### Fallbacks

All produce standard 3-line context with no header annotation:
- Unknown file extension (no tree-sitter grammar)
- Tree-sitter parse failure
- File exceeds `MAX_HIGHLIGHT_BYTES` (256KB)
- No enclosing scope, or all enclosing scopes exceed thresholds

The header ancestry is still computed and shown even when the enclosing scope is
too large to expand — the header and expansion are independent.

## Per-language node types

### Categories

- **Function**: body expansion at ≤15 lines, appears in header chain
- **Block**: body expansion at ≤10 lines, does NOT appear in header chain
- **Class/Container**: header chain only, no body expansion (classes contain sub-structures; expanding them would show too much)

### Mappings

| Language | Function | Block | Class/Container |
|----------|----------|-------|-----------------|
| Rust | `function_item` | `if_expression`, `match_expression`, `for_expression`, `while_expression`, `loop_expression` | `impl_item`, `struct_item`, `enum_item`, `mod_item` |
| Python | `function_definition` | `if_statement`, `for_statement`, `while_statement`, `with_statement`, `try_statement` | `class_definition` |
| TypeScript/TSX | `function_declaration`, `method_definition`, `arrow_function` | `if_statement`, `for_statement`, `while_statement`, `switch_statement`, `try_statement` | `class_declaration` |
| JavaScript | `function_declaration`, `method_definition`, `arrow_function` | `if_statement`, `for_statement`, `while_statement`, `switch_statement`, `try_statement` | `class_declaration` |
| Go | `function_declaration`, `method_declaration` | `if_statement`, `for_statement` | `type_declaration` |
| C | `function_definition` | `if_statement`, `for_statement`, `while_statement`, `switch_statement` | `struct_specifier` |
| C++ | `function_definition` | `if_statement`, `for_statement`, `while_statement`, `switch_statement` | `struct_specifier`, `class_specifier` |
| TOML | — | `table`, `array_table` | — |
| JSON | — | — | — |
| YAML | — | — | — |

Node type names will be verified against actual tree-sitter grammar output during
implementation — these are best-effort from grammar documentation.

## Integration

### Data flow

```
compute_hunks(old, new, 3)              existing: raw hunks, 3-line context
        │
        ▼
expand_hunks(hunks, old, new, ext)      NEW: AST expansion + header annotation
        │
        ▼
DiffContent { hunks, ... }             existing: stored on App
```

`compute_diff_content` calls `expand_hunks` after `compute_hunks`.
`compute_full_diff_content` (full-file view) is **not affected**.

### Type changes

Add to `DiffHunk`:
```rust
pub header_context: Option<String>  // "MyClass > process" — rendered after @@
```

### New module

```
src/context/
    mod.rs           Public API: expand_hunks(hunks, old, new, ext) -> Vec<DiffHunk>
    ast.rs           tree_sitter::Parser wrapper, ancestor chain from line range
    node_types.rs    Per-language node type → category mapping, name extraction
    expansion.rs     Threshold checks, hunk bound adjustment, overlapping hunk merge
```

Uses `tree_sitter::Parser` with Language objects from existing grammar crates.
No new crate dependencies — `tree-sitter 0.25` is already in Cargo.toml.

### Rendering changes

`hunk_header_line` in `ui/diff_view.rs` appends `header_context` after `@@`:
```
@@ -28,8 +28,9 @@ MyClass > process
```

Style the ancestry portion in cyan (matches existing `@@` style).

### Staging compatibility

Expansion adds Equal lines and shifts `old_start`/`new_start` but does not change
which lines are Insert/Delete. `apply_hunk_to_content` works from individual line
entries and their `old_lineno` values, so it should remain correct. Must be
verified with an integration test.

## Testing strategy

### Unit tests (context/)

- Parse a Rust snippet, verify ancestor chain for a line inside a nested method
- Parse a Python snippet with `class Foo: def bar(self):`, verify `Foo > bar` header
- Function ≤15 lines → hunk expanded to full function bounds
- Function >15 lines → hunk unchanged, header still shows function name
- Block ≤10 lines → expanded; block >10 lines → not expanded
- Two hunks with overlapping expanded ranges → merged into one
- TOML table ≤10 lines → expanded
- Unknown extension → hunks returned unchanged
- Name extraction for each supported language (at least one test per language)

### Integration tests

- Stage an expanded hunk, verify index content matches expected
- `compute_full_hunks` output is unaffected by expansion code

### Manual testing

- Run TUI on this repo (Rust), verify headers show `impl > fn` ancestry
- Run on a Python project with classes, verify `MyClass > method` headers
- Verify small functions shown in full, large functions keep 3-line context
- Verify staging works on expanded hunks
- Verify hunk jump (n/N) still works correctly

## Boundaries

### Always
- Fall back to 3-line context when AST is unavailable or parse fails
- Preserve staging correctness — if expansion breaks staging, skip expansion
- Reuse existing Language/grammar objects from the syntax registry

### Ask first
- Adding new tree-sitter grammar crates beyond the current 11
- Changing expansion thresholds (15 for functions, 10 for blocks)
- Adding breadcrumb fold lines in the diff body (Option C — potential v2)
- Reusing the tree-sitter parse between highlighting and context expansion

### Never
- Shell out to git or external processes
- Use regex heuristics for structure detection — tree-sitter only
- Change `compute_full_hunks` behavior
- Break staging for any hunk
