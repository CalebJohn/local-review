# SPEC: Search / Goto

## Objective

Add vim-style search to the TUI so reviewers can quickly find code in diffs and files in the sidebar. `/`/`?` opens a search prompt from either focus; `n`/`N` cycle through matches in whichever pane is focused.

**Target users:** Anyone reviewing diffs who needs to find a specific symbol, string, or file.

## Behavior

### Activation

- `/` opens a forward search prompt in the footer. Works in both Sidebar and DiffView (Normal mode).
- `?` opens a backward search prompt in the footer. Works in both Sidebar and DiffView (Normal mode).
- Both keys are inert in Visual mode and CommentInput focus.
- The focus at the time of activation (`search_origin`: Sidebar or DiffView) is recorded so that Enter returns focus to the originating pane and `n`/`N` know which match list to navigate.

### Input mode

When the search prompt is active, the app enters `Focus::SearchInput`. The footer displays the prompt character (`/` or `?`) followed by the typed query, with a cursor.

- **Character keys** append to the query buffer.
- **Backspace** removes the last character.
- **Enter** commits the search: computes matches in the originating pane, jumps to the first match from the current cursor position (forward or backward per direction), and returns focus to the originating pane.
- **Esc** cancels without searching and returns focus to the originating pane. Any previously committed search pattern is preserved for `n`/`N`.

### Match semantics

- Plain substring match (no regex).
- **Smart case:** if the query is all lowercase, match case-insensitively. If any character is uppercase, match case-sensitively.

**DiffView matches:**
- Matching is performed against `DiffLine.content` for every line in the current `DiffContent` (all hunks, all change kinds — Equal, Insert, Delete).
- Hunk header lines are not searchable.

**Sidebar matches:**
- Matching is performed against `FileEntry.path` for all files in both the staged and unstaged lists (treated as one contiguous list, staged first — same order as sidebar rendering).
- A match means the file path contains the search pattern as a substring.

### Navigation after search

- `n` jumps to the next match in the original search direction (wraps around).
- `N` jumps to the next match in the reverse direction (wraps around).
- `n`/`N` use the last committed search pattern. If no search has been performed, they are no-ops.
- The behavior of `n`/`N` depends on the **current focus**, not the original search origin:
  - **DiffView focused:** `n`/`N` navigate diff line matches.
  - **Sidebar focused:** `n`/`N` navigate file path matches. Jumping to a match updates `selected_index` and `sidebar_section` to select the matching file.
- After jumping in DiffView, `current_hunk_index` and `diff_scroll` update normally (same as `handle_move_cursor_down`/`up`).
- After jumping in Sidebar, the selected file updates and the diff reloads (same as `handle_move_down`/`up`).
- When wrapping occurs, display a status message: `"search wrapped"`.

### Highlighting

- All matching substrings in visible diff lines are highlighted with a yellow background (`Style::new().bg(Color::Yellow).fg(Color::Black)`).
- The line containing the current match (the one the cursor last jumped to) uses a brighter/distinct highlight — not needed for v1; the existing cursor highlight (inverted colors) on the current line is sufficient to distinguish it.
- Highlights are rendered in `diff_lines()` by splitting line content spans at match boundaries.
- When the search pattern is cleared (empty `n`/`N`), no highlights are rendered.

### Lifecycle

- Switching files clears `search_matches` (diff matches) but preserves `search_pattern` — diff matches are recomputed on next `n`/`N` if a pattern exists.
- `ReloadDiff` also clears diff matches (recomputed on next `n`/`N`).
- Staging/unstaging a file or `ReloadDiff` clears `search_sidebar_matches` (recomputed on next `n`/`N`).
- Starting a new `/` or `?` search replaces the previous pattern.

## State changes (App struct)

```rust
// New fields
pub search_query: String,                    // Input buffer while typing
pub search_direction: SearchDirection,       // Forward | Backward
pub search_origin: Focus,                   // Sidebar or DiffView — where search was initiated
pub search_pattern: Option<String>,          // Committed pattern (after Enter)
pub search_case_sensitive: bool,             // Derived from pattern at commit time
pub search_matches: Vec<usize>,             // DiffView: content-line indices with matches
pub search_sidebar_matches: Vec<(SidebarSection, usize)>,  // Sidebar: (section, file_index) pairs
pub search_match_cursor: Option<usize>,     // Index into whichever match list is active
```

```rust
// New enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    Forward,
    Backward,
}
```

