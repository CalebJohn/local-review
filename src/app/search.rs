use crate::diff::types::DiffContent;
use crate::git::types::FileEntry;
use crate::ui::contains_match;

use super::{App, Focus, SearchDirection, SidebarSection};

pub fn is_case_sensitive(pattern: &str) -> bool {
    pattern.chars().any(|c| c.is_uppercase())
}

pub fn compute_diff_matches(
    diff_content: &DiffContent,
    pattern: &str,
    case_sensitive: bool,
) -> Vec<usize> {
    let mut matches = Vec::new();
    let mut idx = 0;
    for hunk in &diff_content.hunks {
        for line in &hunk.lines {
            if contains_match(&line.content, pattern, case_sensitive) {
                matches.push(idx);
            }
            idx += 1;
        }
    }
    matches
}

pub fn compute_sidebar_matches(
    staged: &[FileEntry],
    unstaged: &[FileEntry],
    pattern: &str,
    case_sensitive: bool,
) -> Vec<(SidebarSection, usize)> {
    let mut matches = Vec::new();
    for (i, entry) in staged.iter().enumerate() {
        if contains_match(&entry.path, pattern, case_sensitive) {
            matches.push((SidebarSection::Staged, i));
        }
    }
    for (i, entry) in unstaged.iter().enumerate() {
        if contains_match(&entry.path, pattern, case_sensitive) {
            matches.push((SidebarSection::Unstaged, i));
        }
    }
    matches
}

pub fn find_next_match(
    matches_len: usize,
    current_index: Option<usize>,
    direction: SearchDirection,
) -> Option<usize> {
    if matches_len == 0 {
        return None;
    }
    match current_index {
        None => Some(0),
        Some(cur) => match direction {
            SearchDirection::Forward => Some((cur + 1) % matches_len),
            SearchDirection::Backward => {
                Some(if cur == 0 { matches_len - 1 } else { cur - 1 })
            }
        },
    }
}

fn is_search_wrapped(
    old_cursor: Option<usize>,
    direction: SearchDirection,
    match_idx: usize,
    total: usize,
) -> bool {
    old_cursor.is_some()
        && ((direction == SearchDirection::Forward && match_idx == 0)
            || (direction == SearchDirection::Backward && match_idx == total - 1))
}

pub fn find_nearest_diff_match(
    matches: &[usize],
    cursor_pos: usize,
    direction: SearchDirection,
) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    match direction {
        SearchDirection::Forward => matches
            .iter()
            .position(|&m| m >= cursor_pos)
            .or(Some(0)),
        SearchDirection::Backward => matches
            .iter()
            .rposition(|&m| m <= cursor_pos)
            .or(Some(matches.len() - 1)),
    }
}

pub fn find_nearest_sidebar_match(
    matches: &[(SidebarSection, usize)],
    current_section: SidebarSection,
    current_index: usize,
    direction: SearchDirection,
) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    let cur = (current_section, current_index);
    match direction {
        SearchDirection::Forward => matches
            .iter()
            .position(|m| *m >= cur)
            .or(Some(0)),
        SearchDirection::Backward => matches
            .iter()
            .rposition(|m| *m <= cur)
            .or(Some(matches.len() - 1)),
    }
}

impl App {
    pub(super) fn handle_search_forward(&mut self) {
        self.start_search(SearchDirection::Forward);
    }

    pub(super) fn handle_search_backward(&mut self) {
        self.start_search(SearchDirection::Backward);
    }

    fn start_search(&mut self, direction: SearchDirection) {
        self.search_direction = direction;
        self.search_origin = self.focus;
        self.search_query.clear();
        self.focus = Focus::SearchInput;
    }

    pub(super) fn handle_search_input_char(&mut self, c: char) {
        self.search_query.push(c);
    }

    pub(super) fn handle_search_input_backspace(&mut self) {
        self.search_query.pop();
    }

