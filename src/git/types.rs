use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

impl fmt::Display for FileStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileStatus::Modified => write!(f, "M"),
            FileStatus::Added => write!(f, "A"),
            FileStatus::Deleted => write!(f, "D"),
            FileStatus::Renamed => write!(f, "R"),
            FileStatus::Untracked => write!(f, "?"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContentResult {
    Text(String),
    Binary,
    NotFound,
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: String,
    pub index_status: Option<FileStatus>,
    pub workdir_status: Option<FileStatus>,
}

impl FileEntry {
    /// Returns the single-char indicator for the dominant status.
    /// Prefers workdir_status, falls back to index_status.
    pub fn display_status(&self) -> &str {
        if let Some(ref status) = self.workdir_status {
            match status {
                FileStatus::Modified => "M",
                FileStatus::Added => "A",
                FileStatus::Deleted => "D",
                FileStatus::Renamed => "R",
                FileStatus::Untracked => "?",
            }
        } else if let Some(ref status) = self.index_status {
            match status {
                FileStatus::Modified => "M",
                FileStatus::Added => "A",
                FileStatus::Deleted => "D",
                FileStatus::Renamed => "R",
                FileStatus::Untracked => "?",
            }
        } else {
            " "
        }
    }
}
