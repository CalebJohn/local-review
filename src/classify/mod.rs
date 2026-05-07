use std::collections::HashMap;

mod canonical;

use crate::diff::types::{ChangeKind, DiffHunk};
use crate::syntax::mapping::MAX_HIGHLIGHT_BYTES;

/// Classify diff lines as formatting-only or semantic.
///
/// Parses `old_content` and `new_content` with the given tree-sitter language,
/// extracts tokens per line, and compares the canonical form of each change
/// group within each hunk. Lines in groups whose canonical forms match are
/// marked `formatting_only = true`.
///
/// Mutates `hunks` in place. If the language is `None` or either content
/// exceeds `MAX_HIGHLIGHT_BYTES`, classification is skipped (all lines
/// remain as-is with `formatting_only = false`).
pub fn classify_diff(
    hunks: &mut [DiffHunk],
    old_content: &str,
    new_content: &str,
    lang: Option<tree_sitter::Language>,
) {
    let Some(lang) = lang else { return };
    if old_content.len() > MAX_HIGHLIGHT_BYTES || new_content.len() > MAX_HIGHLIGHT_BYTES {
        return;
    }

    let old_tokens = extract_tokens(old_content, &lang);
    let new_tokens = extract_tokens(new_content, &lang);
    let (Some(old_tokens), Some(new_tokens)) = (old_tokens, new_tokens) else {
        return;
    };

    for hunk in hunks.iter_mut() {
        classify_hunk(hunk, &old_tokens, &new_tokens);
    }
}

/// Identify change groups within a hunk and classify each group.
///
/// A change group is a maximal run of consecutive non-Equal lines.
/// Within a group, Delete lines belong to the old side and Insert lines
/// to the new side. If the canonical forms of both sides match, every
/// line in the group is marked `formatting_only = true`.
fn classify_hunk(
    hunk: &mut DiffHunk,
    old_tokens: &HashMap<u32, Vec<String>>,
    new_tokens: &HashMap<u32, Vec<String>>,
) {
    let lines = &mut hunk.lines;
    let mut i = 0;
    while i < lines.len() {
        if lines[i].kind == ChangeKind::Equal {
            i += 1;
            continue;
        }

        // Start of a change group: collect consecutive non-Equal lines
        let group_start = i;
        while i < lines.len() && lines[i].kind != ChangeKind::Equal {
            i += 1;
        }
        let group_end = i;

        // Separate Delete and Insert lines by their line numbers
        let mut delete_lines: Vec<u32> = Vec::new();
        let mut insert_lines: Vec<u32> = Vec::new();
        for line in &lines[group_start..group_end] {
            if line.kind == ChangeKind::Delete {
                if let Some(ln) = line.old_lineno {
                    delete_lines.push(ln);
                }
            } else if line.kind == ChangeKind::Insert {
                if let Some(ln) = line.new_lineno {
                    insert_lines.push(ln);
                }
            }
        }

        // One-sided groups (pure Insert or pure Delete): formatting-only
        // if all lines are whitespace-only (no tokens on any line).
        if delete_lines.is_empty() || insert_lines.is_empty() {
            let token_map = if delete_lines.is_empty() { new_tokens } else { old_tokens };
            let line_nums = if delete_lines.is_empty() { &insert_lines } else { &delete_lines };
            let all_whitespace = line_nums.iter().all(|ln| {
                token_map.get(ln).is_none_or(|toks| toks.is_empty())
            });
            if all_whitespace {
                for line in &mut lines[group_start..group_end] {
                    line.formatting_only = true;
                }
            }
            continue;
        }

        let old_group_tokens = canonical::collect_tokens(old_tokens, &delete_lines);
        let new_group_tokens = canonical::collect_tokens(new_tokens, &insert_lines);

        if canonical::compare_canonical(&old_group_tokens, &new_group_tokens) {
            for line in &mut lines[group_start..group_end] {
                line.formatting_only = true;
            }
        }
    }
}

