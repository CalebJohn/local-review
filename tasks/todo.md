# Syntax-Aware Diffing — Task List

## Phase 1: Foundation

### Task 1: Data model changes
**Files:** `src/diff/types.rs`
**Do:**
- Add `formatting_only: bool` field to `DiffLine` (default `false`)
- Add `is_formatting_only()` method to `DiffHunk` — true when every non-Equal line has `formatting_only = true`
- Add `is_mixed()` method to `DiffHunk` — true when hunk has both formatting and semantic changed lines
- Update all `DiffLine` construction sites to include `formatting_only: false`

**Accept when:**
- [ ] `cargo build` compiles with no errors
- [ ] `cargo test` — all existing tests pass unchanged (new field defaults to false)
- [ ] Unit tests for `is_formatting_only()` and `is_mixed()` pass

**Verify:** `cargo test && cargo clippy`

---

### Task 2: Token extraction from tree-sitter ASTs
**Files:** new `src/classify/mod.rs`, new `src/classify/canonical.rs`
**Do:**
- Create `classify/` module
- Build a `language_for_extension(ext) -> Option<tree_sitter::Language>` function that maps file extensions to tree-sitter Language objects (reuse the same extensions from `syntax/mod.rs::lang_for_extension`)
- Implement `extract_tokens(source: &str, lang: tree_sitter::Language) -> HashMap<u32, Vec<String>>` — parse source with `tree_sitter::Parser`, walk the tree, collect leaf-node text per 1-based line number, skip whitespace-only nodes
- Gate on `MAX_HIGHLIGHT_BYTES` (256KB)

**Accept when:**
- [ ] Parsing a Rust snippet produces correct tokens keyed by line number
- [ ] Whitespace-only nodes (spaces, newlines) are excluded
- [ ] Files exceeding 256KB return None / empty map
- [ ] Unknown language returns None

**Verify:** `cargo test classify`

---

### Task 3: Canonical comparison and classify_diff API
**Files:** `src/classify/mod.rs`, `src/classify/canonical.rs`
**Do:**
- Implement `canonicalize(tokens: &[String]) -> String` — strips trailing commas, normalizes quote characters (`'` -> `"`), concatenates
- Implement change-group identification: walk hunk lines, find maximal runs of non-Equal lines, separate Delete (old-side) and Insert (new-side) within each group
- Implement `classify_diff(hunks: &mut [DiffHunk], old_content: &str, new_content: &str, extension: Option<&str>)` — public API that mutates `formatting_only` in place
- Pure insertion groups (all Insert, no Delete) and pure deletion groups (all Delete, no Insert) are always semantic
- No grammar available -> return early, all lines remain semantic

**Accept when:**
- [ ] Whitespace-only change: classified as formatting
- [ ] Line split (one line -> many, same tokens): formatting
- [ ] Line join (many -> one): formatting
- [ ] Trailing comma add/remove: formatting
- [ ] Quote normalization (`'` to `"`): formatting
- [ ] Variable rename: semantic
- [ ] Added statement: semantic
- [ ] Comment text change: semantic
- [ ] Mixed group (formatting + semantic): entire group semantic (conservative)
- [ ] No grammar: all lines remain semantic
- [ ] Pure insertion/deletion: semantic
- [ ] Large file (>256KB): skips classification

**Verify:** `cargo test classify`

---

## Phase 2: UI Integration

### Task 4: Wire classification into App
**Files:** `src/app.rs`, `src/main.rs` (mod declaration)
**Do:**
- Add `mod classify;` to `main.rs`
- In `App::load_diff_for_selected()`, after diff computation and before `build_styled_diff`, call `classify_diff()` on the mutable hunks
- Pass the file extension derived from the path

**Accept when:**
- [ ] `cargo build` compiles
- [ ] Running the app on a repo with formatting changes: `DiffLine::formatting_only` is correctly set (verify via debug logging or test)
- [ ] No visible UI change yet (dimming is next task)

**Verify:** `cargo test && cargo run` (in a test repo with formatting changes)

---

