pub mod types;

use std::path::Path;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use crate::diff::types::{ChangeKind, DiffHunk};
use types::{ContentResult, FileEntry, FileStatus};

#[derive(Debug, Clone)]
pub enum WorkdirSnapshot {
    Absent,
    Regular { blob: git2::Oid, executable: bool },
    Symlink { blob: git2::Oid },
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub(crate) path: String,
    pub(crate) index_blob: Option<git2::Oid>,
    pub(crate) index_mode: Option<u32>,
    pub(crate) workdir: WorkdirSnapshot,
}

#[cfg(unix)]
fn is_executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_meta: &std::fs::Metadata) -> bool {
    false
}

/// Create a symlink at `link` pointing to `target`.
#[cfg(unix)]
fn create_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}

pub struct GitRepo {
    repo: git2::Repository,
}

pub fn apply_hunk_to_content(old_content: &str, hunk: &DiffHunk, selected_lines: Option<&[usize]>) -> String {
    let old_lines: Vec<&str> = old_content.lines().collect();
    let hunk_start = (hunk.old_start as usize).saturating_sub(1);

    let old_count = hunk.lines.iter()
        .filter(|l| l.kind == ChangeKind::Delete || l.kind == ChangeKind::Equal)
        .count();

    let mut result: Vec<&str> = Vec::new();

    let before_end = hunk_start.min(old_lines.len());
    result.extend_from_slice(&old_lines[..before_end]);

    for (i, line) in hunk.lines.iter().enumerate() {
        match line.kind {
            ChangeKind::Equal => {
                if let Some(ln) = line.old_lineno {
                    let idx = (ln as usize).saturating_sub(1);
                    if idx < old_lines.len() {
                        result.push(old_lines[idx]);
                    }
                }
            }
            ChangeKind::Insert => {
                if selected_lines.is_none_or(|lines| lines.contains(&i)) {
                    result.push(line.content.trim_end_matches('\n'));
                }
            }
            ChangeKind::Delete => {
                if selected_lines.is_some_and(|lines| !lines.contains(&i)) {
                    if let Some(ln) = line.old_lineno {
                        let idx = (ln as usize).saturating_sub(1);
                        if idx < old_lines.len() {
                            result.push(old_lines[idx]);
                        }
                    }
                }
            }
        }
    }

    let after_start = (hunk_start + old_count).min(old_lines.len());
    result.extend_from_slice(&old_lines[after_start..]);

    let mut text = result.join("\n");
    if old_content.ends_with('\n') {
        text.push('\n');
    }
    text
}

