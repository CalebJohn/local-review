use crate::git::types::ContentResult;
use crate::undo::UndoOutcome;

use super::{App, AppMode, PendingDiscard, SidebarSection, NO_ACTIVE_HUNK_MSG};

impl App {
    pub(super) fn handle_stage_file(&mut self) {
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

    pub(super) fn handle_unstage_file(&mut self) {
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

    pub(super) fn handle_stage_hunk(&mut self) {
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
                    self.save_scroll_position();
                    self.refresh_files();
                    self.restore_hunk_position(hunk_idx);
                }
            }
        }
    }

    pub(super) fn handle_unstage_hunk(&mut self) {
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

    pub(super) fn handle_discard_file(&mut self) {
        if self.sidebar_section != SidebarSection::Unstaged {
            return;
        }
        if let Some(entry) = self.selected_entry().cloned() {
            let pending = PendingDiscard::File { path: entry.path.clone() };
            if self.pending_discard.as_ref() == Some(&pending) {
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
                self.pending_discard = Some(pending);
                self.status_message = Some(
                    format!("Discard all changes to {}? Press d again to confirm (IRREVERSIBLE)", entry.path),
                );
            }
        }
    }

    pub(super) fn handle_discard_hunk(&mut self) {
        if self.sidebar_section != SidebarSection::Unstaged || self.diff_stale {
            return;
        }
        if self.current_hunk_index.is_none() {
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
                self.pending_discard = Some(pending);
                self.status_message = Some(
                    "Discard this hunk? Press d again to confirm (IRREVERSIBLE)".to_string(),
                );
            }
        }
    }

    pub(super) fn handle_stage_selected_lines(&mut self) {
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

    pub(super) fn handle_unstage_selected_lines(&mut self) {
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

    pub(super) fn handle_undo(&mut self) {
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

    pub(super) fn handle_redo(&mut self) {
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
}
