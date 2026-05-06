# SPEC: Hunk Comment to Clipboard

## Objective

Add a comment feature that lets the user annotate the currently selected hunk with a free-text comment, then copies the comment plus enough diff context to the system clipboard for an AI agent to understand and act on.

Target user: the repo author, reviewing AI-generated diffs and sending feedback back to the agent.

## Interaction Flow

1. User presses `c` while a hunk is selected (DiffView focus, `current_hunk_index` is `Some`).
2. Footer transforms into a single-line text input with prompt `comment: `.
3. User types their comment. Normal navigation keys are suppressed — all printable characters go to the input buffer.
4. **Enter** — format the comment with context, copy to system clipboard, show a brief status message ("Copied to clipboard"), return to DiffView.
5. **Esc** — discard input, return to DiffView with no side effects.

If `c` is pressed with no hunk selected (e.g., binary file, empty diff), ignore the keypress.

## Clipboard Output Format

```
File: {relative_path} ({staged|unstaged})
@@ -{old_start},{old_count} +{new_start},{new_count} @@

{diff lines with +/-/space prefixes}

Comment: {user input}
```

Example:

```
File: src/app.rs (unstaged)
@@ -42,3 +42,5 @@

 fn foo() {
-    old_line();
+    new_line();
+    another_line();
 }

Comment: please explain why old_line was replaced
```

The diff lines use standard unified diff prefixes: ` ` for context (Equal), `+` for Insert, `-` for Delete. This is the format AI agents already understand.

## App State Changes

### New enum variant

```rust
pub enum Focus {
    Sidebar,
    DiffView,
    CommentInput,  // new
}
```

### New fields on `App`

```rust
pub comment_input: String,          // current input buffer
pub comment_context: Option<CommentContext>,  // captured when entering comment mode
```

### CommentContext

```rust
pub struct CommentContext {
    pub file_path: String,
    pub section: SidebarSection,    // Staged or Unstaged
    pub hunk_index: usize,
    pub line_range: Option<(u32, u32)>,  // None = whole hunk; Some((start, end)) = selected lines (future)
}
```

`line_range` is always `None` in this version. It exists so the type is ready for line-level selection without a breaking change.

## New Message Variants

```rust
pub enum Message {
    // ... existing variants ...
    StartComment,           // c pressed in DiffView
    CommentInputChar(char), // printable character
    CommentInputBackspace,  // delete last char
    CommentInputSubmit,     // Enter
    CommentInputCancel,     // Esc
}
```

## Key Handling (main.rs)

When `app.focus == Focus::CommentInput`:
- Printable characters -> `CommentInputChar(c)`
- Backspace -> `CommentInputBackspace`
- Enter -> `CommentInputSubmit`
- Esc -> `CommentInputCancel`
- All other keys are ignored (no navigation, no quit)

When `app.focus == Focus::DiffView`:
- `c` -> `StartComment` (only when `current_hunk_index.is_some()`)

## Update Logic (app.rs)

| Message | Effect |
|---------|--------|
| `StartComment` | Capture `CommentContext` from current state. Set `focus = CommentInput`. Clear `comment_input`. |
| `CommentInputChar(c)` | Append `c` to `comment_input`. |
| `CommentInputBackspace` | Pop last char from `comment_input`. |
| `CommentInputSubmit` | Format output string. Copy to clipboard. Set `status_message` to "Copied to clipboard". Set `focus = DiffView`. Clear `comment_input` and `comment_context`. |
| `CommentInputCancel` | Set `focus = DiffView`. Clear `comment_input` and `comment_context`. |

## Formatting Logic

New function (in app.rs or a new `comment.rs` module):

```rust
fn format_comment(context: &CommentContext, hunk: &DiffHunk, comment: &str) -> String
```

Builds the clipboard string described in "Clipboard Output Format" above. Uses `context.line_range` to filter lines when present (future), otherwise includes all lines from the hunk.

## Clipboard

Add `arboard` crate as a dependency. It is cross-platform (macOS/Linux/Windows), well-maintained, and has no runtime dependencies on macOS (uses `NSPasteboard` directly).

Clipboard write happens in `update()` on `CommentInputSubmit`. If clipboard write fails, set `status_message` to the error instead.

## Footer Rendering (ui.rs)

When `app.focus == Focus::CommentInput`:
- Render a single-line input: `comment: {input_text}|` (pipe = cursor).
- Style: white text on default background, cursor character highlighted or blinking.
- This replaces the normal footer content entirely.

When `app.focus != Focus::CommentInput`:
- DiffView footer gains `c=comment` in the help text.

## Testing Strategy

Unit tests (inline `#[cfg(test)] mod tests`):

1. **`format_comment` output** — verify the formatted string matches the expected clipboard format for a known hunk.
2. **`format_comment` with line_range** — verify that when `line_range` is `Some((start, end))`, only matching lines are included (prep for future).
3. **`StartComment` captures context** — verify `comment_context` is populated correctly and focus changes.
4. **`CommentInputChar` / `CommentInputBackspace`** — verify buffer manipulation.
5. **`CommentInputCancel` clears state** — verify focus returns and buffers are cleared.
6. **`StartComment` ignored without hunk** — verify no state change when `current_hunk_index` is `None`.

Clipboard interaction is not unit-tested (requires system clipboard). Manual testing covers the end-to-end flow.

## Boundaries

### Always do
- Capture full hunk context including all Equal/Insert/Delete lines
- Use unified diff prefix format (`+`/`-`/` `)
- Include the hunk header (`@@ ... @@`) for position context
- Suppress all non-input keys during comment mode
- Show feedback after clipboard copy

### Never do
- Shell out to `pbcopy`/`xclip` — use `arboard` for clipboard
- Modify any git state (staging, index) as part of commenting
- Allow comment mode when no hunk is selected
- Persist comments to disk

### Future extensions (not in this version)
- Line-level selection within a hunk (visual mode with highlight)
- Batch comments (annotate multiple hunks, copy all at once)
- Comment history / recall

## Files to Modify

| File | Changes |
|------|---------|
| `Cargo.toml` | Add `arboard` dependency |
| `src/app.rs` | `CommentContext` struct, new `App` fields, `Focus::CommentInput`, new `Message` variants, update logic |
| `src/main.rs` | Key handling for `CommentInput` focus, `c` binding in DiffView |
| `src/ui.rs` | Footer rendering for input mode, `c=comment` in DiffView help text |

Optionally, the formatting logic could live in a new `src/comment.rs` if it grows beyond ~30 lines, but starting inline in `app.rs` is fine.