pub fn reverse_apply_hunk_to_content(new_content: &str, hunk: &DiffHunk, selected_lines: Option<&[usize]>) -> String {
    let new_lines: Vec<&str> = new_content.lines().collect();
    let hunk_start = (hunk.new_start as usize).saturating_sub(1);

    let new_count = hunk.lines.iter()
        .filter(|l| l.kind == ChangeKind::Insert || l.kind == ChangeKind::Equal)
        .count();

    let mut result: Vec<&str> = Vec::new();

    let before_end = hunk_start.min(new_lines.len());
    result.extend_from_slice(&new_lines[..before_end]);

    for (i, line) in hunk.lines.iter().enumerate() {
        match line.kind {
            ChangeKind::Equal => {
                if let Some(ln) = line.new_lineno {
                    let idx = (ln as usize).saturating_sub(1);
                    if idx < new_lines.len() {
                        result.push(new_lines[idx]);
                    }
                }
            }
            ChangeKind::Delete => {
                if selected_lines.is_none_or(|lines| lines.contains(&i)) {
                    result.push(line.content.trim_end_matches('\n'));
                }
            }
            ChangeKind::Insert => {
                if selected_lines.is_some_and(|lines| !lines.contains(&i)) {
                    if let Some(ln) = line.new_lineno {
                        let idx = (ln as usize).saturating_sub(1);
                        if idx < new_lines.len() {
                            result.push(new_lines[idx]);
                        }
                    }
                }
            }
        }
    }

    let after_start = (hunk_start + new_count).min(new_lines.len());
    result.extend_from_slice(&new_lines[after_start..]);

    let mut text = result.join("\n");
    if new_content.ends_with('\n') {
        text.push('\n');
    }
    text
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
        Some(FileStatus::Untracked)
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

    pub fn workdir_path(&self) -> Option<&Path> {
        self.repo.workdir()
    }

    fn workdir_or_err(&self) -> Result<&Path, Box<dyn std::error::Error>> {
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

        // Symlinks: return the target path as text content (matching git blob semantics)
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

    pub fn stage_file(&self, path: &str) -> Result<(), git2::Error> {
        let mut index = self.repo.index()?;
        let full_path = self.repo.workdir()
            .map(|wd| wd.join(path));
        if full_path.is_some_and(|p| p.exists()) {
            index.add_path(std::path::Path::new(path))?;
        } else {
            index.remove_path(std::path::Path::new(path))?;
        }
        index.write()?;
        Ok(())
    }

    pub fn unstage_file(&self, path: &str) -> Result<(), git2::Error> {
        match self.repo.head() {
            Ok(head) => {
                let commit = head.peel_to_commit()?;
                self.repo.reset_default(Some(commit.as_object()), std::iter::once(Path::new(path)))?;
            }
            Err(_) => {
                let mut index = self.repo.index()?;
                index.remove_path(Path::new(path))?;
                index.write()?;
            }
        }
        Ok(())
    }

    pub fn stage_hunk(&self, path: &str, old_content: &str, hunk: &DiffHunk, selected_lines: Option<&[usize]>) -> Result<(), Box<dyn std::error::Error>> {
        let new_text = apply_hunk_to_content(old_content, hunk, selected_lines);

        let mut index = self.repo.index()?;
        let entry = self.index_entry_for_path(&index, path);
        index.add_frombuffer(&entry, new_text.as_bytes())?;
        index.write()?;

        Ok(())
    }

    pub fn unstage_hunk(&self, path: &str, old_index_content: &str, hunk: &DiffHunk, selected_lines: Option<&[usize]>) -> Result<(), Box<dyn std::error::Error>> {
        let new_index_content = reverse_apply_hunk_to_content(old_index_content, hunk, selected_lines);

        let mut index = self.repo.index()?;

        // If the result matches HEAD, remove from index entirely (clean unstage)
        let head = self.head_content(path)?;
        if let ContentResult::Text(ref head_text) = head {
            if *head_text == new_index_content {
                let head_ref = self.repo.head()?;
                let commit = head_ref.peel_to_commit()?;
                self.repo.reset_default(Some(commit.as_object()), std::iter::once(Path::new(path)))?;
                return Ok(());
            }
        }

        let entry = self.index_entry_for_path(&index, path);
        index.add_frombuffer(&entry, new_index_content.as_bytes())?;
        index.write()?;

        Ok(())
    }

    pub fn discard_file(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = self.workdir_or_err()?;

        let index = self.repo.index()?;
        if index.get_path(Path::new(path), 0).is_some() {
            // File is tracked in index — checkout from index to restore workdir
            let mut checkout = git2::build::CheckoutBuilder::new();
            checkout.force();
            checkout.path(path);
            self.repo.checkout_index(None, Some(&mut checkout))?;
        } else {
            // Untracked file — delete from workdir
            let full_path = workdir.join(path);
            std::fs::remove_file(&full_path)?;
        }
        Ok(())
    }

    pub fn discard_hunk(&self, path: &str, workdir_content: &str, hunk: &DiffHunk) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = self.workdir_or_err()?;

        let new_content = reverse_apply_hunk_to_content(workdir_content, hunk, None);
        let full_path = workdir.join(path);

        // Preserve symlinks: if the workdir entry is a symlink, recreate it
        // rather than replacing it with a regular file.
        // Here new_content is the symlink target string, not file content.
        if let Ok(meta) = std::fs::symlink_metadata(&full_path) {
            if meta.file_type().is_symlink() {
                std::fs::remove_file(&full_path)?;
                create_symlink(Path::new(&new_content), &full_path)?;
                return Ok(());
            }
        }

        std::fs::write(&full_path, new_content)?;
        Ok(())
    }

    pub fn snapshot_path(
        &self,
        path: &str,
    ) -> Result<Snapshot, Box<dyn std::error::Error>> {
        let index = self.repo.index()?;
        let (index_blob, index_mode) = match index.get_path(Path::new(path), 0) {
            Some(entry) => (Some(entry.id), Some(entry.mode)),
            None => (None, None),
        };
        let workdir = self.workdir_snapshot(path)?;
        Ok(Snapshot {
            path: path.to_string(),
            index_blob,
            index_mode,
            workdir,
        })
    }

    pub fn restore_snapshot(&self, snap: &Snapshot) -> Result<(), Box<dyn std::error::Error>> {
        let mut index = self.repo.index()?;

        match snap.index_blob {
            Some(oid) => {
                let blob = self.repo.find_blob(oid)?;
                let mut entry = self.index_entry_for_path(&index, &snap.path);
                if let Some(mode) = snap.index_mode {
                    entry.mode = mode;
                }
                index.add_frombuffer(&entry, blob.content())?;
            }
            None => {
                index.remove_path(Path::new(&snap.path))?;
            }
        }
        index.write()?;

        let workdir = self.workdir_or_err()?;
        let full_path = workdir.join(&snap.path);

        match &snap.workdir {
            WorkdirSnapshot::Symlink { blob } => {
                let content = self.repo.find_blob(*blob)?;
                #[cfg(unix)]
                let target = {
                    use std::ffi::OsStr;
                    std::path::PathBuf::from(OsStr::from_bytes(content.content()))
                };
                #[cfg(not(unix))]
                let target = std::path::PathBuf::from(
                    std::str::from_utf8(content.content())?,
                );
                if let Some(parent) = full_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                if std::fs::symlink_metadata(&full_path).is_ok() {
                    std::fs::remove_file(&full_path)?;
                }
                create_symlink(&target, &full_path)?;
            }
            WorkdirSnapshot::Regular { blob, executable } => {
                let content = self.repo.find_blob(*blob)?;
                if let Some(parent) = full_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                if std::fs::symlink_metadata(&full_path).is_ok() {
                    std::fs::remove_file(&full_path)?;
                }
                std::fs::write(&full_path, content.content())?;
                #[cfg(unix)]
                if *executable {
                    use std::os::unix::fs::PermissionsExt;
                    let mut perms = std::fs::metadata(&full_path)?.permissions();
                    perms.set_mode(perms.mode() | 0o111);
                    std::fs::set_permissions(&full_path, perms)?;
                }
            }
            WorkdirSnapshot::Absent => {
                if std::fs::symlink_metadata(&full_path).is_ok() {
                    std::fs::remove_file(&full_path)?;
                }
            }
        }

        Ok(())
    }

    fn workdir_snapshot(&self, path: &str) -> Result<WorkdirSnapshot, Box<dyn std::error::Error>> {
        let workdir = self.workdir_or_err()?;
        let full_path = workdir.join(path);

        match std::fs::symlink_metadata(&full_path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let target = std::fs::read_link(&full_path)?;
                #[cfg(unix)]
                let target_bytes = target.as_os_str().as_bytes();
                #[cfg(not(unix))]
                let target_bytes = target.to_str()
                    .ok_or("non-UTF-8 symlink target on non-unix")?
                    .as_bytes();
                let blob = self.repo.blob(target_bytes)?;
                Ok(WorkdirSnapshot::Symlink { blob })
            }
            Ok(meta) => {
                let executable = is_executable(&meta);
                let bytes = std::fs::read(&full_path)?;
                let blob = self.repo.blob(&bytes)?;
                Ok(WorkdirSnapshot::Regular { blob, executable })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(WorkdirSnapshot::Absent),
            Err(e) => Err(Box::new(e)),
        }
    }

    fn index_entry_for_path(&self, index: &git2::Index, path: &str) -> git2::IndexEntry {
        if let Some(existing) = index.get_path(Path::new(path), 0) {
            existing
        } else {
            // Fallback entry assumes a regular file. Symlinks (mode 0o120000) should
            // always have an existing index entry, since hunk-staging only applies to
            // files already tracked.
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
mod tests {
    use super::*;
    use crate::diff::types::DiffLine;

    // Helper: apply hunk and return lines (strips trailing newline for easy assertion)
    fn apply_hunk_lines(old_content: &str, hunk: &DiffHunk) -> Vec<String> {
        let result = apply_hunk_to_content(old_content, hunk, None);
        result.lines().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_apply_hunk_single_line_replacement() {
        // old: "a\nb\nc\n" → new: "a\nX\nc\n" (replace line 2)
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(2), new_lineno: None,    content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(2), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(3), new_lineno: Some(3), content: "c\n".into() },
            ],
            has_header: true,
        };
        assert_eq!(apply_hunk_lines("a\nb\nc\n", &hunk), vec!["a", "X", "c"]);
    }

    #[test]
    fn test_apply_hunk_delete_only() {
        // old: "a\nb\nc\n" → new: "a\nc\n" (delete line 2)
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(2), new_lineno: None,    content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(3), new_lineno: Some(2), content: "c\n".into() },
            ],
            has_header: true,
        };
        assert_eq!(apply_hunk_lines("a\nb\nc\n", &hunk), vec!["a", "c"]);
    }

    #[test]
    fn test_apply_hunk_insert_only() {
        // old: "a\nc\n" → new: "a\nb\nc\n" (insert line between a and c)
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(2), content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(2), new_lineno: Some(3), content: "c\n".into() },
            ],
            has_header: true,
        };
        assert_eq!(apply_hunk_lines("a\nc\n", &hunk), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_apply_hunk_multiple_consecutive_deletes() {
        // old: "a\nb\nc\nd\n" → new: "a\nd\n" (delete lines 2-3)
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(2), new_lineno: None,    content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(3), new_lineno: None,    content: "c\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(4), new_lineno: Some(2), content: "d\n".into() },
            ],
            has_header: true,
        };
        assert_eq!(apply_hunk_lines("a\nb\nc\nd\n", &hunk), vec!["a", "d"]);
    }

    #[test]
    fn test_apply_hunk_non_contiguous_deletes() {
        // old: "a\nb\nc\nd\n" → new: "b\nd\n" (delete lines 1 and 3)
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(2), new_lineno: Some(1), content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(3), new_lineno: None,    content: "c\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(4), new_lineno: Some(2), content: "d\n".into() },
            ],
            has_header: true,
        };
        assert_eq!(apply_hunk_lines("a\nb\nc\nd\n", &hunk), vec!["b", "d"]);
    }

    #[test]
    fn test_apply_hunk_mid_file() {
        // Hunk at lines 3-5 of a 7-line file. Lines outside hunk are untouched.
        // old: 1,2,3,4,5,6,7 → new: 1,2,X,Y,5,6,7 (replace lines 3-4 with X,Y)
        let hunk = DiffHunk {
            old_start: 3, new_start: 3,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(3), new_lineno: None,    content: "3\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(4), new_lineno: None,    content: "4\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(3), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(4), content: "Y\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(5), new_lineno: Some(5), content: "5\n".into() },
            ],
            has_header: true,
        };
        let old = "1\n2\n3\n4\n5\n6\n7\n";
        assert_eq!(apply_hunk_lines(old, &hunk), vec!["1", "2", "X", "Y", "5", "6", "7"]);
    }

    #[test]
    fn test_apply_hunk_second_hunk_ignores_new_lineno() {
        // Simulate applying the 2nd hunk of a multi-hunk diff.
        // The new_lineno values are offset by a prior hunk that deleted a line,
        // but old_start correctly locates the range in the old file.
        //
        // Scenario: hunk 1 deleted old line 2 (not applied here).
        // Hunk 2 replaces old line 7 with "G".
        // new_lineno=6 reflects the prior deletion — should not affect us.
        let hunk = DiffHunk {
            old_start: 6, new_start: 5,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(6), new_lineno: Some(5), content: "f\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(7), new_lineno: None,    content: "g\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(6), content: "G\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(8), new_lineno: Some(7), content: "h\n".into() },
            ],
            has_header: true,
        };
        let old = "a\nb\nc\nd\ne\nf\ng\nh\n";
        assert_eq!(
            apply_hunk_lines(old, &hunk),
            vec!["a", "b", "c", "d", "e", "f", "G", "h"]
        );
    }

    #[test]
    fn test_apply_hunk_preserves_trailing_newline() {
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
            ],
            has_header: true,
        };
        let result = apply_hunk_to_content("a\nb\n", &hunk, None);
        assert_eq!(result, "X\nb\n");
    }

    #[test]
    fn test_apply_hunk_no_trailing_newline_when_original_has_none() {
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
            ],
            has_header: true,
        };
        let result = apply_hunk_to_content("a\nb", &hunk, None);
        assert_eq!(result, "X\nb");
    }

    #[test]
    fn test_apply_hunk_uses_compute_hunks_output() {
        // Integration: use compute_hunks to generate the hunk, then apply it
        use crate::diff::compute_hunks;
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\nb\nC\nd\ne\n";
        let hunks = compute_hunks(old, new, 3);
        assert_eq!(hunks.len(), 1);
        let result = apply_hunk_to_content(old, &hunks[0], None);
        assert_eq!(result, new);
    }

    #[test]
    fn test_apply_second_hunk_of_two_preserves_rest() {
        // Two hunks: change at line 2 and line 15 in a 20-line file.
        // Applying only the second hunk should leave lines 1-14 and 16-20 unchanged.
        use crate::diff::compute_hunks;
        let old = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nline15\nline16\nline17\nline18\nline19\nline20\n";
        let new = "line1\nLINE2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nLINE15\nline16\nline17\nline18\nline19\nline20\n";
        let hunks = compute_hunks(old, new, 3);
        assert_eq!(hunks.len(), 2, "Expected 2 hunks, got {}", hunks.len());

        // Apply only hunk 2 (the LINE15 change)
        let result = apply_hunk_to_content(old, &hunks[1], None);
        let expected = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nLINE15\nline16\nline17\nline18\nline19\nline20\n";
        assert_eq!(result, expected, "Applying hunk 2 should only change line15, not line2");

        // Apply only hunk 1 (the LINE2 change)
        let result = apply_hunk_to_content(old, &hunks[0], None);
        let expected = "line1\nLINE2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nline15\nline16\nline17\nline18\nline19\nline20\n";
        assert_eq!(result, expected, "Applying hunk 1 should only change line2, not line15");
    }

    // ---- reverse_apply_hunk_to_content tests ----

    fn reverse_apply_hunk_lines(new_content: &str, hunk: &DiffHunk) -> Vec<String> {
        let result = reverse_apply_hunk_to_content(new_content, hunk, None);
        result.lines().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_reverse_apply_hunk_single_line_replacement() {
        // Hunk: a -> X (old has "a", new has "X"). Reversing on new should restore "a".
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(2), new_lineno: Some(2), content: "b\n".into() },
            ],
            has_header: true,
        };
        assert_eq!(reverse_apply_hunk_lines("X\nb\nc\n", &hunk), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_reverse_apply_hunk_restore_deleted_line() {
        // Hunk deleted line "b". Reversing should restore it.
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(2), new_lineno: None,    content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(3), new_lineno: Some(2), content: "c\n".into() },
            ],
            has_header: true,
        };
        assert_eq!(reverse_apply_hunk_lines("a\nc\n", &hunk), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_reverse_apply_hunk_remove_inserted_line() {
        // Hunk inserted line "b". Reversing should remove it.
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(2), content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(2), new_lineno: Some(3), content: "c\n".into() },
            ],
            has_header: true,
        };
        assert_eq!(reverse_apply_hunk_lines("a\nb\nc\n", &hunk), vec!["a", "c"]);
    }

    #[test]
    fn test_reverse_apply_hunk_mid_file() {
        // Hunk replaces lines 3-4 with X,Y in new content. Reversing should restore 3,4.
        let hunk = DiffHunk {
            old_start: 3, new_start: 3,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(3), new_lineno: None,    content: "3\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(4), new_lineno: None,    content: "4\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(3), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(4), content: "Y\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(5), new_lineno: Some(5), content: "5\n".into() },
            ],
            has_header: true,
        };
        let new = "1\n2\nX\nY\n5\n6\n7\n";
        assert_eq!(
            reverse_apply_hunk_lines(new, &hunk),
            vec!["1", "2", "3", "4", "5", "6", "7"]
        );
    }

    #[test]
    fn test_reverse_apply_second_hunk_preserves_first() {
        // Two hunks computed from old->new. Reverse-applying hunk 2 on new content
        // should undo only hunk 2, keeping hunk 1 intact.
        use crate::diff::compute_hunks;
        let old = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nline15\nline16\nline17\nline18\nline19\nline20\n";
        let new = "line1\nLINE2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nLINE15\nline16\nline17\nline18\nline19\nline20\n";
        let hunks = compute_hunks(old, new, 3);
        assert_eq!(hunks.len(), 2);

        // Reverse-apply hunk 2 on new content: should undo LINE15, keep LINE2
        let result = reverse_apply_hunk_to_content(new, &hunks[1], None);
        let expected = "line1\nLINE2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nline15\nline16\nline17\nline18\nline19\nline20\n";
        assert_eq!(result, expected, "Reversing hunk 2 should only undo line15, keeping LINE2");

        // Reverse-apply hunk 1 on new content: should undo LINE2, keep LINE15
        let result = reverse_apply_hunk_to_content(new, &hunks[0], None);
        let expected = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nLINE15\nline16\nline17\nline18\nline19\nline20\n";
        assert_eq!(result, expected, "Reversing hunk 1 should only undo LINE2, keeping LINE15");
    }

    #[test]
    fn test_reverse_apply_preserves_trailing_newline() {
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
            ],
            has_header: true,
        };
        let result = reverse_apply_hunk_to_content("X\nb\n", &hunk, None);
        assert_eq!(result, "a\nb\n");
    }

    #[test]
    fn test_reverse_apply_no_trailing_newline_when_original_has_none() {
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
            ],
            has_header: true,
        };
        let result = reverse_apply_hunk_to_content("X\nb", &hunk, None);
        assert_eq!(result, "a\nb");
    }

    // ---- line-filtered apply tests ----

    fn apply_hunk_filtered_lines(old_content: &str, hunk: &DiffHunk, selected: &[usize]) -> Vec<String> {
        let result = apply_hunk_to_content(old_content, hunk, Some(selected));
        result.lines().map(|s| s.to_string()).collect()
    }

    fn reverse_apply_hunk_filtered_lines(new_content: &str, hunk: &DiffHunk, selected: &[usize]) -> Vec<String> {
        let result = reverse_apply_hunk_to_content(new_content, hunk, Some(selected));
        result.lines().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_filtered_apply_empty_selection_returns_old() {
        // Empty selection: no changes applied, output should equal old content
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(2), new_lineno: None,    content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(2), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(3), new_lineno: Some(3), content: "c\n".into() },
            ],
            has_header: true,
        };
        assert_eq!(apply_hunk_filtered_lines("a\nb\nc\n", &hunk, &[]), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_filtered_apply_all_selected_equals_full_apply() {
        // All change lines selected: should match full apply
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(2), new_lineno: None,    content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(2), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(3), new_lineno: Some(3), content: "c\n".into() },
            ],
            has_header: true,
        };
        let filtered = apply_hunk_filtered_lines("a\nb\nc\n", &hunk, &[1, 2]);
        let full = apply_hunk_lines("a\nb\nc\n", &hunk);
        assert_eq!(filtered, full);
    }

    #[test]
    fn test_filtered_apply_select_only_delete() {
        // Select only the delete (index 1): b removed, X not inserted
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(2), new_lineno: None,    content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(2), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(3), new_lineno: Some(3), content: "c\n".into() },
            ],
            has_header: true,
        };
        assert_eq!(apply_hunk_filtered_lines("a\nb\nc\n", &hunk, &[1]), vec!["a", "c"]);
    }

    #[test]
    fn test_filtered_apply_select_only_insert() {
        // Select only the insert (index 2): X added, b kept
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(2), new_lineno: None,    content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(2), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(3), new_lineno: Some(3), content: "c\n".into() },
            ],
            has_header: true,
        };
        assert_eq!(apply_hunk_filtered_lines("a\nb\nc\n", &hunk, &[2]), vec!["a", "b", "X", "c"]);
    }

    #[test]
    fn test_filtered_apply_non_contiguous_selection() {
        // Two separate changes; select only the first delete
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(2), new_lineno: Some(1), content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(3), new_lineno: None,    content: "c\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(4), new_lineno: Some(2), content: "d\n".into() },
            ],
            has_header: true,
        };
        // Select only first delete (index 0): a removed, c kept
        assert_eq!(apply_hunk_filtered_lines("a\nb\nc\nd\n", &hunk, &[0]), vec!["b", "c", "d"]);
    }

    #[test]
    fn test_filtered_reverse_apply_empty_selection_returns_new() {
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(2), new_lineno: Some(2), content: "b\n".into() },
            ],
            has_header: true,
        };
        assert_eq!(reverse_apply_hunk_filtered_lines("X\nb\nc\n", &hunk, &[]), vec!["X", "b", "c"]);
    }

    #[test]
    fn test_filtered_reverse_apply_all_selected_equals_full_reverse() {
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(2), new_lineno: Some(2), content: "b\n".into() },
            ],
            has_header: true,
        };
        let filtered = reverse_apply_hunk_filtered_lines("X\nb\nc\n", &hunk, &[0, 1]);
        let full = reverse_apply_hunk_lines("X\nb\nc\n", &hunk);
        assert_eq!(filtered, full);
    }

    #[test]
    fn test_filtered_reverse_apply_select_only_delete_restore() {
        // Select only the delete (index 0): restore "a", keep "X"
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(2), new_lineno: Some(2), content: "b\n".into() },
            ],
            has_header: true,
        };
        assert_eq!(reverse_apply_hunk_filtered_lines("X\nb\nc\n", &hunk, &[0]), vec!["a", "X", "b", "c"]);
    }

    #[test]
    fn test_filtered_reverse_apply_select_only_insert_remove() {
        // Select only the insert (index 1): remove "X", keep "a" absent
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(2), new_lineno: Some(2), content: "b\n".into() },
            ],
            has_header: true,
        };
        assert_eq!(reverse_apply_hunk_filtered_lines("X\nb\nc\n", &hunk, &[1]), vec!["b", "c"]);
    }

    #[test]
    fn test_filtered_apply_preserves_trailing_newline() {
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
            ],
            has_header: true,
        };
        // Select both change lines: delete a, insert X
        let result = apply_hunk_to_content("a\nb\n", &hunk, Some(&[0, 1]));
        assert_eq!(result, "X\nb\n");
    }

    #[test]
    fn test_filtered_reverse_apply_preserves_trailing_newline() {
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
            ],
            has_header: true,
        };
        // Select both change lines: restore a, remove X
        let result = reverse_apply_hunk_to_content("X\nb\n", &hunk, Some(&[0, 1]));
        assert_eq!(result, "a\nb\n");
    }

    #[test]
    fn test_reverse_apply_is_inverse_of_apply() {
        // apply_hunk(old, hunk) == new, reverse_apply(new, hunk) == old
        use crate::diff::compute_hunks;
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\nb\nC\nd\ne\n";
        let hunks = compute_hunks(old, new, 3);
        assert_eq!(hunks.len(), 1);

        let applied = apply_hunk_to_content(old, &hunks[0], None);
        assert_eq!(applied, new);

        let reversed = reverse_apply_hunk_to_content(new, &hunks[0], None);
        assert_eq!(reversed, old);
    }

    #[test]
    fn test_stage_hunk_preserves_workdir() {
        // Integration test: stage one hunk and verify workdir is preserved
        use crate::diff::compute_hunks;

        let tmpdir = std::env::temp_dir().join(format!("stage_hunk_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmpdir);

        // Create a git repo
        let repo = git2::Repository::init(&tmpdir).expect("init repo");
        repo.config().unwrap().set_str("user.email", "test@test.com").unwrap();
        repo.config().unwrap().set_str("user.name", "Test").unwrap();

        let file_path = tmpdir.join("test.txt");
        let old_content = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nline15\nline16\nline17\nline18\nline19\nline20\n";
        std::fs::write(&file_path, old_content).unwrap();

        // Stage and commit
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("test.txt")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();

        // Make two changes
        let new_content = "line1\nLINE2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nLINE15\nline16\nline17\nline18\nline19\nline20\n";
        std::fs::write(&file_path, new_content).unwrap();

        // Compute hunks (index vs workdir)
        let hunks = compute_hunks(old_content, new_content, 3);
        assert_eq!(hunks.len(), 2);

        // Stage only the second hunk using GitRepo
        let git_repo = GitRepo::open(tmpdir.to_str().unwrap()).unwrap();
        git_repo.stage_hunk("test.txt", old_content, &hunks[1], None).unwrap();

        // Verify workdir is preserved (should still have both changes)
        let workdir_after = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(workdir_after, new_content, "Workdir should be preserved with all changes");

        // Verify index has only hunk 2 staged
        let index_result = git_repo.index_content("test.txt").unwrap();
        let expected_index = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nLINE15\nline16\nline17\nline18\nline19\nline20\n";
        match index_result {
            ContentResult::Text(s) => assert_eq!(s, expected_index, "Index should have only hunk 2 staged"),
            other => panic!("Expected Text, got {:?}", other),
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[test]
    fn test_unstage_hunk_preserves_workdir_and_other_hunks() {
        // Integration test: stage both hunks, then unstage one.
        // The other hunk should remain staged and workdir should be preserved.
        use crate::diff::compute_hunks;

        let tmpdir = std::env::temp_dir().join(format!("unstage_hunk_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmpdir);

        let repo = git2::Repository::init(&tmpdir).expect("init repo");
        repo.config().unwrap().set_str("user.email", "test@test.com").unwrap();
        repo.config().unwrap().set_str("user.name", "Test").unwrap();

        let file_path = tmpdir.join("test.txt");
        let head_content = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nline15\nline16\nline17\nline18\nline19\nline20\n";
        std::fs::write(&file_path, head_content).unwrap();

        // Stage and commit
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("test.txt")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();

        // Write workdir with two changes and stage both
        let workdir_content = "line1\nLINE2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nLINE15\nline16\nline17\nline18\nline19\nline20\n";
        std::fs::write(&file_path, workdir_content).unwrap();

        let git_repo = GitRepo::open(tmpdir.to_str().unwrap()).unwrap();
        git_repo.stage_file("test.txt").unwrap();

        // Index now has both LINE2 and LINE15. Compute the staged diff hunks (HEAD vs index).
        let index_content_before = match git_repo.index_content("test.txt").unwrap() {
            ContentResult::Text(s) => s,
            other => panic!("Expected Text, got {:?}", other),
        };
        let staged_hunks = compute_hunks(head_content, &index_content_before, 3);
        assert_eq!(staged_hunks.len(), 2, "Expected 2 staged hunks");

        // Unstage hunk 1 (the LINE2 change)
        git_repo.unstage_hunk("test.txt", &index_content_before, &staged_hunks[0], None).unwrap();

        // Verify: workdir should be preserved
        let workdir_after = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(workdir_after, workdir_content, "Workdir should be preserved after unstage_hunk");

        // Verify: index should have only hunk 2 (LINE15) still staged
        let index_after = match git_repo.index_content("test.txt").unwrap() {
            ContentResult::Text(s) => s,
            other => panic!("Expected Text, got {:?}", other),
        };
        let expected_index = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nLINE15\nline16\nline17\nline18\nline19\nline20\n";
        assert_eq!(index_after, expected_index, "Index should have only hunk 2 still staged after unstaging hunk 1");

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmpdir);
    }

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
            Some(FileStatus::Untracked)
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
    fn test_file_status_display() {
        assert_eq!(format!("{}", FileStatus::Modified), "M");
        assert_eq!(format!("{}", FileStatus::Added), "A");
        assert_eq!(format!("{}", FileStatus::Deleted), "D");
        assert_eq!(format!("{}", FileStatus::Renamed), "R");
        assert_eq!(format!("{}", FileStatus::Untracked), "?");
    }

    #[test]
    fn test_gitrepo_open_in_git_repo() {
        let repo = GitRepo::open(env!("CARGO_MANIFEST_DIR"));
        assert!(repo.is_ok());
    }

    #[test]
    fn test_gitrepo_open_nonexistent() {
        let repo = GitRepo::open("/nonexistent/path");
        assert!(repo.is_err());
    }

    #[test]
    fn test_gitrepo_changed_files_returns_vec() {
        let repo = GitRepo::open(env!("CARGO_MANIFEST_DIR")).unwrap();
        let files = repo.changed_files();
        assert!(files.is_ok());
    }

    #[test]
    fn test_gitrepo_head_content_existing_file() {
        let repo = GitRepo::open(env!("CARGO_MANIFEST_DIR")).unwrap();
        let content = repo.head_content("CLAUDE.md");
        assert!(content.is_ok());
        match content.unwrap() {
            ContentResult::Text(s) => assert!(!s.is_empty()),
            other => panic!("Expected Text, got {:?}", other),
        }
    }

    #[test]
    fn test_gitrepo_head_content_nonexistent_file() {
        let repo = GitRepo::open(env!("CARGO_MANIFEST_DIR")).unwrap();
        let content = repo.head_content("definitely_does_not_exist_xyz.txt");
        assert!(content.is_ok());
        assert_eq!(content.unwrap(), ContentResult::NotFound);
    }

    #[test]
    fn test_gitrepo_workdir_content_existing_file() {
        let repo = GitRepo::open(env!("CARGO_MANIFEST_DIR")).unwrap();
        let content = repo.workdir_content("CLAUDE.md");
        assert!(content.is_ok());
        match content.unwrap() {
            ContentResult::Text(s) => assert!(!s.is_empty()),
            other => panic!("Expected Text, got {:?}", other),
        }
    }

    #[test]
    fn test_gitrepo_workdir_content_nonexistent_file() {
        let repo = GitRepo::open(env!("CARGO_MANIFEST_DIR")).unwrap();
        let content = repo.workdir_content("definitely_does_not_exist_xyz.txt");
        assert!(content.is_ok());
        assert_eq!(content.unwrap(), ContentResult::NotFound);
    }

    // ---- discard tests ----

    /// Helper: create a temp repo with an initial commit containing a file.
    /// Returns (tmpdir, GitRepo, file_path).
    fn setup_discard_repo(name: &str, content: &str) -> (std::path::PathBuf, GitRepo, std::path::PathBuf) {
        let tmpdir = std::env::temp_dir().join(format!("discard_{}_test_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&tmpdir);

        let repo = git2::Repository::init(&tmpdir).expect("init repo");
        repo.config().unwrap().set_str("user.email", "test@test.com").unwrap();
        repo.config().unwrap().set_str("user.name", "Test").unwrap();

        let file_path = tmpdir.join("test.txt");
        std::fs::write(&file_path, content).unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new("test.txt")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();

        let git_repo = GitRepo::open(tmpdir.to_str().unwrap()).unwrap();
        (tmpdir, git_repo, file_path)
    }

    #[test]
    fn test_discard_file_modified() {
        let original = "line1\nline2\nline3\n";
        let (tmpdir, git_repo, file_path) = setup_discard_repo("modified", original);

        // Modify the file in workdir
        std::fs::write(&file_path, "line1\nCHANGED\nline3\n").unwrap();

        // Discard should restore to index (== HEAD since nothing staged)
        git_repo.discard_file("test.txt").unwrap();

        let after = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(after, original, "File should be restored to index content");

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[test]
    fn test_discard_file_deleted() {
        let original = "hello\nworld\n";
        let (tmpdir, git_repo, file_path) = setup_discard_repo("deleted", original);

        // Delete the file from workdir
        std::fs::remove_file(&file_path).unwrap();
        assert!(!file_path.exists());

        // Discard should recreate the file from index
        git_repo.discard_file("test.txt").unwrap();

        assert!(file_path.exists(), "File should be recreated");
        let after = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(after, original, "Restored content should match original");

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[test]
    fn test_discard_file_untracked() {
        let tmpdir = std::env::temp_dir().join(format!("discard_untracked_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmpdir);

        let repo = git2::Repository::init(&tmpdir).expect("init repo");
        repo.config().unwrap().set_str("user.email", "test@test.com").unwrap();
        repo.config().unwrap().set_str("user.name", "Test").unwrap();

        // Create initial commit with a different file so HEAD exists
        let other_path = tmpdir.join("other.txt");
        std::fs::write(&other_path, "x\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("other.txt")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();

        // Create an untracked file
        let untracked = tmpdir.join("untracked.txt");
        std::fs::write(&untracked, "new file content\n").unwrap();

        let git_repo = GitRepo::open(tmpdir.to_str().unwrap()).unwrap();
        git_repo.discard_file("untracked.txt").unwrap();

        assert!(!untracked.exists(), "Untracked file should be deleted");

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[test]
    fn test_discard_file_preserves_staged_changes() {
        let original = "line1\nline2\nline3\n";
        let (tmpdir, git_repo, file_path) = setup_discard_repo("staged_preserved", original);

        // Stage a change
        let staged_content = "line1\nSTAGED\nline3\n";
        std::fs::write(&file_path, staged_content).unwrap();
        git_repo.stage_file("test.txt").unwrap();

        // Make further workdir changes on top
        std::fs::write(&file_path, "line1\nSTAGED\nWORKDIR\n").unwrap();

        // Discard unstaged changes — should restore workdir to index (staged version)
        git_repo.discard_file("test.txt").unwrap();

        let after = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(after, staged_content, "Workdir should match index (staged), not HEAD");

        // Verify staged content is still in index
        let idx = git_repo.index_content("test.txt").unwrap();
        match idx {
            ContentResult::Text(s) => assert_eq!(s, staged_content),
            other => panic!("Expected Text, got {:?}", other),
        }

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[test]
    fn test_discard_hunk_single_hunk() {
        let original = "a\nb\nc\n";
        let (tmpdir, git_repo, file_path) = setup_discard_repo("hunk_single", original);

        let modified = "a\nX\nc\n";
        std::fs::write(&file_path, modified).unwrap();

        use crate::diff::compute_hunks;
        let hunks = compute_hunks(original, modified, 3);
        assert_eq!(hunks.len(), 1);

        git_repo.discard_hunk("test.txt", modified, &hunks[0]).unwrap();

        let after = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(after, original, "Workdir should be restored to index content");

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[test]
    fn test_discard_hunk_one_of_two() {
        let original = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nline15\nline16\nline17\nline18\nline19\nline20\n";
        let (tmpdir, git_repo, file_path) = setup_discard_repo("hunk_partial", original);

        let modified = "line1\nLINE2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nLINE15\nline16\nline17\nline18\nline19\nline20\n";
        std::fs::write(&file_path, modified).unwrap();

        use crate::diff::compute_hunks;
        let hunks = compute_hunks(original, modified, 3);
        assert_eq!(hunks.len(), 2);

        // Discard only hunk 1 (the LINE2 change) — LINE15 should remain
        git_repo.discard_hunk("test.txt", modified, &hunks[0]).unwrap();

        let after = std::fs::read_to_string(&file_path).unwrap();
        let expected = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nLINE15\nline16\nline17\nline18\nline19\nline20\n";
        assert_eq!(after, expected, "Only hunk 1 should be discarded; hunk 2 should remain");

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[test]
    fn test_discard_hunk_preserves_staged_changes() {
        let original = "line1\nline2\nline3\n";
        let (tmpdir, git_repo, file_path) = setup_discard_repo("hunk_staged", original);

        // Stage a change
        let staged = "line1\nSTAGED\nline3\n";
        std::fs::write(&file_path, staged).unwrap();
        git_repo.stage_file("test.txt").unwrap();

        // Make workdir change on top of staged
        let workdir = "line1\nSTAGED\nWORKDIR\n";
        std::fs::write(&file_path, workdir).unwrap();

        // The unstaged diff is: staged (index) vs workdir
        // Hunk changes line3 → WORKDIR
        use crate::diff::compute_hunks;
        let hunks = compute_hunks(staged, workdir, 3);
        assert_eq!(hunks.len(), 1);

        git_repo.discard_hunk("test.txt", workdir, &hunks[0]).unwrap();

        let after = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(after, staged, "Workdir should match index (staged content)");

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[cfg(unix)]
    fn make_repo_with_symlink(name: &str, target: &str) -> (std::path::PathBuf, GitRepo, std::path::PathBuf) {
        let tmpdir = std::env::temp_dir().join(format!("symlink_{}_test_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&tmpdir);

        let repo = git2::Repository::init(&tmpdir).expect("init repo");
        repo.config().unwrap().set_str("user.email", "test@test.com").unwrap();
        repo.config().unwrap().set_str("user.name", "Test").unwrap();

        let link_path = tmpdir.join("link");
        std::os::unix::fs::symlink(target, &link_path).unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new("link")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();

        let git_repo = GitRepo::open(tmpdir.to_str().unwrap()).unwrap();
        (tmpdir, git_repo, link_path)
    }

    #[cfg(unix)]
    #[test]
    fn test_workdir_content_returns_symlink_target_as_text() {
        let (tmpdir, git_repo, _link_path) = make_repo_with_symlink("content", "target.txt");

        let content = git_repo.workdir_content("link").unwrap();
        assert_eq!(content, ContentResult::Text("target.txt".to_string()));

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[cfg(unix)]
    #[test]
    fn test_snapshot_captures_symlink_target() {
        let (tmpdir, git_repo, _link_path) = make_repo_with_symlink("snap", "original_target");

        let snap = git_repo.snapshot_path("link").unwrap();
        assert!(matches!(snap.workdir, WorkdirSnapshot::Symlink { .. }));

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[cfg(unix)]
    #[test]
    fn test_restore_snapshot_roundtrips_symlink() {
        let (tmpdir, git_repo, link_path) = make_repo_with_symlink("restore", "original_target");
        let snap = git_repo.snapshot_path("link").unwrap();

        // Change the symlink target
        std::fs::remove_file(&link_path).unwrap();
        std::os::unix::fs::symlink("new_target", &link_path).unwrap();

        // Restore should bring back the original symlink
        git_repo.restore_snapshot(&snap).unwrap();
        let target = std::fs::read_link(&link_path).unwrap();
        assert_eq!(target, std::path::PathBuf::from("original_target"));
        assert!(std::fs::symlink_metadata(&link_path).unwrap().file_type().is_symlink());

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[cfg(unix)]
    #[test]
    fn test_restore_snapshot_regular_file_to_symlink() {
        let tmpdir = std::env::temp_dir().join(format!("symlink_file2link_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmpdir);

        let repo = git2::Repository::init(&tmpdir).expect("init repo");
        repo.config().unwrap().set_str("user.email", "test@test.com").unwrap();
        repo.config().unwrap().set_str("user.name", "Test").unwrap();

        // Start with a symlink, snapshot it
        let file_path = tmpdir.join("entry");
        std::os::unix::fs::symlink("link_target", &file_path).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("entry")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();

        let git_repo = GitRepo::open(tmpdir.to_str().unwrap()).unwrap();
        let snap = git_repo.snapshot_path("entry").unwrap();

        // Replace symlink with a regular file
        std::fs::remove_file(&file_path).unwrap();
        std::fs::write(&file_path, "regular content").unwrap();

        // Restore should bring back the symlink
        git_repo.restore_snapshot(&snap).unwrap();
        assert!(std::fs::symlink_metadata(&file_path).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_link(&file_path).unwrap(), std::path::PathBuf::from("link_target"));

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[cfg(unix)]
    #[test]
    fn test_restore_snapshot_symlink_to_regular_file() {
        let tmpdir = std::env::temp_dir().join(format!("symlink_link2file_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmpdir);

        let repo = git2::Repository::init(&tmpdir).expect("init repo");
        repo.config().unwrap().set_str("user.email", "test@test.com").unwrap();
        repo.config().unwrap().set_str("user.name", "Test").unwrap();

        // Start with a regular file, snapshot it
        let file_path = tmpdir.join("entry");
        std::fs::write(&file_path, "regular content").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("entry")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();

        let git_repo = GitRepo::open(tmpdir.to_str().unwrap()).unwrap();
        let snap = git_repo.snapshot_path("entry").unwrap();

        // Replace regular file with a symlink
        std::fs::remove_file(&file_path).unwrap();
        std::os::unix::fs::symlink("some_target", &file_path).unwrap();

        // Restore should bring back the regular file
        git_repo.restore_snapshot(&snap).unwrap();
        assert!(std::fs::symlink_metadata(&file_path).unwrap().file_type().is_file());
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "regular content");

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[cfg(unix)]
    #[test]
    fn test_snapshot_dangling_symlink() {
        let (tmpdir, git_repo, link_path) = make_repo_with_symlink("dangling", "nonexistent_target");

        // The symlink target doesn't exist — it's dangling
        assert!(!std::path::Path::new("nonexistent_target").exists());

        let snap = git_repo.snapshot_path("link").unwrap();
        assert!(matches!(snap.workdir, WorkdirSnapshot::Symlink { .. }));

        // Delete and restore
        std::fs::remove_file(&link_path).unwrap();
        git_repo.restore_snapshot(&snap).unwrap();

        let target = std::fs::read_link(&link_path).unwrap();
        assert_eq!(target, std::path::PathBuf::from("nonexistent_target"));
        assert!(std::fs::symlink_metadata(&link_path).unwrap().file_type().is_symlink());

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[cfg(unix)]
    #[test]
    fn test_discard_hunk_preserves_symlink() {
        let (tmpdir, git_repo, link_path) = make_repo_with_symlink("discard", "original_target");

        // Change the symlink target (this is the "workdir" change)
        std::fs::remove_file(&link_path).unwrap();
        std::os::unix::fs::symlink("modified_target", &link_path).unwrap();

        use crate::diff::compute_hunks;
        let hunks = compute_hunks("original_target", "modified_target", 3);
        assert_eq!(hunks.len(), 1);

        git_repo.discard_hunk("link", "modified_target", &hunks[0]).unwrap();

        let target = std::fs::read_link(&link_path).unwrap();
        assert_eq!(target, std::path::PathBuf::from("original_target"));
        assert!(std::fs::symlink_metadata(&link_path).unwrap().file_type().is_symlink());

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[cfg(unix)]
    #[test]
    fn test_snapshot_captures_executable_bit() {
        use std::os::unix::fs::PermissionsExt;

        let tmpdir = std::env::temp_dir().join(format!("exec_snap_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmpdir);

        {
            let repo = git2::Repository::init(&tmpdir).expect("init repo");
            repo.config().unwrap().set_str("user.email", "test@test.com").unwrap();
            repo.config().unwrap().set_str("user.name", "Test").unwrap();

            let file_path = tmpdir.join("script.sh");
            std::fs::write(&file_path, "#!/bin/sh\necho hello\n").unwrap();
            let mut perms = std::fs::metadata(&file_path).unwrap().permissions();
            perms.set_mode(0o100755);
            std::fs::set_permissions(&file_path, perms).unwrap();

            let mut index = repo.index().unwrap();
            index.add_path(Path::new("script.sh")).unwrap();
            index.write().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            let sig = repo.signature().unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();
        }

        let git_repo = GitRepo::open(tmpdir.to_str().unwrap()).unwrap();
        let file_path = tmpdir.join("script.sh");
        let snap = git_repo.snapshot_path("script.sh").unwrap();

        assert!(matches!(snap.workdir, WorkdirSnapshot::Regular { executable: true, .. }), "snapshot should capture executable bit");
        assert_eq!(snap.index_mode, Some(0o100755), "snapshot should capture index mode");

        // Overwrite with a non-executable regular file and remove from index
        std::fs::write(&file_path, "overwritten\n").unwrap();
        git_repo.unstage_file("script.sh").unwrap();

        // Restore should bring back executable bit in both index and workdir
        git_repo.restore_snapshot(&snap).unwrap();

        let meta = std::fs::metadata(&file_path).unwrap();
        assert!(meta.permissions().mode() & 0o111 != 0, "workdir file should be executable after restore");

        let index = git_repo.repo.index().unwrap();
        let entry = index.get_path(Path::new("script.sh"), 0).expect("entry should exist");
        assert_eq!(entry.mode, 0o100755, "index entry should have executable mode after restore");

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[cfg(unix)]
    #[test]
    fn test_snapshot_non_executable_stays_non_executable() {
        use std::os::unix::fs::PermissionsExt;

        let tmpdir = std::env::temp_dir().join(format!("noexec_snap_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmpdir);

        {
            let repo = git2::Repository::init(&tmpdir).expect("init repo");
            repo.config().unwrap().set_str("user.email", "test@test.com").unwrap();
            repo.config().unwrap().set_str("user.name", "Test").unwrap();

            let file_path = tmpdir.join("data.txt");
            std::fs::write(&file_path, "just data\n").unwrap();

            let mut index = repo.index().unwrap();
            index.add_path(Path::new("data.txt")).unwrap();
            index.write().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            let sig = repo.signature().unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();
        }

        let git_repo = GitRepo::open(tmpdir.to_str().unwrap()).unwrap();
        let file_path = tmpdir.join("data.txt");
        let snap = git_repo.snapshot_path("data.txt").unwrap();

        assert!(matches!(snap.workdir, WorkdirSnapshot::Regular { executable: false, .. }), "non-executable file should not be marked executable");
        assert_eq!(snap.index_mode, Some(0o100644));

        // Make executable, then restore — should go back to non-executable
        let mut perms = std::fs::metadata(&file_path).unwrap().permissions();
        perms.set_mode(0o100755);
        std::fs::set_permissions(&file_path, perms).unwrap();

        git_repo.restore_snapshot(&snap).unwrap();

        let meta = std::fs::metadata(&file_path).unwrap();
        assert_eq!(meta.permissions().mode() & 0o111, 0, "workdir file should not be executable after restore");

        let _ = std::fs::remove_dir_all(&tmpdir);
    }
}
