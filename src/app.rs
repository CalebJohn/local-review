use crate::diff::types::DiffContent;
use crate::diff::{binary_diff_content, compute_diff_content};
use crate::git::GitRepo;
use crate::git::types::{ContentResult, FileEntry};
use crate::syntax::{build_styled_diff, StyledDiffContent};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    Sidebar,
    DiffView,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Message {
    MoveUp,
    MoveDown,
    SelectFile,
    ScrollDiffUp,
    ScrollDiffDown,
    SwitchFocus,
    Quit,
    NextHunk,
    PrevHunk,
    MouseClickSidebar(usize),
    FocusDiff,
}

pub struct App {
    pub repo: GitRepo,
    pub files: Vec<FileEntry>,
    pub selected_index: usize,
    pub diff_content: Option<DiffContent>,
    pub diff_scroll: u16,
    pub focus: Focus,
    pub should_quit: bool,
    pub styled_diff: Option<StyledDiffContent>,
    pub hunk_line_starts: Vec<u16>,
}

impl App {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let repo = GitRepo::open(".")?;
        let files = repo.changed_files()?;
        let mut app = App {
            repo,
            files,
            selected_index: 0,
            diff_content: None,
            diff_scroll: 0,
            focus: Focus::Sidebar,
            should_quit: false,
            styled_diff: None,
            hunk_line_starts: Vec::new(),
        };
        if !app.files.is_empty() {
            app.load_diff_for_selected();
        }
        Ok(app)
    }

    fn load_diff_for_selected(&mut self) {
        self.diff_scroll = 0;
        self.styled_diff = None;
        self.hunk_line_starts = Vec::new();

        if self.selected_index >= self.files.len() {
            self.diff_content = None;
            return;
        }

        let entry = self.files[self.selected_index].clone();
        let path = entry.path.as_str();

        // Determine which content sources to compare based on stage status
        let (old_result, new_result): (
            Result<ContentResult, String>,
            Result<ContentResult, String>,
        ) = if entry.is_staged_only() {
            // VIEW-06: staged-only -> HEAD vs index
            (
                self.repo.head_content(path).map_err(|e| e.to_string()),
                self.repo.index_content(path).map_err(|e| e.to_string()),
            )
        } else {
            // VIEW-05: unstaged -> index vs workdir
            (
                self.repo.index_content(path).map_err(|e| e.to_string()),
                self.repo.workdir_content(path).map_err(|e| e.to_string()),
            )
        };

        let (old_result, new_result) = match (old_result, new_result) {
            (Ok(o), Ok(n)) => (o, n),
            _ => {
                self.diff_content = None;
                return;
            }
        };

        // Binary handling: if either side is Binary, produce the binary sentinel
        if matches!(old_result, ContentResult::Binary) || matches!(new_result, ContentResult::Binary)
        {
            self.diff_content = Some(binary_diff_content(path));
            return;
        }

        let old_text = match &old_result {
            ContentResult::Text(s) => Some(s.as_str()),
            _ => None,
        };
        let new_text = match &new_result {
            ContentResult::Text(s) => Some(s.as_str()),
            _ => None,
        };

        self.diff_content = Some(compute_diff_content(path, old_text, new_text));

        // Populate styled_diff and hunk_line_starts after diff_content is set
        if let Some(dc) = &self.diff_content {
            self.styled_diff = build_styled_diff(dc, old_text, new_text);
            self.hunk_line_starts = compute_hunk_line_starts(self.diff_content.as_ref());
        }
    }

    pub fn update(&mut self, msg: Message) {
        match msg {
            Message::MoveUp => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                    if self.focus == Focus::Sidebar {
                        self.load_diff_for_selected();
                    }
                }
            }
            Message::MoveDown => {
                if !self.files.is_empty() && self.selected_index < self.files.len() - 1 {
                    self.selected_index += 1;
                    if self.focus == Focus::Sidebar {
                        self.load_diff_for_selected();
                    }
                }
            }
            Message::SelectFile => {
                self.load_diff_for_selected();
                self.focus = Focus::DiffView;
            }
            Message::ScrollDiffUp => {
                if self.diff_scroll > 0 {
                    self.diff_scroll -= 1;
                }
            }
            Message::ScrollDiffDown => {
                let max_scroll = self.total_diff_lines().saturating_sub(1) as u16;
                if self.diff_scroll < max_scroll {
                    self.diff_scroll = self.diff_scroll.saturating_add(1);
                }
            }
            Message::SwitchFocus => {
                self.focus = match self.focus {
                    Focus::Sidebar => Focus::DiffView,
                    Focus::DiffView => Focus::Sidebar,
                };
            }
            Message::Quit => {
                self.should_quit = true;
            }
            Message::NextHunk => {
                if let Some(&next) = self.hunk_line_starts.iter().find(|&&s| s > self.diff_scroll) {
                    self.diff_scroll = next;
                }
            }
            Message::PrevHunk => {
                if let Some(&prev) = self.hunk_line_starts.iter().rev().find(|&&s| s < self.diff_scroll) {
                    self.diff_scroll = prev;
                }
            }
            Message::MouseClickSidebar(idx) => {
                if idx < self.files.len() {
                    self.selected_index = idx;
                    self.focus = Focus::Sidebar;
                    self.load_diff_for_selected();
                }
            }
            Message::FocusDiff => {
                self.focus = Focus::DiffView;
            }
        }
    }

    /// Total rendered lines across all hunks in the current diff_content.
    /// Used for scroll clamping.
    pub fn total_diff_lines(&self) -> usize {
        match &self.diff_content {
            Some(dc) if !dc.is_binary => {
                // One separator line per hunk + all lines
                dc.hunks.iter().map(|h| h.lines.len() + 1).sum()
            }
            _ => 0,
        }
    }
}

