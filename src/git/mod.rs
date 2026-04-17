pub mod types;

use std::path::Path;
use types::{ContentResult, FileEntry, FileStatus};

pub struct GitRepo {
    repo: git2::Repository,
}

/// Check for null byte in the first 8192 bytes of content.
/// Same heuristic as git for binary detection.
fn is_binary_content(bytes: &[u8]) -> bool {
    let check_len = std::cmp::min(bytes.len(), 8192);
    bytes[..check_len].contains(&0)
}

fn map_index_status(status: git2::Status) -> Option<FileStatus> {
    if status.contains(git2::Status::INDEX_NEW) {
        Some(FileStatus::Added)
    } else if status.contains(git2::Status::INDEX_MODIFIED) {
        Some(FileStatus::Modified)
    } else if status.contains(git2::Status::INDEX_DELETED) {
        Some(FileStatus::Deleted)
    } else if status.contains(git2::Status::INDEX_RENAMED) {
        Some(FileStatus::Renamed)
    } else {
        None
    }
}

fn map_workdir_status(status: git2::Status) -> Option<FileStatus> {
    if status.contains(git2::Status::WT_NEW) {
        Some(FileStatus::Added)
    } else if status.contains(git2::Status::WT_MODIFIED) {
        Some(FileStatus::Modified)
    } else if status.contains(git2::Status::WT_DELETED) {
        Some(FileStatus::Deleted)
    } else if status.contains(git2::Status::WT_RENAMED) {
        Some(FileStatus::Renamed)
    } else {
        None
    }
}

impl GitRepo {
    pub fn open(path: &str) -> Result<Self, git2::Error> {
        let repo = git2::Repository::discover(path)?;
        Ok(Self { repo })
    }

    pub fn changed_files(&self) -> Result<Vec<FileEntry>, git2::Error> {
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true);
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

