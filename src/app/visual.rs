use crate::diff::types::{DiffContent, DiffHunk};

use super::{App, AppMode, Focus};

impl App {
    pub(super) fn handle_enter_visual(&mut self) {
        if self.focus != Focus::DiffView || self.mode != AppMode::Normal {
            return;
        }
        self.mode = AppMode::Visual;
        self.visual_cursor = self.diff_cursor;
        self.visual_anchor = self.diff_cursor;
        self.visual_selection = Some((self.diff_cursor, self.diff_cursor));
        self.visual_from_mouse = false;
    }

    pub(super) fn handle_exit_visual(&mut self) {
        if self.mode != AppMode::Visual {
            return;
        }
        self.mode = AppMode::Normal;
        self.visual_selection = None;
        self.visual_from_mouse = false;
    }

    pub(super) fn handle_mouse_click_diff_line(&mut self, line_idx: usize) {
        if self.focus != Focus::DiffView {
            return;
        }
        let max_line = self.total_content_lines().saturating_sub(1);
        let target = line_idx.min(max_line);
        if self.mode == AppMode::Visual {
            self.visual_anchor = target;
            self.visual_cursor = target;
            self.visual_selection = Some((target, target));
            self.visual_from_mouse = true;
        }
        self.diff_cursor = target;
        self.update_hunk_from_cursor();
    }

    pub(super) fn handle_mouse_drag_diff(&mut self, line_idx: usize) {
        if self.focus != Focus::DiffView {
            return;
        }
        let max_line = self.total_content_lines().saturating_sub(1);
        let target = line_idx.min(max_line);
        if self.mode == AppMode::Normal {
            self.mode = AppMode::Visual;
            self.visual_anchor = self.diff_cursor;
            self.visual_cursor = target;
            self.visual_from_mouse = true;
        } else if self.visual_from_mouse {
            self.visual_cursor = target;
        } else {
            self.diff_cursor = target;
            return;
        }
        self.update_visual_selection();
    }

    pub(super) fn handle_extend_selection_up(&mut self) {
        if self.mode != AppMode::Visual {
            return;
        }
        if self.diff_content.is_none() {
            return;
        }
        if self.visual_cursor > 0 {
            self.visual_cursor -= 1;
        }
        self.update_visual_selection();
    }

    pub(super) fn handle_extend_selection_down(&mut self) {
        if self.mode != AppMode::Visual {
            return;
        }
        if self.diff_content.is_none() {
            return;
        }
        let max_line = self.diff_content.as_ref()
            .map(|dc| dc.hunks.iter().map(|h| h.lines.len()).sum::<usize>())
            .unwrap_or(0)
            .saturating_sub(1);
        if self.visual_cursor < max_line {
            self.visual_cursor += 1;
        }
        self.update_visual_selection();
    }

    pub(super) fn update_visual_selection(&mut self) {
        let start = self.visual_anchor.min(self.visual_cursor);
        let end = self.visual_anchor.max(self.visual_cursor);
        self.visual_selection = Some((start, end));
    }

    pub(super) fn local_selected_lines(&self, dc: &DiffContent, hunk_idx: usize, hunk: &DiffHunk) -> Vec<usize> {
        let hunk_line_offset: usize = dc.hunks.iter().take(hunk_idx).map(|h| h.lines.len()).sum();
        let hunk_end = hunk_line_offset + hunk.lines.len();
        let Some((sel_start, sel_end)) = self.visual_selection else {
            return Vec::new();
        };
        let start = sel_start.max(hunk_line_offset);
        let end = (sel_end + 1).min(hunk_end);
        if start >= end {
            return Vec::new();
        }
        (start - hunk_line_offset..end - hunk_line_offset).collect()
    }
}
