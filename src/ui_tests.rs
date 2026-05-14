use super::diff_lines;
use crate::app::AppMode;
use crate::diff::types::{ChangeKind, DiffContent, DiffHunk, DiffLine};
use ratatui::prelude::{Color, Modifier};

fn dl(kind: ChangeKind, old: Option<u32>, new: Option<u32>, formatting_only: bool) -> DiffLine {
    DiffLine { kind, old_lineno: old, new_lineno: new, content: "x\n".to_string(), formatting_only }
}

fn make_dc(hunks: Vec<DiffHunk>) -> DiffContent {
    DiffContent { path: "t.rs".to_string(), hunks, is_binary: false }
}

#[test]
fn test_diff_lines_formatting_only_insert_is_dimmed() {
    let dc = make_dc(vec![DiffHunk {
        old_start: 1,
        new_start: 1,
        lines: vec![
            dl(ChangeKind::Insert, None, Some(1), true),
        ],
        has_header: true,
        header_context: None,
    }]);
    let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, false, None);
    // First line is the hunk header, second is the content line
    assert_eq!(lines.len(), 2);
    let content_line = &lines[1];
    // The body span (index 2 after gutter and lineno) should have dim modifier
    let body_span = content_line.spans.iter().find(|s| s.content.contains('+'));
    assert!(body_span.is_some(), "body span with '+' prefix should exist");
    let body_span = body_span.unwrap();
    assert!(
        body_span.style.add_modifier.contains(Modifier::DIM),
        "formatting-only insert should have DIM modifier: {:?}",
        body_span.style
    );
}

#[test]
fn test_diff_lines_formatting_only_delete_is_dimmed() {
    let dc = make_dc(vec![DiffHunk {
        old_start: 1,
        new_start: 1,
        lines: vec![
            dl(ChangeKind::Delete, Some(1), None, true),
        ],
        has_header: true,
        header_context: None,
    }]);
    let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, false, None);
    assert_eq!(lines.len(), 2);
    let content_line = &lines[1];
    let body_span = content_line.spans.iter().find(|s| s.content.contains('-'));
    assert!(body_span.is_some(), "body span with '-' prefix should exist");
    let body_span = body_span.unwrap();
    assert!(
        body_span.style.add_modifier.contains(Modifier::DIM),
        "formatting-only delete should have DIM modifier: {:?}",
        body_span.style
    );
}

#[test]
fn test_diff_lines_semantic_insert_is_not_dimmed() {
    let dc = make_dc(vec![DiffHunk {
        old_start: 1,
        new_start: 1,
        lines: vec![
            dl(ChangeKind::Insert, None, Some(1), false),
        ],
        has_header: true,
        header_context: None,
    }]);
    let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, false, None);
    assert_eq!(lines.len(), 2);
    let content_line = &lines[1];
    let body_span = content_line.spans.iter().find(|s| s.content.contains('+')).unwrap();
    assert!(
        !body_span.style.add_modifier.contains(Modifier::DIM),
        "semantic insert should NOT have DIM modifier"
    );
}

#[test]
fn test_diff_lines_semantic_delete_is_not_dimmed() {
    let dc = make_dc(vec![DiffHunk {
        old_start: 1,
        new_start: 1,
        lines: vec![
            dl(ChangeKind::Delete, Some(1), None, false),
        ],
        has_header: true,
        header_context: None,
    }]);
    let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, false, None);
    assert_eq!(lines.len(), 2);
    let content_line = &lines[1];
    let body_span = content_line.spans.iter().find(|s| s.content.contains('-')).unwrap();
    assert!(
        !body_span.style.add_modifier.contains(Modifier::DIM),
        "semantic delete should NOT have DIM modifier"
    );
}

#[test]
fn test_diff_lines_equal_line_never_dimmed() {
    let dc = make_dc(vec![DiffHunk {
        old_start: 1,
        new_start: 1,
        lines: vec![
            dl(ChangeKind::Equal, Some(1), Some(1), false),
        ],
        has_header: true,
        header_context: None,
    }]);
    let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, false, None);
    assert_eq!(lines.len(), 2);
    let content_line = &lines[1];
    let body_span = content_line.spans.iter().find(|s| s.content.contains(' ')).unwrap();
    assert!(
        !body_span.style.add_modifier.contains(Modifier::DIM),
        "equal line should NOT have DIM modifier"
    );
}

