use crate::diff::types::{ChangeKind, DiffContent};

pub type DiffLineKey = (ChangeKind, Option<u32>, Option<u32>);

/// Total rendered lines across all hunks (including hunk headers).
pub fn total_diff_lines(dc: &DiffContent) -> usize {
    if dc.is_binary {
        return 0;
    }
    dc.hunks
        .iter()
        .map(|h| h.lines.len() + if h.has_header { 1 } else { 0 })
        .sum()
}

/// Total content lines (excluding hunk headers) across all hunks.
pub fn total_content_lines(dc: &DiffContent) -> usize {
    if dc.is_binary {
        return 0;
    }
    dc.hunks.iter().map(|h| h.lines.len()).sum()
}

/// Convert diff_cursor (content line index) to rendered row index.
pub fn cursor_row(dc: &DiffContent, diff_cursor: usize) -> usize {
    let mut row: usize = 0;
    let mut remaining = diff_cursor;
    for hunk in &dc.hunks {
        if hunk.has_header {
            row += 1;
        }
        if remaining < hunk.lines.len() {
            return row + remaining;
        }
        remaining -= hunk.lines.len();
        row += hunk.lines.len();
    }
    row
}

/// Convert a rendered row offset (relative to scroll) to content line index.
pub fn row_to_cursor(dc: &DiffContent, row_offset: usize) -> usize {
    let mut row: usize = 0;
    let mut cursor: usize = 0;
    for hunk in &dc.hunks {
        if hunk.has_header {
            if row == row_offset {
                return cursor;
            }
            row += 1;
        }
        if row_offset < row + hunk.lines.len() {
            return cursor + (row_offset - row);
        }
        row += hunk.lines.len();
        cursor += hunk.lines.len();
    }
    cursor.saturating_sub(1)
}

/// Return the DiffLine identity rendered at `target_row` in the current view.
///
/// Hunk header rows resolve to the first line in that hunk so that anchoring on
/// a header still produces a meaningful target after recomputing the diff. If
/// `target_row` is past the end, returns the last line. Returns None for empty
/// or binary diffs.
pub fn diff_line_at_row(dc: &DiffContent, target_row: usize) -> Option<DiffLineKey> {
    if dc.is_binary || dc.hunks.is_empty() {
        return None;
    }
    let mut row: usize = 0;
    let mut last: Option<DiffLineKey> = None;
    for hunk in &dc.hunks {
        if hunk.has_header {
            if target_row == row
                && let Some(first) = hunk.lines.first() {
                    return Some((first.kind, first.old_lineno, first.new_lineno));
                }
            row += 1;
        }
        for dl in &hunk.lines {
            if target_row == row {
                return Some((dl.kind, dl.old_lineno, dl.new_lineno));
            }
            last = Some((dl.kind, dl.old_lineno, dl.new_lineno));
            row += 1;
        }
    }
    last
}

pub fn row_for_diff_line(dc: &DiffContent, target: DiffLineKey) -> Option<usize> {
    if dc.is_binary {
        return None;
    }
    let (t_kind, t_old, t_new) = target;
    let mut row: usize = 0;
    for hunk in &dc.hunks {
        if hunk.has_header {
            row += 1;
        }
        for dl in &hunk.lines {
            let matches = match t_kind {
                ChangeKind::Equal => {
                    dl.kind == ChangeKind::Equal
                        && dl.old_lineno == t_old
                        && dl.new_lineno == t_new
                }
                ChangeKind::Insert => {
                    dl.kind == ChangeKind::Insert && dl.new_lineno == t_new
                }
                ChangeKind::Delete => {
                    dl.kind == ChangeKind::Delete && dl.old_lineno == t_old
                }
            };
            if matches {
                return Some(row);
            }
            row += 1;
        }
    }
    None
}

pub(crate) fn nearest_row_for_line(dc: &DiffContent, target: DiffLineKey) -> Option<usize> {
    if dc.is_binary || dc.hunks.is_empty() {
        return None;
    }
    let target_line = target.2.or(target.1)?;
    let mut best_row: Option<usize> = None;
    let mut best_dist: u32 = u32::MAX;
    let mut row: usize = 0;
    for hunk in &dc.hunks {
        if hunk.has_header {
            row += 1;
        }
        for dl in &hunk.lines {
            let line = dl.new_lineno.or(dl.old_lineno);
            if let Some(l) = line {
                let dist = l.abs_diff(target_line);
                if dist < best_dist {
                    best_dist = dist;
                    best_row = Some(row);
                }
            }
            row += 1;
        }
    }
    best_row
}
