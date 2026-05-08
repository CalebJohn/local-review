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
