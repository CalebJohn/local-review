pub mod ast;
pub mod expansion;
pub mod node_types;

use crate::diff::types::{ChangeKind, DiffHunk};
use crate::syntax::lang_for_extension;
use crate::syntax::mapping::MAX_HIGHLIGHT_BYTES;
use ast::{ancestor_chain, parse_source};
use expansion::{expand_hunk, merge_overlapping};

/// Expand hunk context using AST analysis and annotate hunk headers.
///
/// For supported languages, parses the new file with tree-sitter and:
/// 1. Sets `header_context` to a breadcrumb chain of enclosing named scopes
/// 2. Expands hunk bounds when the enclosing function/block is small enough
/// 3. Merges overlapping hunks after expansion
///
/// Returns hunks unchanged when: no extension, unknown language, file too large,
/// or parse failure.
pub fn expand_hunks(
    hunks: Vec<DiffHunk>,
    old: &str,
    new: &str,
    extension: Option<&str>,
) -> Vec<DiffHunk> {
    let lang = match extension.and_then(lang_for_extension) {
        Some(l) => l,
        None => return hunks,
    };

    if new.len() > MAX_HIGHLIGHT_BYTES {
        return hunks;
    }

    let tree = match parse_source(new, lang) {
        Some(t) => t,
        None => return hunks,
    };

    let old_lines: Vec<&str> = if old.is_empty() {
        Vec::new()
    } else {
        old.split_inclusive('\n').collect()
    };
    let new_lines: Vec<&str> = if new.is_empty() {
        Vec::new()
    } else {
        new.split_inclusive('\n').collect()
    };

    let source = new.as_bytes();

    let mut result: Vec<DiffHunk> = Vec::new();

    for mut hunk in hunks {
        let changed_new_lines: Vec<u32> = hunk
            .lines
            .iter()
            .filter(|l| l.kind != ChangeKind::Equal)
            .filter_map(|l| l.new_lineno)
            .collect();

        let (min_line, max_line) = if changed_new_lines.is_empty() {
            if !hunk.lines.iter().any(|l| l.kind != ChangeKind::Equal) {
                result.push(hunk);
                continue;
            }
            // Pure-deletion hunk: use new_start as the approximate position
            (hunk.new_start, hunk.new_start)
        } else {
            (
                changed_new_lines.iter().copied().min().unwrap(),
                changed_new_lines.iter().copied().max().unwrap(),
            )
        };
        // Convert 1-based line numbers to 0-based for tree-sitter
        let line_range = (min_line.saturating_sub(1), max_line.saturating_sub(1));

        let info = ancestor_chain(&tree, source, line_range, lang);

        hunk.header_context = info.header;

        if let Some(expand_to) = info.expand_to {
            hunk = expand_hunk(&hunk, &old_lines, &new_lines, expand_to);
        }

        result.push(hunk);
    }

    merge_overlapping(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::types::{ChangeKind, DiffHunk, DiffLine};

    fn make_line(kind: ChangeKind, old: Option<u32>, new: Option<u32>, content: &str) -> DiffLine {
        DiffLine {
            kind,
            old_lineno: old,
            new_lineno: new,
            content: content.to_string(),
            formatting_only: false,
        }
    }

    #[test]
    fn test_unknown_extension_returns_unchanged() {
        let hunk = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![make_line(ChangeKind::Insert, None, Some(1), "x\n")],
            has_header: true,
            header_context: None,
        };
        let result = expand_hunks(vec![hunk.clone()], "", "x\n", Some("xyz"));
        assert_eq!(result.len(), 1);
        assert!(result[0].header_context.is_none());
    }

    #[test]
    fn test_no_extension_returns_unchanged() {
        let hunk = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![make_line(ChangeKind::Insert, None, Some(1), "x\n")],
            has_header: true,
            header_context: None,
        };
        let result = expand_hunks(vec![hunk], "", "x\n", None);
        assert_eq!(result.len(), 1);
        assert!(result[0].header_context.is_none());
    }

    #[test]
    fn test_small_function_expanded_and_header_set() {
        let old = "// top\nfn foo() {\n    let a = 1;\n    let b = 2;\n}\n// bottom\n";
        let new = "// top\nfn foo() {\n    let a = 1;\n    let b = 99;\n}\n// bottom\n";

        let hunk = DiffHunk {
            old_start: 2,
            new_start: 2,
            lines: vec![
                make_line(ChangeKind::Equal, Some(2), Some(2), "fn foo() {\n"),
                make_line(ChangeKind::Equal, Some(3), Some(3), "    let a = 1;\n"),
                make_line(ChangeKind::Delete, Some(4), None, "    let b = 2;\n"),
                make_line(ChangeKind::Insert, None, Some(4), "    let b = 99;\n"),
                make_line(ChangeKind::Equal, Some(5), Some(5), "}\n"),
            ],
            has_header: true,
            header_context: None,
        };

        let result = expand_hunks(vec![hunk], old, new, Some("rs"));

        assert_eq!(result.len(), 1);
        let h = &result[0];
        assert!(h.header_context.is_some(), "header_context should be set");
        assert!(
            h.header_context.as_ref().unwrap().contains("foo"),
            "header should contain function name"
        );
    }

    #[test]
    fn test_large_function_header_set_but_not_expanded() {
        let mut old_lines = vec!["fn big() {\n".to_string()];
        let mut new_lines = vec!["fn big() {\n".to_string()];
        for i in 0..20 {
            old_lines.push(format!("    let x{i} = {i};\n"));
            if i == 10 {
                new_lines.push(format!("    let x{i} = 999;\n"));
            } else {
                new_lines.push(format!("    let x{i} = {i};\n"));
            }
        }
        old_lines.push("}\n".to_string());
        new_lines.push("}\n".to_string());

        let old = old_lines.join("");
        let new = new_lines.join("");

        // Hunk around line 11-12 (1-based), context 3 gives lines 8-15
        let hunk = DiffHunk {
            old_start: 9,
            new_start: 9,
            lines: vec![
                make_line(ChangeKind::Equal, Some(9), Some(9), "    let x8 = 8;\n"),
                make_line(ChangeKind::Equal, Some(10), Some(10), "    let x9 = 9;\n"),
                make_line(ChangeKind::Equal, Some(11), Some(11), "    let x10 = 10;\n"),
                make_line(ChangeKind::Delete, Some(12), None, "    let x10 = 10;\n"),
                make_line(ChangeKind::Insert, None, Some(12), "    let x10 = 999;\n"),
                make_line(ChangeKind::Equal, Some(13), Some(13), "    let x12 = 12;\n"),
                make_line(ChangeKind::Equal, Some(14), Some(14), "    let x13 = 13;\n"),
            ],
            has_header: true,
            header_context: None,
        };

        let result = expand_hunks(vec![hunk.clone()], &old, &new, Some("rs"));

        assert_eq!(result.len(), 1);
        let h = &result[0];
        assert!(h.header_context.is_some());
        assert!(h.header_context.as_ref().unwrap().contains("big"));
        // Not expanded because function is >15 lines
        assert_eq!(h.old_start, hunk.old_start);
        assert_eq!(h.new_start, hunk.new_start);
    }

    #[test]
    fn test_pure_deletion_hunk_uses_new_start_for_scope() {
        let old = "// top\nfn foo() {\n    let a = 1;\n    let b = 2;\n}\n// bottom\n";
        let new = "// top\nfn foo() {\n    let a = 1;\n}\n// bottom\n";

        let hunk = DiffHunk {
            old_start: 2,
            new_start: 2,
            lines: vec![
                make_line(ChangeKind::Equal, Some(2), Some(2), "fn foo() {\n"),
                make_line(ChangeKind::Equal, Some(3), Some(3), "    let a = 1;\n"),
                make_line(ChangeKind::Delete, Some(4), None, "    let b = 2;\n"),
                make_line(ChangeKind::Equal, Some(5), Some(4), "}\n"),
            ],
            has_header: true,
            header_context: None,
        };

        let result = expand_hunks(vec![hunk], old, new, Some("rs"));

        assert_eq!(result.len(), 1);
        let h = &result[0];
        assert!(
            h.header_context.is_some(),
            "pure-deletion hunk should still get header_context via new_start"
        );
        assert!(h.header_context.as_ref().unwrap().contains("foo"));
    }

    #[test]
    fn test_parse_failure_returns_unchanged() {
        let hunk = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![make_line(ChangeKind::Insert, None, Some(1), "not valid\n")],
            has_header: true,
            header_context: None,
        };
        // JSON/YAML will parse anything, so use a language unlikely to fail.
        // Actually tree-sitter is error-recovering, so let's test the file-too-large path.
        let big = "x".repeat(MAX_HIGHLIGHT_BYTES + 1);
        let result = expand_hunks(vec![hunk], "", &big, Some("rs"));
        assert_eq!(result.len(), 1);
        assert!(result[0].header_context.is_none());
    }
}
