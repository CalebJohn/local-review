use super::*;

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
        header_context: None,
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
    classify_diff(&mut hunks, &old_content, &new_content, lang, "rs");
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
    classify_diff(&mut hunks, &old_content, &new_content, lang, "rs");
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
    classify_diff(&mut hunks, &old_content, &new_content, None, "");
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
    classify_diff(&mut hunks, &old_content, &new_content, lang, "rs");
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
    classify_diff(&mut hunks, &old_content, &new_content, Some(lang), "rs");
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
    classify_diff(&mut hunks, &old_content, &new_content, lang, "rs");
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
        header_context: None,
    };
    let old_content = "fn main() {\nlet x=1;\n}\n";
    let new_content = "fn main() {\n    let x = 1;\n}\n";
    let lang = language_for_extension("rs");
    classify_diff(std::slice::from_mut(&mut hunk), old_content, new_content, lang, "rs");
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
    classify_diff(&mut dc.hunks, old, new, lang, "py");
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
    classify_diff(&mut dc.hunks, old, new, lang, "py");
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
    classify_diff(&mut dc.hunks, old, new, lang, "py");
    let changed: Vec<_> = dc.hunks.iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.kind != ChangeKind::Equal)
        .collect();
    assert!(
        changed.iter().any(|l| !l.formatting_only),
        "inserting actual code should remain semantic"
    );
}

// ── quote normalization e2e tests ──────────────────────────────

#[test]
fn test_classify_diff_python_quote_change_is_formatting() {
    let old = "test = \"a\"\n";
    let new = "test = 'a'\n";
    let lang = language_for_extension("py");
    let mut dc = crate::diff::compute_diff_content("test.py", Some(old), Some(new));
    classify_diff(&mut dc.hunks, old, new, lang, "py");
    let changed: Vec<_> = dc.hunks.iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.kind != ChangeKind::Equal)
        .collect();
    assert!(!changed.is_empty(), "should have changed lines");
    assert!(
        changed.iter().all(|l| l.formatting_only),
        "Python quote style change should be formatting_only"
    );
}

#[test]
fn test_classify_diff_python_quote_and_value_change_is_semantic() {
    let old = "test = \"a\"\n";
    let new = "test = 'b'\n";
    let lang = language_for_extension("py");
    let mut dc = crate::diff::compute_diff_content("test.py", Some(old), Some(new));
    classify_diff(&mut dc.hunks, old, new, lang, "py");
    let changed: Vec<_> = dc.hunks.iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.kind != ChangeKind::Equal)
        .collect();
    assert!(
        changed.iter().any(|l| !l.formatting_only),
        "Python quote + value change should be semantic"
    );
}

#[test]
fn test_classify_diff_rust_char_vs_string_is_semantic() {
    let old = "let x = 'a';\n";
    let new = "let x = \"a\";\n";
    let lang = language_for_extension("rs");
    let mut dc = crate::diff::compute_diff_content("test.rs", Some(old), Some(new));
    classify_diff(&mut dc.hunks, old, new, lang, "rs");
    let changed: Vec<_> = dc.hunks.iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.kind != ChangeKind::Equal)
        .collect();
    assert!(
        changed.iter().any(|l| !l.formatting_only),
        "Rust char vs string should be semantic (different types)"
    );
}

#[test]
fn test_classify_diff_js_quote_change_is_formatting() {
    let old = "const x = \"hello\";\n";
    let new = "const x = 'hello';\n";
    let lang = language_for_extension("js");
    let mut dc = crate::diff::compute_diff_content("test.js", Some(old), Some(new));
    classify_diff(&mut dc.hunks, old, new, lang, "js");
    let changed: Vec<_> = dc.hunks.iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.kind != ChangeKind::Equal)
        .collect();
    assert!(!changed.is_empty(), "should have changed lines");
    assert!(
        changed.iter().all(|l| l.formatting_only),
        "JS quote style change should be formatting_only"
    );
}

#[test]
fn test_classify_diff_c_char_vs_string_is_semantic() {
    let old = "char x = 'a';\n";
    let new = "char *x = \"a\";\n";
    let lang = language_for_extension("c");
    let mut dc = crate::diff::compute_diff_content("test.c", Some(old), Some(new));
    classify_diff(&mut dc.hunks, old, new, lang, "c");
    let changed: Vec<_> = dc.hunks.iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.kind != ChangeKind::Equal)
        .collect();
    assert!(
        changed.iter().any(|l| !l.formatting_only),
        "C char vs string should be semantic (different types)"
    );
}

// ── import reorder tests ──────────────────────────────────────

