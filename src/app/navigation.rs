use super::{diff_line_at_row, nearest_row_for_line, row_for_diff_line};
use super::{geometry, App, Focus, SidebarSection, SCROLL_MARGIN};

impl App {
    pub(super) fn handle_move_up(&mut self) {
        self.status_message = None;
        if self.selected_index > 0 {
            self.save_cursor_position();
            self.selected_index -= 1;
            self.load_diff_for_selected();
        } else if self.sidebar_section == SidebarSection::Unstaged
            && !self.staged_files.is_empty()
        {
            self.save_cursor_position();
            self.sidebar_section = SidebarSection::Staged;
            self.selected_index = self.staged_files.len() - 1;
            self.load_diff_for_selected();
        } else if !self.unstaged_files.is_empty() {
            self.save_cursor_position();
            self.sidebar_section = SidebarSection::Unstaged;
            self.selected_index = self.unstaged_files.len() - 1;
            self.load_diff_for_selected();
        }
    }

    pub(super) fn handle_move_down(&mut self) {
        self.status_message = None;
        let section_len = self.current_section_files().len();
        if section_len > 0 && self.selected_index < section_len - 1 {
            self.save_cursor_position();
            self.selected_index += 1;
            self.load_diff_for_selected();
        } else if self.sidebar_section == SidebarSection::Staged
            && !self.unstaged_files.is_empty()
        {
            self.save_cursor_position();
            self.sidebar_section = SidebarSection::Unstaged;
            self.selected_index = 0;
            self.load_diff_for_selected();
        }
        else if section_len > 0 {
            self.save_cursor_position();
            self.selected_index = 0;
            self.load_diff_for_selected();
        }
    }

    pub(super) fn handle_select_file(&mut self) {
        self.status_message = None;
        self.load_diff_for_selected();
        self.focus = Focus::DiffView;
    }

    pub(super) fn handle_select_sidebar(&mut self) {
        self.status_message = None;
        self.sidebar_collapsed = false;
        self.focus = Focus::Sidebar;
    }

    pub(super) fn handle_move_cursor_up(&mut self) {
        self.status_message = None;
        if self.diff_content.is_none() {
            return;
        }
        if self.diff_cursor > 0 {
            self.diff_cursor -= 1;
        }
        let cursor_row = self.cursor_row();
        let margin = SCROLL_MARGIN as usize;
        let target_scroll = cursor_row.saturating_sub(margin);
        if target_scroll < self.diff_scroll as usize {
            self.diff_scroll = target_scroll as u16;
        }
        self.update_hunk_from_cursor();
    }

    pub(super) fn handle_move_cursor_down(&mut self) {
        self.status_message = None;
        if self.diff_content.is_none() {
            return;
        }
        let max_line = self.total_content_lines().saturating_sub(1);
        if self.diff_cursor < max_line {
            self.diff_cursor += 1;
        }
        let viewport_height = self.diff_viewport_height as usize;
        let cursor_row = self.cursor_row();
        let margin = SCROLL_MARGIN as usize;
        if viewport_height > margin && cursor_row >= self.diff_scroll as usize + viewport_height - margin {
            let new_scroll = cursor_row.saturating_sub(viewport_height).saturating_add(margin);
            let max_scroll = self.total_diff_lines().saturating_sub(1);
            self.diff_scroll = new_scroll.min(max_scroll) as u16;
        }
        self.update_hunk_from_cursor();
    }

    pub(super) fn handle_scroll_diff_up(&mut self) {
        self.status_message = None;
        if self.diff_scroll > 0 {
            self.diff_scroll -= 1;
        }
    }

    pub(super) fn handle_scroll_diff_down(&mut self) {
        self.status_message = None;
        let max_scroll = self.total_diff_lines().saturating_sub(1) as u16;
        if self.diff_scroll < max_scroll {
            self.diff_scroll = self.diff_scroll.saturating_add(1);
        }
    }

    pub(super) fn handle_scroll_to_top(&mut self) {
        self.status_message = None;
        self.diff_cursor = 0;
        self.diff_scroll = 0;
        self.update_hunk_from_cursor();
    }