    pub(super) fn handle_search_input_submit(&mut self) {
        if self.search_query.is_empty() {
            self.focus = self.search_origin;
            return;
        }
        let pattern = self.search_query.clone();
        let case_sensitive = is_case_sensitive(&pattern);
        self.search_pattern = Some(pattern.clone());
        self.search_case_sensitive = case_sensitive;
        self.search_match_cursor = None;

        match self.search_origin {
            Focus::DiffView => {
                if let Some(dc) = &self.diff_content {
                    self.search_matches =
                        compute_diff_matches(dc, &pattern, case_sensitive);
                }
                self.search_sidebar_matches.clear();
                if let Some(idx) = find_nearest_diff_match(
                    &self.search_matches,
                    self.diff_cursor,
                    self.search_direction,
                ) {
                    self.scroll_to_diff_match(idx, false);
                }
            }
            Focus::Sidebar => {
                self.search_sidebar_matches = compute_sidebar_matches(
                    &self.staged_files,
                    &self.unstaged_files,
                    &pattern,
                    case_sensitive,
                );
                self.search_matches.clear();
                if let Some(idx) = find_nearest_sidebar_match(
                    &self.search_sidebar_matches,
                    self.sidebar_section,
                    self.selected_index,
                    self.search_direction,
                ) {
                    self.navigate_to_sidebar_match(idx, false);
                }
            }
            _ => {}
        }
        self.focus = self.search_origin;
    }

    pub(super) fn handle_search_input_cancel(&mut self) {
        self.search_query.clear();
        self.focus = self.search_origin;
    }

    pub(super) fn handle_next_match(&mut self) {
        self.jump_to_match_in_direction(self.search_direction);
    }

    pub(super) fn handle_prev_match(&mut self) {
        let opposite = match self.search_direction {
            SearchDirection::Forward => SearchDirection::Backward,
            SearchDirection::Backward => SearchDirection::Forward,
        };
        self.jump_to_match_in_direction(opposite);
    }

    fn ensure_diff_matches(&mut self) {
        if self.search_matches.is_empty() {
            if let Some(ref pattern) = self.search_pattern {
                if let Some(dc) = &self.diff_content {
                    self.search_matches =
                        compute_diff_matches(dc, pattern, self.search_case_sensitive);
                    self.search_match_cursor = None;
                }
            }
        }
    }

    fn ensure_sidebar_matches(&mut self) {
        if self.search_sidebar_matches.is_empty() {
            if let Some(ref pattern) = self.search_pattern {
                self.search_sidebar_matches = compute_sidebar_matches(
                    &self.staged_files,
                    &self.unstaged_files,
                    pattern,
                    self.search_case_sensitive,
                );
                self.search_match_cursor = None;
            }
        }
    }

    fn scroll_to_diff_match(&mut self, match_idx: usize, wrapped: bool) {
        self.search_match_cursor = Some(match_idx);
        let content_line = self.search_matches[match_idx];
        self.diff_cursor = content_line;
        let cursor_row = self.cursor_row();
        let viewport = self.diff_viewport_height.get() as usize;
        if viewport > 0
            && (cursor_row < self.diff_scroll as usize
                || cursor_row >= self.diff_scroll as usize + viewport)
        {
            self.diff_scroll = cursor_row.saturating_sub(viewport / 4) as u16;
        }
        self.update_hunk_from_cursor();
        if wrapped {
            self.status_message = Some("search wrapped".to_string());
        }
    }

    fn jump_to_match_in_direction(&mut self, direction: SearchDirection) {
        match self.focus {
            Focus::DiffView => {
                self.ensure_diff_matches();
                self.jump_to_next_diff_match(direction);
            }
            Focus::Sidebar => {
                self.ensure_sidebar_matches();
                self.jump_to_next_sidebar_match(direction);
            }
            _ => {}
        }
    }

    fn jump_to_next_diff_match(&mut self, direction: SearchDirection) {
        if self.search_matches.is_empty() {
            return;
        }
        let old_cursor = self.search_match_cursor;
        let next = find_next_match(self.search_matches.len(), self.search_match_cursor, direction);
        if let Some(match_idx) = next {
            let wrapped = is_search_wrapped(old_cursor, direction, match_idx, self.search_matches.len());
            self.scroll_to_diff_match(match_idx, wrapped);
        }
    }

