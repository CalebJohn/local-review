# Syntax-Aware Diffing — Implementation Plan

## Dependency Graph

```
                    ┌─────────────────┐
                    │  1. Data model   │
                    │  (diff/types.rs) │
                    └────────┬────────┘
                             │
                    ┌────────▼────────┐
                    │  2. Token        │
                    │  extraction      │
                    │  (classify/)     │
                    └────────┬────────┘
                             │
                    ┌────────▼────────┐
                    │  3. Canonical    │
                    │  compare         │
                    │  (classify/)     │
                    └────────┬────────┘
                             │
                    ┌────────▼────────┐
                    │  4. classify_diff│
                    │  public API      │
                    │  (classify/)     │
                    └────────┬────────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
     ┌────────▼──────┐ ┌────▼─────┐ ┌──────▼──────┐
     │ 5. Dimming in │ │ 6. App   │ │ 7. Hunk     │
     │ diff_lines()  │ │ state +  │ │ helpers     │
     │ (ui.rs)       │ │ messages │ │ is_format.. │
     └───────────────┘ │ (app.rs) │ └──────┬──────┘
                       └────┬─────┘        │
                            │       ┌──────▼──────┐
              ┌─────────────┼───────│ 8. Toggle   │
              │             │       │ hide hunks  │
              │             │       └──────┬──────┘
     ┌────────▼──────┐      │       ┌──────▼──────┐
     │ 9. Bulk stage │      │       │10. Footer + │
     │ formatting    │      │       │ hunk count  │
     │ (app.rs)      │      │       │ (ui.rs)     │
     └───────────────┘      │       └─────────────┘
                            │
                    ┌───────▼──────┐
                    │11. Integration│
                    │ wire-up      │
                    │ (app.rs)     │
                    └──────────────┘
```

## Design Decisions

### Tree-sitter parse sharing

The spec says classification and syntax highlighting should share parse trees. Currently, `highlight_source_inner` uses `tree-sitter-highlight`'s `Highlighter` which owns its parsing internally — we can't extract a `Tree` from it.

**Decision:** Classification uses `tree-sitter::Parser` directly (not `tree-sitter-highlight`) to get `Tree` objects, then walks leaf nodes for token extraction. This is a separate parse from highlighting, but both are fast and already gated by the 256KB cap. Sharing parse trees would require refactoring the entire syntax module away from `tree-sitter-highlight`, which is out of scope. The spec's suggestion is aspirational — two parses per file (one for highlighting, one for classification) is acceptable for v1.

### Language resolution

Classification needs a `tree_sitter::Language` from the file extension. The registry currently stores `HighlightConfiguration` objects. We'll add a parallel mapping from extension to `tree_sitter::Language` in the classify module (reusing `lang_for_extension` for the name lookup, then mapping name to Language).

### Where classification runs

Classification mutates `DiffLine::formatting_only` in place. It runs in `App::load_diff_for_selected()` after `compute_diff_content` / `compute_full_diff_content` but before `build_styled_diff`. The hunks are already owned by `DiffContent` at that point, so we pass `&mut hunks`.

## Phases

### Phase 1: Foundation (Tasks 1–3)
Data model changes and the core classification engine. No UI changes. Fully testable in isolation.

**Checkpoint:** `cargo test` passes. classify module has unit tests covering all spec cases. Existing tests unaffected since `formatting_only` defaults to `false`.

### Phase 2: UI Integration (Tasks 4–7)
Wire classification into the app, render dimming, implement toggle and hunk hiding.

**Checkpoint:** `cargo run` shows dimmed formatting-only lines. `w` toggles hiding. Footer shows hunk counts.

### Phase 3: Bulk Operations (Tasks 8–9)
Bulk stage formatting hunks, sidebar indicators.

**Checkpoint:** `W` stages formatting hunks across all unstaged files. Full manual test pass.

## Risk Areas

1. **Tree-sitter Language access:** We need `tree_sitter::Language` objects to create parsers. The grammar crates expose these (e.g., `tree_sitter_rust::LANGUAGE`), so this is straightforward but requires a match table.

2. **Token extraction correctness:** The core algorithm hinges on walking tree-sitter ASTs and extracting leaf-node text. Edge cases: comments as tokens (spec says comment *content* changes are semantic), empty files, files that fail to parse.

3. **Hunk staging batch:** `StageFormattingHunks` iterates multiple files and hunks. Must stage hunks in reverse order within a file to avoid line-number shifts. Single undo snapshot for the batch.
