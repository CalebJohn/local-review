# Semantic Diff Context — Implementation Plan

## Dependency Graph

```
                    node_types.rs (language mappings)
                         │
                         ▼
DiffHunk.header_context ──→ ast.rs (parser, ancestor walk)
                         │          │
                         │          ▼
                         └──→ expansion.rs (threshold checks, hunk bound adjustment, merge)
                                    │
                                    ▼
                              context/mod.rs (expand_hunks public API)
                                    │
                                    ▼
                              diff/mod.rs (call expand_hunks from compute_diff_content)
                                    │
                                    ▼
                              ui/diff_view.rs (render header_context after @@)
```

Key dependencies:
- `node_types.rs` is a leaf — no deps on other new code
- `ast.rs` depends on `node_types.rs` for category lookups
- `expansion.rs` depends on `ast.rs` for ancestor info and `node_types.rs` for thresholds
- `context/mod.rs` orchestrates all three
- `diff/mod.rs` integration depends on the public API being stable
- UI rendering depends on the `header_context` field existing on `DiffHunk`

Existing code touched:
- `diff/types.rs` — add `header_context: Option<String>` to `DiffHunk`
- `diff/mod.rs` — call `expand_hunks` in `compute_diff_content`
- `ui/diff_view.rs` — render `header_context` in hunk header line
- `src/main.rs` — add `mod context;`

## Phases

### Phase 1: Foundation (node types + AST parsing)

Build the leaf modules that everything else depends on. Fully testable in isolation.

### Phase 2: Expansion Logic

The core algorithm: threshold checks, hunk bound adjustment, overlapping hunk merge. Depends on Phase 1.

### Phase 3: Integration

Wire `expand_hunks` into the diff pipeline and render the header context in the UI. Verify staging still works.

---

## Task Breakdown

### Phase 1: Foundation

#### Task 1.1: Add `header_context` field to `DiffHunk`

**What:** Add `pub header_context: Option<String>` to `DiffHunk` in `diff/types.rs`. Update all construction sites to set it to `None`.

**Why first:** Every subsequent task references this field. Getting it in early means later tasks compile incrementally.

**Acceptance criteria:**
- `DiffHunk` has the new field
- All existing code compiles (`cargo build`)
- All existing tests pass (`cargo test`)

**Verification:** `cargo test`

---

#### Task 1.2: Create `context/node_types.rs` — per-language node classification

**What:** New module with:
- `enum NodeCategory { Function, Block, ClassContainer }`
- `fn classify_node(lang: &str, node_kind: &str) -> Option<NodeCategory>`
- `fn expansion_threshold(category: &NodeCategory) -> u32` — returns 15 for Function, 10 for Block, 0 for ClassContainer (never expands)
- `fn extract_name(node: &tree_sitter::Node, source: &[u8]) -> Option<String>` — gets name from `name`/`type` child fields
- `fn extract_signature(node: &tree_sitter::Node, source: &[u8]) -> Option<String>` — node start to body/block start, trimmed, max 80 chars

**Acceptance criteria:**
- Covers all language×node-type mappings from the spec table
- Unit tests for each language: at least one snippet parsed, `classify_node` returns correct category
- `extract_name` tested for function, class, impl block, TOML table
- Unknown node kinds return `None`

**Verification:** `cargo test context::node_types`

---

#### Task 1.3: Create `context/ast.rs` — tree-sitter parsing + ancestor walk

**What:** New module with:
- `fn parse_source(source: &str, lang_name: &str) -> Option<tree_sitter::Tree>` — get Language from grammar crate, parse
- `fn ancestor_chain(tree: &Tree, source: &[u8], line_range: (u32, u32), lang: &str) -> AncestorInfo`
  - Walk from deepest node covering the line range up to root
  - Collect classified ancestors (Function/Block/ClassContainer)
  - Return innermost expanding candidate + header breadcrumb string

- `struct AncestorInfo { expand_to: Option<(u32, u32)>, header: Option<String> }`
  - `expand_to`: (start_line, end_line) of the smallest qualifying node, if any
  - `header`: `"MyClass > process"` breadcrumb string, if any named scopes exist

**Acceptance criteria:**
- Rust snippet: `impl Foo { fn bar() {} }` → header = `"Foo > bar"`, expand_to = bar's bounds
- Python snippet: `class Foo:\n  def bar(self):\n    pass` → header = `"Foo > bar"`
- Function >15 lines → `expand_to = None`, header still populated
- No parse → returns empty AncestorInfo
- Uses existing grammar crate Language objects (no new crate deps)

**Verification:** `cargo test context::ast`

---

### Phase 2: Expansion Logic

#### Task 2.1: Create `context/expansion.rs` — hunk expansion + merge

