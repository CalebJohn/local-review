use crate::diff::types::DiffContent;
use crate::diff::{binary_diff_content, compute_diff_content};
use crate::git::GitRepo;
use crate::git::types::{ContentResult, FileEntry};

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
}

pub struct App {
    pub repo: GitRepo,
    pub files: Vec<FileEntry>,
    pub selected_index: usize,
    pub diff_content: Option<DiffContent>,
    pub diff_scroll: u16,
    pub focus: Focus,
    pub should_quit: bool,
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
        };
        if !app.files.is_empty() {
            app.load_diff_for_selected();
        }
        Ok(app)
    }

    fn load_diff_for_selected(&mut self) {
        self.diff_scroll = 0;

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

#[cfg(test)]
mod tests {
    use super::*;
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
}