/// Map a file extension to a tree-sitter Language for parsing.
/// Reuses the same extension-to-name mapping from `syntax::lang_for_extension`,
/// then resolves the name to a `tree_sitter::Language`.
pub fn language_for_extension(ext: &str) -> Option<tree_sitter::Language> {
    let lang_name = crate::syntax::lang_for_extension(ext)?;
    match lang_name {
        "rust"       => Some(tree_sitter_rust::LANGUAGE.into()),
        "typescript" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "tsx"        => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        "javascript" => Some(tree_sitter_javascript::LANGUAGE.into()),
        "python"     => Some(tree_sitter_python::LANGUAGE.into()),
        "go"         => Some(tree_sitter_go::LANGUAGE.into()),
        "c"          => Some(tree_sitter_c::LANGUAGE.into()),
        "cpp"        => Some(tree_sitter_cpp::LANGUAGE.into()),
        "json"       => Some(tree_sitter_json::LANGUAGE.into()),
        "yaml"       => Some(tree_sitter_yaml::LANGUAGE.into()),
        "toml"       => Some(tree_sitter_toml_ng::LANGUAGE.into()),
        _            => None,
    }
}

/// Extract tokens (leaf-node text) from source code, keyed by 1-based line number.
///
/// Parses the source with tree-sitter, walks the AST, and collects the text of
/// every leaf node (nodes with no children) that is not purely whitespace.
///
/// Returns `None` if the source exceeds `MAX_HIGHLIGHT_BYTES` (256KB).
pub fn extract_tokens(source: &str, lang: &tree_sitter::Language) -> Option<HashMap<u32, Vec<String>>> {
    if source.len() > MAX_HIGHLIGHT_BYTES {
        return None;
    }

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(lang).ok()?;

    let tree = parser.parse(source, None)?;
    let root = tree.root_node();

    let mut result: HashMap<u32, Vec<String>> = HashMap::new();
    walk_node(&root, source.as_bytes(), &mut result);

    Some(result)
}