#[test]
fn test_classify_diff_rust_import_reorder_is_formatting() {
    let old = "use std::fs;\nuse std::io;\nuse std::path;\n";
    let new = "use std::io;\nuse std::path;\nuse std::fs;\n";
    let lang = language_for_extension("rs");
    let mut dc = crate::diff::compute_diff_content("test.rs", Some(old), Some(new));
    classify_diff(&mut dc.hunks, old, new, lang, "rs");
    let changed: Vec<_> = dc.hunks.iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.kind != ChangeKind::Equal)
        .collect();
    assert!(!changed.is_empty(), "should have changed lines");
    assert!(
        changed.iter().all(|l| l.formatting_only),
        "Rust import reorder should be formatting_only"
    );
}

#[test]
fn test_classify_diff_python_import_reorder_is_formatting() {
    let old = "import os\nimport sys\nimport json\n";
    let new = "import json\nimport os\nimport sys\n";
    let lang = language_for_extension("py");
    let mut dc = crate::diff::compute_diff_content("test.py", Some(old), Some(new));
    classify_diff(&mut dc.hunks, old, new, lang, "py");
    let changed: Vec<_> = dc.hunks.iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.kind != ChangeKind::Equal)
        .collect();
    assert!(!changed.is_empty(), "should have changed lines");
    assert!(
        changed.iter().all(|l| l.formatting_only),
        "Python import reorder should be formatting_only"
    );
}

#[test]
fn test_classify_diff_python_from_import_reorder_is_formatting() {
    let old = "from os import path\nfrom sys import argv\n";
    let new = "from sys import argv\nfrom os import path\n";
    let lang = language_for_extension("py");
    let mut dc = crate::diff::compute_diff_content("test.py", Some(old), Some(new));
    classify_diff(&mut dc.hunks, old, new, lang, "py");
    let changed: Vec<_> = dc.hunks.iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.kind != ChangeKind::Equal)
        .collect();
    assert!(!changed.is_empty(), "should have changed lines");
    assert!(
        changed.iter().all(|l| l.formatting_only),
        "Python from-import reorder should be formatting_only"
    );
}

#[test]
fn test_classify_diff_js_import_reorder_is_formatting() {
    let old = "import foo from 'foo';\nimport bar from 'bar';\n";
    let new = "import bar from 'bar';\nimport foo from 'foo';\n";
    let lang = language_for_extension("js");
    let mut dc = crate::diff::compute_diff_content("test.js", Some(old), Some(new));
    classify_diff(&mut dc.hunks, old, new, lang, "js");
    let changed: Vec<_> = dc.hunks.iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.kind != ChangeKind::Equal)
        .collect();
    assert!(!changed.is_empty(), "should have changed lines");
    assert!(
        changed.iter().all(|l| l.formatting_only),
        "JS import reorder should be formatting_only"
    );
}

#[test]
fn test_classify_diff_ts_import_reorder_is_formatting() {
    let old = "import { Component } from 'react';\nimport { render } from 'react-dom';\n";
    let new = "import { render } from 'react-dom';\nimport { Component } from 'react';\n";
    let lang = language_for_extension("ts");
    let mut dc = crate::diff::compute_diff_content("test.ts", Some(old), Some(new));
    classify_diff(&mut dc.hunks, old, new, lang, "ts");
    let changed: Vec<_> = dc.hunks.iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.kind != ChangeKind::Equal)
        .collect();
    assert!(!changed.is_empty(), "should have changed lines");
    assert!(
        changed.iter().all(|l| l.formatting_only),
        "TS import reorder should be formatting_only"
    );
}

#[test]
fn test_classify_diff_rust_multiline_import_reorder_is_formatting() {
    let old = "use std::collections::{\n    HashMap,\n    BTreeMap,\n};\nuse std::io;\n";
    let new = "use std::io;\nuse std::collections::{\n    HashMap,\n    BTreeMap,\n};\n";
    let lang = language_for_extension("rs");
    let mut dc = crate::diff::compute_diff_content("test.rs", Some(old), Some(new));
    classify_diff(&mut dc.hunks, old, new, lang, "rs");
    let changed: Vec<_> = dc.hunks.iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.kind != ChangeKind::Equal)
        .collect();
    assert!(!changed.is_empty(), "should have changed lines");
    assert!(
        changed.iter().all(|l| l.formatting_only),
        "Rust multi-line import reorder should be formatting_only"
    );
}

