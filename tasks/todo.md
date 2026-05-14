# Semantic Diff Context — Task List

## Phase 1: Foundation
- [x] **1.1** Add `header_context: Option<String>` to `DiffHunk`, update all construction sites
- [x] **1.2** Create `context/node_types.rs` — per-language node classification + name extraction
- [x] **1.3** Create `context/ast.rs` — tree-sitter parse + ancestor chain walk

**Checkpoint:** `cargo test` green, all Phase 1 unit tests pass → commit

## Phase 2: Expansion Logic
- [x] **2.1** Create `context/expansion.rs` — hunk bound adjustment + overlapping merge
- [x] **2.2** Create `context/mod.rs` — `expand_hunks` public API orchestration

**Checkpoint:** `cargo test` green, expansion works in isolation → commit

## Phase 3: Integration
- [x] **3.1** Wire `expand_hunks` into `compute_diff_content` (not `compute_full_diff_content`)
- [x] **3.2** Render `header_context` in `ui/diff_view.rs` hunk header line
- [x] **3.3** Staging compatibility integration test (expanded hunk stages correctly)

**Checkpoint:** `cargo test` all green, manual TUI verification, staging works → commit
