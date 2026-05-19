use crate::diff::types::{ChangeKind, DiffHunk};

fn split_raw_lines(content: &str) -> Vec<&str> {
    if content.is_empty() {
        Vec::new()
    } else {
        content.split_inclusive('\n').collect()
    }
}

pub fn apply_hunk_to_content(old_content: &str, hunk: &DiffHunk, selected_lines: Option<&[usize]>) -> String {
    let old_raw = split_raw_lines(old_content);
    let hunk_start = (hunk.old_start as usize).saturating_sub(1);

    let old_count = hunk.lines.iter()
        .filter(|l| l.kind == ChangeKind::Delete || l.kind == ChangeKind::Equal)
        .count();

    let mut text = String::new();

    for raw in &old_raw[..hunk_start.min(old_raw.len())] {
        text.push_str(raw);
    }

    for (i, line) in hunk.lines.iter().enumerate() {
        match line.kind {
            ChangeKind::Equal => {
                if let Some(ln) = line.old_lineno {
                    let idx = (ln as usize).saturating_sub(1);
                    if idx < old_raw.len() {
                        text.push_str(old_raw[idx]);
                    }
                }
            }
            ChangeKind::Insert => {
                if selected_lines.is_none_or(|lines| lines.contains(&i)) {
                    text.push_str(&line.content);
                }
            }
            ChangeKind::Delete => {
                if selected_lines.is_some_and(|lines| !lines.contains(&i))
                    && let Some(ln) = line.old_lineno {
                        let idx = (ln as usize).saturating_sub(1);
                        if idx < old_raw.len() {
                            text.push_str(old_raw[idx]);
                        }
                    }
            }
        }
    }

    let after_start = (hunk_start + old_count).min(old_raw.len());
    for raw in &old_raw[after_start..] {
        text.push_str(raw);
    }

    text
}

pub fn reverse_apply_hunk_to_content(new_content: &str, hunk: &DiffHunk, selected_lines: Option<&[usize]>) -> String {
    let new_raw = split_raw_lines(new_content);
    let hunk_start = (hunk.new_start as usize).saturating_sub(1);

    let new_count = hunk.lines.iter()
        .filter(|l| l.kind == ChangeKind::Insert || l.kind == ChangeKind::Equal)
        .count();

    let mut text = String::new();

    for raw in &new_raw[..hunk_start.min(new_raw.len())] {
        text.push_str(raw);
    }

    for (i, line) in hunk.lines.iter().enumerate() {
        match line.kind {
            ChangeKind::Equal => {
                if let Some(ln) = line.new_lineno {
                    let idx = (ln as usize).saturating_sub(1);
                    if idx < new_raw.len() {
                        text.push_str(new_raw[idx]);
                    }
                }
            }
            ChangeKind::Delete => {
                if selected_lines.is_none_or(|lines| lines.contains(&i)) {
                    text.push_str(&line.content);
                }
            }
            ChangeKind::Insert => {
                if selected_lines.is_some_and(|lines| !lines.contains(&i))
                    && let Some(ln) = line.new_lineno {
                        let idx = (ln as usize).saturating_sub(1);
                        if idx < new_raw.len() {
                            text.push_str(new_raw[idx]);
                        }
                    }
            }
        }
    }

    let after_start = (hunk_start + new_count).min(new_raw.len());
    for raw in &new_raw[after_start..] {
        text.push_str(raw);
    }

    text
}
