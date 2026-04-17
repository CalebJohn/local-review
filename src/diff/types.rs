#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChangeKind {
    Equal,
    Insert,
    Delete,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: ChangeKind,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone)]
pub struct DiffContent {
    pub path: String,
    pub hunks: Vec<DiffHunk>,
    pub is_binary: bool,
}
