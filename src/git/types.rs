use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

impl FileStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            FileStatus::Modified => "M",
            FileStatus::Added => "A",
            FileStatus::Deleted => "D",
            FileStatus::Renamed => "R",
            FileStatus::Untracked => "?",
        }
    }
}

impl fmt::Display for FileStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
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
    pub fn display_status(&self) -> &str {
        self.workdir_status
            .or(self.index_status)
            .map(|s| s.as_str())
            .unwrap_or(" ")
    }
}
