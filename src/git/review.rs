//! Review-mode git operations: resolving CLI review specs to concrete
//! base/head trees and enumerating the files changed between them.

use crate::cli::ReviewArgs;

use super::types::{ContentResult, FileEntry, FileStatus};
use super::GitRepo;

#[derive(Debug, Clone)]
pub enum ReviewHead {
    /// Compare against the working tree (staged + unstaged + untracked).
    Workdir,
    /// Compare against a fixed tree (the head commit's tree).
    Commit(git2::Oid),
}

#[derive(Debug, Clone)]
pub struct ReviewTarget {
    pub base_tree: git2::Oid,
    pub head: ReviewHead,
    pub label: String,
}

impl GitRepo {
    /// Resolve CLI review args into a concrete base tree + head, or an error
    /// (unknown revision, no merge base, non-commit object) that the caller
    /// reports and aborts on before entering the TUI.
    pub fn resolve_review(
        &self,
        args: &ReviewArgs,
    ) -> Result<ReviewTarget, Box<dyn std::error::Error>> {
        match args {
            ReviewArgs::SingleRef(r) => {
                let head_commit = self.repo.head()?.peel_to_commit()?;
                let base_commit = self.repo.revparse_single(r)?.peel_to_commit()?;
                let merge_base = match self.repo.merge_base(head_commit.id(), base_commit.id())
                {
                    Ok(oid) => oid,
                    Err(_) => return Err(format!("no merge base between HEAD and '{r}'").into()),
                };
                let base_tree = self.repo.find_commit(merge_base)?.tree_id();
                Ok(ReviewTarget {
                    base_tree,
                    head: ReviewHead::Workdir,
                    label: r.clone(),
                })
            }
            ReviewArgs::Range {
                from,
                to,
                three_dot,
            } => {
                let from_commit = self.repo.revparse_single(from)?.peel_to_commit()?;
                let to_commit = self.repo.revparse_single(to)?.peel_to_commit()?;
                let base = if *three_dot {
                    match self.repo.merge_base(from_commit.id(), to_commit.id()) {
                        Ok(oid) => oid,
                        Err(_) => {
                            return Err(
                                format!("no merge base between '{from}' and '{to}'").into()
                            )
                        }
                    }
                } else {
                    from_commit.id()
                };
                let base_tree = self.repo.find_commit(base)?.tree_id();
                Ok(ReviewTarget {
                    base_tree,
                    head: ReviewHead::Commit(to_commit.tree_id()),
                    label: if *three_dot {
                        format!("{from}...{to}")
                    } else {
                        format!("{from}..{to}")
                    },
                })
            }
        }
    }

    /// Files changed between the target's base tree and its head, with
    /// per-file status. Untracked files sort last, mirroring
    /// `App::partition_files` for the default mode.
    pub fn review_files(&self, target: &ReviewTarget) -> Result<Vec<FileEntry>, git2::Error> {
        let base_tree = self.repo.find_tree(target.base_tree)?;
        let mut opts = git2::DiffOptions::new();
        opts.include_untracked(true).recurse_untracked_dirs(true);
        let mut diff = match target.head {
            ReviewHead::Workdir => {
                self.repo
                    .diff_tree_to_workdir_with_index(Some(&base_tree), Some(&mut opts))?
            }
            ReviewHead::Commit(to) => {
                let to_tree = self.repo.find_tree(to)?;
                self.repo
                    .diff_tree_to_tree(Some(&base_tree), Some(&to_tree), Some(&mut opts))?
            }
        };
        diff.find_similar(None)?;

        let mut files: Vec<FileEntry> = Vec::new();
        diff.foreach(
            &mut |delta, _| {
                if let Some(entry) = delta_to_entry(delta) {
                    files.push(entry);
                }
                true
            },
            None,
            None,
            None,
        )?;

        files.sort_by(|a, b| {
            let a_untracked = matches!(a.workdir_status, Some(FileStatus::Untracked));
            let b_untracked = matches!(b.workdir_status, Some(FileStatus::Untracked));
            a_untracked
                .cmp(&b_untracked)
                .then_with(|| a.path.cmp(&b.path))
        });
        Ok(files)
    }

    /// Content of `path` in the tree identified by `tree`, as `ContentResult`
    /// (NotFound when the path is absent, Binary on NUL-byte detection).
    pub fn tree_content(
        &self,
        tree: git2::Oid,
        path: &str,
    ) -> Result<ContentResult, Box<dyn std::error::Error>> {
        let tree = self.repo.find_tree(tree)?;
        super::content_in_tree(&self.repo, &tree, path)
    }
}

fn delta_to_entry(delta: git2::DiffDelta) -> Option<FileEntry> {
    let status = match delta.status() {
        git2::Delta::Added => FileStatus::Added,
        git2::Delta::Deleted => FileStatus::Deleted,
        git2::Delta::Modified => FileStatus::Modified,
        git2::Delta::Renamed => FileStatus::Renamed,
        git2::Delta::Untracked => FileStatus::Untracked,
        git2::Delta::Typechange
        | git2::Delta::Conflicted
        | git2::Delta::Ignored
        | git2::Delta::Copied
        | git2::Delta::Unmodified
        | git2::Delta::Unreadable => return None,
    };
    let path = match delta.status() {
        git2::Delta::Deleted => delta.old_file().path(),
        _ => delta.new_file().path().or_else(|| delta.old_file().path()),
    }?
    .to_str()?;
    Some(FileEntry {
        path: path.to_string(),
        index_status: None,
        workdir_status: Some(status),
    })
}
