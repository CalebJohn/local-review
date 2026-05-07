use std::cell::Cell;
use std::collections::HashMap;

use crate::classify::{classify_diff, language_for_extension};
use crate::diff::types::{ChangeKind, DiffContent, DiffHunk};
use crate::diff::{binary_diff_content, compute_diff_content, compute_full_diff_content};
use crate::git::GitRepo;
use crate::git::types::{ContentResult, FileEntry};
use crate::syntax::{build_styled_diff, StyledDiffContent};
use crate::undo::{UndoManager, UndoOutcome};

const NO_ACTIVE_HUNK_MSG: &str = "No active hunk in view — press n to navigate to a hunk";
const SCROLL_MARGIN: u16 = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    Sidebar,
    DiffView,
    CommentInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppMode {
    Normal,
    Visual,
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


#[derive(Debug, Clone)]
pub struct CommentContext {
    pub file_path: String,
    pub section: SidebarSection,
    pub hunk_index: usize,
    pub selected_lines: Option<Vec<usize>>,
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
    pub visual_selection: Vec<usize>,
    pub visual_cursor: usize,
    pub visual_anchor: usize,
    pub visual_from_mouse: bool,
    pub semantic_filter: bool,
    pub formatting_only_cache: HashMap<(String, SidebarSection), bool>,
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
            visual_selection: Vec::new(),
            visual_cursor: 0,
            visual_anchor: 0,
            visual_from_mouse: false,
            semantic_filter: false,
            formatting_only_cache: HashMap::new(),
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

    fn save_scroll_position(&mut self) {
        if let Some(entry) = self.selected_entry() {
            let key = (entry.path.clone(), self.sidebar_section, self.show_full_file);
            self.scroll_positions.insert(key, self.diff_scroll);
        }
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
        self.visual_selection.clear();
        self.visual_from_mouse = false;

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
            classify_diff(&mut dc.hunks, old_str, new_str, lang);

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
            Message::SelectSidebar => {
                self.status_message = None;
                self.sidebar_collapsed = false;
                self.focus = Focus::Sidebar;
            }
            Message::MoveCursorUp => {
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
            Message::MoveCursorDown => {
                self.status_message = None;
                if self.diff_content.is_none() {
                    return;
                }
                let max_line = self.total_content_lines().saturating_sub(1);
                if self.diff_cursor < max_line {
                    self.diff_cursor += 1;
                }
                let viewport_height = self.diff_viewport_height.get() as usize;
                let cursor_row = self.cursor_row();
                let margin = SCROLL_MARGIN as usize;
                if viewport_height > margin && cursor_row >= self.diff_scroll as usize + viewport_height - margin {
                    let new_scroll = cursor_row.saturating_sub(viewport_height).saturating_add(margin);
                    let max_scroll = self.total_diff_lines().saturating_sub(1);
                    self.diff_scroll = new_scroll.min(max_scroll) as u16;
                }
                self.update_hunk_from_cursor();
            }
            Message::ScrollDiffUp => {
                self.status_message = None;
                if self.diff_scroll > 0 {
                    self.diff_scroll -= 1;
                }
            }
            Message::ScrollDiffDown => {
                self.status_message = None;
                let max_scroll = self.total_diff_lines().saturating_sub(1) as u16;
                if self.diff_scroll < max_scroll {
                    self.diff_scroll = self.diff_scroll.saturating_add(1);
                }
            }
            Message::ScrollToTop => {
                self.status_message = None;
                self.diff_cursor = 0;
                self.diff_scroll = 0;
                self.update_hunk_from_cursor();
            }
            Message::ScrollToBottom => {
                self.status_message = None;
                let max_line = self.total_content_lines().saturating_sub(1);
                self.diff_cursor = max_line;
                let viewport_height = self.diff_viewport_height.get() as usize;
                let total_lines = self.total_diff_lines();
                let max_scroll = total_lines.saturating_sub(viewport_height).saturating_add(SCROLL_MARGIN.into()) as u16;
                self.diff_scroll = max_scroll.min(total_lines.saturating_sub(1) as u16);
                self.update_hunk_from_cursor();
            }
            Message::SwitchFocus => {
                self.status_message = None;
                self.sidebar_collapsed = false;
                self.focus = match self.focus {
                    Focus::Sidebar => Focus::DiffView,
                    Focus::DiffView | Focus::CommentInput => Focus::Sidebar,
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
                let change_starts = self.change_hunk_starts();
                if let Some(&(pos, s)) = change_starts.iter().find(|(_, s)| *s > self.diff_scroll) {
                    self.diff_scroll = s;
                    self.move_cursor_to_hunk(pos);
                } else if let Some(&(pos, s)) = change_starts.last() {
                    // Wrap to last change hunk
                    self.diff_scroll = s;
                    self.move_cursor_to_hunk(pos);
                }
            }
            Message::PrevHunk => {
                self.status_message = None;
                let change_starts = self.change_hunk_starts();
                if let Some(&(pos, s)) = change_starts.iter().rev().find(|(_, s)| *s < self.diff_scroll) {
                    self.diff_scroll = s;
                    self.move_cursor_to_hunk(pos);
                } else if let Some(&(pos, s)) = change_starts.first() {
                    // Wrap to first change hunk
                    self.diff_scroll = s;
                    self.move_cursor_to_hunk(pos);
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
                self.update_hunk_from_cursor();
            }
            Message::StageFile => {
                if let Some(entry) = self.selected_entry().cloned() {
                    if let Err(e) = self.undo.record(&self.repo, "stage file", std::slice::from_ref(&entry.path)) {
                        self.status_message = Some(format!("Stage failed: {}", e));
                        self.refresh_files();
                        return;
                    }
                    if let Err(e) = self.repo.stage_file(&entry.path) {
                        self.status_message = Some(format!("Stage failed: {}", e));
                        self.undo.discard_last();
                    }
                    self.refresh_files();
                }
            }
            Message::UnstageFile => {
                if let Some(entry) = self.selected_entry().cloned() {
                    if let Err(e) = self.undo.record(&self.repo, "unstage file", std::slice::from_ref(&entry.path)) {
                        self.status_message = Some(format!("Unstage failed: {}", e));
                        self.refresh_files();
                        return;
                    }
                    if let Err(e) = self.repo.unstage_file(&entry.path) {
                        self.status_message = Some(format!("Unstage failed: {}", e));
                        self.undo.discard_last();
                    }
                    self.refresh_files();
                }
            }
            Message::StageHunk => {
                if self.sidebar_section != SidebarSection::Unstaged || self.diff_stale {
                    return;
                }
                let Some(hunk_idx) = self.current_hunk_index else {
                    self.status_message = Some(NO_ACTIVE_HUNK_MSG.to_string());
                    return;
                };
                let entry = self.selected_entry().cloned();
                if let (Some(entry), Some(dc)) = (entry, self.diff_content.as_ref()) {
                    if let Some(hunk) = dc.hunks.get(hunk_idx) {
                        let old_content = self.repo.index_content(&entry.path)
                            .ok()
                            .and_then(|c| match c { ContentResult::Text(s) => Some(s.clone()), _ => None });
                        if let Some(old) = old_content {
                            if let Err(e) = self.undo.record(&self.repo, "stage hunk", std::slice::from_ref(&entry.path)) {
                                self.status_message = Some(format!("Stage hunk failed: {}", e));
                                self.save_scroll_position();
                                self.refresh_files();
                                self.restore_hunk_position(hunk_idx);
                                return;
                            }
                            if let Err(e) = self.repo.stage_hunk(&entry.path, &old, hunk, None) {
                                self.status_message = Some(format!("Stage hunk failed: {}", e));
                                self.undo.discard_last();
                            }
                            // Persist current scroll so the post-refresh reload
                            // can restore the user's reading position instead
                            // of jumping back to the previously saved value.
                            self.save_scroll_position();
                            self.refresh_files();
                            self.restore_hunk_position(hunk_idx);
                        }
                    }
                }
            }
            Message::UnstageHunk => {
                if self.sidebar_section != SidebarSection::Staged || self.diff_stale {
                    return;
                }
                let Some(hunk_idx) = self.current_hunk_index else {
                    self.status_message = Some(NO_ACTIVE_HUNK_MSG.to_string());
                    return;
                };
                let entry = self.selected_entry().cloned();
                if let (Some(entry), Some(dc)) = (entry, self.diff_content.as_ref()) {
                    if let Some(hunk) = dc.hunks.get(hunk_idx) {
                        let index_content = self.repo.index_content(&entry.path)
                            .ok()
                            .and_then(|c| match c { ContentResult::Text(s) => Some(s.clone()), _ => None });
                        if let Some(idx_content) = index_content {
                            if let Err(e) = self.undo.record(&self.repo, "unstage hunk", std::slice::from_ref(&entry.path)) {
                                self.status_message = Some(format!("Unstage hunk failed: {}", e));
                                self.save_scroll_position();
                                self.refresh_files();
                                self.restore_hunk_position(hunk_idx);
                                return;
                            }
                            if let Err(e) = self.repo.unstage_hunk(&entry.path, &idx_content, hunk, None) {
                                self.status_message = Some(format!("Unstage hunk failed: {}", e));
                                self.undo.discard_last();
                            }
                            self.save_scroll_position();
                            self.refresh_files();
                            self.restore_hunk_position(hunk_idx);
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
                        if let Err(e) = self.undo.record(&self.repo, "discard file", std::slice::from_ref(&entry.path)) {
                            self.status_message = Some(format!("Discard failed: {}", e));
                            self.refresh_files();
                            return;
                        }
                        if let Err(e) = self.repo.discard_file(&entry.path) {
                            self.status_message = Some(format!("Discard failed: {}", e));
                            self.undo.discard_last();
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
                if self.current_hunk_index.is_none() {
                    // Drop any prior pending state — confirmation context is gone.
                    self.pending_discard = None;
                    self.status_message = Some(NO_ACTIVE_HUNK_MSG.to_string());
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
                                    if let Err(e) = self.undo.record(&self.repo, "discard hunk", std::slice::from_ref(&entry.path)) {
                                        self.status_message = Some(format!("Discard hunk failed: {}", e));
                                        self.save_scroll_position();
                                        self.refresh_files();
                                        self.restore_hunk_position(hunk_idx);
                                        return;
                                    }
                                    if let Err(e) = self.repo.discard_hunk(&entry.path, &wc, hunk) {
                                        self.status_message = Some(format!("Discard hunk failed: {}", e));
                                        self.undo.discard_last();
                                    }
                                    self.save_scroll_position();
                                    self.refresh_files();
                                    self.restore_hunk_position(hunk_idx);
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
            Message::ToggleFullFile => {
                self.status_message = None;
                self.toggle_full_file();
            }
            Message::Undo => {
                match self.undo.undo(&self.repo) {
                    UndoOutcome::Done(label) => {
                        self.status_message = Some(format!("Undid: {}", label));
                        self.refresh_files();
                    }
                    UndoOutcome::Empty => {
                        self.status_message = Some("Nothing to undo".to_string());
                    }
                    UndoOutcome::Failed(e) => {
                        self.status_message = Some(format!("Undo failed: {}", e));
                    }
                }
            }
            Message::Redo => {
                match self.undo.redo(&self.repo) {
                    UndoOutcome::Done(label) => {
                        self.status_message = Some(format!("Redid: {}", label));
                        self.refresh_files();
                    }
                    UndoOutcome::Empty => {
                        self.status_message = Some("Nothing to redo".to_string());
                    }
                    UndoOutcome::Failed(e) => {
                        self.status_message = Some(format!("Redo failed: {}", e));
                    }
                }
            }
            Message::StartComment => {
                let Some(hunk_idx) = self.current_hunk_index else {
                    self.status_message = Some(NO_ACTIVE_HUNK_MSG.to_string());
                    return;
                };
                let Some(entry) = self.selected_entry() else {
                    self.status_message = Some("No file selected".to_string());
                    return;
                };
                let selected_lines = if self.mode == AppMode::Visual && !self.visual_selection.is_empty() {
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
            Message::CommentInputChar(c) => {
                self.comment_input.push(c);
            }
            Message::CommentInputBackspace => {
                self.comment_input.pop();
            }
            Message::CommentInputSubmit => {
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
                        self.visual_selection.clear();
                    }
                }
                self.comment_input.clear();
                self.focus = Focus::DiffView;
            }
            Message::CommentInputCancel => {
                self.comment_input.clear();
                self.comment_context = None;
                self.focus = Focus::DiffView;
            }
            Message::EnterVisual => {
                if self.focus != Focus::DiffView || self.mode != AppMode::Normal {
                    return;
                }
                self.mode = AppMode::Visual;
                self.visual_cursor = self.diff_cursor;
                self.visual_anchor = self.diff_cursor;
                self.visual_selection = vec![self.diff_cursor];
                self.visual_from_mouse = false;
            }
            Message::ExitVisual => {
                if self.mode != AppMode::Visual {
                    return;
                }
                self.mode = AppMode::Normal;
                self.visual_selection.clear();
                self.visual_from_mouse = false;
            }
            Message::MouseClickDiffLine(line_idx) => {
                if self.focus != Focus::DiffView {
                    return;
                }
                let max_line = self.total_content_lines().saturating_sub(1);
                let target = line_idx.min(max_line);
                if self.mode == AppMode::Visual {
                    self.visual_anchor = target;
                    self.visual_cursor = target;
                    self.visual_selection = vec![target];
                    self.visual_from_mouse = true;
                }
                self.diff_cursor = target;
                self.update_hunk_from_cursor();
            }
            Message::MouseDragDiff(line_idx) => {
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
            Message::ExtendSelectionUp => {
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
            Message::ExtendSelectionDown => {
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
            Message::StageSelectedLines => {
                if self.mode != AppMode::Visual || self.sidebar_section != SidebarSection::Unstaged || self.diff_stale {
                    return;
                }
                if self.visual_selection.is_empty() {
                    self.status_message = Some("No lines selected — use v to enter visual mode, j/k to select".to_string());
                    return;
                }
                let Some(hunk_idx) = self.current_hunk_index else {
                    self.status_message = Some(NO_ACTIVE_HUNK_MSG.to_string());
                    return;
                };
                let entry = self.selected_entry().cloned();
                if let (Some(entry), Some(dc)) = (entry, self.diff_content.as_ref()) {
                    if let Some(hunk) = dc.hunks.get(hunk_idx) {
                        let old_content = self.repo.index_content(&entry.path)
                            .ok()
                            .and_then(|c| match c { ContentResult::Text(s) => Some(s.clone()), _ => None });
                        if let Some(old) = old_content {
                            let local_selection = self.local_selected_lines(dc, hunk_idx, hunk);
                            if local_selection.is_empty() {
                                self.status_message = Some("Selected lines not in active hunk".to_string());
                                return;
                            }
                            if let Err(e) = self.undo.record(&self.repo, "stage selected lines", std::slice::from_ref(&entry.path)) {
                                self.status_message = Some(format!("Stage failed: {}", e));
                                self.mode = AppMode::Normal;
                                self.visual_selection.clear();
                                self.refresh_files();
                                return;
                            }
                            if let Err(e) = self.repo.stage_hunk(&entry.path, &old, hunk, Some(&local_selection)) {
                                self.status_message = Some(format!("Stage failed: {}", e));
                                self.undo.discard_last();
                            }
                            self.mode = AppMode::Normal;
                            self.visual_selection.clear();
                            self.save_scroll_position();
                            self.refresh_files();
                            self.restore_hunk_position(hunk_idx);
                        }
                    }
                }
            }
            Message::UnstageSelectedLines => {
                if self.mode != AppMode::Visual || self.sidebar_section != SidebarSection::Staged || self.diff_stale {
                    return;
                }
                if self.visual_selection.is_empty() {
                    self.status_message = Some("No lines selected — use v to enter visual mode, j/k to select".to_string());
                    return;
                }
                let Some(hunk_idx) = self.current_hunk_index else {
                    self.status_message = Some(NO_ACTIVE_HUNK_MSG.to_string());
                    return;
                };
                let entry = self.selected_entry().cloned();
                if let (Some(entry), Some(dc)) = (entry, self.diff_content.as_ref()) {
                    if let Some(hunk) = dc.hunks.get(hunk_idx) {
                        let index_content = self.repo.index_content(&entry.path)
                            .ok()
                            .and_then(|c| match c { ContentResult::Text(s) => Some(s.clone()), _ => None });
                        if let Some(idx_content) = index_content {
                            let local_selection = self.local_selected_lines(dc, hunk_idx, hunk);
                            if local_selection.is_empty() {
                                self.status_message = Some("Selected lines not in active hunk".to_string());
                                return;
                            }
                            if let Err(e) = self.undo.record(&self.repo, "unstage selected lines", std::slice::from_ref(&entry.path)) {
                                self.status_message = Some(format!("Unstage failed: {}", e));
                                self.mode = AppMode::Normal;
                                self.visual_selection.clear();
                                self.refresh_files();
                                return;
                            }
                            if let Err(e) = self.repo.unstage_hunk(&entry.path, &idx_content, hunk, Some(&local_selection)) {
                                self.status_message = Some(format!("Unstage failed: {}", e));
                                self.undo.discard_last();
                            }
                            self.mode = AppMode::Normal;
                            self.visual_selection.clear();
                            self.save_scroll_position();
                            self.refresh_files();
                            self.restore_hunk_position(hunk_idx);
                        }
                    }
                }
            }
            Message::ToggleSemanticFilter => {
                self.semantic_filter = !self.semantic_filter;
            }
        }
    }

    /// Toggle full-file view, anchoring to the top line of the viewport.
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

    fn refresh_file_list(&mut self) {
        self.formatting_only_cache.clear();
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

    /// Clamp current_hunk_index and restore diff_scroll after a hunk mutation.
    fn restore_hunk_position(&mut self, hunk_idx: usize) {
        if let Some(dc) = self.diff_content.as_ref() {
            if !self.show_full_file && !dc.hunks.is_empty() {
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

    fn local_selected_lines(&self, dc: &DiffContent, hunk_idx: usize, hunk: &DiffHunk) -> Vec<usize> {
        let hunk_line_offset: usize = dc.hunks.iter().take(hunk_idx).map(|h| h.lines.len()).sum();
        let hunk_end = hunk_line_offset + hunk.lines.len();
        let (Some(&sel_start), Some(&sel_end)) = (self.visual_selection.first(), self.visual_selection.last()) else {
            return Vec::new();
        };
        let start = sel_start.max(hunk_line_offset);
        let end = (sel_end + 1).min(hunk_end);
        if start >= end {
            return Vec::new();
        }
        (start - hunk_line_offset..end - hunk_line_offset).collect()
    }

    fn update_visual_selection(&mut self) {
        let start = self.visual_anchor.min(self.visual_cursor);
        let end = self.visual_anchor.max(self.visual_cursor);
        self.visual_selection = (start..=end).collect();
    }

    fn update_hunk_from_cursor(&mut self) {
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

    fn move_cursor_to_hunk(&mut self, hunk_idx: usize) {
        let Some(dc) = self.diff_content.as_ref() else { return };
        let Some(hunk) = dc.hunks.get(hunk_idx) else { return };
        let cursor: usize = dc.hunks.iter().take(hunk_idx).map(|h| h.lines.len()).sum();
        self.diff_cursor = cursor.min(hunk.lines.len().saturating_sub(1) + cursor);
        self.update_hunk_from_cursor();
    }

    /// Return (visible, total, hidden) hunk counts.
    /// When `semantic_filter` is false, all hunks are visible.
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

    /// Return (hunk_index, start_row) for change hunks only — filler hunks
    /// without headers are skipped. Used for `n`/`N` navigation.
    /// When `semantic_filter` is true, pure-formatting hunks are also skipped.
    fn change_hunk_starts(&self) -> Vec<(usize, u16)> {
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

    /// Total rendered lines across all hunks in the current diff_content.
    /// Used for scroll clamping.
    pub fn total_diff_lines(&self) -> usize {
        match &self.diff_content {
            Some(dc) if !dc.is_binary => dc
                .hunks
                .iter()
                .map(|h| h.lines.len() + if h.has_header { 1 } else { 0 })
                .sum(),
            _ => 0,
        }
    }

    /// Total content lines (excluding hunk headers) across all hunks.
    fn total_content_lines(&self) -> usize {
        match &self.diff_content {
            Some(dc) if !dc.is_binary => dc
                .hunks
                .iter()
                .map(|h| h.lines.len())
                .sum(),
            _ => 0,
        }
    }

    /// Convert diff_cursor (content line index) to rendered row index.
    fn cursor_row(&self) -> usize {
        let Some(dc) = self.diff_content.as_ref() else { return 0 };
        let mut row: usize = 0;
        let mut remaining = self.diff_cursor;
        for hunk in &dc.hunks {
            if hunk.has_header {
                row += 1;
            }
            if remaining < hunk.lines.len() {
                return row + remaining;
            }
            remaining -= hunk.lines.len();
            row += hunk.lines.len();
        }
        row
    }

    /// Convert a rendered row offset (relative to scroll) to content line index.
    pub fn row_to_cursor(&self, row_offset: usize) -> usize {
        let Some(dc) = self.diff_content.as_ref() else { return 0 };
        let mut row: usize = 0;
        let mut cursor: usize = 0;
        for hunk in &dc.hunks {
            if hunk.has_header {
                if row == row_offset {
                    return cursor;
                }
                row += 1;
            }
            if row_offset < row + hunk.lines.len() {
                return cursor + (row_offset - row);
            }
            row += hunk.lines.len();
            cursor += hunk.lines.len();
        }
        cursor.saturating_sub(1)
    }
}

/// Identifier for a single rendered diff line, robust to changing context/grouping.
pub type DiffLineKey = (ChangeKind, Option<u32>, Option<u32>);

/// Return the DiffLine identity rendered at `target_row` in the current view.
///
/// Hunk header rows resolve to the first line in that hunk so that anchoring on
/// a header still produces a meaningful target after recomputing the diff. If
/// `target_row` is past the end, returns the last line. Returns None for empty
/// or binary diffs.
pub fn diff_line_at_row(dc: &DiffContent, target_row: usize) -> Option<DiffLineKey> {
    if dc.is_binary || dc.hunks.is_empty() {
        return None;
    }
    let mut row: usize = 0;
    let mut last: Option<DiffLineKey> = None;
    for hunk in &dc.hunks {
        if hunk.has_header {
            if target_row == row {
                if let Some(first) = hunk.lines.first() {
                    return Some((first.kind, first.old_lineno, first.new_lineno));
                }
            }
            row += 1;
        }
        for dl in &hunk.lines {
            if target_row == row {
                return Some((dl.kind, dl.old_lineno, dl.new_lineno));
            }
            last = Some((dl.kind, dl.old_lineno, dl.new_lineno));
            row += 1;
        }
    }
    last
}

pub fn row_for_diff_line(dc: &DiffContent, target: DiffLineKey) -> Option<usize> {
    if dc.is_binary {
        return None;
    }
    let (t_kind, t_old, t_new) = target;
    let mut row: usize = 0;
    for hunk in &dc.hunks {
        if hunk.has_header {
            row += 1;
        }
        for dl in &hunk.lines {
            let matches = match t_kind {
                ChangeKind::Equal => {
                    dl.kind == ChangeKind::Equal
                        && dl.old_lineno == t_old
                        && dl.new_lineno == t_new
                }
                ChangeKind::Insert => {
                    dl.kind == ChangeKind::Insert && dl.new_lineno == t_new
                }
                ChangeKind::Delete => {
                    dl.kind == ChangeKind::Delete && dl.old_lineno == t_old
                }
            };
            if matches {
                return Some(row);
            }
            row += 1;
        }
    }
    None
}

fn nearest_row_for_line(dc: &DiffContent, target: DiffLineKey) -> Option<usize> {
    if dc.is_binary || dc.hunks.is_empty() {
        return None;
    }
    let target_line = target.2.or(target.1)?;
    let mut best_row: Option<usize> = None;
    let mut best_dist: u32 = u32::MAX;
    let mut row: usize = 0;
    for hunk in &dc.hunks {
        if hunk.has_header {
            row += 1;
        }
        for dl in &hunk.lines {
            let line = dl.new_lineno.or(dl.old_lineno);
            if let Some(l) = line {
                let dist = l.abs_diff(target_line);
                if dist < best_dist {
                    best_dist = dist;
                    best_row = Some(row);
                }
            }
            row += 1;
        }
    }
    best_row
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
            visual_selection: Vec::new(),
            visual_cursor: 0,
            visual_anchor: 0,
            visual_from_mouse: false,
            semantic_filter: false,
            formatting_only_cache: HashMap::new(),
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
    fn test_next_hunk_no_op_on_empty() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.diff_scroll = 0;
        app.update(Message::NextHunk);
        assert_eq!(app.diff_scroll, 0);
    }

    fn three_hunk_dc() -> DiffContent {
        make_dc(vec![
            DiffHunk { old_start: 1, new_start: 1, lines: vec![dl(ChangeKind::Insert, None, Some(1)); 2], has_header: true },
            DiffHunk { old_start: 3, new_start: 3, lines: vec![dl(ChangeKind::Insert, None, Some(3)); 3], has_header: true },
            DiffHunk { old_start: 8, new_start: 8, lines: vec![dl(ChangeKind::Insert, None, Some(8)); 5], has_header: true },
        ])
    }

    #[test]
    fn test_next_hunk_advances_to_next_start() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.diff_content = Some(three_hunk_dc());
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
        app.diff_content = Some(three_hunk_dc());
        app.diff_scroll = 0;
        app.update(Message::PrevHunk);
        assert_eq!(app.diff_scroll, 0);
    }

    #[test]
    fn test_prev_hunk_goes_to_previous_start() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.diff_content = Some(three_hunk_dc());
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

    // ---- full-file toggle tests ----

    use crate::diff::types::{DiffHunk, DiffLine};

    fn make_dc(hunks: Vec<DiffHunk>) -> DiffContent {
        DiffContent { path: "t.rs".to_string(), hunks, is_binary: false }
    }

    fn dl(kind: ChangeKind, old: Option<u32>, new: Option<u32>) -> DiffLine {
        DiffLine { kind, old_lineno: old, new_lineno: new, content: "x\n".to_string(), formatting_only: false }
    }

    #[test]
    fn test_diff_line_at_row_header_resolves_to_first_line() {
        let dc = make_dc(vec![DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![dl(ChangeKind::Equal, Some(1), Some(1)), dl(ChangeKind::Insert, None, Some(2))],
            has_header: true,
        }]);
        // row 0 = header → resolves to first line
        assert_eq!(diff_line_at_row(&dc, 0), Some((ChangeKind::Equal, Some(1), Some(1))));
        // row 1 = first content line
        assert_eq!(diff_line_at_row(&dc, 1), Some((ChangeKind::Equal, Some(1), Some(1))));
        // row 2 = second content line
        assert_eq!(diff_line_at_row(&dc, 2), Some((ChangeKind::Insert, None, Some(2))));
    }

    #[test]
    fn test_diff_line_at_row_past_end_returns_last() {
        let dc = make_dc(vec![DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![dl(ChangeKind::Equal, Some(1), Some(1))],
            has_header: true,
        }]);
        assert_eq!(diff_line_at_row(&dc, 999), Some((ChangeKind::Equal, Some(1), Some(1))));
    }

    #[test]
    fn test_diff_line_at_row_empty_or_binary_is_none() {
        let empty = make_dc(vec![]);
        assert_eq!(diff_line_at_row(&empty, 0), None);
        let mut bin = make_dc(vec![]);
        bin.is_binary = true;
        assert_eq!(diff_line_at_row(&bin, 0), None);
    }

    #[test]
    fn test_row_for_diff_line_finds_match() {
        let dc = make_dc(vec![DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![
                dl(ChangeKind::Equal, Some(1), Some(1)),
                dl(ChangeKind::Equal, Some(2), Some(2)),
                dl(ChangeKind::Insert, None, Some(3)),
            ],
            has_header: true,
        }]);
        // row layout: 0 header, 1 line1, 2 line2, 3 insert
        assert_eq!(row_for_diff_line(&dc, (ChangeKind::Equal, Some(2), Some(2))), Some(2));
        assert_eq!(row_for_diff_line(&dc, (ChangeKind::Insert, None, Some(3))), Some(3));
    }

    #[test]
    fn test_row_for_diff_line_returns_none_when_missing() {
        let dc = make_dc(vec![DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![dl(ChangeKind::Equal, Some(1), Some(1))],
            has_header: true,
        }]);
        // line 99 is not present in this diff
        assert_eq!(row_for_diff_line(&dc, (ChangeKind::Equal, Some(99), Some(99))), None);
    }

    #[test]
    fn test_compute_full_diff_includes_all_lines() {
        // 5-line file with one change in the middle: the change hunk's 3 lines
        // of context cover the whole file, so full-file mode produces a single
        // change hunk with no surrounding fillers.
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\nb\nC\nd\ne\n";
        let dc = crate::diff::compute_full_diff_content("t.rs", Some(old), Some(new));
        assert_eq!(dc.hunks.len(), 1);
        let hunk = &dc.hunks[0];
        assert!(hunk.has_header);
        // Equal a, Equal b, Delete c, Insert C, Equal d, Equal e = 6 lines
        assert_eq!(hunk.lines.len(), 6);
        assert!(hunk.lines.iter().any(|l| l.kind == ChangeKind::Delete));
        assert!(hunk.lines.iter().any(|l| l.kind == ChangeKind::Insert));
        // First and last lines are unchanged context, far from the actual change
        assert_eq!(hunk.lines[0].kind, ChangeKind::Equal);
        assert_eq!(hunk.lines[0].old_lineno, Some(1));
        assert_eq!(hunk.lines.last().unwrap().new_lineno, Some(5));
    }

    #[test]
    fn test_compute_full_diff_keeps_change_hunks_in_place_with_fillers() {
        // 200-line file with one change at line 100. Full-file mode should
        // produce: leading filler (no header) + change hunk (with header) +
        // trailing filler (no header). The change hunk must keep its proper
        // position; the gap before/after is filled without hunk headers.
        let old: String = (1..=200).map(|n| format!("line{}\n", n)).collect();
        let new = old.replace("line100\n", "LINE100\n");

        let dc = crate::diff::compute_full_diff_content("t.rs", Some(&old), Some(&new));
        assert_eq!(dc.hunks.len(), 3);

        let leading = &dc.hunks[0];
        assert!(!leading.has_header);
        assert_eq!(leading.old_start, 1);
        assert_eq!(leading.new_start, 1);
        assert!(leading.lines.iter().all(|l| l.kind == ChangeKind::Equal));
        assert_eq!(leading.lines.first().unwrap().new_lineno, Some(1));
        assert_eq!(leading.lines.last().unwrap().new_lineno, Some(96));

        let change = &dc.hunks[1];
        assert!(change.has_header);
        assert_eq!(change.old_start, 97);
        assert_eq!(change.new_start, 97);
        assert!(change.lines.iter().any(|l| l.kind == ChangeKind::Delete));
        assert!(change.lines.iter().any(|l| l.kind == ChangeKind::Insert));

        let trailing = &dc.hunks[2];
        assert!(!trailing.has_header);
        assert!(trailing.lines.iter().all(|l| l.kind == ChangeKind::Equal));
        assert_eq!(trailing.lines.first().unwrap().new_lineno, Some(104));
        assert_eq!(trailing.lines.last().unwrap().new_lineno, Some(200));
    }

    /// Cursor preservation across mode toggle: a line visible in hunk mode
    /// maps to the same line identity in full-file mode.
    #[test]
    fn test_cursor_preservation_across_modes() {
        let old: String = (1..=200).map(|n| format!("line{}\n", n)).collect();
        let new = old.replace("line100\n", "LINE100\n");

        let hunk_dc = crate::diff::compute_diff_content("t.rs", Some(&old), Some(&new));
        let full_dc = crate::diff::compute_full_diff_content("t.rs", Some(&old), Some(&new));

        let target = (ChangeKind::Insert, None, Some(100u32));
        let hunk_row = row_for_diff_line(&hunk_dc, target).expect("insert in hunk mode");
        let full_row = row_for_diff_line(&full_dc, target).expect("insert in full mode");

        let cursor_key = diff_line_at_row(&hunk_dc, hunk_row).expect("cursor key");
        assert_eq!(cursor_key, target);

        let restored = row_for_diff_line(&full_dc, cursor_key).expect("found in full");
        assert_eq!(restored, full_row);

        let resolved = diff_line_at_row(&full_dc, restored);
        assert_eq!(resolved, Some(target));
    }

    /// When cursor is on a context-only line in full-file mode that doesn't
    /// exist in hunk mode, nearest_row_for_line finds the closest line.
    #[test]
    fn test_nearest_row_fallback() {
        let old: String = (1..=200).map(|n| format!("line{}\n", n)).collect();
        let new = old.replace("line100\n", "LINE100\n");

        let hunk_dc = crate::diff::compute_diff_content("t.rs", Some(&old), Some(&new));

        let far_away = (ChangeKind::Equal, Some(5u32), Some(5u32));
        assert!(row_for_diff_line(&hunk_dc, far_away).is_none());

        let nearest = nearest_row_for_line(&hunk_dc, far_away);
        assert!(nearest.is_some());
    }

    /// `Message::ToggleFullFile` flips the flag and reloads the diff (or clears
    /// it when no file is selected).
    #[test]
    fn test_toggle_full_file_flips_flag() {
        let mut app = test_app_with_files(vec![]);
        assert!(!app.show_full_file);
        app.update(Message::ToggleFullFile);
        assert!(app.show_full_file);
        app.update(Message::ToggleFullFile);
        assert!(!app.show_full_file);
    }

    #[test]
    fn test_scroll_positions_are_per_mode() {
        // Saved scroll for hunk mode should not bleed into full-file mode.
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.scroll_positions.insert(
            ("unstaged.rs".to_string(), SidebarSection::Unstaged, false),
            42,
        );
        app.scroll_positions.insert(
            ("unstaged.rs".to_string(), SidebarSection::Unstaged, true),
            7,
        );
        // Hunk mode reads false-keyed entry
        app.show_full_file = false;
        app.load_diff_for_selected();
        assert_eq!(app.diff_scroll, 42);
        // Full-file mode reads true-keyed entry
        app.show_full_file = true;
        app.load_diff_for_selected();
        assert_eq!(app.diff_scroll, 7);
    }

    // ---- active-hunk visibility tests ----

    /// Build a small App state with a single change hunk located somewhere in
    /// the rendered diff.
    #[test]
    fn test_update_hunk_from_cursor_first_hunk() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        let hunk1 = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![dl(ChangeKind::Insert, None, Some(1)); 5],
            has_header: true,
        };
        let hunk2 = DiffHunk {
            old_start: 10,
            new_start: 10,
            lines: vec![dl(ChangeKind::Insert, None, Some(10)); 3],
            has_header: true,
        };
        app.diff_content = Some(make_dc(vec![hunk1, hunk2]));
        app.diff_cursor = 0;
        app.update_hunk_from_cursor();
        assert_eq!(app.current_hunk_index, Some(0));
    }

    #[test]
    fn test_update_hunk_from_cursor_second_hunk() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        let hunk1 = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![dl(ChangeKind::Insert, None, Some(1)); 5],
            has_header: true,
        };
        let hunk2 = DiffHunk {
            old_start: 10,
            new_start: 10,
            lines: vec![dl(ChangeKind::Insert, None, Some(10)); 3],
            has_header: true,
        };
        app.diff_content = Some(make_dc(vec![hunk1, hunk2]));
        app.diff_cursor = 6;
        app.update_hunk_from_cursor();
        assert_eq!(app.current_hunk_index, Some(1));
    }

    #[test]
    fn test_update_hunk_from_cursor_last_line_of_last_hunk() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        let hunk1 = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![dl(ChangeKind::Insert, None, Some(1)); 5],
            has_header: true,
        };
        let hunk2 = DiffHunk {
            old_start: 10,
            new_start: 10,
            lines: vec![dl(ChangeKind::Insert, None, Some(10)); 3],
            has_header: true,
        };
        app.diff_content = Some(make_dc(vec![hunk1, hunk2]));
        app.diff_cursor = 7;
        app.update_hunk_from_cursor();
        assert_eq!(app.current_hunk_index, Some(1));
    }

    #[test]
    fn test_update_hunk_from_cursor_single_hunk() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        let hunk = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![dl(ChangeKind::Insert, None, Some(1)); 3],
            has_header: true,
        };
        app.diff_content = Some(make_dc(vec![hunk]));
        app.diff_cursor = 2;
        app.update_hunk_from_cursor();
        assert_eq!(app.current_hunk_index, Some(0));
    }

    #[test]
    fn test_update_hunk_from_cursor_no_content() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.diff_content = None;
        app.diff_cursor = 0;
        app.update_hunk_from_cursor();
        assert_eq!(app.current_hunk_index, None);
    }

    #[test]
    fn test_move_cursor_to_hunk() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        let hunk1 = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![dl(ChangeKind::Insert, None, Some(1)); 5],
            has_header: true,
        };
        let hunk2 = DiffHunk {
            old_start: 10,
            new_start: 10,
            lines: vec![dl(ChangeKind::Insert, None, Some(10)); 3],
            has_header: true,
        };
        app.diff_content = Some(make_dc(vec![hunk1, hunk2]));
        app.move_cursor_to_hunk(1);
        assert_eq!(app.diff_cursor, 5);
        assert_eq!(app.current_hunk_index, Some(1));
    }

    #[test]
    fn test_stage_hunk_warns_when_no_active_hunk() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.focus = Focus::DiffView;
        app.current_hunk_index = None;
        app.update(Message::StageHunk);
        assert!(app.status_message.as_deref().unwrap_or("").contains("No active hunk"));
    }

    #[test]
    fn test_unstage_hunk_warns_when_no_active_hunk() {
        let mut app = test_app_with_files(vec![staged_only_entry()]);
        app.focus = Focus::DiffView;
        app.current_hunk_index = None;
        app.update(Message::UnstageHunk);
        assert!(app.status_message.as_deref().unwrap_or("").contains("No active hunk"));
    }

    #[test]
    fn test_discard_hunk_warns_and_clears_pending_when_no_active_hunk() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.focus = Focus::DiffView;
        // Simulate a stale pending discard from a prior hunk.
        app.pending_discard = Some(PendingDiscard::Hunk {
            path: "unstaged.rs".to_string(),
            hunk_index: 0,
        });
        app.current_hunk_index = None;
        app.update(Message::DiscardHunk);
        assert!(app.pending_discard.is_none());
        assert!(app.status_message.as_deref().unwrap_or("").contains("No active hunk"));
    }

    // ---- format_comment tests ----

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
        };
        let result = format_comment(&ctx, &hunk, "These two lines");
        assert!(result.starts_with("File: src/main.rs (unstaged)\n"));
        assert!(result.contains("Selected lines (2):"));
        assert!(!result.contains("@@ -"), "should not include hunk header when lines selected");
        assert!(result.contains("\nThese two lines\n"));
    }

    // ---- comment update logic tests ----

    #[test]
    fn test_start_comment_captures_context() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.focus = Focus::DiffView;
        app.current_hunk_index = Some(2);
        app.update(Message::StartComment);
        assert_eq!(app.focus, Focus::CommentInput);
        assert!(app.comment_context.is_some());
        let ctx = app.comment_context.as_ref().unwrap();
        assert_eq!(ctx.file_path, "unstaged.rs");
        assert_eq!(ctx.section, SidebarSection::Unstaged);
        assert_eq!(ctx.hunk_index, 2);
        assert!(app.comment_input.is_empty());
    }

    #[test]
    fn test_start_comment_ignored_when_no_hunk() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.focus = Focus::DiffView;
        app.current_hunk_index = None;
        app.update(Message::StartComment);
        assert_eq!(app.focus, Focus::DiffView);
        assert!(app.comment_context.is_none());
    }

    #[test]
    fn test_comment_input_char_and_backspace() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::CommentInput;
        app.update(Message::CommentInputChar('h'));
        app.update(Message::CommentInputChar('i'));
        assert_eq!(app.comment_input, "hi");
        app.update(Message::CommentInputBackspace);
        assert_eq!(app.comment_input, "h");
        app.update(Message::CommentInputBackspace);
        assert!(app.comment_input.is_empty());
        app.update(Message::CommentInputBackspace);
        assert!(app.comment_input.is_empty());
    }

    #[test]
    fn test_comment_input_cancel_clears_state() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.focus = Focus::CommentInput;
        app.comment_input = "partial".to_string();
        app.comment_context = Some(CommentContext {
            file_path: "unstaged.rs".to_string(),
            section: SidebarSection::Unstaged,
            hunk_index: 0,
            selected_lines: None,
        });
        app.update(Message::CommentInputCancel);
        assert_eq!(app.focus, Focus::DiffView);
        assert!(app.comment_input.is_empty());
        assert!(app.comment_context.is_none());
    }

    // ---- visual mode tests ----

    #[test]
    fn test_enter_visual_from_normal_in_diff_view() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.mode = AppMode::Normal;
        app.current_hunk_index = Some(0);
        app.diff_content = Some(make_dc(vec![DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![dl(ChangeKind::Equal, Some(1), Some(1)), dl(ChangeKind::Insert, None, Some(2))],
            has_header: true,
        }]));
        app.update(Message::EnterVisual);
        assert_eq!(app.mode, AppMode::Visual);
        assert!(!app.visual_selection.is_empty());
    }

    #[test]
    fn test_enter_visual_ignored_in_sidebar() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::Sidebar;
        app.mode = AppMode::Normal;
        app.update(Message::EnterVisual);
        assert_eq!(app.mode, AppMode::Normal);
        assert!(app.visual_selection.is_empty());
    }

    #[test]
    fn test_enter_visual_ignored_when_already_in_visual() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.mode = AppMode::Visual;
        app.visual_selection = vec![0, 1];
        app.update(Message::EnterVisual);
        assert_eq!(app.visual_selection, vec![0, 1]);
    }

    #[test]
    fn test_exit_visual_clears_selection() {
        let mut app = test_app_with_files(vec![]);
        app.mode = AppMode::Visual;
        app.visual_selection = vec![0, 1, 2];
        app.update(Message::ExitVisual);
        assert_eq!(app.mode, AppMode::Normal);
        assert!(app.visual_selection.is_empty());
    }

    #[test]
    fn test_exit_visual_ignored_in_normal_mode() {
        let mut app = test_app_with_files(vec![]);
        app.mode = AppMode::Normal;
        app.update(Message::ExitVisual);
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn test_extend_selection_down() {
        let mut app = test_app_with_files(vec![]);
        app.mode = AppMode::Visual;
        app.visual_anchor = 0;
        app.visual_cursor = 0;
        app.diff_content = Some(make_dc(vec![DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                dl(ChangeKind::Equal, Some(1), Some(1)),
                dl(ChangeKind::Insert, None, Some(2)),
                dl(ChangeKind::Equal, Some(3), Some(3)),
            ],
            has_header: true,
        }]));
        app.update(Message::ExtendSelectionDown);
        assert_eq!(app.visual_cursor, 1);
        assert_eq!(app.visual_selection, vec![0, 1]);
        app.update(Message::ExtendSelectionDown);
        assert_eq!(app.visual_cursor, 2);
        assert_eq!(app.visual_selection, vec![0, 1, 2]);
    }

    #[test]
    fn test_extend_selection_up() {
        let mut app = test_app_with_files(vec![]);
        app.mode = AppMode::Visual;
        app.visual_anchor = 2;
        app.visual_cursor = 2;
        app.diff_content = Some(make_dc(vec![DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                dl(ChangeKind::Equal, Some(1), Some(1)),
                dl(ChangeKind::Insert, None, Some(2)),
                dl(ChangeKind::Equal, Some(3), Some(3)),
            ],
            has_header: true,
        }]));
        app.update(Message::ExtendSelectionUp);
        assert_eq!(app.visual_cursor, 1);
        assert_eq!(app.visual_selection, vec![1, 2]);
        app.update(Message::ExtendSelectionUp);
        assert_eq!(app.visual_cursor, 0);
        assert_eq!(app.visual_selection, vec![0, 1, 2]);
    }

    #[test]
    fn test_extend_selection_up_at_zero_stays() {
        let mut app = test_app_with_files(vec![]);
        app.mode = AppMode::Visual;
        app.visual_anchor = 0;
        app.visual_cursor = 0;
        app.diff_content = Some(make_dc(vec![DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![dl(ChangeKind::Equal, Some(1), Some(1))],
            has_header: true,
        }]));
        app.update(Message::ExtendSelectionUp);
        assert_eq!(app.visual_cursor, 0);
        assert_eq!(app.visual_selection, vec![0]);
    }

    #[test]
    fn test_extend_selection_down_at_max_stays() {
        let mut app = test_app_with_files(vec![]);
        app.mode = AppMode::Visual;
        app.visual_anchor = 2;
        app.visual_cursor = 2;
        app.diff_content = Some(make_dc(vec![DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                dl(ChangeKind::Equal, Some(1), Some(1)),
                dl(ChangeKind::Insert, None, Some(2)),
                dl(ChangeKind::Equal, Some(3), Some(3)),
            ],
            has_header: true,
        }]));
        app.update(Message::ExtendSelectionDown);
        app.update(Message::ExtendSelectionDown);
        assert_eq!(app.visual_cursor, 2);
    }

    #[test]
    fn test_extend_selection_ignored_in_normal_mode() {
        let mut app = test_app_with_files(vec![]);
        app.mode = AppMode::Normal;
        app.visual_cursor = 0;
        app.update(Message::ExtendSelectionDown);
        assert_eq!(app.visual_cursor, 0);
        app.update(Message::ExtendSelectionUp);
        assert_eq!(app.visual_cursor, 0);
    }

    #[test]
    fn test_extend_selection_down_ignored_when_no_diff_content() {
        let mut app = test_app_with_files(vec![]);
        app.mode = AppMode::Visual;
        app.visual_anchor = 0;
        app.visual_cursor = 0;
        app.diff_content = None;
        app.update(Message::ExtendSelectionDown);
        assert_eq!(app.visual_cursor, 0);
        assert!(app.visual_selection.is_empty());
    }

    #[test]
    fn test_extend_selection_up_ignored_when_no_diff_content() {
        let mut app = test_app_with_files(vec![]);
        app.mode = AppMode::Visual;
        app.visual_anchor = 2;
        app.visual_cursor = 2;
        app.diff_content = None;
        app.update(Message::ExtendSelectionUp);
        assert_eq!(app.visual_cursor, 2);
        assert!(app.visual_selection.is_empty());
    }

    // ---- semantic filter tests (Task 6) ----

    #[test]
    fn test_semantic_filter_defaults_to_false() {
        let app = test_app_with_files(vec![]);
        assert!(!app.semantic_filter);
    }

    #[test]
    fn test_toggle_semantic_filter_flips_state() {
        let mut app = test_app_with_files(vec![]);
        assert!(!app.semantic_filter);
        app.update(Message::ToggleSemanticFilter);
        assert!(app.semantic_filter);
        app.update(Message::ToggleSemanticFilter);
        assert!(!app.semantic_filter);
    }

    #[test]
    fn test_toggle_semantic_filter_available_in_sidebar() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::Sidebar;
        app.update(Message::ToggleSemanticFilter);
        assert!(app.semantic_filter);
    }

    #[test]
    fn test_toggle_semantic_filter_available_in_diff_view() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.update(Message::ToggleSemanticFilter);
        assert!(app.semantic_filter);
    }

    // ---- semantic filter navigation tests (Task 7) ----

    fn dl_fmt(kind: ChangeKind, old: Option<u32>, new: Option<u32>, formatting_only: bool) -> DiffLine {
        DiffLine { kind, old_lineno: old, new_lineno: new, content: "x\n".to_string(), formatting_only }
    }

    #[test]
    fn test_change_hunk_starts_skips_formatting_hunks_when_filter_on() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.diff_content = Some(make_dc(vec![
            DiffHunk { old_start: 1, new_start: 1, lines: vec![dl_fmt(ChangeKind::Insert, None, Some(1), true)], has_header: true },
            DiffHunk { old_start: 5, new_start: 5, lines: vec![dl_fmt(ChangeKind::Delete, Some(5), None, false)], has_header: true },
            DiffHunk { old_start: 10, new_start: 10, lines: vec![dl_fmt(ChangeKind::Insert, None, Some(10), true)], has_header: true },
        ]));

        // Filter off: all 3 hunks visible
        app.semantic_filter = false;
        let starts_off = app.change_hunk_starts();
        assert_eq!(starts_off.len(), 3);

        // Filter on: only the semantic hunk (index 1) visible
        app.semantic_filter = true;
        let starts_on = app.change_hunk_starts();
        assert_eq!(starts_on.len(), 1);
        assert_eq!(starts_on[0].0, 1);
    }

    #[test]
    fn test_next_hunk_skips_formatting_hunks_when_filter_on() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.diff_content = Some(make_dc(vec![
            DiffHunk { old_start: 1, new_start: 1, lines: vec![dl_fmt(ChangeKind::Insert, None, Some(1), true); 2], has_header: true },
            DiffHunk { old_start: 5, new_start: 5, lines: vec![dl_fmt(ChangeKind::Delete, Some(5), None, false); 3], has_header: true },
        ]));
        app.semantic_filter = true;
        app.diff_scroll = 0;

        // Next hunk should skip the formatting hunk and go to the semantic one.
        // The formatting hunk is hidden (0 rows), so the semantic hunk starts at row 0.
        app.update(Message::NextHunk);
        assert_eq!(app.diff_scroll, 0);
    }

    #[test]
    fn test_prev_hunk_skips_formatting_hunks_when_filter_on() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.diff_content = Some(make_dc(vec![
            DiffHunk { old_start: 1, new_start: 1, lines: vec![dl_fmt(ChangeKind::Delete, Some(1), None, false); 3], has_header: true },
            DiffHunk { old_start: 5, new_start: 5, lines: vec![dl_fmt(ChangeKind::Insert, None, Some(5), true); 2], has_header: true },
        ]));
        app.semantic_filter = true;
        app.diff_scroll = 4; // past the second hunk

        // Prev hunk should skip the formatting hunk and go to the semantic one
        app.update(Message::PrevHunk);
        assert_eq!(app.diff_scroll, 0);
    }

    // ── hunk_counts tests (Task 8) ──────────────────────────────────

    #[test]
    fn test_hunk_counts_none_when_no_diff_content() {
        let app = test_app_with_files(vec![]);
        assert!(app.hunk_counts().is_none());
    }

    #[test]
    fn test_hunk_counts_none_when_binary() {
        let mut app = test_app_with_files(vec![]);
        app.diff_content = Some(DiffContent {
            path: "img.png".to_string(),
            hunks: vec![],
            is_binary: true,
        });
        assert!(app.hunk_counts().is_none());
    }

    #[test]
    fn test_hunk_counts_none_when_empty_hunks() {
        let mut app = test_app_with_files(vec![]);
        app.diff_content = Some(DiffContent {
            path: "t.rs".to_string(),
            hunks: vec![],
            is_binary: false,
        });
        assert!(app.hunk_counts().is_none());
    }

    #[test]
    fn test_hunk_counts_all_visible_when_filter_off() {
        let mut app = test_app_with_files(vec![]);
        app.diff_content = Some(make_dc(vec![
            DiffHunk { old_start: 1, new_start: 1, lines: vec![dl_fmt(ChangeKind::Insert, None, Some(1), true)], has_header: true },
            DiffHunk { old_start: 5, new_start: 5, lines: vec![dl_fmt(ChangeKind::Delete, Some(5), None, false)], has_header: true },
        ]));
        app.semantic_filter = false;
        assert_eq!(app.hunk_counts(), Some((2, 2, 0)));
    }

    #[test]
    fn test_hunk_counts_hides_formatting_when_filter_on() {
        let mut app = test_app_with_files(vec![]);
        app.diff_content = Some(make_dc(vec![
            DiffHunk { old_start: 1, new_start: 1, lines: vec![dl_fmt(ChangeKind::Insert, None, Some(1), true)], has_header: true },
            DiffHunk { old_start: 5, new_start: 5, lines: vec![dl_fmt(ChangeKind::Delete, Some(5), None, false)], has_header: true },
            DiffHunk { old_start: 10, new_start: 10, lines: vec![dl_fmt(ChangeKind::Insert, None, Some(10), true)], has_header: true },
        ]));
        app.semantic_filter = true;
        assert_eq!(app.hunk_counts(), Some((1, 3, 2)));
    }

    #[test]
    fn test_hunk_counts_all_formatting_hidden() {
        let mut app = test_app_with_files(vec![]);
        app.diff_content = Some(make_dc(vec![
            DiffHunk { old_start: 1, new_start: 1, lines: vec![dl_fmt(ChangeKind::Insert, None, Some(1), true)], has_header: true },
            DiffHunk { old_start: 5, new_start: 5, lines: vec![dl_fmt(ChangeKind::Delete, Some(5), None, true)], has_header: true },
        ]));
        app.semantic_filter = true;
        assert_eq!(app.hunk_counts(), Some((0, 2, 2)));
    }

    // ── Task 4: classify_diff wired into load_diff_for_selected ──────

    #[test]
    fn test_classify_diff_integration_whitespace_change_marked_formatting() {
        // Simulates what load_diff_for_selected does: compute diff, then classify.
        // A whitespace-only change should have formatting_only = true after classification.
        let old = "fn foo() {\nlet x=1;\n}\n";
        let new = "fn foo() {\n    let x = 1;\n}\n";
        let mut dc = crate::diff::compute_diff_content("t.rs", Some(old), Some(new));

        let lang = crate::classify::language_for_extension("rs");
        crate::classify::classify_diff(&mut dc.hunks, old, new, lang);

        let changed: Vec<_> = dc.hunks[0].lines.iter().filter(|l| l.kind != ChangeKind::Equal).collect();
        assert!(
            !changed.is_empty(),
            "diff should have changed lines"
        );
        assert!(
            changed.iter().all(|l| l.formatting_only),
            "whitespace-only changes should be formatting_only after classification: {:?}",
            changed.iter().map(|l| (&l.content, l.formatting_only)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_classify_diff_integration_semantic_change_not_formatting() {
        let old = "let x = 1;\n";
        let new = "let y = 1;\n";
        let mut dc = crate::diff::compute_diff_content("t.rs", Some(old), Some(new));

        let lang = crate::classify::language_for_extension("rs");
        crate::classify::classify_diff(&mut dc.hunks, old, new, lang);

        let changed: Vec<_> = dc.hunks[0].lines.iter().filter(|l| l.kind != ChangeKind::Equal).collect();
        assert!(
            changed.iter().all(|l| !l.formatting_only),
            "semantic changes should NOT be formatting_only after classification"
        );
    }

    #[test]
    fn test_classify_diff_integration_unknown_extension_skips() {
        let old = "hello world\n";
        let new = "hello  world\n";
        let mut dc = crate::diff::compute_diff_content("README.txt", Some(old), Some(new));

        let lang = crate::classify::language_for_extension("txt");
        crate::classify::classify_diff(&mut dc.hunks, old, new, lang);

        // Unknown language: all lines should remain non-formatting
        let changed: Vec<_> = dc.hunks.iter().flat_map(|h| h.lines.iter()).filter(|l| l.kind != ChangeKind::Equal).collect();
        assert!(
            changed.iter().all(|l| !l.formatting_only),
            "unknown language should leave all lines as non-formatting"
        );
    }

    // ---- sidebar formatting indicator tests (Task 10) ----

    #[test]
    fn test_formatting_only_cache_populated_for_formatting_changes() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        // Simulate loading a diff with only formatting changes
        let old = "fn foo() {\nlet x=1;\n}\n";
        let new = "fn foo() {\n    let x = 1;\n}\n";
        let mut dc = crate::diff::compute_diff_content("unstaged.rs", Some(old), Some(new));
        let lang = crate::classify::language_for_extension("rs");
        crate::classify::classify_diff(&mut dc.hunks, old, new, lang);
        app.diff_content = Some(dc);

        // After classification, all hunks should be formatting-only
        let all_formatting = app.diff_content.as_ref().unwrap().hunks.iter().all(|h| h.is_formatting_only());
        assert!(all_formatting, "whitespace-only changes should be formatting-only");

        // Manually populate the cache as load_diff_for_selected would
        app.formatting_only_cache.insert(("unstaged.rs".to_string(), SidebarSection::Unstaged), all_formatting);
        assert_eq!(app.formatting_only_cache.get(&("unstaged.rs".to_string(), SidebarSection::Unstaged)), Some(&true));
    }

    #[test]
    fn test_formatting_only_cache_populated_for_semantic_changes() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        let old = "let x = 1;\n";
        let new = "let y = 2;\n";
        let mut dc = crate::diff::compute_diff_content("unstaged.rs", Some(old), Some(new));
        let lang = crate::classify::language_for_extension("rs");
        crate::classify::classify_diff(&mut dc.hunks, old, new, lang);
        app.diff_content = Some(dc);

        let all_formatting = app.diff_content.as_ref().unwrap().hunks.iter().all(|h| h.is_formatting_only());
        assert!(!all_formatting, "semantic changes should NOT be formatting-only");

        app.formatting_only_cache.insert(("unstaged.rs".to_string(), SidebarSection::Unstaged), all_formatting);
        assert_eq!(app.formatting_only_cache.get(&("unstaged.rs".to_string(), SidebarSection::Unstaged)), Some(&false));
    }

    #[test]
    fn test_formatting_only_cache_cleared_on_refresh() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.formatting_only_cache.insert(("unstaged.rs".to_string(), SidebarSection::Unstaged), true);
        assert!(app.formatting_only_cache.contains_key(&("unstaged.rs".to_string(), SidebarSection::Unstaged)));

        app.refresh_file_list();
        assert!(!app.formatting_only_cache.contains_key(&("unstaged.rs".to_string(), SidebarSection::Unstaged)));
    }
}
