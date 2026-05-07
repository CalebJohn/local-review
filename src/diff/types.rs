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
    #[allow(dead_code)]
    pub formatting_only: bool,
}

#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub old_start: u32,
    pub new_start: u32,
    pub lines: Vec<DiffLine>,
    /// When false, the hunk is a context-only "filler" surrounding real change
    /// hunks in full-file view. The renderer omits the `@@` header for fillers
    /// and navigation/staging skip them.
    pub has_header: bool,
}

impl DiffHunk {
    /// Returns true when every non-Equal line in the hunk has `formatting_only = true`.
    /// A hunk with no changed lines (only Equal) is vacuously formatting-only.
    #[allow(dead_code)]
    pub fn is_formatting_only(&self) -> bool {
        self.lines
            .iter()
            .filter(|l| l.kind != ChangeKind::Equal)
            .all(|l| l.formatting_only)
    }

    /// Returns true when the hunk has both formatting-only and semantic changed lines.
    #[allow(dead_code)]
    pub fn is_mixed(&self) -> bool {
        let mut has_formatting = false;
        let mut has_semantic = false;
        for line in &self.lines {
            if line.kind == ChangeKind::Equal {
                continue;
            }
            if line.formatting_only {
                has_formatting = true;
            } else {
                has_semantic = true;
            }
            if has_formatting && has_semantic {
                return true;
            }
        }
        false
    }
}

#[derive(Debug, Clone)]
pub struct DiffContent {
    pub path: String,
    pub hunks: Vec<DiffHunk>,
    pub is_binary: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_line(kind: ChangeKind, formatting_only: bool) -> DiffLine {
        DiffLine {
            kind,
            old_lineno: None,
            new_lineno: None,
            content: "test".to_string(),
            formatting_only,
        }
    }

    #[test]
    fn test_is_formatting_only_all_formatting() {
        let hunk = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![
                make_line(ChangeKind::Equal, false),
                make_line(ChangeKind::Insert, true),
                make_line(ChangeKind::Delete, true),
            ],
            has_header: true,
        };
        assert!(hunk.is_formatting_only());
    }

    #[test]
    fn test_is_formatting_only_has_semantic_change() {
        let hunk = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![
                make_line(ChangeKind::Equal, false),
                make_line(ChangeKind::Insert, false),
                make_line(ChangeKind::Delete, true),
            ],
            has_header: true,
        };
        assert!(!hunk.is_formatting_only());
    }

    #[test]
    fn test_is_formatting_only_only_equal_lines() {
        let hunk = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![
                make_line(ChangeKind::Equal, false),
                make_line(ChangeKind::Equal, false),
            ],
            has_header: true,
        };
        // No changed lines, so vacuously all non-Equal lines have formatting_only=true
        assert!(hunk.is_formatting_only());
    }

    #[test]
    fn test_is_mixed_has_both_formatting_and_semantic() {
        let hunk = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![
                make_line(ChangeKind::Insert, true),
                make_line(ChangeKind::Delete, false),
            ],
            has_header: true,
        };
        assert!(hunk.is_mixed());
    }

    #[test]
    fn test_is_mixed_all_formatting() {
        let hunk = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![
                make_line(ChangeKind::Insert, true),
                make_line(ChangeKind::Delete, true),
            ],
            has_header: true,
        };
        assert!(!hunk.is_mixed());
    }

    #[test]
    fn test_is_mixed_all_semantic() {
        let hunk = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![
                make_line(ChangeKind::Insert, false),
                make_line(ChangeKind::Delete, false),
            ],
            has_header: true,
        };
        assert!(!hunk.is_mixed());
    }

    #[test]
    fn test_is_mixed_only_equal_lines() {
        let hunk = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![
                make_line(ChangeKind::Equal, false),
            ],
            has_header: true,
        };
        assert!(!hunk.is_mixed());
    }
}