    pub fn head_content(&self, path: &str) -> Result<ContentResult, git2::Error> {
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

    pub fn index_content(&self, path: &str) -> Result<ContentResult, git2::Error> {
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
        let workdir = self
            .repo
            .workdir()
            .ok_or_else(|| -> Box<dyn std::error::Error> {
                "bare repository has no working directory".into()
            })?;
        let full_path = workdir.join(path);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_binary_content_with_null_byte() {
        assert!(is_binary_content(b"hello\x00world"));
    }

    #[test]
    fn test_is_binary_content_without_null_byte() {
        assert!(!is_binary_content(b"hello world"));
    }

    #[test]
    fn test_is_binary_content_empty() {
        assert!(!is_binary_content(b""));
    }

    #[test]
    fn test_is_binary_content_null_after_8kb() {
        // Null byte at position 8193 should NOT be detected (beyond 8KB check)
        let mut data = vec![b'a'; 8193];
        data.push(0);
        assert!(!is_binary_content(&data));
    }

    #[test]
    fn test_map_index_status_new() {
        assert_eq!(
            map_index_status(git2::Status::INDEX_NEW),
            Some(FileStatus::Added)
        );
    }

    #[test]
    fn test_map_index_status_modified() {
        assert_eq!(
            map_index_status(git2::Status::INDEX_MODIFIED),
            Some(FileStatus::Modified)
        );
    }

    #[test]
    fn test_map_index_status_deleted() {
        assert_eq!(
            map_index_status(git2::Status::INDEX_DELETED),
            Some(FileStatus::Deleted)
        );
    }

    #[test]
    fn test_map_index_status_renamed() {
        assert_eq!(
            map_index_status(git2::Status::INDEX_RENAMED),
            Some(FileStatus::Renamed)
        );
    }

    #[test]
    fn test_map_index_status_none() {
        assert_eq!(map_index_status(git2::Status::CURRENT), None);
    }

    #[test]
    fn test_map_workdir_status_new() {
        assert_eq!(
            map_workdir_status(git2::Status::WT_NEW),
            Some(FileStatus::Added)
        );
    }

    #[test]
    fn test_map_workdir_status_modified() {
        assert_eq!(
            map_workdir_status(git2::Status::WT_MODIFIED),
            Some(FileStatus::Modified)
        );
    }

    #[test]
    fn test_map_workdir_status_deleted() {
        assert_eq!(
            map_workdir_status(git2::Status::WT_DELETED),
            Some(FileStatus::Deleted)
        );
    }

    #[test]
    fn test_map_workdir_status_renamed() {
        assert_eq!(
            map_workdir_status(git2::Status::WT_RENAMED),
            Some(FileStatus::Renamed)
        );
    }

    #[test]
    fn test_map_workdir_status_none() {
        assert_eq!(map_workdir_status(git2::Status::CURRENT), None);
    }

    #[test]
    fn test_file_entry_display_status_workdir_preferred() {
        let entry = FileEntry {
            path: "test.rs".to_string(),
            index_status: Some(FileStatus::Added),
            workdir_status: Some(FileStatus::Modified),
        };
        assert_eq!(entry.display_status(), "M");
    }

    #[test]
    fn test_file_entry_display_status_fallback_to_index() {
        let entry = FileEntry {
            path: "test.rs".to_string(),
            index_status: Some(FileStatus::Added),
            workdir_status: None,
        };
        assert_eq!(entry.display_status(), "A");
    }

    #[test]
    fn test_file_entry_display_status_untracked() {
        let entry = FileEntry {
            path: "test.rs".to_string(),
            index_status: None,
            workdir_status: Some(FileStatus::Untracked),
        };
        assert_eq!(entry.display_status(), "?");
    }

    #[test]
    fn test_file_entry_is_staged_only_true() {
        let entry = FileEntry {
            path: "test.rs".to_string(),
            index_status: Some(FileStatus::Modified),
            workdir_status: None,
        };
        assert!(entry.is_staged_only());
    }

    #[test]
    fn test_file_entry_is_staged_only_false_with_workdir() {
        let entry = FileEntry {
            path: "test.rs".to_string(),
            index_status: Some(FileStatus::Modified),
            workdir_status: Some(FileStatus::Modified),
        };
        assert!(!entry.is_staged_only());
    }

    #[test]
    fn test_file_entry_is_staged_only_false_no_index() {
        let entry = FileEntry {
            path: "test.rs".to_string(),
            index_status: None,
            workdir_status: Some(FileStatus::Added),
        };
        assert!(!entry.is_staged_only());
    }

    #[test]
    fn test_file_status_display() {
        assert_eq!(format!("{}", FileStatus::Modified), "M");
        assert_eq!(format!("{}", FileStatus::Added), "A");
        assert_eq!(format!("{}", FileStatus::Deleted), "D");
        assert_eq!(format!("{}", FileStatus::Renamed), "R");
        assert_eq!(format!("{}", FileStatus::Untracked), "?");
    }

    #[test]
    fn test_gitrepo_open_in_git_repo() {
        // The workspace is a git repo, so this should succeed
        let repo = GitRepo::open("/workspace");
        assert!(repo.is_ok());
    }

    #[test]
    fn test_gitrepo_open_nonexistent() {
        let repo = GitRepo::open("/nonexistent/path");
        assert!(repo.is_err());
    }

    #[test]
    fn test_gitrepo_changed_files_returns_vec() {
        let repo = GitRepo::open("/workspace").unwrap();
        let files = repo.changed_files();
        assert!(files.is_ok());
        // We just verify it returns a Vec without errors
    }

    #[test]
    fn test_gitrepo_head_content_existing_file() {
        // CLAUDE.md should exist in HEAD
        let repo = GitRepo::open("/workspace").unwrap();
        let content = repo.head_content("CLAUDE.md");
        assert!(content.is_ok());
        match content.unwrap() {
            ContentResult::Text(s) => assert!(!s.is_empty()),
            other => panic!("Expected Text, got {:?}", other),
        }
    }

    #[test]
    fn test_gitrepo_head_content_nonexistent_file() {
        let repo = GitRepo::open("/workspace").unwrap();
        let content = repo.head_content("definitely_does_not_exist_xyz.txt");
        assert!(content.is_ok());
        assert_eq!(content.unwrap(), ContentResult::NotFound);
    }

    #[test]
    fn test_gitrepo_workdir_content_existing_file() {
        let repo = GitRepo::open("/workspace").unwrap();
        let content = repo.workdir_content("CLAUDE.md");
        assert!(content.is_ok());
        match content.unwrap() {
            ContentResult::Text(s) => assert!(!s.is_empty()),
            other => panic!("Expected Text, got {:?}", other),
        }
    }

    #[test]
    fn test_gitrepo_workdir_content_nonexistent_file() {
        let repo = GitRepo::open("/workspace").unwrap();
        let content = repo.workdir_content("definitely_does_not_exist_xyz.txt");
        assert!(content.is_ok());
        assert_eq!(content.unwrap(), ContentResult::NotFound);
    }
}
