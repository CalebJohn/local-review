use std::cell::Cell;
use std::collections::HashMap;

use crate::classify::{classify_diff, language_for_extension};
use crate::diff::types::DiffContent;
use crate::diff::{binary_diff_content, compute_diff_content, compute_full_diff_content};
use crate::git::GitRepo;
use crate::git::types::{ContentResult, FileEntry};
use crate::syntax::{build_styled_diff, StyledDiffContent};
use crate::undo::UndoManager;

mod comment;
mod geometry;
mod navigation;
mod search;
mod staging;
mod visual;
pub use comment::CommentContext;
pub use geometry::{diff_line_at_row, row_for_diff_line};
pub(crate) use geometry::nearest_row_for_line;

const NO_ACTIVE_HUNK_MSG: &str = "No active hunk in view — press ] to navigate to a hunk";
const SCROLL_MARGIN: u16 = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    Sidebar,
    DiffView,
    CommentInput,
    SearchInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    Forward,
    Backward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppMode {
    Normal,
    Visual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
    SelectSidebar,
    ScrollDiffUp,
    ScrollDiffDown,
    SwitchFocus,
    Quit,
    NextHunk,
    PrevHunk,
    MouseClickStagedSidebar(usize),
    MouseClickUnstagedSidebar(usize),
    MouseClickDiffLine(usize),
    MouseDragDiff(usize),
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
    ToggleFullFile,
    Undo,
    Redo,
    StartComment,
    CommentInputChar(char),
    CommentInputBackspace,
    CommentInputSubmit,
    CommentInputCancel,
    EnterVisual,
    ExitVisual,
    MoveCursorUp,
    MoveCursorDown,
    ExtendSelectionUp,
    ExtendSelectionDown,
    StageSelectedLines,
    UnstageSelectedLines,
    ToggleSemanticFilter,
    SearchForward,
    SearchBackward,
    SearchInputChar(char),
    SearchInputBackspace,
    SearchInputSubmit,
    SearchInputCancel,
    NextMatch,
    PrevMatch,
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
    pub current_hunk_index: Option<usize>,
    pub scroll_positions: HashMap<(String, SidebarSection, bool), u16>,
    pub diff_stale: bool,
    pub auto_reload: bool,
    pub status_message: Option<String>,
    pub sidebar_collapsed: bool,
    pub pending_discard: Option<PendingDiscard>,
    pub show_full_file: bool,
    pub diff_viewport_height: Cell<u16>,
    pub undo: UndoManager,
    pub comment_input: String,
    pub comment_context: Option<CommentContext>,
    pub mode: AppMode,
    pub diff_cursor: usize,
    pub visual_selection: Option<(usize, usize)>,
    pub visual_cursor: usize,
    pub visual_anchor: usize,
    pub visual_from_mouse: bool,
    pub semantic_filter: bool,
    pub formatting_only_cache: HashMap<(String, SidebarSection), bool>,
    pub search_query: String,
    pub search_direction: SearchDirection,
    pub search_origin: Focus,
    pub search_pattern: Option<String>,
    pub search_case_sensitive: bool,
    pub search_matches: Vec<usize>,
    pub search_sidebar_matches: Vec<(SidebarSection, usize)>,
    pub search_match_cursor: Option<usize>,
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
            current_hunk_index: None,
            scroll_positions: HashMap::new(),
            diff_stale: false,
            auto_reload: false,
            status_message: None,
            sidebar_collapsed: false,
            pending_discard: None,
            show_full_file: false,
            diff_viewport_height: Cell::new(0),
            undo: UndoManager::new(),
            comment_input: String::new(),
            comment_context: None,
            mode: AppMode::Normal,
            diff_cursor: 0,
            visual_selection: None,
            visual_cursor: 0,
            visual_anchor: 0,
            visual_from_mouse: false,
            semantic_filter: false,
            formatting_only_cache: HashMap::new(),
            search_query: String::new(),
            search_direction: SearchDirection::Forward,
            search_origin: Focus::Sidebar,
            search_pattern: None,
            search_case_sensitive: false,
            search_matches: Vec::new(),
            search_sidebar_matches: Vec::new(),
            search_match_cursor: None,
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
        let mut unstaged: Vec<FileEntry> = files
            .iter()
            .filter(|f| f.workdir_status.is_some())
            .cloned()
            .collect();
        unstaged.sort_by_key(|f| {
            matches!(f.workdir_status, Some(crate::git::types::FileStatus::Untracked))
        });
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

    fn load_diff_for_selected(&mut self) {
        // Restore scroll position for the newly selected file (per-mode)
        let files = self.current_section_files();
        if self.selected_index < files.len() {
            let entry = &files[self.selected_index];
            let key = (entry.path.clone(), self.sidebar_section, self.show_full_file);
            self.diff_scroll = self.scroll_positions.get(&key).copied().unwrap_or(0);
        } else {
            self.diff_scroll = 0;
        }

        self.styled_diff = None;
        self.diff_stale = false;
        self.mode = AppMode::Normal;
        self.diff_cursor = 0;
        self.visual_selection = None;
        self.visual_from_mouse = false;
        self.search_matches.clear();
        self.search_match_cursor = None;

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

        self.diff_content = Some(if self.show_full_file {
            compute_full_diff_content(path, old_text, new_text)
        } else {
            compute_diff_content(path, old_text, new_text)
        });

        // Classify diff lines as formatting-only or semantic (Task 4)
        if let Some(dc) = &mut self.diff_content {
            let old_str = old_text.unwrap_or("");
            let new_str = new_text.unwrap_or("");
            let ext = std::path::Path::new(&dc.path)
                .extension()
                .and_then(|e| e.to_str());
            let lang = ext.and_then(language_for_extension);
            classify_diff(&mut dc.hunks, old_str, new_str, lang, ext.unwrap_or(""));

            // Cache whether this file has only formatting changes (Task 10)
            let all_formatting = dc.hunks.iter().all(|h| h.is_formatting_only());
            self.formatting_only_cache.insert((dc.path.clone(), self.sidebar_section), all_formatting);
        }

        if let Some(dc) = &self.diff_content {
            self.styled_diff = build_styled_diff(dc, old_text, new_text);
        }

        // Set the active hunk based on the current cursor position.
        self.update_hunk_from_cursor();
    }

    pub fn selected_file_path(&self) -> Option<String> {
        self.selected_entry().map(|e| e.path.clone())
    }

    pub(super) fn save_scroll_position(&mut self) {
        if let Some(entry) = self.selected_entry() {
            let key = (entry.path.clone(), self.sidebar_section, self.show_full_file);
            self.scroll_positions.insert(key, self.diff_scroll);
        }
    }

    pub(super) fn refresh_file_list(&mut self) {
        self.formatting_only_cache.clear();
        self.search_sidebar_matches.clear();
        if let Ok(all_files) = self.repo.changed_files() {
            let selected_path = self.selected_entry().map(|e| e.path.clone());
            let old_section = self.sidebar_section;
            let (staged, unstaged) = Self::partition_files(&all_files);
            self.staged_files = staged;
            self.unstaged_files = unstaged;

            if let Some(ref path) = selected_path {
                let section_files = match old_section {
                    SidebarSection::Staged => &self.staged_files,
                    SidebarSection::Unstaged => &self.unstaged_files,
                };
                if let Some(pos) = section_files.iter().position(|f| f.path == *path) {
                    self.selected_index = pos;
                } else {
                    let section_len = section_files.len();
                    if section_len == 0 {
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

    pub fn update(&mut self, msg: Message) {
        // Clear pending discard on any non-discard action
        if !matches!(msg, Message::DiscardFile | Message::DiscardHunk)
            && self.pending_discard.take().is_some()
        {
            self.status_message = None;
        }

        match msg {
            Message::MoveUp => self.handle_move_up(),
            Message::MoveDown => self.handle_move_down(),
            Message::SelectFile => self.handle_select_file(),
            Message::SelectSidebar => self.handle_select_sidebar(),
            Message::MoveCursorUp => self.handle_move_cursor_up(),
            Message::MoveCursorDown => self.handle_move_cursor_down(),
            Message::ScrollDiffUp => self.handle_scroll_diff_up(),
            Message::ScrollDiffDown => self.handle_scroll_diff_down(),
            Message::ScrollToTop => self.handle_scroll_to_top(),
            Message::ScrollToBottom => self.handle_scroll_to_bottom(),
            Message::SwitchFocus => self.handle_switch_focus(),
            Message::ToggleSidebar => self.handle_toggle_sidebar(),
            Message::Quit => { self.should_quit = true; }
            Message::NextHunk => self.handle_next_hunk(),
            Message::PrevHunk => self.handle_prev_hunk(),
            Message::MouseClickStagedSidebar(idx) => self.handle_mouse_click_staged_sidebar(idx),
            Message::MouseClickUnstagedSidebar(idx) => self.handle_mouse_click_unstaged_sidebar(idx),
            Message::FocusDiff => self.handle_focus_diff(),
            Message::StageFile => self.handle_stage_file(),
            Message::UnstageFile => self.handle_unstage_file(),
            Message::StageHunk => self.handle_stage_hunk(),
            Message::UnstageHunk => self.handle_unstage_hunk(),
            Message::DiscardFile => self.handle_discard_file(),
            Message::DiscardHunk => self.handle_discard_hunk(),
            Message::WorkdirChanged => self.handle_workdir_changed(),
            Message::IndexChanged => self.refresh_files(),
            Message::ReloadDiff => self.load_diff_for_selected(),
            Message::ToggleFullFile => self.handle_toggle_full_file(),
            Message::Undo => self.handle_undo(),
            Message::Redo => self.handle_redo(),
            Message::StartComment => self.handle_start_comment(),
            Message::CommentInputChar(c) => self.handle_comment_input_char(c),
            Message::CommentInputBackspace => self.handle_comment_input_backspace(),
            Message::CommentInputSubmit => self.handle_comment_input_submit(),
            Message::CommentInputCancel => self.handle_comment_input_cancel(),
            Message::EnterVisual => self.handle_enter_visual(),
            Message::ExitVisual => self.handle_exit_visual(),
            Message::MouseClickDiffLine(line_idx) => self.handle_mouse_click_diff_line(line_idx),
            Message::MouseDragDiff(line_idx) => self.handle_mouse_drag_diff(line_idx),
            Message::ExtendSelectionUp => self.handle_extend_selection_up(),
            Message::ExtendSelectionDown => self.handle_extend_selection_down(),
            Message::StageSelectedLines => self.handle_stage_selected_lines(),
            Message::UnstageSelectedLines => self.handle_unstage_selected_lines(),
            Message::ToggleSemanticFilter => { self.semantic_filter = !self.semantic_filter; }
            Message::SearchForward => self.handle_search_forward(),
            Message::SearchBackward => self.handle_search_backward(),
            Message::SearchInputChar(c) => self.handle_search_input_char(c),
            Message::SearchInputBackspace => self.handle_search_input_backspace(),
            Message::SearchInputSubmit => self.handle_search_input_submit(),
            Message::SearchInputCancel => self.handle_search_input_cancel(),
            Message::NextMatch => self.handle_next_match(),
            Message::PrevMatch => self.handle_prev_match(),
        }
    }

}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
