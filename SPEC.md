# Syntax-Aware Diffing Spec

## Objective

Add a classification layer that identifies formatting-only changes in diffs using tree-sitter AST comparison. Formatting-only lines are always visually dimmed. A global toggle hides pure-formatting hunks entirely. A bulk-stage action lets users accept all formatting-only hunks at once.

**Target user:** Developer reviewing a diff that mixes real logic changes with formatting/refactoring noise (auto-formatter ran, lines were split, indentation changed). They want to focus on what actually changed semantically.

**Core principle:** Classification is always computed. Dimming is always visible. The toggle controls hiding, not classification.

## Definitions

- **Formatting-only change:** A change where the old and new content are structurally identical in the AST. The only differences are whitespace, indentation, line breaks, trailing commas, or quote style.
- **Semantic change:** Any change that alters the AST structure, including comment content changes.
- **Pure-formatting hunk:** A hunk where every Insert/Delete line is formatting-only.
- **Mixed hunk:** A hunk containing both formatting-only and semantic changed lines.
- **Semantic hunk:** A hunk where every changed line is semantic.

## Classification Rules

**Formatting (ignored):**
- Whitespace changes (indentation, trailing whitespace, blank lines between tokens)
- Line splits and joins (one line becomes many, many become one, same tokens)
- Trailing comma addition/removal
- String quote style changes (`"` vs `'` in languages where they're equivalent)
- Import/include reorder (same set of imports, different order) -- deferred to v2

**Semantic (always shown):**
- Any change to comment text content
- Any change that adds, removes, or reorders AST nodes (beyond the formatting cases above)
- New or deleted lines with no counterpart on the other side
- Changes in files with no tree-sitter grammar (no classification possible, treated as all-semantic)

## Classification Algorithm

### Overview

For each hunk, group consecutive changed lines (Insert/Delete), pair Delete groups with adjacent Insert groups, canonicalize both sides using tree-sitter token extraction, and compare. If the canonical forms match, every line in the group is formatting-only.

### Steps

1. **Parse full files.** When a diff is loaded, parse the old file content and new file content with tree-sitter (reusing the existing syntax infrastructure). If no grammar exists for the file's language, skip classification -- all lines remain semantic.

2. **Extract tokens per line.** Walk each AST, collecting leaf-node tokens keyed by their line number. Skip whitespace/newline tokens. This produces `HashMap<u32, Vec<Token>>` for old and new files where `Token` is the node text.

3. **For each hunk, identify change groups.** A change group is a maximal run of consecutive non-Equal lines. Within a change group, separate the Delete lines (old side) and Insert lines (new side).

4. **Canonicalize each side of the group.** Collect all tokens from the relevant lines (using the per-line token maps from step 2). Normalize:
   - Strip trailing commas from token sequences
   - Normalize string quote characters to a canonical form
   - Concatenate into a single canonical string

5. **Compare.** If `canonical(old_group) == canonical(new_group)`, mark every Insert and Delete line in the group as `formatting_only = true`. Otherwise, all lines in the group remain semantic (formatting_only = false).

### Limitations (v1)

- Mixed groups are classified as entirely semantic. If a change group's canonical forms don't match, we don't attempt sub-group analysis. This is conservative -- some formatting lines in mixed groups won't be dimmed. Can be refined in v2.
- Import reordering is not detected in v1 (would require identifying import blocks and sorting before comparison).
- Files exceeding `MAX_HIGHLIGHT_BYTES` (256KB) skip classification, same as syntax highlighting.

## User Experience

### Always On: Dimming

Regardless of toggle state, formatting-only lines in the diff view render with faded colors. Insert lines that are formatting-only use a faded green. Delete lines that are formatting-only use a faded red. This gives the reviewer a signal without hiding anything.

Equal (context) lines are never classified or dimmed -- they aren't changes.

### Toggle: Hide Pure-Formatting Hunks

Keybinding: `w` (mnemonic: like `git diff -w` for whitespace-ignore; available in both Sidebar and DiffView focus modes).

- **Off (default):** All hunks rendered. Formatting-only lines dimmed within their hunks.
- **On:** Pure-formatting hunks are hidden from the diff view. Mixed hunks are shown with formatting-only lines dimmed. Semantic hunks are shown normally.

When the toggle is on:
- Hunk navigation (`n`/`N`) skips hidden hunks.
- The hunk count in the footer reflects only visible hunks (e.g., "Hunk 2/5 (3 formatting hidden)").
- The footer or a status indicator shows that the semantic filter is active.

When toggled on for a file with zero visible hunks remaining (all hunks are pure-formatting), show a centered message like "All changes are formatting-only" instead of an empty diff view.

### Bulk Stage Formatting Hunks

Keybinding: `W` (shift-w, available in both Sidebar and DiffView focus modes).

Action: Stage all pure-formatting hunks across all unstaged files. Mixed hunks are skipped entirely. This is an undoable action (integrates with existing undo/redo system).

Behavior:
- Only operates on the unstaged section (staging formatted hunks from already-staged files doesn't make sense).
- After bulk staging, the staged file list updates to reflect newly staged content.
- If a file's only unstaged changes were pure-formatting, the file moves entirely to the staged section.
- A brief confirmation in the footer: "Staged N formatting-only hunks across M files".

### Sidebar Indicators

Files in the unstaged list that are entirely formatting-only (all hunks are pure-formatting) could show a subtle indicator (e.g., dimmed filename or a small marker). This helps the reviewer see at a glance which files they can skip or bulk-stage. Exact visual treatment TBD during implementation.

## Data Model Changes

### `diff/types.rs`

```rust
pub struct DiffLine {
    pub kind: ChangeKind,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub content: String,
    pub formatting_only: bool, // NEW -- default false
}
```

```rust
impl DiffHunk {
    /// True if every non-Equal line is formatting_only
    pub fn is_formatting_only(&self) -> bool { ... }

    /// True if hunk has a mix of formatting and semantic changes
    pub fn is_mixed(&self) -> bool { ... }
}
```

### `app.rs` State

```rust
pub struct App {
    // ...existing fields...
    pub semantic_filter: bool, // NEW -- toggle state, default false
}
```

### New Messages

```rust
pub enum Message {
    // ...existing variants...
    ToggleSemanticFilter,    // 'w' key
    StageFormattingHunks,    // 'W' key
}
```

## Architecture

### New Module: `classify/`

```
classify/
  mod.rs          Public API: classify_diff(hunks, old_content, new_content, language) -> ()
  canonical.rs    Token extraction, normalization, canonical string building
```

This module mutates `DiffLine::formatting_only` in place on the existing `Vec<DiffHunk>`.

### Integration Point

Classification runs in `App` after diff computation and before (or alongside) syntax highlighting, since both need tree-sitter parses. The existing syntax module already parses full files -- the classification module should share those parse trees rather than parsing twice.

Suggested data flow:

```
1. compute_hunks(old, new)           -> Vec<DiffHunk>           [existing]
2. classify_diff(hunks, old, new, lang)  -> () (mutates hunks)  [NEW]
3. build_styled_diff(hunks, old, new)    -> StyledDiffContent   [existing]
```

Step 2 and 3 can share tree-sitter parse results. The simplest approach: parse in a shared utility, pass `tree_sitter::Tree` references to both classify and highlight.

### Rendering Changes (`ui.rs`)

- When rendering a DiffLine with `formatting_only = true`, apply a dimmed modifier to the line's style (reduce color intensity or add `Modifier::DIM`).
- When `semantic_filter` is on and a hunk's `is_formatting_only()` returns true, skip rendering that hunk entirely.
- Update footer to show filter state and hidden hunk count.

### Staging Changes (`git/mod.rs`)

`StageFormattingHunks` reuses the existing hunk-staging infrastructure. It iterates all unstaged files, computes which hunks are pure-formatting, and stages each one via the existing `stage_hunk` / write-stage-restore flow. The whole batch is wrapped in a single undo snapshot.

## Testing Strategy

All tests inline as `#[cfg(test)] mod tests` per project convention.

### Unit Tests (`classify/`)

- **Whitespace-only change:** Indentation change classified as formatting.
- **Line split:** Single line split into multiple lines, same tokens, classified as formatting.
- **Line join:** Multiple lines joined into one, classified as formatting.
- **Trailing comma:** Added/removed trailing comma classified as formatting.
- **Quote normalization:** `"foo"` to `'foo'` classified as formatting (for JS/Python grammars).
- **Semantic change:** Variable rename, added statement, changed value -- classified as semantic.
- **Comment change:** Modified comment text classified as semantic.
- **Mixed group:** Group with both formatting and semantic changes classified as entirely semantic (v1 conservative behavior).
- **No grammar:** File with no tree-sitter grammar -- all lines remain semantic (formatting_only = false).
- **Large file:** File exceeding MAX_HIGHLIGHT_BYTES skips classification.
- **Pure insertion/deletion:** Lines with no counterpart on the other side are semantic.

### Integration Tests (`app.rs`)

- Toggle on/off updates `semantic_filter` state.
- Hunk navigation with filter on skips pure-formatting hunks.
- `StageFormattingHunks` stages only pure-formatting hunks, skips mixed.
- Undo after `StageFormattingHunks` restores previous state.
- Footer reflects accurate visible/hidden hunk counts.

### Manual Testing Scenarios

- Open a repo where an auto-formatter ran (e.g., `prettier`, `rustfmt`). Verify formatting hunks are dimmed.
- Toggle `w` on. Verify only semantic changes remain visible.
- Press `W` to bulk-stage formatting. Verify staged section updates correctly.
- File with only formatting changes shows "All changes are formatting-only" when filter is on.
- File with no tree-sitter grammar shows no classification (no dimming, no hiding).

## Boundaries

### Always Do
- Classify conservatively: when uncertain, treat as semantic (never hide real changes).
- Respect the existing 256KB size cap -- skip classification for oversized files.
- Integrate with undo/redo for all staging operations.
- Share tree-sitter parse trees between classification and syntax highlighting.

### Ask First
- Any changes to the existing diff computation algorithm in `diff/mod.rs`.
- Adding new tree-sitter grammar dependencies.
- Changes to the hunk staging flow in `git/mod.rs`.

### Never Do
- Shell out to external tools (git CLI, diff utilities).
- Silently hide semantic changes -- classification errors should fail toward showing, not hiding.
- Auto-stage anything without explicit user action.
- Break existing keybindings or navigation behavior.

## Future Work (Out of Scope for This Spec)

- **Sub-group analysis in mixed hunks:** Re-diff canonical forms within a change group to classify individual lines, rather than marking the whole group as semantic.
- **Import reorder detection:** Identify import blocks, sort canonically, compare.
- **Semantic hunk boundaries:** Use tree-sitter to align hunk boundaries to functions/blocks (roadmap/05). The classification infrastructure built here (full-file tree-sitter parse, per-line token maps) directly supports this.
- **User-configurable rules:** Let users toggle individual classification rules (e.g., "treat comment changes as formatting").
- **Per-language quote equivalence:** Only normalize quotes in languages where `"` and `'` are interchangeable for the same construct.