    pub(super) fn handle_scroll_to_bottom(&mut self) {
        self.status_message = None;
        let max_line = self.total_content_lines().saturating_sub(1);
        self.diff_cursor = max_line;
        let viewport_height = self.diff_viewport_height as usize;
        let total_lines = self.total_diff_lines();
        let max_scroll = total_lines.saturating_sub(viewport_height).saturating_add(SCROLL_MARGIN.into()) as u16;
        self.diff_scroll = max_scroll.min(total_lines.saturating_sub(1) as u16);
        self.update_hunk_from_cursor();
    }

    pub(super) fn handle_switch_focus(&mut self) {
        self.status_message = None;
        self.sidebar_collapsed = false;
        self.focus = match self.focus {
            Focus::Sidebar => Focus::DiffView,
            Focus::DiffView | Focus::CommentInput | Focus::SearchInput => Focus::Sidebar,
        };
    }

    pub(super) fn handle_toggle_sidebar(&mut self) {
        self.status_message = None;
        self.sidebar_collapsed = !self.sidebar_collapsed;
        if self.sidebar_collapsed && self.focus == Focus::Sidebar {
            self.focus = Focus::DiffView;
        }
    }

    pub(super) fn handle_next_hunk(&mut self) {
        self.status_message = None;
        let change_starts = self.change_hunk_starts();
        if change_starts.is_empty() { return; }
        let cursor_row = self.cursor_row();
        let current = change_starts.iter()
            .rposition(|(_, s)| (*s as usize) <= cursor_row);
        let target = match current {
            Some(i) if i + 1 < change_starts.len() => i + 1,
            Some(i) => i,
            None => 0,
        };
        let (pos, _) = change_starts[target];
        self.move_cursor_to_hunk(pos);
        self.scroll_to_cursor_hunk();
    }

    pub(super) fn handle_prev_hunk(&mut self) {
        self.status_message = None;
        let change_starts = self.change_hunk_starts();
        if change_starts.is_empty() { return; }
        let cursor_row = self.cursor_row();
        let current = change_starts.iter()
            .rposition(|(_, s)| (*s as usize) <= cursor_row);
        let target = match current {
            Some(i) if i > 0 => i - 1,
            Some(i) => i,
            None => 0,
        };
        let (pos, _) = change_starts[target];
        self.move_cursor_to_hunk(pos);
        self.scroll_to_cursor_hunk();
    }

    fn scroll_to_cursor_hunk(&mut self) {
        let cursor_row = self.cursor_row();
        let max_scroll = self.total_diff_lines().saturating_sub(1);
        self.diff_scroll = cursor_row.saturating_sub(1).min(max_scroll) as u16;
    }

    pub(super) fn handle_mouse_click_staged_sidebar(&mut self, idx: usize) {
        if idx < self.staged_files.len() {
            self.save_cursor_position();
            self.sidebar_section = SidebarSection::Staged;
            self.selected_index = idx;
            self.focus = Focus::Sidebar;
            self.load_diff_for_selected();
        }
    }

    pub(super) fn handle_mouse_click_unstaged_sidebar(&mut self, idx: usize) {
        if idx < self.unstaged_files.len() {
            self.save_cursor_position();
            self.sidebar_section = SidebarSection::Unstaged;
            self.selected_index = idx;
            self.focus = Focus::Sidebar;
            self.load_diff_for_selected();
        }
    }

    pub(super) fn handle_mouse_click_review_sidebar(&mut self, idx: usize) {
        if idx < self.review_files.len() {
            self.save_cursor_position();
            self.sidebar_section = SidebarSection::Review;
            self.selected_index = idx;
            self.focus = Focus::Sidebar;
            self.load_diff_for_selected();
        }
    }

    pub(super) fn handle_focus_diff(&mut self) {
        self.focus = Focus::DiffView;
        self.update_hunk_from_cursor();
    }

    pub(super) fn handle_workdir_changed(&mut self) {
        self.refresh_file_list();
        if self.auto_reload {
            self.load_diff_for_selected();
        } else {
            self.diff_stale = true;
        }
    }

    pub(super) fn handle_toggle_full_file(&mut self) {
        self.status_message = None;
        self.toggle_full_file();
    }

