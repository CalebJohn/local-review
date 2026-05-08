use crate::diff::types::{ChangeKind, DiffHunk};

pub fn apply_hunk_to_content(old_content: &str, hunk: &DiffHunk, selected_lines: Option<&[usize]>) -> String {
    let old_lines: Vec<&str> = old_content.lines().collect();
    let hunk_start = (hunk.old_start as usize).saturating_sub(1);

    let old_count = hunk.lines.iter()
        .filter(|l| l.kind == ChangeKind::Delete || l.kind == ChangeKind::Equal)
        .count();

    let mut result: Vec<&str> = Vec::new();

    let before_end = hunk_start.min(old_lines.len());
    result.extend_from_slice(&old_lines[..before_end]);

    for (i, line) in hunk.lines.iter().enumerate() {
        match line.kind {
            ChangeKind::Equal => {
                if let Some(ln) = line.old_lineno {
                    let idx = (ln as usize).saturating_sub(1);
                    if idx < old_lines.len() {
                        result.push(old_lines[idx]);
                    }
                }
            }
            ChangeKind::Insert => {
                if selected_lines.is_none_or(|lines| lines.contains(&i)) {
                    result.push(line.content.trim_end_matches('\n'));
                }
            }
            ChangeKind::Delete => {
                if selected_lines.is_some_and(|lines| !lines.contains(&i)) {
                    if let Some(ln) = line.old_lineno {
                        let idx = (ln as usize).saturating_sub(1);
                        if idx < old_lines.len() {
                            result.push(old_lines[idx]);
                        }
                    }
                }
            }
        }
    }

    let after_start = (hunk_start + old_count).min(old_lines.len());
    result.extend_from_slice(&old_lines[after_start..]);

    let mut text = result.join("\n");
    if old_content.ends_with('\n') {
        text.push('\n');
    }
    text
}

pub fn reverse_apply_hunk_to_content(new_content: &str, hunk: &DiffHunk, selected_lines: Option<&[usize]>) -> String {
    let new_lines: Vec<&str> = new_content.lines().collect();
    let hunk_start = (hunk.new_start as usize).saturating_sub(1);

    let new_count = hunk.lines.iter()
        .filter(|l| l.kind == ChangeKind::Insert || l.kind == ChangeKind::Equal)
        .count();

    let mut result: Vec<&str> = Vec::new();

    let before_end = hunk_start.min(new_lines.len());
    result.extend_from_slice(&new_lines[..before_end]);

    for (i, line) in hunk.lines.iter().enumerate() {
        match line.kind {
            ChangeKind::Equal => {
                if let Some(ln) = line.new_lineno {
                    let idx = (ln as usize).saturating_sub(1);
                    if idx < new_lines.len() {
                        result.push(new_lines[idx]);
                    }
                }
            }
            ChangeKind::Delete => {
                if selected_lines.is_none_or(|lines| lines.contains(&i)) {
                    result.push(line.content.trim_end_matches('\n'));
                }
            }
            ChangeKind::Insert => {
                if selected_lines.is_some_and(|lines| !lines.contains(&i)) {
                    if let Some(ln) = line.new_lineno {
                        let idx = (ln as usize).saturating_sub(1);
                        if idx < new_lines.len() {
                            result.push(new_lines[idx]);
                        }
                    }
                }
            }
        }
    }

    let after_start = (hunk_start + new_count).min(new_lines.len());
    result.extend_from_slice(&new_lines[after_start..]);

    let mut text = result.join("\n");
    if new_content.ends_with('\n') {
        text.push('\n');
    }
    text
}