```rust
// Extend Focus
pub enum Focus {
    Sidebar,
    DiffView,
    CommentInput,
    SearchInput,  // new
}
```

## New Message variants

```rust
SearchForward,          // '/' pressed — open prompt, set direction Forward
SearchBackward,         // '?' pressed — open prompt, set direction Backward
SearchInputChar(char),  // Character typed in search prompt
SearchInputBackspace,   // Backspace in search prompt
SearchInputSubmit,      // Enter — commit pattern and jump
SearchInputCancel,      // Esc — cancel prompt
NextMatch,              // 'n' — jump to next match
PrevMatch,              // 'N' — jump to previous match
```

## New module: `app/search.rs`

Handlers for all search messages, plus:

```rust
fn compute_diff_matches(diff_content: &DiffContent, pattern: &str, case_sensitive: bool) -> Vec<usize>
```

Returns sorted content-line indices where the line content contains the pattern. Content-line index is the flat index across all hunks (same coordinate space as `diff_cursor`).

```rust
fn compute_sidebar_matches(
    staged: &[FileEntry], unstaged: &[FileEntry], pattern: &str, case_sensitive: bool
) -> Vec<(SidebarSection, usize)>
```

Returns matches as `(section, file_index)` pairs in sidebar display order (all staged files first, then all unstaged files).

```rust
fn find_next_match(matches_len: usize, current_index: Option<usize>, direction: SearchDirection) -> Option<usize>
```

Returns the index into the match list for the next match in the given direction, wrapping around. Returns `None` if `matches_len` is 0.

## Input mapping changes

### `main.rs` event loop

Add `Focus::SearchInput` handling alongside the existing `Focus::CommentInput` block — same structure (char/backspace/enter/esc), different message variants.

### DiffView Normal mode keybindings

Add `/` → `SearchForward`, `?` → `SearchBackward`.
- `n` → `NextMatch`, `N` → `PrevMatch` (currently unmapped in DiffView Normal — `]`/`[` handle hunk navigation).

### Sidebar keybindings

Add `/` → `SearchForward`, `?` → `SearchBackward`.
- `n` → `NextMatch`, `N` → `PrevMatch` — cycle through matching file paths.

## UI changes

### Footer (`ui/footer.rs`)

When `focus == SearchInput`:
- Render `/ {search_query}█` or `? {search_query}█` (prompt char + query + block cursor).
- No other keybinding hints.

### Sidebar (`ui/sidebar.rs`)

When a search pattern is active, matching file names highlight the matched substring with the same yellow background style used in the diff view.

### Diff view (`ui/diff_view.rs`)

`diff_lines()` gains a new parameter: `search_pattern: Option<(&str, bool)>` — `(pattern, case_sensitive)`.

When present, for each content line span, split the text at match boundaries and apply the highlight style to matched substrings. This interleaves with existing syntax highlighting / change-kind coloring.

## Testing strategy

Unit tests in `app/search.rs`:
1. `compute_diff_matches` — basic match, case-insensitive, case-sensitive, no matches, multiple matches per line (still one entry), across multiple hunks.
2. `compute_sidebar_matches` — matches file paths, respects smart case, returns results in sidebar display order (staged then unstaged).
3. `find_next_match` — forward from middle, backward from middle, wrap forward, wrap backward, empty matches, single match.
4. Smart-case detection — all lowercase → insensitive, mixed case → sensitive.

Unit tests in `ui/diff_view.rs`:
5. `diff_lines` with a search pattern highlights the correct spans.

Integration-style tests:
6. `SearchInputSubmit` from DiffView populates `search_matches` and moves cursor.
7. `SearchInputSubmit` from Sidebar populates `search_sidebar_matches` and selects matching file.
8. `NextMatch`/`PrevMatch` cycle through matches and wrap (both panes).
9. Switching files clears diff matches but preserves pattern.
10. `n`/`N` in Sidebar navigate file matches; `n`/`N` in DiffView navigate line matches — same pattern, independent match lists.

## Boundaries

### Always do
- Preserve existing `n`/`N` behavior if they were mapped to something (verify they're free first).
- Reuse the `Focus` + footer input pattern from CommentInput.
- Keep search state minimal — just line indices, not byte offsets within lines.

### Never do
- Regex support.
- Search across files (only current diff).
- Filtering (hiding non-matching files/lines) — search highlights and navigates, it doesn't filter.
- Persistent search history.
- Live/incremental filtering as you type (jump only happens on Enter).

### Ask first
- Whether to support searching in full-file mode (`show_full_file`) — for now, yes, search all visible lines.
