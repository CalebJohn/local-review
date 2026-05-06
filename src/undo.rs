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
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct TempRepo {
        path: PathBuf,
        repo: GitRepo,
    }

    impl TempRepo {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("undo_test_{}_{}", name, std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            let r = git2::Repository::init(&path).expect("init repo");
            r.config().unwrap().set_str("user.email", "test@test.com").unwrap();
            r.config().unwrap().set_str("user.name", "Test").unwrap();

            // Empty initial commit so HEAD exists.
            let mut index = r.index().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = r.find_tree(tree_id).unwrap();
            let sig = r.signature().unwrap();
            r.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
                .unwrap();

            let repo = GitRepo::open(path.to_str().unwrap()).expect("open");
            TempRepo { path, repo }
        }

        fn write(&self, name: &str, content: &str) {
            std::fs::write(self.path.join(name), content).unwrap();
        }

        fn read(&self, name: &str) -> String {
            std::fs::read_to_string(self.path.join(name)).unwrap()
        }

        fn exists(&self, name: &str) -> bool {
            self.path.join(name).exists()
        }

        fn stage(&self, name: &str) {
            self.repo.stage_file(name).unwrap();
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn record_then_undo_restores_prior_state() {
        let t = TempRepo::new("undo_restore");
        t.write("a.txt", "before\n");

        let mut mgr = UndoManager::new();
        mgr.record(&t.repo, "edit", &["a.txt".to_string()]).unwrap();

        t.write("a.txt", "after\n");
        assert_eq!(t.read("a.txt"), "after\n");

        match mgr.undo(&t.repo) {
            UndoOutcome::Done(label) => assert_eq!(label, "edit"),
            other => panic!("expected Done, got {:?}", other),
        }
        assert_eq!(t.read("a.txt"), "before\n");
    }

    #[test]
    fn undo_then_redo_returns_to_post_mutation_state() {
        let t = TempRepo::new("redo_round_trip");
        t.write("a.txt", "v1\n");

        let mut mgr = UndoManager::new();
        mgr.record(&t.repo, "stage file", &["a.txt".to_string()]).unwrap();
        t.write("a.txt", "v2\n");

        mgr.undo(&t.repo);
        assert_eq!(t.read("a.txt"), "v1\n");

        match mgr.redo(&t.repo) {
            UndoOutcome::Done(label) => assert_eq!(label, "stage file"),
            other => panic!("expected Done, got {:?}", other),
        }
        assert_eq!(t.read("a.txt"), "v2\n");
    }

    #[test]
    fn record_clears_redo_stack() {
        let t = TempRepo::new("clear_redo");
        t.write("a.txt", "v1\n");

        let mut mgr = UndoManager::new();
        mgr.record(&t.repo, "first", &["a.txt".to_string()]).unwrap();
        t.write("a.txt", "v2\n");
        mgr.undo(&t.repo);

        // Now there's a redo entry. Recording a new action should drop it.
        mgr.record(&t.repo, "second", &["a.txt".to_string()]).unwrap();

        match mgr.redo(&t.repo) {
            UndoOutcome::Empty => {}
            other => panic!("expected Empty after record cleared redo, got {:?}", other),
        }
    }

    #[test]
    fn discard_last_removes_only_undo_entry() {
        let t = TempRepo::new("discard_last");
        t.write("a.txt", "v1\n");

        let mut mgr = UndoManager::new();
        mgr.record(&t.repo, "first", &["a.txt".to_string()]).unwrap();
        t.write("a.txt", "v2\n");
        mgr.undo(&t.repo);
        // Redo stack now has an entry.

        mgr.record(&t.repo, "second", &["a.txt".to_string()]).unwrap();
        // record cleared redo.

        // Stage a third entry so we have something to discard.
        mgr.record(&t.repo, "third", &["a.txt".to_string()]).unwrap();
        mgr.discard_last();

        // Undo should now run "second", not "third".
        match mgr.undo(&t.repo) {
            UndoOutcome::Done(label) => assert_eq!(label, "second"),
            other => panic!("expected Done(second), got {:?}", other),
        }
    }

    #[test]
    fn cap_drops_oldest_entry() {
        let t = TempRepo::new("cap");
        t.write("a.txt", "v0\n");

        let mut mgr = UndoManager::new();
        // Push MAX_ENTRIES + 1 entries; the very first one should be dropped.
        for i in 0..=MAX_ENTRIES {
            t.write("a.txt", &format!("v{}\n", i));
            mgr.record(&t.repo, "tick", &["a.txt".to_string()]).unwrap();
        }
        assert_eq!(mgr.undo.len(), MAX_ENTRIES);

        // The oldest snapshot ("v0") should no longer be reachable. Undo all
        // the way down and confirm we end at v1, not v0.
        let final_workdir = format!("v{}\n", MAX_ENTRIES);
        t.write("a.txt", &final_workdir);
        for _ in 0..MAX_ENTRIES {
            match mgr.undo(&t.repo) {
                UndoOutcome::Done(_) => {}
                other => panic!("expected Done, got {:?}", other),
            }
        }
        assert_eq!(t.read("a.txt"), "v1\n");
    }

    #[test]
    fn empty_outcome_on_empty_stacks() {
        let t = TempRepo::new("empty");
        let mut mgr = UndoManager::new();
        match mgr.undo(&t.repo) {
            UndoOutcome::Empty => {}
            other => panic!("expected Empty, got {:?}", other),
        }
        match mgr.redo(&t.repo) {
            UndoOutcome::Empty => {}
            other => panic!("expected Empty, got {:?}", other),
        }
    }

    #[test]
    fn done_surfaces_label() {
        let t = TempRepo::new("label");
        t.write("a.txt", "v1\n");

        let mut mgr = UndoManager::new();
        mgr.record(&t.repo, "discard hunk", &["a.txt".to_string()]).unwrap();
        t.write("a.txt", "v2\n");

        match mgr.undo(&t.repo) {
            UndoOutcome::Done(label) => assert_eq!(label, "discard hunk"),
            other => panic!("expected Done(\"discard hunk\"), got {:?}", other),
        }
        match mgr.redo(&t.repo) {
            UndoOutcome::Done(label) => assert_eq!(label, "discard hunk"),
            other => panic!("expected Done(\"discard hunk\"), got {:?}", other),
        }
    }

    #[test]
    fn record_snapshots_multiple_paths_atomically() {
        let t = TempRepo::new("multi_path");
        t.write("a.txt", "a-before\n");
        t.write("b.txt", "b-before\n");
        t.stage("a.txt");
        t.stage("b.txt");

        let mut mgr = UndoManager::new();
        mgr.record(
            &t.repo,
            "multi",
            &["a.txt".to_string(), "b.txt".to_string()],
        )
        .unwrap();

        t.write("a.txt", "a-after\n");
        t.write("b.txt", "b-after\n");

        mgr.undo(&t.repo);
        assert_eq!(t.read("a.txt"), "a-before\n");
        assert_eq!(t.read("b.txt"), "b-before\n");
    }

    #[test]
    fn restores_deleted_workdir_file() {
        let t = TempRepo::new("restore_deleted");
        t.write("a.txt", "v1\n");

        let mut mgr = UndoManager::new();
        mgr.record(&t.repo, "delete", &["a.txt".to_string()]).unwrap();

        std::fs::remove_file(t.path.join("a.txt")).unwrap();
        assert!(!t.exists("a.txt"));

        mgr.undo(&t.repo);
        assert_eq!(t.read("a.txt"), "v1\n");
    }
}
