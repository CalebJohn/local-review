use std::collections::VecDeque;
use std::error::Error;

use crate::git::{GitRepo, Snapshot};

const MAX_ENTRIES: usize = 100;

#[derive(Debug, Clone)]
pub struct UndoEntry {
    pub label: &'static str,
    pub snapshots: Vec<Snapshot>,
}

#[derive(Debug)]
pub enum UndoOutcome {
    Done(&'static str),
    Empty,
    Failed(String),
}

#[derive(Debug, Default)]
pub struct UndoManager {
    undo: VecDeque<UndoEntry>,
    redo: VecDeque<UndoEntry>,
}

impl UndoManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(
        &mut self,
        repo: &GitRepo,
        label: &'static str,
        paths: &[String],
    ) -> Result<(), Box<dyn Error>> {
        let mut snapshots = Vec::with_capacity(paths.len());
        for path in paths {
            snapshots.push(repo.snapshot_path(path)?);
        }
        self.undo.push_back(UndoEntry { label, snapshots });
        self.redo.clear();
        if self.undo.len() > MAX_ENTRIES {
            self.undo.pop_front();
        }
        Ok(())
    }

    pub fn discard_last(&mut self) {
        self.undo.pop_back();
    }

    pub fn undo(&mut self, repo: &GitRepo) -> UndoOutcome {
        let Some(entry) = self.undo.pop_back() else {
            return UndoOutcome::Empty;
        };
        Self::swap(entry, &mut self.undo, &mut self.redo, repo)
    }

    pub fn redo(&mut self, repo: &GitRepo) -> UndoOutcome {
        let Some(entry) = self.redo.pop_back() else {
            return UndoOutcome::Empty;
        };
        Self::swap(entry, &mut self.redo, &mut self.undo, repo)
    }

    fn swap(
        entry: UndoEntry,
        rollback: &mut VecDeque<UndoEntry>,
        forward: &mut VecDeque<UndoEntry>,
        repo: &GitRepo,
    ) -> UndoOutcome {
        let mut current_snaps = Vec::with_capacity(entry.snapshots.len());
        for snap in &entry.snapshots {
            match repo.snapshot_path(&snap.path) {
                Ok(s) => current_snaps.push(s),
                Err(e) => {
                    rollback.push_back(entry);
                    return UndoOutcome::Failed(e.to_string());
                }
            }
        }

        for snap in &entry.snapshots {
            if let Err(e) = repo.restore_snapshot(snap) {
                rollback.push_back(entry);
                return UndoOutcome::Failed(e.to_string());
            }
        }

        let label = entry.label;
        forward.push_back(UndoEntry {
            label,
            snapshots: current_snaps,
        });
        UndoOutcome::Done(label)
    }
}

#[cfg(test)]
#[path = "undo_tests.rs"]
mod tests;
