use std::collections::HashMap;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SidebarSection {
    Staged,
    Unstaged,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PendingDiscard {
    File { path: String },
    Hunk { path: String, hunk_index: usize },
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
    MouseClickStagedSidebar(usize),
    MouseClickUnstagedSidebar(usize),
    FocusDiff,
    StageFile,
    UnstageFile,
    StageHunk,
    UnstageHunk,
    ScrollToTop,
    ScrollToBottom,
    ToggleSidebar,
    DiscardFile,
    DiscardHunk,
    WorkdirChanged,
    IndexChanged,
    ReloadDiff,
}

pub struct App {
    pub repo: GitRepo,
    pub staged_files: Vec<FileEntry>,
    pub unstaged_files: Vec<FileEntry>,
    pub selected_index: usize,
    pub sidebar_section: SidebarSection,
    pub diff_content: Option<DiffContent>,
    pub diff_scroll: u16,
    pub focus: Focus,
    pub should_quit: bool,
    pub styled_diff: Option<StyledDiffContent>,
    pub hunk_line_starts: Vec<u16>,
    pub current_hunk_index: Option<usize>,
    pub scroll_positions: HashMap<(String, SidebarSection), u16>,
    pub diff_stale: bool,
    pub auto_reload: bool,
    pub status_message: Option<String>,
    pub sidebar_collapsed: bool,
    pub pending_discard: Option<PendingDiscard>,
}

impl App {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let repo = GitRepo::open(".")?;
        let all_files = repo.changed_files()?;
        let (staged_files, unstaged_files) = Self::partition_files(&all_files);

        let initial_section = if !staged_files.is_empty() {
            SidebarSection::Staged
        } else {
            SidebarSection::Unstaged
        };

        let mut app = App {
            repo,
            staged_files,
            unstaged_files,
            selected_index: 0,
            sidebar_section: initial_section,
            diff_content: None,
            diff_scroll: 0,
            focus: Focus::Sidebar,
            should_quit: false,
            styled_diff: None,
            hunk_line_starts: Vec::new(),
            current_hunk_index: None,
            scroll_positions: HashMap::new(),
            diff_stale: false,
            auto_reload: false,
            status_message: None,
            sidebar_collapsed: false,
            pending_discard: None,
        };
        if !app.current_section_files().is_empty() {
            app.load_diff_for_selected();
        }
        Ok(app)
    }

    fn partition_files(files: &[FileEntry]) -> (Vec<FileEntry>, Vec<FileEntry>) {
        let staged: Vec<FileEntry> = files
            .iter()
            .filter(|f| f.index_status.is_some())
            .cloned()
            .collect();
        let unstaged: Vec<FileEntry> = files
            .iter()
            .filter(|f| f.workdir_status.is_some())
            .cloned()
            .collect();
        (staged, unstaged)
    }

    pub fn current_section_files(&self) -> &[FileEntry] {
        match self.sidebar_section {
            SidebarSection::Staged => &self.staged_files,
            SidebarSection::Unstaged => &self.unstaged_files,
        }
    }

    pub fn selected_entry(&self) -> Option<&FileEntry> {
        self.current_section_files().get(self.selected_index)
    }

    fn save_scroll_position(&mut self) {
        if let Some(entry) = self.selected_entry() {
            let key = (entry.path.clone(), self.sidebar_section);
            self.scroll_positions.insert(key, self.diff_scroll);
        }
    }

    fn load_diff_for_selected(&mut self) {
        // Restore scroll position for the newly selected file
        let files = self.current_section_files();
        if self.selected_index < files.len() {
            let entry = &files[self.selected_index];
            let key = (entry.path.clone(), self.sidebar_section);
            self.diff_scroll = self.scroll_positions.get(&key).copied().unwrap_or(0);
        } else {
            self.diff_scroll = 0;
        }

        self.styled_diff = None;
        self.hunk_line_starts = Vec::new();
        self.diff_stale = false;

        let files = self.current_section_files();
        if self.selected_index >= files.len() {
            self.diff_content = None;
            return;
        }

        let entry = files[self.selected_index].clone();
        let path = entry.path.as_str();

        // Determine which content sources to compare based on sidebar section
        let (old_result, new_result): (
            Result<ContentResult, String>,
            Result<ContentResult, String>,
        ) = match self.sidebar_section {
            SidebarSection::Staged => {
                // Staged section: HEAD vs index
                (
                    self.repo.head_content(path).map_err(|e| e.to_string()),
                    self.repo.index_content(path).map_err(|e| e.to_string()),
                )
            }
            SidebarSection::Unstaged => {
                // Unstaged section: index vs workdir
                (
                    self.repo.index_content(path).map_err(|e| e.to_string()),
                    self.repo.workdir_content(path).map_err(|e| e.to_string()),
                )
            }
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

        // Set initial hunk index when diff loads successfully
        if !self.hunk_line_starts.is_empty() {
            if self.current_hunk_index.is_none() {
                self.current_hunk_index = Some(0);
            }
        } else {
            self.current_hunk_index = None;
        }
    }

    pub fn selected_file_path(&self) -> Option<String> {
        self.selected_entry().map(|e| e.path.clone())
    }

    pub fn update(&mut self, msg: Message) {
        // Clear pending discard on any non-discard action
        if !matches!(msg, Message::DiscardFile | Message::DiscardHunk)
            && self.pending_discard.take().is_some()
        {
            self.status_message = None;
        }

        match msg {
            Message::MoveUp => {
                self.status_message = None;
                if self.selected_index > 0 {
                    self.save_scroll_position();
                    self.selected_index -= 1;
                    self.load_diff_for_selected();
                } else if self.sidebar_section == SidebarSection::Unstaged
                    && !self.staged_files.is_empty()
                {
                    // Cross from top of unstaged to bottom of staged
                    self.save_scroll_position();
                    self.sidebar_section = SidebarSection::Staged;
                    self.selected_index = self.staged_files.len() - 1;
                    self.load_diff_for_selected();
                }
            }
            Message::MoveDown => {
                self.status_message = None;
                let section_len = self.current_section_files().len();
                if section_len > 0 && self.selected_index < section_len - 1 {
                    self.save_scroll_position();
                    self.selected_index += 1;
                    self.load_diff_for_selected();
                } else if self.sidebar_section == SidebarSection::Staged
                    && !self.unstaged_files.is_empty()
                {
                    // Cross from bottom of staged to top of unstaged
                    self.save_scroll_position();
                    self.sidebar_section = SidebarSection::Unstaged;
                    self.selected_index = 0;
                    self.load_diff_for_selected();
                }
            }
            Message::SelectFile => {
                self.status_message = None;
                self.load_diff_for_selected();
                self.focus = Focus::DiffView;
            }
            Message::ScrollDiffUp => {
                self.status_message = None;
                if self.diff_scroll > 0 {
                    self.diff_scroll -= 1;
                    self.update_hunk_from_scroll();
                }
            }
            Message::ScrollDiffDown => {
                self.status_message = None;
                let max_scroll = self.total_diff_lines().saturating_sub(1) as u16;
                if self.diff_scroll < max_scroll {
                    self.diff_scroll = self.diff_scroll.saturating_add(1);
                    self.update_hunk_from_scroll();
                }
            }
            Message::ScrollToTop => {
                self.status_message = None;
                self.diff_scroll = 0;
                self.update_hunk_from_scroll();
            }
            Message::ScrollToBottom => {
                self.status_message = None;
                self.diff_scroll = self.total_diff_lines().saturating_sub(1) as u16;
                self.update_hunk_from_scroll();
            }
            Message::SwitchFocus => {
                self.status_message = None;
                if self.sidebar_collapsed {
                    // Can't switch to sidebar when it's hidden
                    return;
                }
                self.focus = match self.focus {
                    Focus::Sidebar => Focus::DiffView,
                    Focus::DiffView => Focus::Sidebar,
                };
            }
            Message::ToggleSidebar => {
                self.status_message = None;
                self.sidebar_collapsed = !self.sidebar_collapsed;
                if self.sidebar_collapsed && self.focus == Focus::Sidebar {
                    self.focus = Focus::DiffView;
                }
            }
            Message::Quit => {
                self.should_quit = true;
            }
            Message::NextHunk => {
                self.status_message = None;
                if let Some(pos) = self.hunk_line_starts.iter().position(|&s| s > self.diff_scroll) {
                    self.diff_scroll = self.hunk_line_starts[pos];
                    self.current_hunk_index = Some(pos);
                } else if !self.hunk_line_starts.is_empty() {
                    // Wrap to last hunk
                    let pos = self.hunk_line_starts.len() - 1;
                    self.diff_scroll = self.hunk_line_starts[pos];
                    self.current_hunk_index = Some(pos);
                }
            }
            Message::PrevHunk => {
                self.status_message = None;
                if let Some(pos) = self.hunk_line_starts.iter().rposition(|&s| s < self.diff_scroll) {
                    self.diff_scroll = self.hunk_line_starts[pos];
                    self.current_hunk_index = Some(pos);
                } else if !self.hunk_line_starts.is_empty() {
                    // Wrap to first hunk
                    self.diff_scroll = self.hunk_line_starts[0];
                    self.current_hunk_index = Some(0);
                }
            }
            Message::MouseClickStagedSidebar(idx) => {
                if idx < self.staged_files.len() {
                    self.save_scroll_position();
                    self.sidebar_section = SidebarSection::Staged;
                    self.selected_index = idx;
                    self.focus = Focus::Sidebar;
                    self.load_diff_for_selected();
                }
            }
            Message::MouseClickUnstagedSidebar(idx) => {
                if idx < self.unstaged_files.len() {
                    self.save_scroll_position();
                    self.sidebar_section = SidebarSection::Unstaged;
                    self.selected_index = idx;
                    self.focus = Focus::Sidebar;
                    self.load_diff_for_selected();
                }
            }
            Message::FocusDiff => {
                self.focus = Focus::DiffView;
                self.update_hunk_from_scroll();
            }
            Message::StageFile => {
                if let Some(entry) = self.selected_entry().cloned() {
                    if let Err(e) = self.repo.stage_file(&entry.path) {
                        self.status_message = Some(format!("Stage failed: {}", e));
                    }
                    self.refresh_files();
                }
            }
            Message::UnstageFile => {
                if let Some(entry) = self.selected_entry().cloned() {
                    if let Err(e) = self.repo.unstage_file(&entry.path) {
                        self.status_message = Some(format!("Unstage failed: {}", e));
                    }
                    self.refresh_files();
                }
            }
            Message::StageHunk => {
                if self.sidebar_section != SidebarSection::Unstaged || self.diff_stale {
                    return;
                }
                let entry = self.selected_entry().cloned();
                if let (Some(entry), Some(dc), Some(hunk_idx)) = (
                    entry,
                    self.diff_content.as_ref(),
                    self.current_hunk_index,
                ) {
                    if let Some(hunk) = dc.hunks.get(hunk_idx) {
                        let old_content = self.repo.index_content(&entry.path)
                            .ok()
                            .and_then(|c| match c { ContentResult::Text(s) => Some(s.clone()), _ => None });
                        let new_content = self.repo.workdir_content(&entry.path)
                            .ok()
                            .and_then(|c| match c { ContentResult::Text(s) => Some(s.clone()), _ => None });
                        if let (Some(old), Some(new)) = (old_content, new_content) {
                            if let Err(e) = self.repo.stage_hunk(&entry.path, &old, &new, hunk) {
                                self.status_message = Some(format!("Stage hunk failed: {}", e));
                            }
                            self.refresh_files();
                            if let Some(idx) = self.current_hunk_index {
                                if self.hunk_line_starts.is_empty() {
                                    self.current_hunk_index = None;
                                } else {
                                    let clamped = idx.min(self.hunk_line_starts.len() - 1);
                                    self.current_hunk_index = Some(clamped);
                                    self.diff_scroll = self.hunk_line_starts[clamped];
                                }
                            }
                        }
                    }
                }
            }
            Message::UnstageHunk => {
                if self.sidebar_section != SidebarSection::Staged || self.diff_stale {
                    return;
                }
                let entry = self.selected_entry().cloned();
                if let (Some(entry), Some(dc), Some(hunk_idx)) = (
                    entry,
                    self.diff_content.as_ref(),
                    self.current_hunk_index,
                ) {
                    if let Some(hunk) = dc.hunks.get(hunk_idx) {
                        let index_content = self.repo.index_content(&entry.path)
                            .ok()
                            .and_then(|c| match c { ContentResult::Text(s) => Some(s.clone()), _ => None });
                        if let Some(idx_content) = index_content {
                            if let Err(e) = self.repo.unstage_hunk(&entry.path, &idx_content, hunk) {
                                self.status_message = Some(format!("Unstage hunk failed: {}", e));
                            }
                            self.refresh_files();
                            if let Some(idx) = self.current_hunk_index {
                                if self.hunk_line_starts.is_empty() {
                                    self.current_hunk_index = None;
                                } else {
                                    let clamped = idx.min(self.hunk_line_starts.len() - 1);
                                    self.current_hunk_index = Some(clamped);
                                    self.diff_scroll = self.hunk_line_starts[clamped];
                                }
                            }
                        }
                    }
                }
            }
            Message::DiscardFile => {
                if self.sidebar_section != SidebarSection::Unstaged {
                    return;
                }
                if let Some(entry) = self.selected_entry().cloned() {
                    let pending = PendingDiscard::File { path: entry.path.clone() };
                    if self.pending_discard.as_ref() == Some(&pending) {
                        // Confirmed — execute discard
                        self.pending_discard = None;
                        self.status_message = None;
                        if let Err(e) = self.repo.discard_file(&entry.path) {
                            self.status_message = Some(format!("Discard failed: {}", e));
                        }
                        self.refresh_files();
                    } else {
                        // First press — ask for confirmation
                        self.pending_discard = Some(pending);
                        self.status_message = Some(
                            format!("Discard all changes to {}? Press d again to confirm (IRREVERSIBLE)", entry.path),
                        );
                    }
                }
            }
            Message::DiscardHunk => {
                if self.sidebar_section != SidebarSection::Unstaged || self.diff_stale {
                    return;
                }
                let entry = self.selected_entry().cloned();
                if let (Some(entry), Some(hunk_idx)) = (entry, self.current_hunk_index) {
                    let pending = PendingDiscard::Hunk {
                        path: entry.path.clone(),
                        hunk_index: hunk_idx,
                    };
                    if self.pending_discard.as_ref() == Some(&pending) {
                        // Confirmed — execute discard
                        self.pending_discard = None;
                        self.status_message = None;
                        if let Some(dc) = self.diff_content.as_ref() {
                            if let Some(hunk) = dc.hunks.get(hunk_idx) {
                                let workdir_content = self.repo.workdir_content(&entry.path)
                                    .ok()
                                    .and_then(|c| match c { ContentResult::Text(s) => Some(s), _ => None });
                                if let Some(wc) = workdir_content {
                                    if let Err(e) = self.repo.discard_hunk(&entry.path, &wc, hunk) {
                                        self.status_message = Some(format!("Discard hunk failed: {}", e));
                                    }
                                    self.refresh_files();
                                    if let Some(idx) = self.current_hunk_index {
                                        if self.hunk_line_starts.is_empty() {
                                            self.current_hunk_index = None;
                                        } else {
                                            let clamped = idx.min(self.hunk_line_starts.len() - 1);
                                            self.current_hunk_index = Some(clamped);
                                            self.diff_scroll = self.hunk_line_starts[clamped];
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        // First press — ask for confirmation
                        self.pending_discard = Some(pending);
                        self.status_message = Some(
                            "Discard this hunk? Press d again to confirm (IRREVERSIBLE)".to_string(),
                        );
                    }
                }
            }
            Message::WorkdirChanged => {
                self.refresh_file_list();
                if self.auto_reload {
                    self.load_diff_for_selected();
                } else {
                    self.diff_stale = true;
                }
            }
            Message::IndexChanged => {
                self.refresh_files();
            }
            Message::ReloadDiff => {
                self.load_diff_for_selected();
            }
        }
    }

    fn refresh_file_list(&mut self) {
        if let Ok(all_files) = self.repo.changed_files() {
            let selected_path = self.selected_entry().map(|e| e.path.clone());
            let old_section = self.sidebar_section;
            let (staged, unstaged) = Self::partition_files(&all_files);
            self.staged_files = staged;
            self.unstaged_files = unstaged;

            // Try to preserve selection in the same section
            if let Some(ref path) = selected_path {
                let section_files = match old_section {
                    SidebarSection::Staged => &self.staged_files,
                    SidebarSection::Unstaged => &self.unstaged_files,
                };
                if let Some(pos) = section_files.iter().position(|f| f.path == *path) {
                    self.selected_index = pos;
                } else {
                    // File moved to other section or disappeared
                    let section_len = section_files.len();
                    if section_len == 0 {
                        // Section is now empty, switch to the other
                        self.sidebar_section = match old_section {
                            SidebarSection::Staged => SidebarSection::Unstaged,
                            SidebarSection::Unstaged => SidebarSection::Staged,
                        };
                        self.selected_index = 0;
                    } else {
                        self.selected_index = self.selected_index.min(section_len - 1);
                    }
                }
            }
        }
    }

    pub fn refresh_files(&mut self) {
        self.refresh_file_list();
        self.load_diff_for_selected();
    }

    fn update_hunk_from_scroll(&mut self) {
        if self.hunk_line_starts.is_empty() {
            self.current_hunk_index = None;
            return;
        }
        // Switch when a hunk is within 3 lines of the top of the viewport
        let threshold = self.diff_scroll.saturating_add(3);
        self.current_hunk_index = self
            .hunk_line_starts
            .iter()
            .rposition(|&s| s <= threshold);
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

    fn both_entry() -> FileEntry {
        FileEntry {
            path: "both.rs".to_string(),
            index_status: Some(FileStatus::Modified),
            workdir_status: Some(FileStatus::Modified),
        }
    }

    /// Build an App without opening a repo. Used for testing update() logic.
    fn test_app_with_files(files: Vec<FileEntry>) -> App {
        let repo = GitRepo::open(".").expect("repo should open");
        let (staged_files, unstaged_files) = App::partition_files(&files);
        let initial_section = if !staged_files.is_empty() {
            SidebarSection::Staged
        } else {
            SidebarSection::Unstaged
        };
        App {
            repo,
            staged_files,
            unstaged_files,
            selected_index: 0,
            sidebar_section: initial_section,
            diff_content: None,
            diff_scroll: 0,
            focus: Focus::Sidebar,
            should_quit: false,
            styled_diff: None,
            hunk_line_starts: Vec::new(),
            current_hunk_index: None,
            scroll_positions: HashMap::new(),
            diff_stale: false,
            auto_reload: false,
            status_message: None,
            sidebar_collapsed: false,
            pending_discard: None,
        }
    }

    #[test]
    fn test_partition_files() {
        let files = vec![staged_only_entry(), unstaged_entry(), both_entry()];
        let (staged, unstaged) = App::partition_files(&files);
        // staged_only_entry has index_status, both_entry has index_status
        assert_eq!(staged.len(), 2);
        // unstaged_entry has workdir_status, both_entry has workdir_status
        assert_eq!(unstaged.len(), 2);
    }

    #[test]
    fn test_binary_produces_binary_diff_content() {
        let dc = binary_diff_content("image.png");
        assert!(dc.is_binary);
        assert!(dc.hunks.is_empty());
    }

    #[test]
    fn test_update_move_down_within_section() {
        let mut app = test_app_with_files(vec![
            staged_only_entry(),
            FileEntry {
                path: "staged2.rs".to_string(),
                index_status: Some(FileStatus::Added),
                workdir_status: None,
            },
        ]);
        app.focus = Focus::DiffView;
        assert_eq!(app.sidebar_section, SidebarSection::Staged);
        assert_eq!(app.selected_index, 0);
        app.update(Message::MoveDown);
        assert_eq!(app.selected_index, 1);
        assert_eq!(app.sidebar_section, SidebarSection::Staged);
    }

    #[test]
    fn test_update_move_down_crosses_to_unstaged() {
        let mut app = test_app_with_files(vec![staged_only_entry(), unstaged_entry()]);
        app.focus = Focus::DiffView;
        assert_eq!(app.sidebar_section, SidebarSection::Staged);
        assert_eq!(app.staged_files.len(), 1);
        assert_eq!(app.unstaged_files.len(), 1);
        // At bottom of staged (index 0, len 1), move down should cross
        app.update(Message::MoveDown);
        assert_eq!(app.sidebar_section, SidebarSection::Unstaged);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn test_update_move_up_crosses_to_staged() {
        let mut app = test_app_with_files(vec![staged_only_entry(), unstaged_entry()]);
        app.focus = Focus::DiffView;
        app.sidebar_section = SidebarSection::Unstaged;
        app.selected_index = 0;
        app.update(Message::MoveUp);
        assert_eq!(app.sidebar_section, SidebarSection::Staged);
        assert_eq!(app.selected_index, 0); // last item in staged (len 1, so index 0)
    }

    #[test]
    fn test_update_move_up_at_top_of_staged_stays() {
        let mut app = test_app_with_files(vec![staged_only_entry()]);
        app.focus = Focus::DiffView;
        assert_eq!(app.sidebar_section, SidebarSection::Staged);
        assert_eq!(app.selected_index, 0);
        app.update(Message::MoveUp);
        assert_eq!(app.selected_index, 0);
        assert_eq!(app.sidebar_section, SidebarSection::Staged);
    }

    #[test]
    fn test_update_move_down_at_bottom_of_unstaged_stays() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.focus = Focus::DiffView;
        assert_eq!(app.sidebar_section, SidebarSection::Unstaged);
        app.update(Message::MoveDown);
        assert_eq!(app.selected_index, 0);
        assert_eq!(app.sidebar_section, SidebarSection::Unstaged);
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
    fn test_mouse_click_staged_sidebar() {
        let mut app = test_app_with_files(vec![
            staged_only_entry(),
            FileEntry {
                path: "staged2.rs".to_string(),
                index_status: Some(FileStatus::Added),
                workdir_status: None,
            },
        ]);
        app.focus = Focus::DiffView;
        app.update(Message::MouseClickStagedSidebar(1));
        assert_eq!(app.selected_index, 1);
        assert_eq!(app.sidebar_section, SidebarSection::Staged);
        assert_eq!(app.focus, Focus::Sidebar);
    }

    #[test]
    fn test_mouse_click_unstaged_sidebar() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.focus = Focus::DiffView;
        app.update(Message::MouseClickUnstagedSidebar(0));
        assert_eq!(app.selected_index, 0);
        assert_eq!(app.sidebar_section, SidebarSection::Unstaged);
        assert_eq!(app.focus, Focus::Sidebar);
    }

    #[test]
    fn test_mouse_click_staged_out_of_bounds_noop() {
        let mut app = test_app_with_files(vec![staged_only_entry()]);
        app.focus = Focus::DiffView;
        let before = app.selected_index;
        app.update(Message::MouseClickStagedSidebar(99));
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

    #[test]
    fn test_move_down_resets_scroll_for_new_file() {
        let mut app = test_app_with_files(vec![
            staged_only_entry(),
            FileEntry {
                path: "staged2.rs".to_string(),
                index_status: Some(FileStatus::Added),
                workdir_status: None,
            },
        ]);
        // Simulate scrolling down in the first file
        app.diff_scroll = 42;
        // Navigate to the second file
        app.update(Message::MoveDown);
        assert_eq!(app.selected_index, 1);
        // Second file was never visited, scroll should be 0
        assert_eq!(app.diff_scroll, 0);
    }

    #[test]
    fn test_move_up_resets_scroll_for_new_file() {
        let mut app = test_app_with_files(vec![
            staged_only_entry(),
            FileEntry {
                path: "staged2.rs".to_string(),
                index_status: Some(FileStatus::Added),
                workdir_status: None,
            },
        ]);
        app.selected_index = 1;
        app.diff_scroll = 30;
        app.update(Message::MoveUp);
        assert_eq!(app.selected_index, 0);
        assert_eq!(app.diff_scroll, 0);
    }

    #[test]
    fn test_scroll_position_saved_and_restored_on_navigation() {
        let mut app = test_app_with_files(vec![
            staged_only_entry(),
            FileEntry {
                path: "staged2.rs".to_string(),
                index_status: Some(FileStatus::Added),
                workdir_status: None,
            },
        ]);
        // Scroll in first file
        app.diff_scroll = 15;
        // Move to second file — should save 15 for first file
        app.update(Message::MoveDown);
        assert_eq!(app.diff_scroll, 0);
        // Scroll in second file
        app.diff_scroll = 25;
        // Move back to first file — should save 25 for second, restore 15 for first
        app.update(Message::MoveUp);
        assert_eq!(app.diff_scroll, 15);
        // Move to second file again — should restore 25
        app.update(Message::MoveDown);
        assert_eq!(app.diff_scroll, 25);
    }

    #[test]
    fn test_cross_section_resets_scroll_for_new_file() {
        let mut app = test_app_with_files(vec![staged_only_entry(), unstaged_entry()]);
        assert_eq!(app.sidebar_section, SidebarSection::Staged);
        app.diff_scroll = 20;
        // Cross from staged to unstaged
        app.update(Message::MoveDown);
        assert_eq!(app.sidebar_section, SidebarSection::Unstaged);
        assert_eq!(app.diff_scroll, 0);
        // Scroll in unstaged, then cross back
        app.diff_scroll = 10;
        app.update(Message::MoveUp);
        assert_eq!(app.sidebar_section, SidebarSection::Staged);
        assert_eq!(app.diff_scroll, 20);
    }

    #[test]
    fn test_mouse_click_resets_scroll_for_new_file() {
        let mut app = test_app_with_files(vec![
            staged_only_entry(),
            FileEntry {
                path: "staged2.rs".to_string(),
                index_status: Some(FileStatus::Added),
                workdir_status: None,
            },
        ]);
        app.diff_scroll = 50;
        app.update(Message::MouseClickStagedSidebar(1));
        assert_eq!(app.selected_index, 1);
        assert_eq!(app.diff_scroll, 0);
    }

    #[test]
    fn test_mouse_click_cross_section_saves_scroll() {
        let mut app = test_app_with_files(vec![staged_only_entry(), unstaged_entry()]);
        assert_eq!(app.sidebar_section, SidebarSection::Staged);
        app.diff_scroll = 33;
        // Click into unstaged section
        app.update(Message::MouseClickUnstagedSidebar(0));
        assert_eq!(app.diff_scroll, 0);
        // Click back to staged
        app.update(Message::MouseClickStagedSidebar(0));
        assert_eq!(app.diff_scroll, 33);
    }

    // ---- discard confirmation flow tests ----

    #[test]
    fn test_discard_file_noop_in_staged_section() {
        let mut app = test_app_with_files(vec![staged_only_entry()]);
        assert_eq!(app.sidebar_section, SidebarSection::Staged);
        app.update(Message::DiscardFile);
        assert!(app.pending_discard.is_none());
        assert!(app.status_message.is_none());
    }

    #[test]
    fn test_discard_file_first_press_sets_pending() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        assert_eq!(app.sidebar_section, SidebarSection::Unstaged);
        app.update(Message::DiscardFile);
        assert_eq!(
            app.pending_discard,
            Some(PendingDiscard::File { path: "unstaged.rs".to_string() }),
        );
        assert!(app.status_message.is_some());
        assert!(app.status_message.as_ref().unwrap().contains("IRREVERSIBLE"));
    }

    #[test]
    fn test_discard_other_key_clears_pending() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.update(Message::DiscardFile);
        assert!(app.pending_discard.is_some());
        assert!(app.status_message.is_some());
        // Any non-discard message should clear pending
        app.update(Message::ScrollDiffDown);
        assert!(app.pending_discard.is_none());
        assert!(app.status_message.is_none());
    }

    #[test]
    fn test_discard_hunk_noop_in_staged_section() {
        let mut app = test_app_with_files(vec![staged_only_entry()]);
        app.focus = Focus::DiffView;
        app.current_hunk_index = Some(0);
        app.update(Message::DiscardHunk);
        assert!(app.pending_discard.is_none());
    }

    #[test]
    fn test_discard_hunk_first_press_sets_pending() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.focus = Focus::DiffView;
        app.current_hunk_index = Some(0);
        app.update(Message::DiscardHunk);
        assert_eq!(
            app.pending_discard,
            Some(PendingDiscard::Hunk { path: "unstaged.rs".to_string(), hunk_index: 0 }),
        );
        assert!(app.status_message.as_ref().unwrap().contains("IRREVERSIBLE"));
    }

    #[test]
    fn test_discard_hunk_noop_when_no_hunk_selected() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.focus = Focus::DiffView;
        app.current_hunk_index = None;
        app.update(Message::DiscardHunk);
        assert!(app.pending_discard.is_none());
    }

    #[test]
    fn test_discard_file_then_hunk_resets_pending() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.current_hunk_index = Some(0);

        // First: file discard pending
        app.update(Message::DiscardFile);
        assert!(matches!(app.pending_discard, Some(PendingDiscard::File { .. })));

        // Then: hunk discard — should replace pending (not confirm file discard)
        app.update(Message::DiscardHunk);
        assert!(matches!(app.pending_discard, Some(PendingDiscard::Hunk { .. })));
    }

    #[test]
    fn test_discard_hunk_noop_when_diff_stale() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.focus = Focus::DiffView;
        app.current_hunk_index = Some(0);
        app.diff_stale = true;
        app.update(Message::DiscardHunk);
        assert!(app.pending_discard.is_none());
    }
}