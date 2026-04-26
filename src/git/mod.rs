pub mod types;

use std::path::Path;
use crate::diff::types::{ChangeKind, DiffHunk};
use types::{ContentResult, FileEntry, FileStatus};

pub struct GitRepo {
    repo: git2::Repository,
}

/// Apply a single hunk to old content using the standard patch walk approach.
///
/// Walks the hunk's lines to reconstruct the output:
/// 1. Copy old lines before the hunk (using old_start)
/// 2. For each hunk line: keep Equal, add Insert, skip Delete
/// 3. Copy old lines after the hunk
///
/// This avoids using new_lineno for positioning, which breaks for non-first
/// hunks in multi-hunk diffs (new_lineno is global to the full new file).
fn apply_hunk_to_content(old_content: &str, hunk: &DiffHunk) -> String {
    let old_lines: Vec<&str> = old_content.lines().collect();
    let hunk_start = (hunk.old_start as usize).saturating_sub(1);

    // Number of old-file lines this hunk spans (Equal + Delete lines)
    let old_count = hunk.lines.iter()
        .filter(|l| l.kind == ChangeKind::Delete || l.kind == ChangeKind::Equal)
        .count();

    let mut result: Vec<&str> = Vec::new();

    // Lines before the hunk — unchanged
    let before_end = hunk_start.min(old_lines.len());
    result.extend_from_slice(&old_lines[..before_end]);

    // Walk the hunk
    for line in &hunk.lines {
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
                result.push(line.content.trim_end_matches('\n'));
            }
            ChangeKind::Delete => {} // skip
        }
    }

    // Lines after the hunk — unchanged
    let after_start = (hunk_start + old_count).min(old_lines.len());
    result.extend_from_slice(&old_lines[after_start..]);

    let mut text = result.join("\n");
    if old_content.ends_with('\n') {
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

    pub fn stage_file(&self, path: &str) -> Result<(), git2::Error> {
        let mut index = self.repo.index()?;
        index.add_path(std::path::Path::new(path))?;
        index.write()?;
        Ok(())
    }

    pub fn unstage_file(&self, path: &str) -> Result<(), git2::Error> {
        let head = self.repo.head()?;
        let commit = head.peel_to_commit()?;
        self.repo.reset_default(Some(commit.as_object()), std::iter::once(Path::new(path)))?;
        Ok(())
    }

    pub fn stage_hunk(&self, path: &str, old_content: &str, _new_content: &str, hunk: &DiffHunk) -> Result<(), Box<dyn std::error::Error>> {
        let new_text = apply_hunk_to_content(old_content, hunk);
        let workdir = self.repo.workdir().ok_or_else(|| -> Box<dyn std::error::Error> {
            "bare repository has no working directory".into()
        })?;

        let full_path = workdir.join(path);

        // Save the real working directory content before overwriting
        let original_workdir = std::fs::read(&full_path)?;

        // Write the hunk-applied content, stage it, then restore
        std::fs::write(&full_path, new_text)?;

        let mut index = self.repo.index()?;
        index.add_path(Path::new(path))?;
        index.write()?;

        // Restore the original working directory content
        std::fs::write(&full_path, original_workdir)?;

        Ok(())
    }

    pub fn unstage_hunk(&self, path: &str, old_index_content: &str, hunk: &DiffHunk) -> Result<(), Box<dyn std::error::Error>> {
        let new_text = apply_hunk_to_content(old_index_content, hunk);

        let head = self.repo.head()?;
        let commit = head.peel_to_commit()?;
        self.repo.reset_default(Some(commit.as_object()), std::iter::once(Path::new(path)))?;

        let workdir = self.repo.workdir().ok_or_else(|| -> Box<dyn std::error::Error> {
            "bare repository has no working directory".into()
        })?;
        let full_path = workdir.join(path);
        std::fs::write(&full_path, new_text)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::types::DiffLine;

    // Helper: apply hunk and return lines (strips trailing newline for easy assertion)
    fn apply_hunk_lines(old_content: &str, hunk: &DiffHunk) -> Vec<String> {
        let result = apply_hunk_to_content(old_content, hunk);
        result.lines().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_apply_hunk_single_line_replacement() {
        // old: "a\nb\nc\n" → new: "a\nX\nc\n" (replace line 2)
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { kind: ChangeKind::Delete, old_lineno: Some(2), new_lineno: None,    content: "b\n".into() },
                DiffLine { kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(2), content: "X\n".into() },
                DiffLine { kind: ChangeKind::Equal,  old_lineno: Some(3), new_lineno: Some(3), content: "c\n".into() },
            ],
        };
        assert_eq!(apply_hunk_lines("a\nb\nc\n", &hunk), vec!["a", "X", "c"]);
    }

    #[test]
    fn test_apply_hunk_delete_only() {
        // old: "a\nb\nc\n" → new: "a\nc\n" (delete line 2)
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { kind: ChangeKind::Delete, old_lineno: Some(2), new_lineno: None,    content: "b\n".into() },
                DiffLine { kind: ChangeKind::Equal,  old_lineno: Some(3), new_lineno: Some(2), content: "c\n".into() },
            ],
        };
        assert_eq!(apply_hunk_lines("a\nb\nc\n", &hunk), vec!["a", "c"]);
    }

    #[test]
    fn test_apply_hunk_insert_only() {
        // old: "a\nc\n" → new: "a\nb\nc\n" (insert line between a and c)
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(2), content: "b\n".into() },
                DiffLine { kind: ChangeKind::Equal,  old_lineno: Some(2), new_lineno: Some(3), content: "c\n".into() },
            ],
        };
        assert_eq!(apply_hunk_lines("a\nc\n", &hunk), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_apply_hunk_multiple_consecutive_deletes() {
        // old: "a\nb\nc\nd\n" → new: "a\nd\n" (delete lines 2-3)
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { kind: ChangeKind::Delete, old_lineno: Some(2), new_lineno: None,    content: "b\n".into() },
                DiffLine { kind: ChangeKind::Delete, old_lineno: Some(3), new_lineno: None,    content: "c\n".into() },
                DiffLine { kind: ChangeKind::Equal,  old_lineno: Some(4), new_lineno: Some(2), content: "d\n".into() },
            ],
        };
        assert_eq!(apply_hunk_lines("a\nb\nc\nd\n", &hunk), vec!["a", "d"]);
    }

    #[test]
    fn test_apply_hunk_non_contiguous_deletes() {
        // old: "a\nb\nc\nd\n" → new: "b\nd\n" (delete lines 1 and 3)
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { kind: ChangeKind::Equal,  old_lineno: Some(2), new_lineno: Some(1), content: "b\n".into() },
                DiffLine { kind: ChangeKind::Delete, old_lineno: Some(3), new_lineno: None,    content: "c\n".into() },
                DiffLine { kind: ChangeKind::Equal,  old_lineno: Some(4), new_lineno: Some(2), content: "d\n".into() },
            ],
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
                DiffLine { kind: ChangeKind::Delete, old_lineno: Some(3), new_lineno: None,    content: "3\n".into() },
                DiffLine { kind: ChangeKind::Delete, old_lineno: Some(4), new_lineno: None,    content: "4\n".into() },
                DiffLine { kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(3), content: "X\n".into() },
                DiffLine { kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(4), content: "Y\n".into() },
                DiffLine { kind: ChangeKind::Equal,  old_lineno: Some(5), new_lineno: Some(5), content: "5\n".into() },
            ],
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
                DiffLine { kind: ChangeKind::Equal,  old_lineno: Some(6), new_lineno: Some(5), content: "f\n".into() },
                DiffLine { kind: ChangeKind::Delete, old_lineno: Some(7), new_lineno: None,    content: "g\n".into() },
                DiffLine { kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(6), content: "G\n".into() },
                DiffLine { kind: ChangeKind::Equal,  old_lineno: Some(8), new_lineno: Some(7), content: "h\n".into() },
            ],
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
                DiffLine { kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
            ],
        };
        let result = apply_hunk_to_content("a\nb\n", &hunk);
        assert_eq!(result, "X\nb\n");
    }

    #[test]
    fn test_apply_hunk_no_trailing_newline_when_original_has_none() {
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
            ],
        };
        let result = apply_hunk_to_content("a\nb", &hunk);
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
        let result = apply_hunk_to_content(old, &hunks[0]);
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
        let result = apply_hunk_to_content(old, &hunks[1]);
        let expected = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nLINE15\nline16\nline17\nline18\nline19\nline20\n";
        assert_eq!(result, expected, "Applying hunk 2 should only change line15, not line2");

        // Apply only hunk 1 (the LINE2 change)
        let result = apply_hunk_to_content(old, &hunks[0]);
        let expected = "line1\nLINE2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nline15\nline16\nline17\nline18\nline19\nline20\n";
        assert_eq!(result, expected, "Applying hunk 1 should only change line2, not line15");
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
        git_repo.stage_hunk("test.txt", old_content, new_content, &hunks[1]).unwrap();

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