    fn toggle_full_file(&mut self) {
        let cursor_key = self
            .diff_content
            .as_ref()
            .and_then(|dc| diff_line_at_row(dc, self.cursor_row()));
        let cursor_offset_from_scroll = self.cursor_row().saturating_sub(self.diff_scroll as usize);

        self.show_full_file = !self.show_full_file;
        self.load_diff_for_selected();

        let max_cursor = self.total_content_lines().saturating_sub(1);
        if let (Some(key), Some(dc)) = (cursor_key, self.diff_content.as_ref()) {
            if let Some(row) = row_for_diff_line(dc, key) {
                self.diff_cursor = self.row_to_cursor(row);
            } else if let Some(row) = nearest_row_for_line(dc, key) {
                self.diff_cursor = self.row_to_cursor(row);
            } else {
                self.diff_cursor = self.diff_cursor.min(max_cursor);
            }
        } else {
            self.diff_cursor = self.diff_cursor.min(max_cursor);
        }

        let cursor_row = self.cursor_row();
        let max_scroll = self.total_diff_lines().saturating_sub(1);
        self.diff_scroll = cursor_row.saturating_sub(cursor_offset_from_scroll).min(max_scroll) as u16;
        self.update_hunk_from_cursor();
    }

    pub(super) fn update_hunk_from_cursor(&mut self) {
        let Some(dc) = self.diff_content.as_ref() else {
            self.current_hunk_index = None;
            return;
        };
        let mut content_offset = 0;
        for (i, hunk) in dc.hunks.iter().enumerate() {
            if self.diff_cursor < content_offset + hunk.lines.len() {
                self.current_hunk_index = Some(i);
                return;
            }
            content_offset += hunk.lines.len();
        }
        self.current_hunk_index = dc.hunks.last().map(|_| dc.hunks.len() - 1);
    }

    pub(super) fn move_cursor_to_hunk(&mut self, hunk_idx: usize) {
        let Some(dc) = self.diff_content.as_ref() else { return };
        let Some(hunk) = dc.hunks.get(hunk_idx) else { return };
        let cursor: usize = dc.hunks.iter().take(hunk_idx).map(|h| h.lines.len()).sum();
        self.diff_cursor = cursor.min(hunk.lines.len().saturating_sub(1) + cursor);
        self.update_hunk_from_cursor();
    }

    pub fn hunk_counts(&self) -> Option<(usize, usize, usize)> {
        let dc = self.diff_content.as_ref()?;
        if dc.is_binary || dc.hunks.is_empty() {
            return None;
        }
        let total = dc.hunks.len();
        if self.semantic_filter {
            let hidden = dc.hunks.iter().filter(|h| h.is_formatting_only()).count();
            Some((total - hidden, total, hidden))
        } else {
            Some((total, total, 0))
        }
    }

    pub(super) fn change_hunk_starts(&self) -> Vec<(usize, u16)> {
        let Some(dc) = self.diff_content.as_ref() else { return Vec::new() };
        if dc.is_binary { return Vec::new(); }
        let mut result = Vec::new();
        let mut row: u16 = 0;
        for (i, h) in dc.hunks.iter().enumerate() {
            if self.semantic_filter && h.is_formatting_only() {
                continue;
            }
            if h.has_header {
                result.push((i, row));
            }
            row = row.saturating_add(if h.has_header { 1 } else { 0 })
                .saturating_add(h.lines.len() as u16);
        }
        result
    }

    pub fn total_diff_lines(&self) -> usize {
        match &self.diff_content {
            Some(dc) if !dc.is_binary => geometry::total_diff_lines(dc),
            _ => 0,
        }
    }

    pub(super) fn total_content_lines(&self) -> usize {
        match &self.diff_content {
            Some(dc) if !dc.is_binary => geometry::total_content_lines(dc),
            _ => 0,
        }
    }

    pub(super) fn cursor_row(&self) -> usize {
        let Some(dc) = self.diff_content.as_ref() else { return 0 };
        geometry::cursor_row(dc, self.diff_cursor)
    }

    pub fn row_to_cursor(&self, row_offset: usize) -> usize {
        let Some(dc) = self.diff_content.as_ref() else { return 0 };
        geometry::row_to_cursor(dc, row_offset)
    }

    pub(super) fn restore_hunk_position(&mut self, hunk_idx: usize) {
        if let Some(dc) = self.diff_content.as_ref()
            && !self.show_full_file && !dc.hunks.is_empty() {
                let clamped = hunk_idx.min(dc.hunks.len() - 1);
                self.current_hunk_index = Some(clamped);
                let mut row: u16 = 0;
                for h in dc.hunks.iter().take(clamped) {
                    row = row.saturating_add(if h.has_header { 1 } else { 0 })
                        .saturating_add(h.lines.len() as u16);
                }
                self.diff_scroll = row;
            }
    }
}
