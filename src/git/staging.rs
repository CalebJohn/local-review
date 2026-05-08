use std::path::Path;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

use crate::diff::types::DiffHunk;
use super::hunk::{apply_hunk_to_content, reverse_apply_hunk_to_content};
use super::types::ContentResult;
use super::GitRepo;

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

#[cfg(unix)]
fn create_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}

impl GitRepo {
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
            let mut checkout = git2::build::CheckoutBuilder::new();
            checkout.force();
            checkout.path(path);
            self.repo.checkout_index(None, Some(&mut checkout))?;
        } else {
            let full_path = workdir.join(path);
            std::fs::remove_file(&full_path)?;
        }
        Ok(())
    }

    pub fn discard_hunk(&self, path: &str, workdir_content: &str, hunk: &DiffHunk) -> Result<(), Box<dyn std::error::Error>> {
        let workdir = self.workdir_or_err()?;

        let new_content = reverse_apply_hunk_to_content(workdir_content, hunk, None);
        let full_path = workdir.join(path);

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
}
