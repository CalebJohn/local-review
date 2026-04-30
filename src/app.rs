use std::cell::Cell;
use std::collections::HashMap;

use crate::diff::types::{ChangeKind, DiffContent};
use crate::diff::{binary_diff_content, compute_diff_content, compute_full_diff_content};
use crate::git::GitRepo;
use crate::git::types::{ContentResult, FileEntry};
use crate::syntax::{build_styled_diff, StyledDiffContent};

const NO_ACTIVE_HUNK_MSG: &str = "No active hunk in view — press n to navigate to a hunk";

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
    SelectSidebar,
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
    ToggleFullFile,
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
    pub scroll_positions: HashMap<(String, SidebarSection, bool), u16>,
    pub diff_stale: bool,
    pub auto_reload: bool,
    pub status_message: Option<String>,
    pub sidebar_collapsed: bool,
    pub pending_discard: Option<PendingDiscard>,
    pub show_full_file: bool,
    pub diff_viewport_height: Cell<u16>,
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
            show_full_file: false,
            diff_viewport_height: Cell::new(0),
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

        self.diff_content = Some(if self.show_full_file {
            compute_full_diff_content(path, old_text, new_text)
        } else {
            compute_diff_content(path, old_text, new_text)
        });

        // Populate styled_diff and hunk_line_starts after diff_content is set
        if let Some(dc) = &self.diff_content {
            self.styled_diff = build_styled_diff(dc, old_text, new_text);
            self.hunk_line_starts = compute_hunk_line_starts(self.diff_content.as_ref());
        }

        // Set the active hunk based on the current (restored) scroll position so
        // that filler "hunks" without headers are skipped and an off-screen hunk
        // isn't treated as active.
        self.update_hunk_from_scroll();
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
                self.sidebar_collapsed = false;
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
                let change_starts = self.change_hunk_starts();
                if let Some(&(pos, s)) = change_starts.iter().find(|(_, s)| *s > self.diff_scroll) {
                    self.diff_scroll = s;
                    self.current_hunk_index = Some(pos);
                } else if let Some(&(pos, s)) = change_starts.last() {
                    // Wrap to last change hunk
                    self.diff_scroll = s;
                    self.current_hunk_index = Some(pos);
                }
            }
            Message::PrevHunk => {
                self.status_message = None;
                let change_starts = self.change_hunk_starts();
                if let Some(&(pos, s)) = change_starts.iter().rev().find(|(_, s)| *s < self.diff_scroll) {
                    self.diff_scroll = s;
                    self.current_hunk_index = Some(pos);
                } else if let Some(&(pos, s)) = change_starts.first() {
                    // Wrap to first change hunk
                    self.diff_scroll = s;
                    self.current_hunk_index = Some(pos);
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
                        let new_content = self.repo.workdir_content(&entry.path)
                            .ok()
                            .and_then(|c| match c { ContentResult::Text(s) => Some(s.clone()), _ => None });
                        if let (Some(old), Some(new)) = (old_content, new_content) {
                            if let Err(e) = self.repo.stage_hunk(&entry.path, &old, &new, hunk) {
                                self.status_message = Some(format!("Stage hunk failed: {}", e));
                            }
                            // Persist current scroll so the post-refresh reload
                            // can restore the user's reading position instead
                            // of jumping back to the previously saved value.
                            self.save_scroll_position();
                            self.refresh_files();
                            if !self.show_full_file && !self.hunk_line_starts.is_empty() {
                                // Hunks-only: jump to a nearby remaining hunk so
                                // the user can keep staging. In full-file mode,
                                // hunk indices include fillers, so clamping by
                                // index would land on context — leave the scroll
                                // alone and let update_hunk_from_scroll pick the
                                // active hunk.
                                let clamped = hunk_idx.min(self.hunk_line_starts.len() - 1);
                                self.current_hunk_index = Some(clamped);
                                self.diff_scroll = self.hunk_line_starts[clamped];
                            }
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
                            if let Err(e) = self.repo.unstage_hunk(&entry.path, &idx_content, hunk) {
                                self.status_message = Some(format!("Unstage hunk failed: {}", e));
                            }
                            self.save_scroll_position();
                            self.refresh_files();
                            if !self.show_full_file && !self.hunk_line_starts.is_empty() {
                                let clamped = hunk_idx.min(self.hunk_line_starts.len() - 1);
                                self.current_hunk_index = Some(clamped);
                                self.diff_scroll = self.hunk_line_starts[clamped];
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
                                    if let Err(e) = self.repo.discard_hunk(&entry.path, &wc, hunk) {
                                        self.status_message = Some(format!("Discard hunk failed: {}", e));
                                    }
                                    self.save_scroll_position();
                                    self.refresh_files();
                                    if !self.show_full_file && !self.hunk_line_starts.is_empty() {
                                        let clamped = hunk_idx.min(self.hunk_line_starts.len() - 1);
                                        self.current_hunk_index = Some(clamped);
                                        self.diff_scroll = self.hunk_line_starts[clamped];
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
            Message::ToggleFullFile => {
                self.status_message = None;
                self.toggle_full_file();
            }
        }
    }

    /// Toggle full-file view, anchoring to the top line of the viewport.
    fn toggle_full_file(&mut self) {
        let anchor_row = self.diff_scroll as usize;
        let was_header = self
            .diff_content
            .as_ref()
            .map(|dc| is_header_row(dc, anchor_row))
            .unwrap_or(false);
        let anchor = self
            .diff_content
            .as_ref()
            .and_then(|dc| diff_line_at_row(dc, anchor_row));
        let prior_scroll = self.diff_scroll;

        self.show_full_file = !self.show_full_file;
        self.load_diff_for_selected();

        let max_scroll = self.total_diff_lines().saturating_sub(1) as u16;
        let new_row = match (anchor, self.diff_content.as_ref()) {
            (Some(a), Some(dc)) => row_for_diff_line(dc, a),
            _ => None,
        };
        if let Some(row) = new_row {
            // row_for_diff_line matches on file line numbers (ignoring ChangeKind::Header),
            // so when the anchor was a header line, it returns the first content line below it.
            // Subtract 1 to place the header itself at the top of the viewport.
            let row = if was_header && row > 0 { row - 1 } else { row };
            self.diff_scroll = (row as u16).min(max_scroll);
        } else if let Some(a) = anchor {
            // Anchor line not in any hunk (e.g. a context-only Equal line in full-file view
            // that doesn't fall within any change group). Jump to the first hunk that starts
            // after the anchor's file line, or the last hunk if all are before it.
            let anchor_line = a.2.or(a.1).unwrap_or(0);
            let hunk_row = self.diff_content.as_ref()
                .map(|dc| first_hunk_row_after(dc, anchor_line))
                .unwrap_or(0);
            self.diff_scroll = hunk_row.min(max_scroll);
        } else {
            self.diff_scroll = prior_scroll.min(max_scroll);
        }
        self.update_hunk_from_scroll();
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
        let viewport_top = self.diff_scroll;
        let viewport_height = self.diff_viewport_height.get();
        let cushion = viewport_top.saturating_add(3);
        let cushion_lower = viewport_top.saturating_add(viewport_height / 2);
        let diff = self.diff_content.as_ref();

        let candidates: Vec<(usize, u16, u16)> = self
            .hunk_line_starts
            .iter()
            .enumerate()
            .filter_map(|(i, &start)| {
                let hunk = diff?.hunks.get(i)?;
                if !hunk.has_header {
                    return None;
                }
                let length = 1u16.saturating_add(hunk.lines.len() as u16);
                let end = start.saturating_add(length);
                Some((i, start, end))
            })
            .collect();

        // Preferred: latest hunk whose header is at-or-just-below the top
        // (the existing +3 cushion rule).
        let cushioned = candidates
            .iter()
            .rfind(|(_, start, end)| *start <= cushion && *end > viewport_top)
            .map(|(i, _, _)| *i);

        // Fallback: topmost hunk that overlaps the viewport at all. Activates
        // the next hunk as soon as any part of it scrolls into view, instead
        // of waiting for its header to reach the top.
        let fallback = || {
            candidates
                .iter()
                .find(|(_, start, end)| *end > viewport_top && *start < cushion_lower)
                .map(|(i, _, _)| *i)
        };

        self.current_hunk_index = cushioned.or_else(fallback);
    }

    /// Return (hunk_index, start_row) for change hunks only — filler hunks
    /// without headers are skipped. Used for `n`/`N` navigation.
    fn change_hunk_starts(&self) -> Vec<(usize, u16)> {
        let diff = self.diff_content.as_ref();
        self.hunk_line_starts
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                diff.and_then(|dc| dc.hunks.get(*i))
                    .is_none_or(|h| h.has_header)
            })
            .map(|(i, &s)| (i, s))
            .collect()
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

/// Check if `target_row` corresponds to a hunk header row in `dc`.
fn is_header_row(dc: &DiffContent, target_row: usize) -> bool {
    if dc.is_binary || dc.hunks.is_empty() {
        return false;
    }
    let mut row: usize = 0;
    for hunk in &dc.hunks {
        if hunk.has_header {
            if target_row == row {
                return true;
            }
            row += 1;
        }
        row += hunk.lines.len();
    }
    false
}

/// Find the rendered row for a given DiffLine identity in `dc`. Returns None if
/// the line cannot be located (e.g. an Equal context line that no longer falls
/// within any hunk after switching modes).
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

/// Return the rendered start row of the first hunk in `dc` whose new-file start
/// line exceeds `anchor_line`. Falls back to the last hunk's row when all hunks
/// begin at or before `anchor_line`.
fn first_hunk_row_after(dc: &DiffContent, anchor_line: u32) -> u16 {
    let mut row: usize = 0;
    let mut last_hunk_row: u16 = 0;
    for hunk in &dc.hunks {
        let hunk_row = row as u16;
        if hunk.new_start > anchor_line {
            return hunk_row;
        }
        last_hunk_row = hunk_row;
        if hunk.has_header {
            row += 1;
        }
        row += hunk.lines.len();
    }
    last_hunk_row
}

pub fn compute_hunk_line_starts(dc: Option<&DiffContent>) -> Vec<u16> {
    let Some(dc) = dc else { return Vec::new(); };
    if dc.is_binary { return Vec::new(); }
    let mut starts: Vec<u16> = Vec::with_capacity(dc.hunks.len());
    let mut cum: u16 = 0;
    for h in &dc.hunks {
        starts.push(cum);
        let header_rows: u16 = if h.has_header { 1 } else { 0 };
        cum = cum.saturating_add(header_rows.saturating_add(h.lines.len() as u16));
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
            show_full_file: false,
            diff_viewport_height: Cell::new(0),
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
            DiffHunk { old_start: 1, new_start: 1, lines: vec![DiffLine { kind: ChangeKind::Equal, old_lineno: Some(1), new_lineno: Some(1), content: "x\n".to_string() }, DiffLine { kind: ChangeKind::Equal, old_lineno: Some(2), new_lineno: Some(2), content: "x\n".to_string() }], has_header: true },
            DiffHunk { old_start: 1, new_start: 1, lines: vec![DiffLine { kind: ChangeKind::Equal, old_lineno: Some(1), new_lineno: Some(1), content: "x\n".to_string() }, DiffLine { kind: ChangeKind::Equal, old_lineno: Some(2), new_lineno: Some(2), content: "x\n".to_string() }, DiffLine { kind: ChangeKind::Equal, old_lineno: Some(3), new_lineno: Some(3), content: "x\n".to_string() }], has_header: true },
            DiffHunk { old_start: 1, new_start: 1, lines: vec![DiffLine { kind: ChangeKind::Equal, old_lineno: Some(1), new_lineno: Some(1), content: "x\n".to_string() }, DiffLine { kind: ChangeKind::Equal, old_lineno: Some(2), new_lineno: Some(2), content: "x\n".to_string() }, DiffLine { kind: ChangeKind::Equal, old_lineno: Some(3), new_lineno: Some(3), content: "x\n".to_string() }, DiffLine { kind: ChangeKind::Equal, old_lineno: Some(4), new_lineno: Some(4), content: "x\n".to_string() }, DiffLine { kind: ChangeKind::Equal, old_lineno: Some(5), new_lineno: Some(5), content: "x\n".to_string() }], has_header: true },
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

    // ---- full-file toggle tests ----

    use crate::diff::types::{DiffHunk, DiffLine};

    fn make_dc(hunks: Vec<DiffHunk>) -> DiffContent {
        DiffContent { path: "t.rs".to_string(), hunks, is_binary: false }
    }

    fn dl(kind: ChangeKind, old: Option<u32>, new: Option<u32>) -> DiffLine {
        DiffLine { kind, old_lineno: old, new_lineno: new, content: "x\n".to_string() }
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

    /// The anchor calculation maps a hunk-mode row to a full-file-mode row
    /// such that the top visible line stays at the top after toggling.
    /// This tests the anchor-preservation logic directly using the helpers.
    #[test]
    fn test_anchor_preservation_across_modes() {
        // 200-line file with a single change at line 100
        let old: String = (1..=200).map(|n| format!("line{}\n", n)).collect();
        let new = old.replace("line100\n", "LINE100\n");

        let hunk_dc = crate::diff::compute_diff_content("t.rs", Some(&old), Some(&new));
        let full_dc = crate::diff::compute_full_diff_content("t.rs", Some(&old), Some(&new));

        // The Insert at new line 100 exists in both modes
        let target = (ChangeKind::Insert, None, Some(100u32));
        let hunk_row = row_for_diff_line(&hunk_dc, target).expect("insert in hunk mode");
        let full_row = row_for_diff_line(&full_dc, target).expect("insert in full mode");

        // Simulate: anchor at top of viewport (scroll = target row).
        let hunk_scroll = hunk_row as u16;

        // Anchor logic: walk current view to find what line is at anchor row.
        let anchor_row = hunk_scroll as usize;
        let anchor = diff_line_at_row(&hunk_dc, anchor_row).expect("anchor present");
        assert_eq!(anchor, target);

        // Compute new scroll for full-file mode: anchor line goes to top.
        let new_row = row_for_diff_line(&full_dc, anchor).expect("found in full");
        assert_eq!(new_row, full_row);
        let full_scroll = new_row as u16;

        // Verify the same line is at the top of the viewport.
        let resolved = diff_line_at_row(&full_dc, full_scroll as usize);
        assert_eq!(resolved, Some(target));
    }

    /// When the top line is a hunk header, toggling should keep the header
    /// visible at the top (not push it out of view).
    #[test]
    fn test_anchor_preservation_with_header_at_top() {
        // Multi-hunk file where a hunk starts at row 0 (top of viewport)
        let old: String = (1..=200).map(|n| format!("line{}\n", n)).collect();
        // Two separate changes: one at line 10, another at line 100
        let new = old.replace("line10\n", "LINE10\n").replace("line100\n", "LINE100\n");

        let hunk_dc = crate::diff::compute_diff_content("t.rs", Some(&old), Some(&new));
        let full_dc = crate::diff::compute_full_diff_content("t.rs", Some(&old), Some(&new));

        // Scroll so the first hunk header is at the top (row 0)
        let hunk_scroll: u16 = 0;

        // Verify we're on a header row
        assert!(is_header_row(&hunk_dc, hunk_scroll as usize));

        // Anchor logic from toggle_full_file
        let was_header = is_header_row(&hunk_dc, hunk_scroll as usize);
        let anchor = diff_line_at_row(&hunk_dc, hunk_scroll as usize).expect("anchor");

        // Find the corresponding row in full mode
        let new_row = row_for_diff_line(&full_dc, anchor).expect("found in full");
        let full_scroll = if was_header && new_row > 0 {
            new_row - 1
        } else {
            new_row
        } as u16;

        // Verify the header is at the top of the viewport in full mode
        assert!(is_header_row(&full_dc, full_scroll as usize));
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
    /// the rendered diff and the corresponding `hunk_line_starts` populated.
    /// `hunk_start_row` is the rendered row at which the hunk begins; the hunk
    /// is `lines_count` content lines long (plus its header row).
    fn app_with_single_change_hunk(hunk_start_row: u16, lines_count: usize) -> App {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        let hunk = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: (0..lines_count)
                .map(|i| dl(ChangeKind::Equal, Some(i as u32 + 1), Some(i as u32 + 1)))
                .collect(),
            has_header: true,
        };
        app.diff_content = Some(make_dc(vec![hunk]));
        app.hunk_line_starts = vec![hunk_start_row];
        app.current_hunk_index = Some(0);
        app
    }

    #[test]
    fn test_update_hunk_from_scroll_clears_when_scrolled_past_hunk() {
        // 3-line hunk at row 10 (header + 2 content = 3 rendered rows).
        // Scrolled well past so its end row (13) is above the viewport top.
        let mut app = app_with_single_change_hunk(10, 2);
        app.diff_scroll = 50;
        app.update_hunk_from_scroll();
        assert_eq!(app.current_hunk_index, None);
    }

    #[test]
    fn test_update_hunk_from_scroll_keeps_hunk_when_at_top() {
        // Scroll lined up with the hunk header — it's at the top of the viewport.
        let mut app = app_with_single_change_hunk(10, 5);
        app.diff_scroll = 10;
        app.update_hunk_from_scroll();
        assert_eq!(app.current_hunk_index, Some(0));
    }

    #[test]
    fn test_update_hunk_from_scroll_clears_when_scrolled_one_past_end() {
        // Header at row 10, 2 content lines, last rendered row = 12.
        // Scrolling to row 13 means the hunk has just left the viewport top.
        let mut app = app_with_single_change_hunk(10, 2);
        app.diff_scroll = 13;
        app.update_hunk_from_scroll();
        assert_eq!(app.current_hunk_index, None);
    }

    #[test]
    fn test_update_hunk_from_scroll_full_file_no_hunk_when_in_leading_filler() {
        // Full-file layout: leading filler at row 0 (no header) followed by a
        // change hunk far below. Scroll=0 sits inside the filler, so no
        // change hunk should be active.
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        let leading = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![dl(ChangeKind::Equal, Some(1), Some(1)); 90],
            has_header: false,
        };
        let change = DiffHunk {
            old_start: 91,
            new_start: 91,
            lines: vec![dl(ChangeKind::Insert, None, Some(91))],
            has_header: true,
        };
        app.diff_content = Some(make_dc(vec![leading, change]));
        app.hunk_line_starts = vec![0, 90];
        app.diff_scroll = 0;
        app.update_hunk_from_scroll();
        assert_eq!(app.current_hunk_index, None);
    }

    #[test]
    fn test_fallback_activates_hunk_in_full_file_dead_zone() {
        // Two hunks: first at row 5 (scrolled off), second at row 50.
        // Viewport: top=30, height=40 → bottom=70, which includes second hunk.
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        let hunk1 = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![dl(ChangeKind::Insert, None, Some(1)); 3],
            has_header: true,
        };
        let hunk2 = DiffHunk {
            old_start: 50,
            new_start: 50,
            lines: vec![dl(ChangeKind::Insert, None, Some(50)); 3],
            has_header: true,
        };
        app.diff_content = Some(make_dc(vec![hunk1, hunk2]));
        app.hunk_line_starts = vec![5, 50];
        app.diff_scroll = 35;
        app.diff_viewport_height.set(40);
        app.update_hunk_from_scroll();
        assert_eq!(app.current_hunk_index, Some(1));
    }

    #[test]
    fn test_fallback_ignores_hunks_below_viewport() {
        // Same layout but viewport height=10 → bottom=40, second hunk at row 50
        // is below the viewport. Expect no active hunk.
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        let hunk1 = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![dl(ChangeKind::Insert, None, Some(1)); 3],
            has_header: true,
        };
        let hunk2 = DiffHunk {
            old_start: 50,
            new_start: 50,
            lines: vec![dl(ChangeKind::Insert, None, Some(50)); 3],
            has_header: true,
        };
        app.diff_content = Some(make_dc(vec![hunk1, hunk2]));
        app.hunk_line_starts = vec![5, 50];
        app.diff_scroll = 30;
        app.diff_viewport_height.set(10);
        app.update_hunk_from_scroll();
        assert_eq!(app.current_hunk_index, None);
    }

    #[test]
    fn test_cushion_wins_over_fallback() {
        // Hunk at row 2 satisfies the cushion (start=2 <= scroll+3=3), hunk at
        // row 10 is in viewport but cushion should take priority.
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
            lines: vec![dl(ChangeKind::Insert, None, Some(10)); 5],
            has_header: true,
        };
        app.diff_content = Some(make_dc(vec![hunk1, hunk2]));
        app.hunk_line_starts = vec![2, 10];
        app.diff_scroll = 0;
        app.diff_viewport_height.set(30);
        app.update_hunk_from_scroll();
        assert_eq!(app.current_hunk_index, Some(0));
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
}