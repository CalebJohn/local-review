# Phase 2 Plan 02-01: Summary

**Completed:** 2026-04-23

## Tasks Completed

1. **Task 1: Add tree-sitter stack to Cargo.toml**
   - Added 13 tree-sitter dependencies at pinned versions
   - First build compiled all C grammars via `cc` (~2-5 minutes)
   - Deviation: `tree-sitter-javascript` uses `HIGHLIGHT_QUERY` (singular) not `HIGHLIGHTS_QUERY` (plural) — fixed in registry.rs

2. **Task 2: Scaffold src/syntax/ module**
   - Created: mod.rs, registry.rs, scope.rs, mapping.rs
   - Added `mod syntax;` to main.rs
   - Tests: `test_lang_for_extension_*`, `test_registry_contains_every_expected_language`, `test_smoke_parse_every_grammar`, `test_scope_to_style_*`

## Grammar Crate Versions Resolved

```
tree-sitter v0.25.10
tree-sitter-highlight v0.25.10
tree-sitter-rust v0.24.2
tree-sitter-typescript v0.23.2
tree-sitter-javascript v0.25.0
tree-sitter-python v0.25.0
tree-sitter-go v0.25.0
tree-sitter-c v0.24.2
tree-sitter-cpp v0.23.4
tree-sitter-json v0.24.8
tree-sitter-yaml v0.7.2
tree-sitter-toml-ng v0.7.0
```

## Smoke Test Results

All 11 grammars loaded and parsed successfully:
- rust, typescript, tsx, javascript, python, go, c, cpp, json, yaml, toml

## Test Summary

Total tests: 88 (Phase 1: 46 + 02-01 new: syntax tests)

## Artifacts Created

- `/workspace/Cargo.toml` — 13 tree-sitter entries
- `/workspace/src/syntax/mod.rs` — public types, highlight_source, lang_for_extension, build_styled_diff
- `/workspace/src/syntax/registry.rs` — HighlightRegistry, HIGHLIGHT_NAMES, registry()
- `/workspace/src/syntax/scope.rs` — scope_to_style mapping
- `/workspace/src/syntax/mapping.rs` — highlight_source_inner, emit_spans, compute_line_starts
- `/workspace/src/main.rs` — `mod syntax;` declaration

## Notes

- Cpp grammar (tree-sitter-cpp v0.23.4) works despite Assumption A1 MEDIUM-confidence flag
- No ABI mismatches detected in smoke test