fn walk_node(
    node: &tree_sitter::Node,
    source: &[u8],
    result: &mut HashMap<u32, Vec<String>>,
) {
    if node.child_count() == 0 {
        // Leaf node: extract text
        let text = node.utf8_text(source).unwrap_or("");
        if !text.trim().is_empty() {
            let line = node.start_position().row as u32 + 1; // 1-based
            result.entry(line).or_default().push(text.to_string());
        }
    } else {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk_node(&child, source, result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── language_for_extension ──────────────────────────────────────

    #[test]
    fn test_language_for_extension_rust() {
        let lang = language_for_extension("rs");
        assert!(lang.is_some(), "rs should map to a Language");
    }

    #[test]
    fn test_language_for_extension_typescript() {
        let lang = language_for_extension("ts");
        assert!(lang.is_some(), "ts should map to a Language");
    }

    #[test]
    fn test_language_for_extension_python() {
        let lang = language_for_extension("py");
        assert!(lang.is_some(), "py should map to a Language");
    }

    #[test]
    fn test_language_for_extension_go() {
        let lang = language_for_extension("go");
        assert!(lang.is_some(), "go should map to a Language");
    }

    #[test]
    fn test_language_for_extension_unknown() {
        let lang = language_for_extension("xyz");
        assert!(lang.is_none(), "unknown extension should return None");
    }

    #[test]
    fn test_language_for_extension_empty() {
        let lang = language_for_extension("");
        assert!(lang.is_none(), "empty extension should return None");
    }

    // ── extract_tokens ──────────────────────────────────────────────

    #[test]
    fn test_extract_tokens_rust_produces_tokens_by_line() {
        let src = "fn main() {\n    let x = 1;\n}\n";
        let lang = language_for_extension("rs").unwrap();
        let tokens = extract_tokens(src, &lang).expect("parsing must succeed");

        // Line 1: fn main() {
        let line1 = tokens.get(&1).expect("line 1 must have tokens");
        assert!(line1.iter().any(|t| t == "fn"), "line 1 should contain 'fn'");
        assert!(line1.iter().any(|t| t == "main"), "line 1 should contain 'main'");

        // Line 2: let x = 1;
        let line2 = tokens.get(&2).expect("line 2 must have tokens");
        assert!(line2.iter().any(|t| t == "let"), "line 2 should contain 'let'");
        assert!(line2.iter().any(|t| t == "x"), "line 2 should contain 'x'");
        assert!(line2.iter().any(|t| t == "1"), "line 2 should contain '1'");
    }

    #[test]
    fn test_extract_tokens_excludes_whitespace() {
        let src = "fn   main(  ) { }\n";
        let lang = language_for_extension("rs").unwrap();
        let tokens = extract_tokens(src, &lang).expect("parsing must succeed");

        let line1 = tokens.get(&1).expect("line 1 must have tokens");
        // No token should be purely whitespace
        for token in line1 {
            assert!(!token.trim().is_empty(), "whitespace-only token found: {:?}", token);
        }
    }

    #[test]
    fn test_extract_tokens_oversize_returns_none() {
        let big = "x".repeat(MAX_HIGHLIGHT_BYTES + 1);
        let lang = language_for_extension("rs").unwrap();
        let result = extract_tokens(&big, &lang);
        assert!(result.is_none(), "oversize source should return None");
    }

    #[test]
    fn test_extract_tokens_empty_source() {
        let lang = language_for_extension("rs").unwrap();
        let tokens = extract_tokens("", &lang).expect("empty source must succeed");
        assert!(tokens.is_empty(), "empty source should produce no tokens");
    }

    #[test]
    fn test_extract_tokens_multiline_python() {
        let src = "def hello():\n    print('world')\n";
        let lang = language_for_extension("py").unwrap();
        let tokens = extract_tokens(src, &lang).expect("parsing must succeed");

        let line1 = tokens.get(&1).expect("line 1 must have tokens");
        assert!(line1.iter().any(|t| t == "def"), "line 1 should contain 'def'");
        assert!(line1.iter().any(|t| t == "hello"), "line 1 should contain 'hello'");

        let line2 = tokens.get(&2).expect("line 2 must have tokens");
        assert!(line2.iter().any(|t| t == "print"), "line 2 should contain 'print'");
    }

    #[test]
    fn test_extract_tokens_go() {
        let src = "package main\n\nfunc main() {\n}\n";
        let lang = language_for_extension("go").unwrap();
        let tokens = extract_tokens(src, &lang).expect("parsing must succeed");

        let line1 = tokens.get(&1).expect("line 1 must have tokens");
        assert!(line1.iter().any(|t| t == "package"), "line 1 should contain 'package'");
        assert!(line1.iter().any(|t| t == "main"), "line 1 should contain 'main'");

        let line3 = tokens.get(&3).expect("line 3 must have tokens");
        assert!(line3.iter().any(|t| t == "func"), "line 3 should contain 'func'");
    }

    #[test]
    fn test_extract_tokens_typescript() {
        let src = "const x: number = 42;\n";
        let lang = language_for_extension("ts").unwrap();
        let tokens = extract_tokens(src, &lang).expect("parsing must succeed");

        let line1 = tokens.get(&1).expect("line 1 must have tokens");
        assert!(line1.iter().any(|t| t == "const"), "line 1 should contain 'const'");
        assert!(line1.iter().any(|t| t == "x"), "line 1 should contain 'x'");
        assert!(line1.iter().any(|t| t == "42"), "line 1 should contain '42'");
    }

    // ── classify_diff ───────────────────────────────────────────────

    use crate::diff::types::DiffLine;

    fn make_hunk_with_changes(
        old_lines: Vec<(u32, &str)>,
        new_lines: Vec<(u32, &str)>,
    ) -> (DiffHunk, String, String) {
        let mut lines = Vec::new();
        let mut old_content = String::new();
        let mut new_content = String::new();

        for (ln, content) in &old_lines {
            lines.push(DiffLine {
                kind: ChangeKind::Delete,
                old_lineno: Some(*ln),
                new_lineno: None,
                content: content.to_string(),
                formatting_only: false,
            });
            old_content.push_str(content);
        }
        for (ln, content) in &new_lines {
            lines.push(DiffLine {
                kind: ChangeKind::Insert,
                old_lineno: None,
                new_lineno: Some(*ln),
                content: content.to_string(),
                formatting_only: false,
            });
            new_content.push_str(content);
        }

        let hunk = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines,
            has_header: true,
        };
        (hunk, old_content, new_content)
    }

    #[test]
    fn test_classify_diff_whitespace_only_is_formatting() {
        let (hunk, old_content, new_content) = make_hunk_with_changes(
            vec![(1, "fn foo() {\n"), (2, "let x=1;\n")],
            vec![(1, "fn foo() {\n"), (2, "    let x = 1;\n")],
        );
        let lang = language_for_extension("rs");
        let mut hunks = vec![hunk];
        classify_diff(&mut hunks, &old_content, &new_content, lang);
        assert!(
            hunks[0].lines.iter().all(|l| l.kind == ChangeKind::Equal || l.formatting_only),
            "whitespace-only changes should be formatting_only"
        );
    }

    #[test]
    fn test_classify_diff_semantic_change_is_not_formatting() {
        let (hunk, old_content, new_content) = make_hunk_with_changes(
            vec![(1, "let x = 1;\n")],
            vec![(1, "let y = 1;\n")],
        );
        let lang = language_for_extension("rs");
        let mut hunks = vec![hunk];
        classify_diff(&mut hunks, &old_content, &new_content, lang);
        let changed: Vec<_> = hunks[0].lines.iter().filter(|l| l.kind != ChangeKind::Equal).collect();
        assert!(
            changed.iter().all(|l| !l.formatting_only),
            "semantic changes should NOT be formatting_only"
        );
    }

    #[test]
    fn test_classify_diff_no_language_skips_classification() {
        let (hunk, old_content, new_content) = make_hunk_with_changes(
            vec![(1, "let x = 1;\n")],
            vec![(1, "let x = 1;\n")],
        );
        let mut hunks = vec![hunk];
        classify_diff(&mut hunks, &old_content, &new_content, None);
        assert!(
            hunks[0].lines.iter().all(|l| !l.formatting_only),
            "no language should leave all lines as non-formatting"
        );
    }

    #[test]
    fn test_classify_diff_trailing_comma_is_formatting() {
        // Trailing comma at the end of a function call
        let (hunk, old_content, new_content) = make_hunk_with_changes(
            vec![(1, "foo(1, 2)\n")],
            vec![(1, "foo(1, 2,)\n")],
        );
        let lang = language_for_extension("rs");
        let mut hunks = vec![hunk];
        classify_diff(&mut hunks, &old_content, &new_content, lang);
        assert!(
            hunks[0].lines.iter().filter(|l| l.kind != ChangeKind::Equal).all(|l| l.formatting_only),
            "trailing comma addition should be formatting_only"
        );
    }

    #[test]
    fn test_classify_diff_line_split_is_formatting() {
        let (hunk, old_content, new_content) = make_hunk_with_changes(
            vec![(1, "fn foo(a: i32, b: i32) {}\n")],
            vec![(1, "fn foo(\n"), (2, "    a: i32,\n"), (3, "    b: i32\n"), (4, ") {}\n")],
        );
        let lang = language_for_extension("rs").unwrap();
        let mut hunks = vec![hunk];
        classify_diff(&mut hunks, &old_content, &new_content, Some(lang));
        assert!(
            hunks[0].lines.iter().filter(|l| l.kind != ChangeKind::Equal).all(|l| l.formatting_only),
            "line split with same tokens should be formatting_only"
        );
    }

    #[test]
    fn test_classify_diff_pure_insertion_is_semantic() {
        let (hunk, old_content, new_content) = make_hunk_with_changes(
            vec![],
            vec![(1, "fn main() {}\n")],
        );
        let lang = language_for_extension("rs");
        let mut hunks = vec![hunk];
        classify_diff(&mut hunks, &old_content, &new_content, lang);
        assert!(
            hunks[0].lines.iter().filter(|l| l.kind != ChangeKind::Equal).all(|l| !l.formatting_only),
            "pure insertion with no counterpart should be semantic"
        );
    }

    #[test]
    fn test_classify_diff_mixed_context_and_changes() {
        let mut lines = Vec::new();
        lines.push(DiffLine {
            kind: ChangeKind::Equal,
            old_lineno: Some(1),
            new_lineno: Some(1),
            content: "fn main() {\n".to_string(),
            formatting_only: false,
        });
        lines.push(DiffLine {
            kind: ChangeKind::Delete,
            old_lineno: Some(2),
            new_lineno: None,
            content: "let x=1;\n".to_string(),
            formatting_only: false,
        });
        lines.push(DiffLine {
            kind: ChangeKind::Insert,
            old_lineno: None,
            new_lineno: Some(2),
            content: "    let x = 1;\n".to_string(),
            formatting_only: false,
        });
        lines.push(DiffLine {
            kind: ChangeKind::Equal,
            old_lineno: Some(3),
            new_lineno: Some(3),
            content: "}\n".to_string(),
            formatting_only: false,
        });
        let mut hunk = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines,
            has_header: true,
        };
        let old_content = "fn main() {\nlet x=1;\n}\n";
        let new_content = "fn main() {\n    let x = 1;\n}\n";
        let lang = language_for_extension("rs");
        classify_diff(std::slice::from_mut(&mut hunk), old_content, new_content, lang);
        assert!(!hunk.lines[0].formatting_only, "Equal line should not be formatting_only");
        assert!(hunk.lines[1].formatting_only, "Delete should be formatting_only");
        assert!(hunk.lines[2].formatting_only, "Insert should be formatting_only");
        assert!(!hunk.lines[3].formatting_only, "Equal line should not be formatting_only");
    }

    #[test]
    fn test_classify_diff_blank_line_insertion_is_formatting() {
        let old = "def hello():\n    print('world')\n";
        let new = "def hello():\n    print('world')\n\n";
        let lang = language_for_extension("py");
        let mut dc = crate::diff::compute_diff_content("test.py", Some(old), Some(new));
        classify_diff(&mut dc.hunks, old, new, lang);
        let changed: Vec<_> = dc.hunks.iter()
            .flat_map(|h| h.lines.iter())
            .filter(|l| l.kind != ChangeKind::Equal)
            .collect();
        assert!(!changed.is_empty(), "should have at least one changed line");
        assert!(
            changed.iter().all(|l| l.formatting_only),
            "blank line insertion should be formatting_only"
        );
    }

    #[test]
    fn test_classify_diff_blank_line_deletion_is_formatting() {
        let old = "def hello():\n    print('world')\n\n";
        let new = "def hello():\n    print('world')\n";
        let lang = language_for_extension("py");
        let mut dc = crate::diff::compute_diff_content("test.py", Some(old), Some(new));
        classify_diff(&mut dc.hunks, old, new, lang);
        let changed: Vec<_> = dc.hunks.iter()
            .flat_map(|h| h.lines.iter())
            .filter(|l| l.kind != ChangeKind::Equal)
            .collect();
        assert!(!changed.is_empty(), "should have at least one changed line");
        assert!(
            changed.iter().all(|l| l.formatting_only),
            "blank line deletion should be formatting_only"
        );
    }

    #[test]
    fn test_classify_diff_pure_code_insertion_stays_semantic() {
        let old = "def hello():\n    pass\n";
        let new = "def hello():\n    print('world')\n    pass\n";
        let lang = language_for_extension("py");
        let mut dc = crate::diff::compute_diff_content("test.py", Some(old), Some(new));
        classify_diff(&mut dc.hunks, old, new, lang);
        let changed: Vec<_> = dc.hunks.iter()
            .flat_map(|h| h.lines.iter())
            .filter(|l| l.kind != ChangeKind::Equal)
            .collect();
        assert!(
            changed.iter().any(|l| !l.formatting_only),
            "inserting actual code should remain semantic"
        );
    }
}