    fn navigate_to_sidebar_match(&mut self, match_idx: usize, wrapped: bool) {
        let (section, file_idx) = self.search_sidebar_matches[match_idx];
        self.save_scroll_position();
        self.sidebar_section = section;
        self.selected_index = file_idx;
        self.load_diff_for_selected();
        self.search_match_cursor = Some(match_idx);
        if wrapped {
            self.status_message = Some("search wrapped".to_string());
        }
    }

    fn jump_to_next_sidebar_match(&mut self, direction: SearchDirection) {
        if self.search_sidebar_matches.is_empty() {
            return;
        }
        let old_cursor = self.search_match_cursor;
        let next = find_next_match(self.search_sidebar_matches.len(), self.search_match_cursor, direction);
        if let Some(match_idx) = next {
            let wrapped = is_search_wrapped(old_cursor, direction, match_idx, self.search_sidebar_matches.len());
            self.navigate_to_sidebar_match(match_idx, wrapped);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::types::{ChangeKind, DiffContent, DiffHunk, DiffLine};
    use crate::git::types::{FileEntry, FileStatus};

    fn dl(kind: ChangeKind, content: &str) -> DiffLine {
        DiffLine {
            kind,
            old_lineno: None,
            new_lineno: None,
            content: content.to_string(),
            formatting_only: false,
        }
    }

    fn make_dc(hunks: Vec<DiffHunk>) -> DiffContent {
        DiffContent {
            path: "test.rs".to_string(),
            hunks,
            is_binary: false,
        }
    }

    fn make_hunk(lines: Vec<DiffLine>) -> DiffHunk {
        DiffHunk {
            old_start: 1,
            new_start: 1,
            lines,
            has_header: true,
            header_context: None,
        }
    }

    // ---- is_case_sensitive ----

    #[test]
    fn test_is_case_sensitive_all_lower() {
        assert!(!is_case_sensitive("hello"));
    }

    #[test]
    fn test_is_case_sensitive_mixed() {
        assert!(is_case_sensitive("Hello"));
    }

    #[test]
    fn test_is_case_sensitive_all_upper() {
        assert!(is_case_sensitive("HELLO"));
    }

    #[test]
    fn test_is_case_sensitive_with_numbers() {
        assert!(!is_case_sensitive("hello123"));
    }

    #[test]
    fn test_is_case_sensitive_empty() {
        assert!(!is_case_sensitive(""));
    }

    // ---- compute_diff_matches ----

    #[test]
    fn test_compute_diff_matches_basic() {
        let dc = make_dc(vec![make_hunk(vec![
            dl(ChangeKind::Equal, "hello world\n"),
            dl(ChangeKind::Insert, "foo bar\n"),
            dl(ChangeKind::Delete, "baz hello\n"),
        ])]);
        let matches = compute_diff_matches(&dc, "hello", false);
        assert_eq!(matches, vec![0, 2]);
    }

    #[test]
    fn test_compute_diff_matches_case_insensitive() {
        let dc = make_dc(vec![make_hunk(vec![
            dl(ChangeKind::Equal, "Hello World\n"),
            dl(ChangeKind::Insert, "HELLO\n"),
        ])]);
        let matches = compute_diff_matches(&dc, "hello", false);
        assert_eq!(matches, vec![0, 1]);
    }

    #[test]
    fn test_compute_diff_matches_case_sensitive() {
        let dc = make_dc(vec![make_hunk(vec![
            dl(ChangeKind::Equal, "Hello World\n"),
            dl(ChangeKind::Insert, "HELLO\n"),
            dl(ChangeKind::Delete, "Hello\n"),
        ])]);
        let matches = compute_diff_matches(&dc, "Hello", true);
        assert_eq!(matches, vec![0, 2]);
    }

    #[test]
    fn test_compute_diff_matches_no_matches() {
        let dc = make_dc(vec![make_hunk(vec![
            dl(ChangeKind::Equal, "hello\n"),
        ])]);
        let matches = compute_diff_matches(&dc, "xyz", false);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_compute_diff_matches_multiple_per_line_single_entry() {
        let dc = make_dc(vec![make_hunk(vec![
            dl(ChangeKind::Equal, "hello hello hello\n"),
        ])]);
        let matches = compute_diff_matches(&dc, "hello", false);
        assert_eq!(matches, vec![0]);
    }

    #[test]
    fn test_compute_diff_matches_across_hunks() {
        let dc = make_dc(vec![
            make_hunk(vec![
                dl(ChangeKind::Equal, "foo\n"),
                dl(ChangeKind::Insert, "target\n"),
            ]),
            DiffHunk {
                old_start: 10,
                new_start: 10,
                lines: vec![
                    dl(ChangeKind::Delete, "other\n"),
                    dl(ChangeKind::Equal, "target again\n"),
                ],
                has_header: true,
                header_context: None,
            },
        ]);
        let matches = compute_diff_matches(&dc, "target", false);
        assert_eq!(matches, vec![1, 3]);
    }

    // ---- compute_sidebar_matches ----

    #[test]
    fn test_compute_sidebar_matches_basic() {
        let staged = vec![
            FileEntry { path: "src/main.rs".to_string(), index_status: Some(FileStatus::Modified), workdir_status: None },
            FileEntry { path: "src/lib.rs".to_string(), index_status: Some(FileStatus::Modified), workdir_status: None },
        ];
        let unstaged = vec![
            FileEntry { path: "src/main.rs".to_string(), index_status: None, workdir_status: Some(FileStatus::Modified) },
            FileEntry { path: "tests/test.rs".to_string(), index_status: None, workdir_status: Some(FileStatus::Modified) },
        ];
        let matches = compute_sidebar_matches(&staged, &unstaged, "main", false);
        assert_eq!(matches, vec![(SidebarSection::Staged, 0), (SidebarSection::Unstaged, 0)]);
    }

    #[test]
    fn test_compute_sidebar_matches_smart_case() {
        let staged = vec![
            FileEntry { path: "README.md".to_string(), index_status: Some(FileStatus::Modified), workdir_status: None },
            FileEntry { path: "readme.txt".to_string(), index_status: Some(FileStatus::Modified), workdir_status: None },
        ];
        let unstaged = vec![];
        // lowercase pattern -> case insensitive
        let matches_lower = compute_sidebar_matches(&staged, &unstaged, "readme", false);
        assert_eq!(matches_lower.len(), 2);
        // uppercase pattern -> case sensitive
        let matches_upper = compute_sidebar_matches(&staged, &unstaged, "README", true);
        assert_eq!(matches_upper, vec![(SidebarSection::Staged, 0)]);
    }

    #[test]
    fn test_compute_sidebar_matches_staged_then_unstaged_order() {
        let staged = vec![
            FileEntry { path: "b.rs".to_string(), index_status: Some(FileStatus::Modified), workdir_status: None },
        ];
        let unstaged = vec![
            FileEntry { path: "a.rs".to_string(), index_status: None, workdir_status: Some(FileStatus::Modified) },
        ];
        let matches = compute_sidebar_matches(&staged, &unstaged, ".rs", false);
        assert_eq!(matches, vec![(SidebarSection::Staged, 0), (SidebarSection::Unstaged, 0)]);
    }

    // ---- find_next_match ----

    #[test]
    fn test_find_next_match_forward_from_middle() {
        assert_eq!(find_next_match(5, Some(2), SearchDirection::Forward), Some(3));
    }

    #[test]
    fn test_find_next_match_backward_from_middle() {
        assert_eq!(find_next_match(5, Some(2), SearchDirection::Backward), Some(1));
    }

    #[test]
    fn test_find_next_match_forward_wraps() {
        assert_eq!(find_next_match(3, Some(2), SearchDirection::Forward), Some(0));
    }

    #[test]
    fn test_find_next_match_backward_wraps() {
        assert_eq!(find_next_match(3, Some(0), SearchDirection::Backward), Some(2));
    }

    #[test]
    fn test_find_next_match_empty() {
        assert_eq!(find_next_match(0, None, SearchDirection::Forward), None);
    }

    #[test]
    fn test_find_next_match_single() {
        assert_eq!(find_next_match(1, Some(0), SearchDirection::Forward), Some(0));
        assert_eq!(find_next_match(1, Some(0), SearchDirection::Backward), Some(0));
    }

    #[test]
    fn test_find_next_match_none_cursor() {
        assert_eq!(find_next_match(5, None, SearchDirection::Forward), Some(0));
        assert_eq!(find_next_match(5, None, SearchDirection::Backward), Some(0));
    }

    // ---- stale cursor reset on recompute ----

    #[test]
    fn test_stale_cursor_reset_prevents_oob_backward() {
        // Simulates: search in DiffView (cursor=5), switch to Sidebar, press N.
        // find_next_match with stale cursor > matches_len would return OOB index.
        // After ensure_* resets cursor to None, find_next_match returns Some(0).
        let stale_cursor = Some(5);
        let new_matches_len = 2;

        // Without fix: backward from 5 -> Some(4), which is OOB for len=2
        // With fix: cursor reset to None -> Some(0)
        let result = find_next_match(new_matches_len, None, SearchDirection::Backward);
        assert_eq!(result, Some(0));

        // Verify the dangerous case would produce OOB
        let dangerous = find_next_match(new_matches_len, stale_cursor, SearchDirection::Backward);
        assert!(dangerous.unwrap() >= new_matches_len, "stale cursor produces OOB index");
    }

    #[test]
    fn test_stale_cursor_reset_prevents_oob_forward() {
        let stale_cursor = Some(5);
        let new_matches_len = 2;

        // Forward wraps modularly: (5+1)%2 = 0, which happens to be valid,
        // but the cursor is still semantically wrong. Reset ensures clean start.
        let result = find_next_match(new_matches_len, None, SearchDirection::Forward);
        assert_eq!(result, Some(0));
    }

    // ---- find_nearest_diff_match ----

    #[test]
    fn test_find_nearest_diff_forward_from_middle() {
        let matches = vec![2, 5, 8];
        assert_eq!(find_nearest_diff_match(&matches, 4, SearchDirection::Forward), Some(1));
    }

    #[test]
    fn test_find_nearest_diff_forward_exact() {
        let matches = vec![2, 5, 8];
        assert_eq!(find_nearest_diff_match(&matches, 5, SearchDirection::Forward), Some(1));
    }

    #[test]
    fn test_find_nearest_diff_forward_wraps() {
        let matches = vec![2, 5];
        assert_eq!(find_nearest_diff_match(&matches, 7, SearchDirection::Forward), Some(0));
    }

    #[test]
    fn test_find_nearest_diff_backward_from_middle() {
        let matches = vec![2, 5, 8];
        assert_eq!(find_nearest_diff_match(&matches, 6, SearchDirection::Backward), Some(1));
    }

    #[test]
    fn test_find_nearest_diff_backward_wraps() {
        let matches = vec![2, 5];
        assert_eq!(find_nearest_diff_match(&matches, 1, SearchDirection::Backward), Some(1));
    }

    #[test]
    fn test_find_nearest_diff_empty() {
        assert_eq!(find_nearest_diff_match(&[], 0, SearchDirection::Forward), None);
    }

    // ---- find_nearest_sidebar_match ----

    #[test]
    fn test_find_nearest_sidebar_forward() {
        let matches = vec![
            (SidebarSection::Staged, 0),
            (SidebarSection::Staged, 2),
            (SidebarSection::Unstaged, 1),
        ];
        assert_eq!(
            find_nearest_sidebar_match(&matches, SidebarSection::Staged, 1, SearchDirection::Forward),
            Some(1)
        );
    }

    #[test]
    fn test_find_nearest_sidebar_backward() {
        let matches = vec![
            (SidebarSection::Staged, 0),
            (SidebarSection::Staged, 2),
            (SidebarSection::Unstaged, 1),
        ];
        assert_eq!(
            find_nearest_sidebar_match(&matches, SidebarSection::Unstaged, 0, SearchDirection::Backward),
            Some(1)
        );
    }

    #[test]
    fn test_find_nearest_sidebar_forward_wraps() {
        let matches = vec![
            (SidebarSection::Staged, 0),
        ];
        assert_eq!(
            find_nearest_sidebar_match(&matches, SidebarSection::Unstaged, 5, SearchDirection::Forward),
            Some(0)
        );
    }

    #[test]
    fn test_find_nearest_sidebar_empty() {
        assert_eq!(
            find_nearest_sidebar_match(&[], SidebarSection::Staged, 0, SearchDirection::Forward),
            None
        );
    }
}
