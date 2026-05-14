use crate::diff::types::{ChangeKind, DiffHunk};

use super::SidebarSection;

#[derive(Debug, Clone)]
pub struct CommentContext {
    pub file_path: String,
    pub section: SidebarSection,
    pub hunk_index: usize,
    pub selected_lines: Option<Vec<usize>>,
}

pub fn format_comment(context: &CommentContext, hunk: &DiffHunk, comment: &str) -> String {
    let section_label = match context.section {
        SidebarSection::Staged => "staged",
        SidebarSection::Unstaged => "unstaged",
    };

    let mut out = String::new();
    out.push_str(&format!("File: {} ({})\n", context.file_path, section_label));

    if let Some(ref selected) = context.selected_lines {
        let selected_line_info: Vec<String> = selected.iter()
            .filter_map(|&i| hunk.lines.get(i))
            .map(|dl| {
                let prefix = match dl.kind {
                    ChangeKind::Equal => ' ',
                    ChangeKind::Insert => '+',
                    ChangeKind::Delete => '-',
                };
                let lineno = dl.old_lineno.or(dl.new_lineno).unwrap_or(0);
                format!("  {}{}: {}", prefix, lineno, dl.content.trim_end_matches('\n'))
            })
            .collect();
        out.push_str(&format!("Selected lines ({}):\n", selected.len()));
        for line in &selected_line_info {
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
    } else {
        let old_count = hunk.lines.iter().filter(|l| matches!(l.kind, ChangeKind::Delete | ChangeKind::Equal)).count();
        let new_count = hunk.lines.iter().filter(|l| matches!(l.kind, ChangeKind::Insert | ChangeKind::Equal)).count();
        out.push_str(&format!("@@ -{},{} +{},{} @@\n\n", hunk.old_start, old_count, hunk.new_start, new_count));

        for line in &hunk.lines {
            let prefix = match line.kind {
                ChangeKind::Equal => ' ',
                ChangeKind::Insert => '+',
                ChangeKind::Delete => '-',
            };
            out.push(prefix);
            out.push_str(&line.content);
        }
    }

    out.push_str(&format!("\n{}\n", comment));
    out
}

use super::{App, AppMode, Focus};

impl App {
    pub(super) fn handle_start_comment(&mut self) {
        let Some(hunk_idx) = self.current_hunk_index else {
            self.status_message = Some(super::NO_ACTIVE_HUNK_MSG.to_string());
            return;
        };
        let Some(entry) = self.selected_entry() else {
            self.status_message = Some("No file selected".to_string());
            return;
        };
        let selected_lines = if self.mode == AppMode::Visual && self.visual_selection.is_some() {
            if let Some(dc) = self.diff_content.as_ref() {
                if let Some(hunk) = dc.hunks.get(hunk_idx) {
                    let local = self.local_selected_lines(dc, hunk_idx, hunk);
                    if local.is_empty() { None } else { Some(local) }
                } else { None }
            } else { None }
        } else {
            None
        };
        self.comment_context = Some(CommentContext {
            file_path: entry.path.clone(),
            section: self.sidebar_section,
            hunk_index: hunk_idx,
            selected_lines,
        });
        self.comment_input.clear();
        self.focus = Focus::CommentInput;
    }

    pub(super) fn handle_comment_input_char(&mut self, c: char) {
        self.comment_input.push(c);
    }

    pub(super) fn handle_comment_input_backspace(&mut self) {
        self.comment_input.pop();
    }

    pub(super) fn handle_comment_input_submit(&mut self) {
        if let Some(ctx) = self.comment_context.take() {
            let hunk = self.diff_content.as_ref()
                .and_then(|dc| dc.hunks.get(ctx.hunk_index));
            if let Some(hunk) = hunk {
                let text = format_comment(&ctx, hunk, &self.comment_input);
                match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(text)) {
                    Ok(()) => self.status_message = Some("Copied to clipboard".to_string()),
                    Err(e) => self.status_message = Some(format!("Clipboard error: {}", e)),
                }
            }
            if ctx.selected_lines.is_some() {
                self.mode = AppMode::Normal;
                self.visual_selection = None;
            }
        }
        self.comment_input.clear();
        self.focus = Focus::DiffView;
    }

    pub(super) fn handle_comment_input_cancel(&mut self) {
        self.comment_input.clear();
        self.comment_context = None;
        self.focus = Focus::DiffView;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::types::{ChangeKind, DiffHunk, DiffLine};

    fn dl(kind: ChangeKind, old: Option<u32>, new: Option<u32>) -> DiffLine {
        DiffLine {
            kind,
            old_lineno: old,
            new_lineno: new,
            content: "x\n".to_string(),
            formatting_only: false,
        }
    }

    #[test]
    fn test_format_comment_full_hunk() {
        let ctx = CommentContext {
            file_path: "src/main.rs".to_string(),
            section: SidebarSection::Unstaged,
            hunk_index: 0,
            selected_lines: None,
        };
        let hunk = DiffHunk {
            old_start: 10,
            new_start: 10,
            lines: vec![
                dl(ChangeKind::Equal, Some(10), Some(10)),
                dl(ChangeKind::Delete, Some(11), None),
                dl(ChangeKind::Insert, None, Some(11)),
                dl(ChangeKind::Equal, Some(12), Some(12)),
            ],
            has_header: true,
            header_context: None,
        };
        let result = format_comment(&ctx, &hunk, "This looks wrong");
        assert!(result.starts_with("File: src/main.rs (unstaged)\n"));
        assert!(result.contains("@@ -10,3 +10,3 @@"));
        assert!(result.contains("\nThis looks wrong\n"));
        assert!(result.contains(" x\n"));
        assert!(result.contains("-x\n"));
        assert!(result.contains("+x\n"));
    }

    #[test]
    fn test_format_comment_staged_section() {
        let ctx = CommentContext {
            file_path: "lib.rs".to_string(),
            section: SidebarSection::Staged,
            hunk_index: 0,
            selected_lines: None,
        };
        let hunk = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![
                dl(ChangeKind::Insert, None, Some(1)),
            ],
            has_header: true,
            header_context: None,
        };
        let result = format_comment(&ctx, &hunk, "New line added");
        assert!(result.contains("(staged)"));
        assert!(result.contains("@@ -1,0 +1,1 @@"));
    }

    #[test]
    fn test_format_comment_with_selected_lines() {
        let ctx = CommentContext {
            file_path: "src/main.rs".to_string(),
            section: SidebarSection::Unstaged,
            hunk_index: 0,
            selected_lines: Some(vec![1, 2]),
        };
        let hunk = DiffHunk {
            old_start: 10,
            new_start: 10,
            lines: vec![
                dl(ChangeKind::Equal, Some(10), Some(10)),
                dl(ChangeKind::Delete, Some(11), None),
                dl(ChangeKind::Insert, None, Some(11)),
                dl(ChangeKind::Equal, Some(12), Some(12)),
            ],
            has_header: true,
            header_context: None,
        };
        let result = format_comment(&ctx, &hunk, "These two lines");
        assert!(result.starts_with("File: src/main.rs (unstaged)\n"));
        assert!(result.contains("Selected lines (2):"));
        assert!(!result.contains("@@ -"), "should not include hunk header when lines selected");
        assert!(result.contains("\nThese two lines\n"));
    }

    #[test]
    fn test_format_comment_empty_comment() {
        let ctx = CommentContext {
            file_path: "foo.rs".to_string(),
            section: SidebarSection::Unstaged,
            hunk_index: 0,
            selected_lines: None,
        };
        let hunk = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![dl(ChangeKind::Insert, None, Some(1))],
            has_header: true,
            header_context: None,
        };
        let result = format_comment(&ctx, &hunk, "");
        assert!(result.ends_with("\n\n"));
    }
}