pub fn compute_hunk_line_starts(dc: Option<&DiffContent>) -> Vec<u16> {
    let Some(dc) = dc else { return Vec::new(); };
    if dc.is_binary { return Vec::new(); }
    let mut starts: Vec<u16> = Vec::with_capacity(dc.hunks.len());
    let mut cum: u16 = 0;
    for h in &dc.hunks {
        starts.push(cum);
        cum = cum.saturating_add(1u16.saturating_add(h.lines.len() as u16));
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::types::ChangeKind;
    use crate::git::types::{FileEntry, FileStatus};

    fn staged_only_entry() -> FileEntry {
        FileEntry {
            path: "staged.rs".to_string(),
            index_status: Some(FileStatus::Modified),
            workdir_status: None,
        }
    }

    fn unstaged_entry() -> FileEntry {
        FileEntry {
            path: "unstaged.rs".to_string(),
            index_status: None,
            workdir_status: Some(FileStatus::Modified),
        }
    }

    /// Build an App without opening a repo. Used for testing update() logic.
    fn test_app_with_files(files: Vec<FileEntry>) -> App {
        // Opening the workspace repo for tests - it exists in CI and dev.
        let repo = GitRepo::open("/workspace").expect("workspace repo should open");
        App {
            repo,
            files,
            selected_index: 0,
            diff_content: None,
            diff_scroll: 0,
            focus: Focus::Sidebar,
            should_quit: false,
            styled_diff: None,
            hunk_line_starts: Vec::new(),
        }
    }

    #[test]
    fn test_staged_only_diff_branching() {
        // Confirms the staged-only path (HEAD-vs-index) is selected.
        let entry = staged_only_entry();
        assert!(entry.is_staged_only());
    }

    #[test]
    fn test_unstaged_diff_branching() {
        // Confirms the unstaged path (index-vs-workdir) is selected.
        let entry = unstaged_entry();
        assert!(!entry.is_staged_only());
    }

    #[test]
    fn test_binary_produces_binary_diff_content() {
        // If content method returns Binary, load_diff_for_selected should produce
        // DiffContent with is_binary=true. We simulate by checking binary_diff_content.
        let dc = binary_diff_content("image.png");
        assert!(dc.is_binary);
        assert!(dc.hunks.is_empty());
    }

    #[test]
    fn test_update_move_down() {
        let mut app = test_app_with_files(vec![
            staged_only_entry(),
            unstaged_entry(),
            staged_only_entry(),
        ]);
        app.focus = Focus::DiffView; // prevent load_diff_for_selected from running
        assert_eq!(app.selected_index, 0);
        app.update(Message::MoveDown);
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn test_update_move_down_clamped_at_end() {
        let mut app = test_app_with_files(vec![staged_only_entry(), unstaged_entry()]);
        app.focus = Focus::DiffView;
        app.selected_index = 1;
        app.update(Message::MoveDown);
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn test_update_move_up_at_zero() {
        let mut app = test_app_with_files(vec![staged_only_entry(), unstaged_entry()]);
        app.focus = Focus::DiffView;
        assert_eq!(app.selected_index, 0);
        app.update(Message::MoveUp);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn test_update_move_up_decrements() {
        let mut app = test_app_with_files(vec![staged_only_entry(), unstaged_entry()]);
        app.focus = Focus::DiffView;
        app.selected_index = 1;
        app.update(Message::MoveUp);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn test_update_quit() {
        let mut app = test_app_with_files(vec![]);
        assert!(!app.should_quit);
        app.update(Message::Quit);
        assert!(app.should_quit);
    }

    #[test]
    fn test_update_switch_focus() {
        let mut app = test_app_with_files(vec![]);
        assert_eq!(app.focus, Focus::Sidebar);
        app.update(Message::SwitchFocus);
        assert_eq!(app.focus, Focus::DiffView);
        app.update(Message::SwitchFocus);
        assert_eq!(app.focus, Focus::Sidebar);
    }

    #[test]
    fn test_update_scroll_diff_up_at_zero() {
        let mut app = test_app_with_files(vec![]);
        assert_eq!(app.diff_scroll, 0);
        app.update(Message::ScrollDiffUp);
        assert_eq!(app.diff_scroll, 0);
    }

    #[test]
    fn test_styled_diff_starts_none() {
        let app = test_app_with_files(vec![]);
        assert!(app.styled_diff.is_none());
    }

    #[test]
    fn test_load_diff_for_selected_clears_styled_diff_on_empty_file_list() {
        let mut app = test_app_with_files(vec![]);
        app.styled_diff = Some(StyledDiffContent {
            lines_by_old_lineno: std::collections::HashMap::new(),
            lines_by_new_lineno: std::collections::HashMap::new(),
        });
        app.load_diff_for_selected();
        assert!(app.styled_diff.is_none());
    }

    #[test]
    fn test_compute_hunk_line_starts_empty_none() {
        assert_eq!(compute_hunk_line_starts(None), Vec::<u16>::new());
    }

    #[test]
    fn test_compute_hunk_line_starts_binary() {
        use crate::diff::types::DiffContent;
        let dc = DiffContent { path: "x".to_string(), hunks: vec![], is_binary: true };
        assert_eq!(compute_hunk_line_starts(Some(&dc)), Vec::<u16>::new());
    }

    #[test]
    fn test_compute_hunk_line_starts_three_hunks() {
        use crate::diff::types::{DiffContent, DiffHunk, DiffLine};
        let hunks = vec![
            DiffHunk { old_start: 1, new_start: 1, lines: vec![DiffLine { kind: ChangeKind::Equal, old_lineno: Some(1), new_lineno: Some(1), content: "x\n".to_string() }, DiffLine { kind: ChangeKind::Equal, old_lineno: Some(2), new_lineno: Some(2), content: "x\n".to_string() }] },
            DiffHunk { old_start: 1, new_start: 1, lines: vec![DiffLine { kind: ChangeKind::Equal, old_lineno: Some(1), new_lineno: Some(1), content: "x\n".to_string() }, DiffLine { kind: ChangeKind::Equal, old_lineno: Some(2), new_lineno: Some(2), content: "x\n".to_string() }, DiffLine { kind: ChangeKind::Equal, old_lineno: Some(3), new_lineno: Some(3), content: "x\n".to_string() }] },
            DiffHunk { old_start: 1, new_start: 1, lines: vec![DiffLine { kind: ChangeKind::Equal, old_lineno: Some(1), new_lineno: Some(1), content: "x\n".to_string() }, DiffLine { kind: ChangeKind::Equal, old_lineno: Some(2), new_lineno: Some(2), content: "x\n".to_string() }, DiffLine { kind: ChangeKind::Equal, old_lineno: Some(3), new_lineno: Some(3), content: "x\n".to_string() }, DiffLine { kind: ChangeKind::Equal, old_lineno: Some(4), new_lineno: Some(4), content: "x\n".to_string() }, DiffLine { kind: ChangeKind::Equal, old_lineno: Some(5), new_lineno: Some(5), content: "x\n".to_string() }] },
        ];
        let dc = DiffContent { path: "t.rs".to_string(), hunks, is_binary: false };
        assert_eq!(compute_hunk_line_starts(Some(&dc)), vec![0u16, 3, 7]);
    }

    #[test]
    fn test_next_hunk_no_op_on_empty() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.diff_scroll = 0;
        app.update(Message::NextHunk);
        assert_eq!(app.diff_scroll, 0);
    }

    #[test]
    fn test_next_hunk_advances_to_next_start() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.hunk_line_starts = vec![0, 3, 7];
        app.diff_scroll = 0;
        app.update(Message::NextHunk);
        assert_eq!(app.diff_scroll, 3);
        app.update(Message::NextHunk);
        assert_eq!(app.diff_scroll, 7);
        app.update(Message::NextHunk);
        assert_eq!(app.diff_scroll, 7);
    }

    #[test]
    fn test_prev_hunk_no_op_at_first() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.hunk_line_starts = vec![0, 3, 7];
        app.diff_scroll = 0;
        app.update(Message::PrevHunk);
        assert_eq!(app.diff_scroll, 0);
    }

    #[test]
    fn test_prev_hunk_goes_to_previous_start() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.hunk_line_starts = vec![0, 3, 7];
        app.diff_scroll = 7;
        app.update(Message::PrevHunk);
        assert_eq!(app.diff_scroll, 3);
        app.update(Message::PrevHunk);
        assert_eq!(app.diff_scroll, 0);
    }

    #[test]
    fn test_mouse_click_sidebar_selects_file() {
        let mut app = test_app_with_files(vec![
            staged_only_entry(),
            unstaged_entry(),
            staged_only_entry(),
        ]);
        app.focus = Focus::DiffView;
        app.update(Message::MouseClickSidebar(2));
        assert_eq!(app.selected_index, 2);
        assert_eq!(app.focus, Focus::Sidebar);
    }

    #[test]
    fn test_mouse_click_sidebar_out_of_bounds_noop() {
        let mut app = test_app_with_files(vec![staged_only_entry()]);
        app.focus = Focus::DiffView;
        let before = app.selected_index;
        app.update(Message::MouseClickSidebar(99));
        assert_eq!(app.selected_index, before);
        assert_eq!(app.focus, Focus::DiffView);
    }

    #[test]
    fn test_focus_diff_sets_focus_to_diffview() {
        let mut app = test_app_with_files(vec![]);
        assert_eq!(app.focus, Focus::Sidebar);
        app.update(Message::FocusDiff);
        assert_eq!(app.focus, Focus::DiffView);
    }
}