### Task 5: Dimmed rendering for formatting-only lines
**Files:** `src/ui.rs`
**Do:**
- In `diff_lines()`, when a DiffLine has `formatting_only = true` and kind is Insert or Delete, apply `Modifier::DIM` to the content style (dimmed green / dimmed red)
- Equal lines are never dimmed (they aren't changes)

**Accept when:**
- [ ] Formatting-only Insert lines render as dimmed green
- [ ] Formatting-only Delete lines render as dimmed red
- [ ] Semantic lines render normally (no change)
- [ ] Equal lines unaffected

**Verify:** `cargo run` — visual inspection with a repo containing formatting changes

---

### Task 6: App state and messages for semantic filter toggle
**Files:** `src/app.rs`, `src/main.rs`
**Do:**
- Add `semantic_filter: bool` field to `App` (default `false`)
- Add `ToggleSemanticFilter` variant to `Message` enum
- Add `StageFormattingHunks` variant to `Message` enum
- Handle `ToggleSemanticFilter` in `App::update()` — flip the bool
- Bind `w` key to `ToggleSemanticFilter` in `main.rs` (both Sidebar and DiffView focus modes)
- Bind `W` key to `StageFormattingHunks` in `main.rs` (both focus modes)

**Accept when:**
- [ ] Pressing `w` toggles `semantic_filter` state
- [ ] Message variants compile and dispatch correctly
- [ ] No behavior change yet (hiding and bulk staging are separate tasks)

**Verify:** `cargo build && cargo test`

---

### Task 7: Toggle hides pure-formatting hunks
**Files:** `src/ui.rs`, `src/app.rs`
**Do:**
- In `diff_lines()`, when `semantic_filter` is true, skip rendering hunks where `is_formatting_only()` returns true
- In hunk navigation (`NextHunk`/`PrevHunk` in `App::update()`), skip pure-formatting hunks when `semantic_filter` is true
- When all hunks in a file are hidden, render centered message "All changes are formatting-only"
- Add visible hunk index tracking: compute `visible_hunks` and `hidden_count` for footer

**Accept when:**
- [ ] With `semantic_filter = true`, pure-formatting hunks disappear from diff view
- [ ] Mixed hunks still show (with formatting lines dimmed)
- [ ] `n`/`N` navigation skips hidden hunks
- [ ] File with all formatting hunks shows the "All changes are formatting-only" message
- [ ] Toggling off restores all hunks

**Verify:** `cargo run` — manual test with formatting-heavy repo

---

### Task 8: Footer updates for semantic filter
**Files:** `src/ui.rs`
**Do:**
- When `semantic_filter` is true, show indicator in footer (e.g., `[semantic]` or filter icon)
- Show hunk count reflecting visible hunks: "Hunk 2/5 (3 formatting hidden)"
- Add `w` key hint to footer for both Sidebar and DiffView modes

**Accept when:**
- [ ] Footer shows `w` keybinding hint
- [ ] When filter is active, footer shows visible/hidden hunk counts
- [ ] When filter is inactive, footer is unchanged from current behavior

**Verify:** `cargo run` — visual inspection

---

## Phase 3: Bulk Operations

### Task 9: Bulk stage formatting hunks
**Files:** `src/app.rs`
**Do:**
- Handle `StageFormattingHunks` in `App::update()`
- Iterate all unstaged files, compute diffs, classify, find pure-formatting hunks
- Stage each pure-formatting hunk using existing `stage_hunk` flow
- Stage hunks in reverse order within each file (to avoid line-number shifts)
- Collect all affected file paths, record a single undo snapshot before staging
- Show confirmation in footer: "Staged N formatting-only hunks across M files"
- Skip mixed hunks entirely

**Accept when:**
- [ ] `W` stages all pure-formatting hunks across all unstaged files
- [ ] Mixed hunks are not staged
- [ ] File list updates correctly after staging
- [ ] Files that become fully staged move to the staged section
- [ ] Single undo restores the entire batch
- [ ] Footer shows confirmation message

**Verify:** `cargo test` (undo test), `cargo run` — manual test staging formatting hunks then undoing

---

### Task 10: Sidebar formatting indicators (stretch)
**Files:** `src/ui.rs`
**Do:**
- For files in the unstaged list where all hunks are pure-formatting, apply `Modifier::DIM` to the filename
- This requires classification data to be available for sidebar rendering (may need to cache classification results per file)

**Accept when:**
- [ ] Files with only formatting changes render with dimmed filename in sidebar
- [ ] Files with semantic or mixed changes render normally

**Verify:** `cargo run` — visual inspection

---

## Verification Checkpoints

**After Phase 1 (Tasks 1–3):**
```bash
cargo test                    # All tests pass
cargo test classify           # Classification tests pass
cargo clippy                  # No warnings
```

**After Phase 2 (Tasks 4–8):**
```bash
cargo test                    # All tests pass
cargo clippy                  # No warnings
cargo run                     # Manual: dimming visible, toggle works, footer correct
```

**After Phase 3 (Tasks 9–10):**
```bash
cargo test                    # All tests pass
cargo clippy                  # No warnings
cargo run                     # Manual: W stages formatting, undo works, sidebar dims
```