#[test]
fn test_diff_lines_mixed_hunk_only_semantic_dimmed() {
    let dc = make_dc(vec![DiffHunk {
        old_start: 1,
        new_start: 1,
        lines: vec![
            dl(ChangeKind::Delete, Some(1), None, true),  // formatting
            dl(ChangeKind::Insert, None, Some(1), true),  // formatting
            dl(ChangeKind::Delete, Some(2), None, false), // semantic
            dl(ChangeKind::Insert, None, Some(2), false), // semantic
        ],
        has_header: true,
        header_context: None,
    }]);
    let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, false, None);
    assert_eq!(lines.len(), 5); // header + 4 content lines

    // Line 1 (formatting delete): dimmed
    let body_span = lines[1].spans.iter().find(|s| s.content.contains('-')).unwrap();
    assert!(body_span.style.add_modifier.contains(Modifier::DIM));

    // Line 2 (formatting insert): dimmed
    let body_span = lines[2].spans.iter().find(|s| s.content.contains('+')).unwrap();
    assert!(body_span.style.add_modifier.contains(Modifier::DIM));

    // Line 3 (semantic delete): not dimmed
    let body_span = lines[3].spans.iter().find(|s| s.content.contains('-')).unwrap();
    assert!(!body_span.style.add_modifier.contains(Modifier::DIM));

    // Line 4 (semantic insert): not dimmed
    let body_span = lines[4].spans.iter().find(|s| s.content.contains('+')).unwrap();
    assert!(!body_span.style.add_modifier.contains(Modifier::DIM));
}

// ── Semantic filter: hide pure-formatting hunks ─────────────────

#[test]
fn test_diff_lines_semantic_filter_hides_pure_formatting_hunk() {
    let dc = make_dc(vec![
        DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![
                dl(ChangeKind::Insert, None, Some(1), true),
            ],
            has_header: true,
            header_context: None,
        },
    ]);
    // With filter off: hunk renders (header + content)
    let lines_off = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, false, None);
    assert_eq!(lines_off.len(), 2);

    // With filter on: pure-formatting hunk is hidden
    let lines_on = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, true, None);
    assert_eq!(lines_on.len(), 0);
}

#[test]
fn test_diff_lines_semantic_filter_shows_mixed_hunk() {
    let dc = make_dc(vec![DiffHunk {
        old_start: 1,
        new_start: 1,
        lines: vec![
            dl(ChangeKind::Delete, Some(1), None, true),  // formatting
            dl(ChangeKind::Insert, None, Some(1), true),  // formatting
            dl(ChangeKind::Delete, Some(2), None, false), // semantic
            dl(ChangeKind::Insert, None, Some(2), false), // semantic
        ],
        has_header: true,
        header_context: None,
    }]);
    // Mixed hunk should still show with filter on
    let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, true, None);
    assert_eq!(lines.len(), 5); // header + 4 content lines
}

#[test]
fn test_diff_lines_semantic_filter_shows_semantic_hunk() {
    let dc = make_dc(vec![DiffHunk {
        old_start: 1,
        new_start: 1,
        lines: vec![
            dl(ChangeKind::Delete, Some(1), None, false),
            dl(ChangeKind::Insert, None, Some(1), false),
        ],
        has_header: true,
        header_context: None,
    }]);
    let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, true, None);
    assert_eq!(lines.len(), 3); // header + 2 content lines
}

#[test]
fn test_diff_lines_semantic_filter_mixed_hunks_hides_only_formatting() {
    let dc = make_dc(vec![
        DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![
                dl(ChangeKind::Insert, None, Some(1), true),
            ],
            has_header: true,
            header_context: None,
        },
        DiffHunk {
            old_start: 5,
            new_start: 5,
            lines: vec![
                dl(ChangeKind::Delete, Some(5), None, false),
                dl(ChangeKind::Insert, None, Some(5), false),
            ],
            has_header: true,
            header_context: None,
        },
        DiffHunk {
            old_start: 10,
            new_start: 10,
            lines: vec![
                dl(ChangeKind::Insert, None, Some(10), true),
            ],
            has_header: true,
            header_context: None,
        },
    ]);
    // With filter on: only the middle semantic hunk should render
    let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, true, None);
    assert_eq!(lines.len(), 3); // header + 2 content lines from middle hunk only
}