#[test]
fn test_classify_diff_python_multiline_import_reorder_is_formatting() {
    let old = "from os import (\n    path,\n    getcwd,\n)\nimport sys\n";
    let new = "import sys\nfrom os import (\n    path,\n    getcwd,\n)\n";
    let lang = language_for_extension("py");
    let mut dc = crate::diff::compute_diff_content("test.py", Some(old), Some(new));
    classify_diff(&mut dc.hunks, old, new, lang, "py");
    let changed: Vec<_> = dc.hunks.iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.kind != ChangeKind::Equal)
        .collect();
    assert!(!changed.is_empty(), "should have changed lines");
    assert!(
        changed.iter().all(|l| l.formatting_only),
        "Python multi-line import reorder should be formatting_only"
    );
}

#[test]
fn test_classify_diff_import_reorder_with_blank_lines_is_formatting() {
    let old = "import os\nimport sys\n\nimport json\n";
    let new = "import json\n\nimport os\nimport sys\n";
    let lang = language_for_extension("py");
    let mut dc = crate::diff::compute_diff_content("test.py", Some(old), Some(new));
    classify_diff(&mut dc.hunks, old, new, lang, "py");
    let changed: Vec<_> = dc.hunks.iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.kind != ChangeKind::Equal)
        .collect();
    assert!(!changed.is_empty(), "should have changed lines");
    assert!(
        changed.iter().all(|l| l.formatting_only),
        "Import reorder with blank line changes should be formatting_only"
    );
}

#[test]
fn test_classify_diff_import_with_addition_is_semantic() {
    let old = "import os\nimport sys\n";
    let new = "import sys\nimport os\nimport json\n";
    let lang = language_for_extension("py");
    let mut dc = crate::diff::compute_diff_content("test.py", Some(old), Some(new));
    classify_diff(&mut dc.hunks, old, new, lang, "py");
    let changed: Vec<_> = dc.hunks.iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.kind != ChangeKind::Equal)
        .collect();
    assert!(
        changed.iter().any(|l| !l.formatting_only),
        "Import reorder with a new import added should be semantic"
    );
}

#[test]
fn test_classify_diff_import_with_removal_is_semantic() {
    let old = "import os\nimport sys\nimport json\n";
    let new = "import sys\nimport os\n";
    let lang = language_for_extension("py");
    let mut dc = crate::diff::compute_diff_content("test.py", Some(old), Some(new));
    classify_diff(&mut dc.hunks, old, new, lang, "py");
    let changed: Vec<_> = dc.hunks.iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.kind != ChangeKind::Equal)
        .collect();
    assert!(
        changed.iter().any(|l| !l.formatting_only),
        "Import reorder with a removal should be semantic"
    );
}

#[test]
fn test_classify_diff_go_import_block_reorder_is_formatting() {
    let old = "package main\n\nimport (\n\t\"fmt\"\n\t\"os\"\n)\n";
    let new = "package main\n\nimport (\n\t\"os\"\n\t\"fmt\"\n)\n";
    let lang = language_for_extension("go");
    let mut dc = crate::diff::compute_diff_content("test.go", Some(old), Some(new));
    classify_diff(&mut dc.hunks, old, new, lang, "go");
    let changed: Vec<_> = dc.hunks.iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.kind != ChangeKind::Equal)
        .collect();
    assert!(!changed.is_empty(), "should have changed lines");
    assert!(
        changed.iter().all(|l| l.formatting_only),
        "Go import block reorder should be formatting_only"
    );
}

#[test]
fn test_classify_diff_non_import_reorder_is_semantic() {
    let old = "let a = 1;\nlet b = 2;\n";
    let new = "let b = 2;\nlet a = 1;\n";
    let lang = language_for_extension("rs");
    let mut dc = crate::diff::compute_diff_content("test.rs", Some(old), Some(new));
    classify_diff(&mut dc.hunks, old, new, lang, "rs");
    let changed: Vec<_> = dc.hunks.iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.kind != ChangeKind::Equal)
        .collect();
    assert!(
        changed.iter().any(|l| !l.formatting_only),
        "Reordering non-import statements should be semantic"
    );
}

#[test]
fn test_classify_diff_c_include_reorder_is_formatting() {
    let old = "#include <stdio.h>\n#include <stdlib.h>\n";
    let new = "#include <stdlib.h>\n#include <stdio.h>\n";
    let lang = language_for_extension("c");
    let mut dc = crate::diff::compute_diff_content("test.c", Some(old), Some(new));
    classify_diff(&mut dc.hunks, old, new, lang, "c");
    let changed: Vec<_> = dc.hunks.iter()
        .flat_map(|h| h.lines.iter())
        .filter(|l| l.kind != ChangeKind::Equal)
        .collect();
    assert!(!changed.is_empty(), "should have changed lines");
    assert!(
        changed.iter().all(|l| l.formatting_only),
        "C #include reorder should be formatting_only"
    );
}