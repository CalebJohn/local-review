use super::types::FileStatus;

pub fn is_binary_content(bytes: &[u8]) -> bool {
    let check_len = std::cmp::min(bytes.len(), 8192);
    bytes[..check_len].contains(&0)
}

pub fn map_index_status(status: git2::Status) -> Option<FileStatus> {
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

pub fn map_workdir_status(status: git2::Status) -> Option<FileStatus> {
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
