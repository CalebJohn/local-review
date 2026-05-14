use crate::diff::types::{ChangeKind, DiffHunk, DiffLine};

/// Expand a hunk's context to cover the given line range.
///
/// `expand_to` is 0-based new-file line numbers (from tree-sitter).
/// Prepends/appends Equal lines sourced from `new_lines` content.
pub fn expand_hunk(
    hunk: &DiffHunk,
    old_lines: &[&str],
    new_lines: &[&str],
    expand_to: (u32, u32),
) -> DiffHunk {
    let expand_new_start = (expand_to.0 + 1).max(1);
    let expand_new_end = (expand_to.1 + 1).min(new_lines.len() as u32);

    let hunk_new_start = hunk.new_start;
    let hunk_new_end = hunk
        .lines
        .iter()
        .filter_map(|l| l.new_lineno)
        .max()
        .unwrap_or(hunk.new_start);

    let start_offset = hunk.old_start as i64 - hunk.new_start as i64;
    let hunk_old_end = hunk
        .lines
        .iter()
        .filter_map(|l| l.old_lineno)
        .max()
        .unwrap_or(hunk.old_start);
    let end_offset = hunk_old_end as i64 - hunk_new_end as i64;

    let mut lines = Vec::new();

    if expand_new_start < hunk_new_start {
        for new_lno in expand_new_start..hunk_new_start {
            let old_lno = new_lno as i64 + start_offset;
            if old_lno < 1 || old_lno > old_lines.len() as i64 {
                continue;
            }
            let content = new_lines
                .get((new_lno - 1) as usize)
                .map(|s| s.to_string())
                .unwrap_or_default();
            lines.push(DiffLine {
                kind: ChangeKind::Equal,
                old_lineno: Some(old_lno as u32),
                new_lineno: Some(new_lno),
                content,
                formatting_only: false,
            });
        }
    }

    lines.extend(hunk.lines.iter().cloned());

    if expand_new_end > hunk_new_end {
        for new_lno in (hunk_new_end + 1)..=expand_new_end {
            let old_lno = new_lno as i64 + end_offset;
            if old_lno < 1 || old_lno > old_lines.len() as i64 {
                continue;
            }
            let content = new_lines
                .get((new_lno - 1) as usize)
                .map(|s| s.to_string())
                .unwrap_or_default();
            lines.push(DiffLine {
                kind: ChangeKind::Equal,
                old_lineno: Some(old_lno as u32),
                new_lineno: Some(new_lno),
                content,
                formatting_only: false,
            });
        }
    }

    let new_new_start = expand_new_start.min(hunk_new_start);
    let new_old_start = if expand_new_start < hunk_new_start {
        let old = expand_new_start as i64 + start_offset;
        if old < 1 { 1 } else { old as u32 }
    } else {
        hunk.old_start
    };

    DiffHunk {
        old_start: new_old_start,
        new_start: new_new_start,
        lines,
        has_header: hunk.has_header,
        header_context: hunk.header_context.clone(),
    }
}

