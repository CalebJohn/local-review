pub mod types;

use similar::{ChangeTag, TextDiff};
use types::{ChangeKind, DiffContent, DiffHunk, DiffLine};

/// Compute structured diff hunks from two text inputs.
///
/// Uses `similar::TextDiff::from_lines` with grouped_ops to produce
/// hunks with context lines. Each hunk contains DiffLine entries with
/// 1-based line numbers.
pub fn compute_hunks(old: &str, new: &str, context_lines: usize) -> Vec<DiffHunk> {
    let diff = TextDiff::from_lines(old, new);
    let grouped = diff.grouped_ops(context_lines);

    let mut hunks = Vec::new();

    for group in grouped {
        if group.is_empty() {
            continue;
        }

        // Determine hunk start from first op in group
        let old_start = group[0].old_range().start as u32 + 1;
        let new_start = group[0].new_range().start as u32 + 1;

        let mut lines = Vec::new();

        for op in &group {
            for change in diff.iter_changes(op) {
                let old_idx = change.old_index();
                let new_idx = change.new_index();
                let content = change.value().to_string();

                let (kind, old_lineno, new_lineno) = match change.tag() {
                    ChangeTag::Equal => (
                        ChangeKind::Equal,
                        Some(old_idx.unwrap() as u32 + 1),
                        Some(new_idx.unwrap() as u32 + 1),
                    ),
                    ChangeTag::Delete => (
                        ChangeKind::Delete,
                        Some(old_idx.unwrap() as u32 + 1),
                        None,
                    ),
                    ChangeTag::Insert => (
                        ChangeKind::Insert,
                        None,
                        Some(new_idx.unwrap() as u32 + 1),
                    ),
                };

                lines.push(DiffLine {
                    kind,
                    old_lineno,
                    new_lineno,
                    content,
                    formatting_only: false,
                });
            }
        }

        hunks.push(DiffHunk {
            old_start,
            new_start,
            lines,
            has_header: true,
        });
    }

    hunks
}

/// Build a DiffContent from optional old/new content strings.
///
/// Treats None as empty string (handles new/deleted files).
/// Uses 3 context lines (standard git diff context).
pub fn compute_diff_content(path: &str, old_content: Option<&str>, new_content: Option<&str>) -> DiffContent {
    let old = old_content.unwrap_or("");
    let new = new_content.unwrap_or("");
    let hunks = compute_hunks(old, new, 3);
    DiffContent {
        path: path.to_string(),
        hunks,
        is_binary: false,
    }
}

/// Create a DiffContent sentinel for binary files.
///
/// Returns a DiffContent with is_binary=true and empty hunks.
/// Used when ContentResult::Binary is returned from git content methods.
pub fn binary_diff_content(path: &str) -> DiffContent {
    DiffContent {
        path: path.to_string(),
        hunks: vec![],
        is_binary: true,
    }
}

/// Build hunks covering the entire file: regular change hunks (with headers)
/// interspersed with Equal-only "filler" hunks (no headers) that fill the
/// gaps before, between, and after the change hunks. Fillers use lines from
/// the new file (which equals old in those ranges).
pub fn compute_full_hunks(old: &str, new: &str) -> Vec<DiffHunk> {
    let change_hunks = compute_hunks(old, new, 3);

    let old_lines: Vec<&str> = if old.is_empty() {
        Vec::new()
    } else {
        old.split_inclusive('\n').collect()
    };
    let new_lines: Vec<&str> = if new.is_empty() {
        Vec::new()
    } else {
        new.split_inclusive('\n').collect()
    };
    let total_old = old_lines.len() as u32;
    let total_new = new_lines.len() as u32;

    let make_filler =
        |old_start: u32, new_start: u32, count: u32| -> DiffHunk {
            let lines: Vec<DiffLine> = (0..count)
                .map(|i| {
                    let old_lno = old_start + i;
                    let new_lno = new_start + i;
                    let idx = (new_lno - 1) as usize;
                    let content = new_lines
                        .get(idx)
                        .map(|s| s.to_string())
                        .or_else(|| old_lines.get((old_lno - 1) as usize).map(|s| s.to_string()))
                        .unwrap_or_default();
                    DiffLine {
                        kind: ChangeKind::Equal,
                        old_lineno: Some(old_lno),
                        new_lineno: Some(new_lno),
                        content,
                        formatting_only: false,
                    }
                })
                .collect();
            DiffHunk {
                old_start,
                new_start,
                lines,
                has_header: false,
            }
        };

    if change_hunks.is_empty() {
        if total_old == 0 && total_new == 0 {
            return Vec::new();
        }
        // Files are identical (no changes). Render as one filler.
        return vec![make_filler(1, 1, total_new.max(total_old))];
    }

    let mut result: Vec<DiffHunk> = Vec::new();
    let mut prev_old_end: u32 = 0;
    let mut prev_new_end: u32 = 0;

    for hunk in change_hunks {
        // Leading/inter-hunk filler covers [prev_*_end + 1, hunk_*_start - 1].
        if hunk.old_start > prev_old_end + 1 {
            let count = hunk.old_start - 1 - prev_old_end;
            result.push(make_filler(prev_old_end + 1, prev_new_end + 1, count));
        }

        // Track the last old/new line covered by this change hunk.
        let mut max_old: u32 = hunk.old_start.saturating_sub(1);
        let mut max_new: u32 = hunk.new_start.saturating_sub(1);
        for l in &hunk.lines {
            if let Some(o) = l.old_lineno {
                if o > max_old {
                    max_old = o;
                }
            }
            if let Some(n) = l.new_lineno {
                if n > max_new {
                    max_new = n;
                }
            }
        }

        result.push(hunk);
        prev_old_end = max_old;
        prev_new_end = max_new;
    }

    // Trailing filler covers anything after the last change hunk.
    if prev_old_end < total_old || prev_new_end < total_new {
        let count = total_old.saturating_sub(prev_old_end);
        if count > 0 {
            result.push(make_filler(prev_old_end + 1, prev_new_end + 1, count));
        }
    }

    result
}

/// Build a DiffContent containing every line of the file in a single hunk.
pub fn compute_full_diff_content(
    path: &str,
    old_content: Option<&str>,
    new_content: Option<&str>,
) -> DiffContent {
    let old = old_content.unwrap_or("");
    let new = new_content.unwrap_or("");
    let hunks = compute_full_hunks(old, new);
    DiffContent {
        path: path.to_string(),
        hunks,
        is_binary: false,
    }
}

#[cfg(test)]
mod tests;
