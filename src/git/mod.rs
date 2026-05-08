pub mod hunk;
pub mod staging;
pub mod status;
pub mod types;

use std::path::Path;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use status::{is_binary_content, map_index_status, map_workdir_status};
use types::{ContentResult, FileEntry};

pub use staging::Snapshot;

pub struct GitRepo {
    pub(super) repo: git2::Repository,
}

impl GitRepo {
    pub fn open(path: &str) -> Result<Self, git2::Error> {
        let repo = git2::Repository::discover(path)?;
        Ok(Self { repo })
    }

    pub fn workdir_path(&self) -> Option<&Path> {
        self.repo.workdir()
    }

    pub(super) fn workdir_or_err(&self) -> Result<&Path, Box<dyn std::error::Error>> {
        self.repo.workdir()
            .ok_or_else(|| -> Box<dyn std::error::Error> {
                "bare repository has no working directory".into()
            })
    }

    pub fn git_dir(&self) -> &Path {
        self.repo.path()
    }

    pub fn changed_files(&self) -> Result<Vec<FileEntry>, git2::Error> {
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true);
        opts.recurse_untracked_dirs(true);
        let statuses = self.repo.statuses(Some(&mut opts))?;

        let files: Vec<FileEntry> = statuses
            .iter()
            .filter(|e| e.status() != git2::Status::CURRENT)
            .map(|entry| {
                let path = entry.path().unwrap_or("").to_string();
                let index_status = map_index_status(entry.status());
                let workdir_status = map_workdir_status(entry.status());
                FileEntry {
                    path,
                    index_status,
                    workdir_status,
                }
            })
            .collect();

        Ok(files)
    }

    pub fn head_content(&self, path: &str) -> Result<ContentResult, Box<dyn std::error::Error>> {
        let head = match self.repo.head() {
            Ok(h) => h,
            Err(_) => return Ok(ContentResult::NotFound),
        };
        let tree = head.peel_to_tree()?;
        match tree.get_path(Path::new(path)) {
            Ok(entry) => {
                let blob = self.repo.find_blob(entry.id())?;
                let content = blob.content();
                if is_binary_content(content) {
                    Ok(ContentResult::Binary)
                } else {
                    Ok(ContentResult::Text(
                        String::from_utf8_lossy(content).to_string(),
                    ))
                }
            }
            Err(_) => Ok(ContentResult::NotFound),
        }
    }

    pub fn index_content(&self, path: &str) -> Result<ContentResult, Box<dyn std::error::Error>> {
        let index = self.repo.index()?;
        match index.get_path(Path::new(path), 0) {
            Some(entry) => {
                let blob = self.repo.find_blob(entry.id)?;
                let content = blob.content();
                if is_binary_content(content) {
                    Ok(ContentResult::Binary)
                } else {
                    Ok(ContentResult::Text(
                        String::from_utf8_lossy(content).to_string(),
                    ))
                }
            }
            None => Ok(ContentResult::NotFound),
        }
    }

    pub fn workdir_content(&self, path: &str) -> Result<ContentResult, Box<dyn std::error::Error>> {
        let workdir = self.workdir_or_err()?;
        let full_path = workdir.join(path);

        if let Ok(meta) = std::fs::symlink_metadata(&full_path) {
            if meta.file_type().is_symlink() {
                let target = std::fs::read_link(&full_path)?;
                #[cfg(unix)]
                let text = String::from_utf8_lossy(target.as_os_str().as_bytes()).into_owned();
                #[cfg(not(unix))]
                let text = target.to_string_lossy().into_owned();
                return Ok(ContentResult::Text(text));
            }
        }

        match std::fs::read(&full_path) {
            Ok(bytes) => {
                if is_binary_content(&bytes) {
                    Ok(ContentResult::Binary)
                } else {
                    Ok(ContentResult::Text(
                        String::from_utf8_lossy(&bytes).to_string(),
                    ))
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ContentResult::NotFound),
            Err(e) => Err(Box::new(e)),
        }
    }

    pub(super) fn index_entry_for_path(&self, index: &git2::Index, path: &str) -> git2::IndexEntry {
        if let Some(existing) = index.get_path(Path::new(path), 0) {
            existing
        } else {
            debug_assert!(
                !self.repo.workdir()
                    .map(|wd| std::fs::symlink_metadata(wd.join(path))
                        .map(|m| m.file_type().is_symlink())
                        .unwrap_or(false))
                    .unwrap_or(false),
                "index_entry_for_path fallback reached for symlink: {path}"
            );
            git2::IndexEntry {
                ctime: git2::IndexTime::new(0, 0),
                mtime: git2::IndexTime::new(0, 0),
                dev: 0,
                ino: 0,
                mode: 0o100644,
                uid: 0,
                gid: 0,
                file_size: 0,
                id: git2::Oid::from_bytes(&[0; 20]).unwrap(),
                flags: 0,
                flags_extended: 0,
                path: path.as_bytes().to_vec(),
            }
        }
    }
}

#[cfg(test)]
mod tests;