/// Merge overlapping or contiguous hunks after expansion.
///
/// Two adjacent hunks merge when the next hunk's `new_start` falls within
/// or just past the current hunk's last `new_lineno`. Duplicate Equal lines
/// in the overlap region are deduplicated.
pub fn merge_overlapping(hunks: Vec<DiffHunk>) -> Vec<DiffHunk> {
    if hunks.len() <= 1 {
        return hunks;
    }

    let mut result: Vec<DiffHunk> = Vec::new();
    let mut iter = hunks.into_iter();
    let mut current = iter.next().unwrap();

    for next in iter {
        let current_new_end = current
            .lines
            .iter()
            .filter_map(|l| l.new_lineno)
            .max()
            .unwrap_or(current.new_start);

        if next.new_start <= current_new_end + 1 {
            for line in &next.lines {
                let skip = line.kind == ChangeKind::Equal
                    && line.new_lineno.is_some_and(|n| n <= current_new_end);
                if !skip {
                    current.lines.push(line.clone());
                }
            }
            if current.header_context.is_none() {
                current.header_context = next.header_context;
            }
        } else {
            result.push(current);
            current = next;
        }
    }
    result.push(current);

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eq_line(old: u32, new: u32, content: &str) -> DiffLine {
        DiffLine {
            kind: ChangeKind::Equal,
            old_lineno: Some(old),
            new_lineno: Some(new),
            content: content.to_string(),
            formatting_only: false,
        }
    }

    fn del_line(old: u32, content: &str) -> DiffLine {
        DiffLine {
            kind: ChangeKind::Delete,
            old_lineno: Some(old),
            new_lineno: None,
            content: content.to_string(),
            formatting_only: false,
        }
    }

    fn ins_line(new: u32, content: &str) -> DiffLine {
        DiffLine {
            kind: ChangeKind::Insert,
            old_lineno: None,
            new_lineno: Some(new),
            content: content.to_string(),
            formatting_only: false,
        }
    }

    fn split(s: &str) -> Vec<&str> {
        if s.is_empty() { Vec::new() } else { s.split_inclusive('\n').collect() }
    }

    #[test]
    fn test_expand_prepends_and_appends_context() {
        let content = "a\nb\nc\nd\ne\nf\ng\nh\n";
        let lines = split(content);

        let hunk = DiffHunk {
            old_start: 3,
            new_start: 3,
            lines: vec![
                eq_line(3, 3, "c\n"),
                eq_line(4, 4, "d\n"),
                del_line(5, "e\n"),
                ins_line(5, "E\n"),
                eq_line(6, 6, "f\n"),
            ],
            has_header: true,
            header_context: None,
        };

        let expanded = expand_hunk(&hunk, &lines, &lines, (0, 7));

        assert_eq!(expanded.old_start, 1);
        assert_eq!(expanded.new_start, 1);
        // 2 prepended + 5 original + 2 appended = 9
        assert_eq!(expanded.lines.len(), 9);

        assert_eq!(expanded.lines[0].new_lineno, Some(1));
        assert_eq!(expanded.lines[0].old_lineno, Some(1));
        assert_eq!(expanded.lines[0].kind, ChangeKind::Equal);
        assert_eq!(expanded.lines[1].new_lineno, Some(2));

        assert_eq!(expanded.lines[4].kind, ChangeKind::Delete);
        assert_eq!(expanded.lines[5].kind, ChangeKind::Insert);

        assert_eq!(expanded.lines[7].new_lineno, Some(7));
        assert_eq!(expanded.lines[8].new_lineno, Some(8));
    }

    #[test]
    fn test_expand_at_file_start_no_negative_lines() {
        let content = "a\nb\nc\n";
        let lines = split(content);

        let hunk = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![
                del_line(1, "a\n"),
                ins_line(1, "A\n"),
                eq_line(2, 2, "b\n"),
            ],
            has_header: true,
            header_context: None,
        };

        let expanded = expand_hunk(&hunk, &lines, &lines, (0, 2));

        assert_eq!(expanded.old_start, 1);
        assert_eq!(expanded.new_start, 1);
        // Original 3 + append line 3 = 4
        assert_eq!(expanded.lines.len(), 4);

        for line in &expanded.lines {
            if let Some(o) = line.old_lineno {
                assert!(o >= 1, "old_lineno must be >= 1, got {o}");
            }
            if let Some(n) = line.new_lineno {
                assert!(n >= 1, "new_lineno must be >= 1, got {n}");
            }
        }
    }

    #[test]
    fn test_expand_at_file_end_clamps() {
        let content = "a\nb\nc\n";
        let lines = split(content);

        let hunk = DiffHunk {
            old_start: 2,
            new_start: 2,
            lines: vec![
                eq_line(2, 2, "b\n"),
                del_line(3, "c\n"),
                ins_line(3, "C\n"),
            ],
            has_header: true,
            header_context: None,
        };

        let expanded = expand_hunk(&hunk, &lines, &lines, (0, 99));

        assert_eq!(expanded.new_start, 1);
        // Prepend line 1, no out-of-bounds appends
        assert_eq!(expanded.lines.len(), 4);
        for line in &expanded.lines {
            if let Some(n) = line.new_lineno {
                assert!(n <= 3);
            }
        }
    }

    #[test]
    fn test_expand_preserves_insert_delete_lines() {
        let old = "a\nb\nc\n";
        let new = "a\nB\nc\n";
        let old_l = split(old);
        let new_l = split(new);

        let hunk = DiffHunk {
            old_start: 2,
            new_start: 2,
            lines: vec![del_line(2, "b\n"), ins_line(2, "B\n")],
            has_header: true,
            header_context: Some("test_fn".to_string()),
        };

        let expanded = expand_hunk(&hunk, &old_l, &new_l, (0, 2));

        let del = expanded.lines.iter().find(|l| l.kind == ChangeKind::Delete).unwrap();
        assert_eq!(del.content, "b\n");
        assert_eq!(del.old_lineno, Some(2));

        let ins = expanded.lines.iter().find(|l| l.kind == ChangeKind::Insert).unwrap();
        assert_eq!(ins.content, "B\n");
        assert_eq!(ins.new_lineno, Some(2));

        assert_eq!(expanded.header_context, Some("test_fn".to_string()));
    }

    #[test]
    fn test_expand_no_expansion_when_already_covered() {
        let content = "a\nb\nc\n";
        let lines = split(content);

        let hunk = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![
                eq_line(1, 1, "a\n"),
                del_line(2, "b\n"),
                ins_line(2, "B\n"),
                eq_line(3, 3, "c\n"),
            ],
            has_header: true,
            header_context: None,
        };

        let expanded = expand_hunk(&hunk, &lines, &lines, (0, 2));
        assert_eq!(expanded.lines.len(), 4);
        assert_eq!(expanded.old_start, 1);
        assert_eq!(expanded.new_start, 1);
    }

    #[test]
    fn test_merge_overlapping_deduplicates_equal_lines() {
        let hunk1 = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![
                del_line(1, "a\n"),
                ins_line(1, "A\n"),
                eq_line(2, 2, "b\n"),
                eq_line(3, 3, "c\n"),
                eq_line(4, 4, "d\n"),
                eq_line(5, 5, "e\n"),
            ],
            has_header: true,
            header_context: Some("fn foo".to_string()),
        };

        let hunk2 = DiffHunk {
            old_start: 3,
            new_start: 3,
            lines: vec![
                eq_line(3, 3, "c\n"),
                eq_line(4, 4, "d\n"),
                eq_line(5, 5, "e\n"),
                del_line(6, "f\n"),
                ins_line(6, "F\n"),
                eq_line(7, 7, "g\n"),
            ],
            has_header: true,
            header_context: Some("fn bar".to_string()),
        };

        let merged = merge_overlapping(vec![hunk1, hunk2]);

        assert_eq!(merged.len(), 1);
        let h = &merged[0];
        assert_eq!(h.old_start, 1);
        assert_eq!(h.new_start, 1);
        assert_eq!(h.header_context, Some("fn foo".to_string()));

        let eq_new_lnos: Vec<u32> = h
            .lines
            .iter()
            .filter(|l| l.kind == ChangeKind::Equal)
            .filter_map(|l| l.new_lineno)
            .collect();
        let unique: std::collections::HashSet<u32> = eq_new_lnos.iter().copied().collect();
        assert_eq!(eq_new_lnos.len(), unique.len(), "duplicate Equal lines found");
    }

    #[test]
    fn test_merge_non_overlapping_preserved() {
        let hunk1 = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![del_line(1, "a\n"), ins_line(1, "A\n"), eq_line(2, 2, "b\n")],
            has_header: true,
            header_context: None,
        };

        let hunk2 = DiffHunk {
            old_start: 10,
            new_start: 10,
            lines: vec![eq_line(10, 10, "j\n"), del_line(11, "k\n"), ins_line(11, "K\n")],
            has_header: true,
            header_context: None,
        };

        let merged = merge_overlapping(vec![hunk1, hunk2]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_merge_contiguous_hunks() {
        let hunk1 = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![
                del_line(1, "a\n"),
                ins_line(1, "A\n"),
                eq_line(2, 2, "b\n"),
                eq_line(3, 3, "c\n"),
                eq_line(4, 4, "d\n"),
                eq_line(5, 5, "e\n"),
            ],
            has_header: true,
            header_context: None,
        };

        let hunk2 = DiffHunk {
            old_start: 6,
            new_start: 6,
            lines: vec![
                eq_line(6, 6, "f\n"),
                del_line(7, "g\n"),
                ins_line(7, "G\n"),
            ],
            has_header: true,
            header_context: None,
        };

        let merged = merge_overlapping(vec![hunk1, hunk2]);
        assert_eq!(merged.len(), 1, "contiguous hunks should merge");
    }

    #[test]
    fn test_merge_single_hunk_unchanged() {
        let hunk = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![del_line(1, "a\n")],
            has_header: true,
            header_context: None,
        };

        let merged = merge_overlapping(vec![hunk]);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn test_merge_empty_vec() {
        let merged = merge_overlapping(vec![]);
        assert!(merged.is_empty());
    }
}