**What:** New module with:
- `fn expand_hunk(hunk: &DiffHunk, old_lines: &[&str], new_lines: &[&str], expand_to: (u32, u32)) -> DiffHunk`
  - Adjust `old_start`/`new_start`, prepend/append Equal lines to cover `expand_to` range
  - Preserve all existing Insert/Delete lines unchanged
- `fn merge_overlapping(hunks: Vec<DiffHunk>) -> Vec<DiffHunk>`
  - When two adjacent hunks overlap or are contiguous after expansion, merge into one
  - Deduplicate Equal lines in the overlap region

**Acceptance criteria:**
- Single hunk inside a 10-line function → expanded to cover full function (old_start/new_start adjusted, Equal lines prepended/appended)
- Two hunks that overlap after expansion → merged into single hunk with no duplicate lines
- Hunk at file start → no negative line numbers
- Hunk at file end → no out-of-bounds lines
- Insert/Delete lines are never modified by expansion

**Verification:** `cargo test context::expansion`

---

#### Task 2.2: Create `context/mod.rs` — public API `expand_hunks`

**What:** Orchestration function:
```rust
pub fn expand_hunks(hunks: Vec<DiffHunk>, old: &str, new: &str, extension: Option<&str>) -> Vec<DiffHunk>
```

Logic:
1. Determine lang from extension via `syntax::lang_for_extension`
2. If no lang or file too large (>256KB), return hunks unchanged
3. Parse `new` with tree-sitter
4. For each hunk: find changed line range, call `ancestor_chain`, set `header_context`, optionally expand
5. Call `merge_overlapping` on the result
6. Return

**Acceptance criteria:**
- Unknown extension → hunks returned unchanged, no `header_context`
- Known extension, small function → hunks expanded + header set
- Known extension, large function → hunks unchanged but header still set
- File >256KB → hunks returned unchanged
- Parse failure → hunks returned unchanged

**Verification:** `cargo test context`

---

### Phase 3: Integration

#### Task 3.1: Wire `expand_hunks` into `compute_diff_content`

**What:** In `diff/mod.rs`, after `compute_hunks`, call `context::expand_hunks` on the result before returning `DiffContent`.

Pass the file extension extracted from `path`.

**Acceptance criteria:**
- `compute_diff_content` now returns hunks with `header_context` populated for supported languages
- `compute_full_diff_content` is NOT affected (spec requirement)
- `cargo test` — all existing diff tests still pass
- New integration test: compute_diff_content on a small Rust file → verify header_context is set

**Verification:** `cargo test diff` + `cargo test context`

---

#### Task 3.2: Render `header_context` in UI

**What:** In `ui/diff_view.rs`, modify `hunk_header_line` to append `header_context` after the `@@` markers, styled in cyan.

```rust
fn hunk_header_line(old_start: u32, new_start: u32, header_context: Option<&str>, highlighted: bool) -> Line<'static>
```

**Acceptance criteria:**
- Header with context: `@@ -28 +28 @@ MyClass > process` (cyan styling on context portion)
- Header without context: `@@ -28 +28 @@` (unchanged from current)
- No layout shift — context appended inline on the same line

**Verification:** `cargo build` + manual TUI inspection

---

#### Task 3.3: Staging compatibility verification

**What:** Write an integration test that:
1. Creates a temp git repo with a Rust file containing a small function
2. Makes a one-line change inside the function
3. Computes diff with expansion (hunk covers entire function)
4. Stages the expanded hunk via `apply_hunk_to_content`
5. Verifies the result matches expected (only the one-line change applied, context lines untouched)

**Acceptance criteria:**
- Expanded hunk stages correctly — only Insert/Delete lines affect the output
- Equal lines added by expansion do not corrupt the staged content
- Test passes with `cargo test`

**Verification:** `cargo test staging_expanded_hunk` (or similar name)

---

## Checkpoints

| After | Gate | Action if fails |
|-------|------|-----------------|
| Phase 1 (Tasks 1.1–1.3) | `cargo test` passes, all new unit tests green | Fix before proceeding — Phase 2 depends on these APIs |
| Phase 2 (Tasks 2.1–2.2) | `cargo test` passes, expansion logic correct in isolation | Fix before wiring into main pipeline |
| Phase 3 (Tasks 3.1–3.3) | `cargo test` all green, manual TUI shows headers, staging works | Ship it |

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Tree-sitter node type names differ from spec table | Ancestor walk finds nothing | Task 1.2 tests verify against actual parser output; adjust mappings empirically |
| Expansion breaks staging | Data loss | Task 3.3 is a dedicated integration test; expansion only adds Equal lines |
| Performance regression on large files | Slow diff rendering | 256KB cap already exists; tree-sitter parse is fast (<5ms for typical files) |
| Overlapping hunk merge produces duplicate lines | Visual glitch | Task 2.1 specifically tests this case |