#[test]
fn test_diff_lines_semantic_filter_all_formatting_returns_empty() {
    let dc = make_dc(vec![
        DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![dl(ChangeKind::Insert, None, Some(1), true)],
            has_header: true,
            header_context: None,
        },
        DiffHunk {
            old_start: 5,
            new_start: 5,
            lines: vec![dl(ChangeKind::Delete, Some(5), None, true)],
            has_header: true,
            header_context: None,
        },
    ]);
    let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, true, None);
    assert!(lines.is_empty(), "all formatting hunks should produce no lines");
}

// ── Search highlighting tests ─────────────────────────────────

fn dl_with_content(kind: ChangeKind, old: Option<u32>, new: Option<u32>, content: &str) -> DiffLine {
    DiffLine { kind, old_lineno: old, new_lineno: new, content: content.to_string(), formatting_only: false }
}

#[test]
fn test_diff_lines_search_highlights_matching_text() {
    let dc = make_dc(vec![DiffHunk {
        old_start: 1,
        new_start: 1,
        lines: vec![
            dl_with_content(ChangeKind::Insert, None, Some(1), "hello world\n"),
        ],
        has_header: true,
        header_context: None,
    }]);
    let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, false, Some(("world", false)));
    let content_line = &lines[1]; // skip header
    let has_yellow = content_line.spans.iter().any(|s|
        s.content.as_ref() == "world" && s.style.bg == Some(Color::Yellow)
    );
    assert!(has_yellow, "matched text should have yellow background: {:?}", content_line.spans);
}

#[test]
fn test_diff_lines_search_case_insensitive() {
    let dc = make_dc(vec![DiffHunk {
        old_start: 1,
        new_start: 1,
        lines: vec![
            dl_with_content(ChangeKind::Equal, Some(1), Some(1), "Hello World\n"),
        ],
        has_header: true,
        header_context: None,
    }]);
    let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, false, Some(("hello", false)));
    let content_line = &lines[1];
    let has_yellow = content_line.spans.iter().any(|s|
        s.content.as_ref() == "Hello" && s.style.bg == Some(Color::Yellow)
    );
    assert!(has_yellow, "case-insensitive match should highlight: {:?}", content_line.spans);
}

#[test]
fn test_diff_lines_search_no_match_no_highlight() {
    let dc = make_dc(vec![DiffHunk {
        old_start: 1,
        new_start: 1,
        lines: vec![
            dl_with_content(ChangeKind::Insert, None, Some(1), "hello\n"),
        ],
        has_header: true,
        header_context: None,
    }]);
    let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, false, Some(("xyz", false)));
    let content_line = &lines[1];
    let has_yellow = content_line.spans.iter().any(|s| s.style.bg == Some(Color::Yellow));
    assert!(!has_yellow, "no match should produce no yellow spans");
}

#[test]
fn test_diff_lines_search_none_pattern_no_highlight() {
    let dc = make_dc(vec![DiffHunk {
        old_start: 1,
        new_start: 1,
        lines: vec![
            dl_with_content(ChangeKind::Insert, None, Some(1), "hello\n"),
        ],
        has_header: true,
        header_context: None,
    }]);
    let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, false, None);
    let content_line = &lines[1];
    let has_yellow = content_line.spans.iter().any(|s| s.style.bg == Some(Color::Yellow));
    assert!(!has_yellow, "None pattern should produce no highlights");
}

#[test]
fn test_diff_lines_search_multiple_matches_in_line() {
    let dc = make_dc(vec![DiffHunk {
        old_start: 1,
        new_start: 1,
        lines: vec![
            dl_with_content(ChangeKind::Insert, None, Some(1), "foo bar foo\n"),
        ],
        has_header: true,
        header_context: None,
    }]);
    let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, false, Some(("foo", false)));
    let content_line = &lines[1];
    let yellow_count = content_line.spans.iter()
        .filter(|s| s.content.as_ref() == "foo" && s.style.bg == Some(Color::Yellow))
        .count();
    assert_eq!(yellow_count, 2, "should highlight both occurrences");
}
