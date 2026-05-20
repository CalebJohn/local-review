    use super::*;
    use crate::diff::types::{ChangeKind, DiffHunk, DiffLine};
    use crate::git::hunk::{apply_hunk_to_content, reverse_apply_hunk_to_content};
    use crate::git::staging::WorkdirSnapshot;
    use crate::git::types::FileStatus;

    // Helper: apply hunk and return lines (strips trailing newline for easy assertion)
    fn apply_hunk_lines(old_content: &str, hunk: &DiffHunk) -> Vec<String> {
        let result = apply_hunk_to_content(old_content, hunk, None);
        result.lines().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_apply_hunk_single_line_replacement() {
        // old: "a\nb\nc\n" → new: "a\nX\nc\n" (replace line 2)
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(2), new_lineno: None,    content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(2), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(3), new_lineno: Some(3), content: "c\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        assert_eq!(apply_hunk_lines("a\nb\nc\n", &hunk), vec!["a", "X", "c"]);
    }

    #[test]
    fn test_apply_hunk_delete_only() {
        // old: "a\nb\nc\n" → new: "a\nc\n" (delete line 2)
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(2), new_lineno: None,    content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(3), new_lineno: Some(2), content: "c\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        assert_eq!(apply_hunk_lines("a\nb\nc\n", &hunk), vec!["a", "c"]);
    }

    #[test]
    fn test_apply_hunk_insert_only() {
        // old: "a\nc\n" → new: "a\nb\nc\n" (insert line between a and c)
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(2), content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(2), new_lineno: Some(3), content: "c\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        assert_eq!(apply_hunk_lines("a\nc\n", &hunk), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_apply_hunk_multiple_consecutive_deletes() {
        // old: "a\nb\nc\nd\n" → new: "a\nd\n" (delete lines 2-3)
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(2), new_lineno: None,    content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(3), new_lineno: None,    content: "c\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(4), new_lineno: Some(2), content: "d\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        assert_eq!(apply_hunk_lines("a\nb\nc\nd\n", &hunk), vec!["a", "d"]);
    }

    #[test]
    fn test_apply_hunk_non_contiguous_deletes() {
        // old: "a\nb\nc\nd\n" → new: "b\nd\n" (delete lines 1 and 3)
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(2), new_lineno: Some(1), content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(3), new_lineno: None,    content: "c\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(4), new_lineno: Some(2), content: "d\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        assert_eq!(apply_hunk_lines("a\nb\nc\nd\n", &hunk), vec!["b", "d"]);
    }

    #[test]
    fn test_apply_hunk_mid_file() {
        // Hunk at lines 3-5 of a 7-line file. Lines outside hunk are untouched.
        // old: 1,2,3,4,5,6,7 → new: 1,2,X,Y,5,6,7 (replace lines 3-4 with X,Y)
        let hunk = DiffHunk {
            old_start: 3, new_start: 3,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(3), new_lineno: None,    content: "3\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(4), new_lineno: None,    content: "4\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(3), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(4), content: "Y\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(5), new_lineno: Some(5), content: "5\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        let old = "1\n2\n3\n4\n5\n6\n7\n";
        assert_eq!(apply_hunk_lines(old, &hunk), vec!["1", "2", "X", "Y", "5", "6", "7"]);
    }

    #[test]
    fn test_apply_hunk_second_hunk_ignores_new_lineno() {
        // Simulate applying the 2nd hunk of a multi-hunk diff.
        // The new_lineno values are offset by a prior hunk that deleted a line,
        // but old_start correctly locates the range in the old file.
        //
        // Scenario: hunk 1 deleted old line 2 (not applied here).
        // Hunk 2 replaces old line 7 with "G".
        // new_lineno=6 reflects the prior deletion — should not affect us.
        let hunk = DiffHunk {
            old_start: 6, new_start: 5,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(6), new_lineno: Some(5), content: "f\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(7), new_lineno: None,    content: "g\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(6), content: "G\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(8), new_lineno: Some(7), content: "h\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        let old = "a\nb\nc\nd\ne\nf\ng\nh\n";
        assert_eq!(
            apply_hunk_lines(old, &hunk),
            vec!["a", "b", "c", "d", "e", "f", "G", "h"]
        );
    }

    #[test]
    fn test_apply_hunk_preserves_trailing_newline() {
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        let result = apply_hunk_to_content("a\nb\n", &hunk, None);
        assert_eq!(result, "X\nb\n");
    }

    #[test]
    fn test_apply_hunk_no_trailing_newline_when_original_has_none() {
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        let result = apply_hunk_to_content("a\nb", &hunk, None);
        assert_eq!(result, "X\nb");
    }

    #[test]
    fn test_apply_hunk_uses_compute_hunks_output() {
        // Integration: use compute_hunks to generate the hunk, then apply it
        use crate::diff::compute_hunks;
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\nb\nC\nd\ne\n";
        let hunks = compute_hunks(old, new, 3);
        assert_eq!(hunks.len(), 1);
        let result = apply_hunk_to_content(old, &hunks[0], None);
        assert_eq!(result, new);
    }

    #[test]
    fn test_apply_second_hunk_of_two_preserves_rest() {
        // Two hunks: change at line 2 and line 15 in a 20-line file.
        // Applying only the second hunk should leave lines 1-14 and 16-20 unchanged.
        use crate::diff::compute_hunks;
        let old = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nline15\nline16\nline17\nline18\nline19\nline20\n";
        let new = "line1\nLINE2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nLINE15\nline16\nline17\nline18\nline19\nline20\n";
        let hunks = compute_hunks(old, new, 3);
        assert_eq!(hunks.len(), 2, "Expected 2 hunks, got {}", hunks.len());

        // Apply only hunk 2 (the LINE15 change)
        let result = apply_hunk_to_content(old, &hunks[1], None);
        let expected = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nLINE15\nline16\nline17\nline18\nline19\nline20\n";
        assert_eq!(result, expected, "Applying hunk 2 should only change line15, not line2");

        // Apply only hunk 1 (the LINE2 change)
        let result = apply_hunk_to_content(old, &hunks[0], None);
        let expected = "line1\nLINE2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nline15\nline16\nline17\nline18\nline19\nline20\n";
        assert_eq!(result, expected, "Applying hunk 1 should only change line2, not line15");
    }

    // ---- reverse_apply_hunk_to_content tests ----

    fn reverse_apply_hunk_lines(new_content: &str, hunk: &DiffHunk) -> Vec<String> {
        let result = reverse_apply_hunk_to_content(new_content, hunk, None);
        result.lines().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_reverse_apply_hunk_single_line_replacement() {
        // Hunk: a -> X (old has "a", new has "X"). Reversing on new should restore "a".
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(2), new_lineno: Some(2), content: "b\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        assert_eq!(reverse_apply_hunk_lines("X\nb\nc\n", &hunk), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_reverse_apply_hunk_restore_deleted_line() {
        // Hunk deleted line "b". Reversing should restore it.
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(2), new_lineno: None,    content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(3), new_lineno: Some(2), content: "c\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        assert_eq!(reverse_apply_hunk_lines("a\nc\n", &hunk), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_reverse_apply_hunk_remove_inserted_line() {
        // Hunk inserted line "b". Reversing should remove it.
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(2), content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(2), new_lineno: Some(3), content: "c\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        assert_eq!(reverse_apply_hunk_lines("a\nb\nc\n", &hunk), vec!["a", "c"]);
    }

    #[test]
    fn test_reverse_apply_hunk_mid_file() {
        // Hunk replaces lines 3-4 with X,Y in new content. Reversing should restore 3,4.
        let hunk = DiffHunk {
            old_start: 3, new_start: 3,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(3), new_lineno: None,    content: "3\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(4), new_lineno: None,    content: "4\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(3), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(4), content: "Y\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(5), new_lineno: Some(5), content: "5\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        let new = "1\n2\nX\nY\n5\n6\n7\n";
        assert_eq!(
            reverse_apply_hunk_lines(new, &hunk),
            vec!["1", "2", "3", "4", "5", "6", "7"]
        );
    }

    #[test]
    fn test_reverse_apply_second_hunk_preserves_first() {
        // Two hunks computed from old->new. Reverse-applying hunk 2 on new content
        // should undo only hunk 2, keeping hunk 1 intact.
        use crate::diff::compute_hunks;
        let old = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nline15\nline16\nline17\nline18\nline19\nline20\n";
        let new = "line1\nLINE2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nLINE15\nline16\nline17\nline18\nline19\nline20\n";
        let hunks = compute_hunks(old, new, 3);
        assert_eq!(hunks.len(), 2);

        // Reverse-apply hunk 2 on new content: should undo LINE15, keep LINE2
        let result = reverse_apply_hunk_to_content(new, &hunks[1], None);
        let expected = "line1\nLINE2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nline15\nline16\nline17\nline18\nline19\nline20\n";
        assert_eq!(result, expected, "Reversing hunk 2 should only undo line15, keeping LINE2");

        // Reverse-apply hunk 1 on new content: should undo LINE2, keep LINE15
        let result = reverse_apply_hunk_to_content(new, &hunks[0], None);
        let expected = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nLINE15\nline16\nline17\nline18\nline19\nline20\n";
        assert_eq!(result, expected, "Reversing hunk 1 should only undo LINE2, keeping LINE15");
    }

    #[test]
    fn test_reverse_apply_preserves_trailing_newline() {
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        let result = reverse_apply_hunk_to_content("X\nb\n", &hunk, None);
        assert_eq!(result, "a\nb\n");
    }

    #[test]
    fn test_reverse_apply_no_trailing_newline_when_original_has_none() {
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        let result = reverse_apply_hunk_to_content("X\nb", &hunk, None);
        assert_eq!(result, "a\nb");
    }

    // ---- line-filtered apply tests ----

    fn apply_hunk_filtered_lines(old_content: &str, hunk: &DiffHunk, selected: &[usize]) -> Vec<String> {
        let result = apply_hunk_to_content(old_content, hunk, Some(selected));
        result.lines().map(|s| s.to_string()).collect()
    }

    fn reverse_apply_hunk_filtered_lines(new_content: &str, hunk: &DiffHunk, selected: &[usize]) -> Vec<String> {
        let result = reverse_apply_hunk_to_content(new_content, hunk, Some(selected));
        result.lines().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_filtered_apply_empty_selection_returns_old() {
        // Empty selection: no changes applied, output should equal old content
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(2), new_lineno: None,    content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(2), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(3), new_lineno: Some(3), content: "c\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        assert_eq!(apply_hunk_filtered_lines("a\nb\nc\n", &hunk, &[]), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_filtered_apply_all_selected_equals_full_apply() {
        // All change lines selected: should match full apply
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(2), new_lineno: None,    content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(2), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(3), new_lineno: Some(3), content: "c\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        let filtered = apply_hunk_filtered_lines("a\nb\nc\n", &hunk, &[1, 2]);
        let full = apply_hunk_lines("a\nb\nc\n", &hunk);
        assert_eq!(filtered, full);
    }

    #[test]
    fn test_filtered_apply_select_only_delete() {
        // Select only the delete (index 1): b removed, X not inserted
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(2), new_lineno: None,    content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(2), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(3), new_lineno: Some(3), content: "c\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        assert_eq!(apply_hunk_filtered_lines("a\nb\nc\n", &hunk, &[1]), vec!["a", "c"]);
    }

    #[test]
    fn test_filtered_apply_select_only_insert() {
        // Select only the insert (index 2): X added, b kept
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(1), new_lineno: Some(1), content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(2), new_lineno: None,    content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(2), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(3), new_lineno: Some(3), content: "c\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        assert_eq!(apply_hunk_filtered_lines("a\nb\nc\n", &hunk, &[2]), vec!["a", "b", "X", "c"]);
    }

    #[test]
    fn test_filtered_apply_non_contiguous_selection() {
        // Two separate changes; select only the first delete
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(2), new_lineno: Some(1), content: "b\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(3), new_lineno: None,    content: "c\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(4), new_lineno: Some(2), content: "d\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        // Select only first delete (index 0): a removed, c kept
        assert_eq!(apply_hunk_filtered_lines("a\nb\nc\nd\n", &hunk, &[0]), vec!["b", "c", "d"]);
    }

    #[test]
    fn test_filtered_reverse_apply_empty_selection_returns_new() {
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(2), new_lineno: Some(2), content: "b\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        assert_eq!(reverse_apply_hunk_filtered_lines("X\nb\nc\n", &hunk, &[]), vec!["X", "b", "c"]);
    }

    #[test]
    fn test_filtered_reverse_apply_all_selected_equals_full_reverse() {
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(2), new_lineno: Some(2), content: "b\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        let filtered = reverse_apply_hunk_filtered_lines("X\nb\nc\n", &hunk, &[0, 1]);
        let full = reverse_apply_hunk_lines("X\nb\nc\n", &hunk);
        assert_eq!(filtered, full);
    }

    #[test]
    fn test_filtered_reverse_apply_select_only_delete_restore() {
        // Select only the delete (index 0): restore "a", keep "X"
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(2), new_lineno: Some(2), content: "b\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        assert_eq!(reverse_apply_hunk_filtered_lines("X\nb\nc\n", &hunk, &[0]), vec!["a", "X", "b", "c"]);
    }

    #[test]
    fn test_filtered_reverse_apply_select_only_insert_remove() {
        // Select only the insert (index 1): remove "X", keep "a" absent
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Equal,  old_lineno: Some(2), new_lineno: Some(2), content: "b\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        assert_eq!(reverse_apply_hunk_filtered_lines("X\nb\nc\n", &hunk, &[1]), vec!["b", "c"]);
    }

    #[test]
    fn test_filtered_apply_preserves_trailing_newline() {
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        // Select both change lines: delete a, insert X
        let result = apply_hunk_to_content("a\nb\n", &hunk, Some(&[0, 1]));
        assert_eq!(result, "X\nb\n");
    }

    #[test]
    fn test_filtered_reverse_apply_preserves_trailing_newline() {
        let hunk = DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                DiffLine { formatting_only: false, kind: ChangeKind::Delete, old_lineno: Some(1), new_lineno: None,    content: "a\n".into() },
                DiffLine { formatting_only: false, kind: ChangeKind::Insert, old_lineno: None,    new_lineno: Some(1), content: "X\n".into() },
            ],
            has_header: true,
            header_context: None,
        };
        // Select both change lines: restore a, remove X
        let result = reverse_apply_hunk_to_content("X\nb\n", &hunk, Some(&[0, 1]));
        assert_eq!(result, "a\nb\n");
    }

    #[test]
    fn test_reverse_apply_is_inverse_of_apply() {
        // apply_hunk(old, hunk) == new, reverse_apply(new, hunk) == old
        use crate::diff::compute_hunks;
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\nb\nC\nd\ne\n";
        let hunks = compute_hunks(old, new, 3);
        assert_eq!(hunks.len(), 1);

        let applied = apply_hunk_to_content(old, &hunks[0], None);
        assert_eq!(applied, new);

        let reversed = reverse_apply_hunk_to_content(new, &hunks[0], None);
        assert_eq!(reversed, old);
    }

    #[test]
    fn test_stage_hunk_preserves_workdir() {
        // Integration test: stage one hunk and verify workdir is preserved
        use crate::diff::compute_hunks;

        let tmpdir = std::env::temp_dir().join(format!("stage_hunk_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmpdir);

        // Create a git repo
        let repo = git2::Repository::init(&tmpdir).expect("init repo");
        repo.config().unwrap().set_str("user.email", "test@test.com").unwrap();
        repo.config().unwrap().set_str("user.name", "Test").unwrap();

        let file_path = tmpdir.join("test.txt");
        let old_content = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nline15\nline16\nline17\nline18\nline19\nline20\n";
        std::fs::write(&file_path, old_content).unwrap();

        // Stage and commit
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("test.txt")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();

        // Make two changes
        let new_content = "line1\nLINE2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nLINE15\nline16\nline17\nline18\nline19\nline20\n";
        std::fs::write(&file_path, new_content).unwrap();

        // Compute hunks (index vs workdir)
        let hunks = compute_hunks(old_content, new_content, 3);
        assert_eq!(hunks.len(), 2);

        // Stage only the second hunk using GitRepo
        let git_repo = GitRepo::open(tmpdir.to_str().unwrap()).unwrap();
        git_repo.stage_hunk("test.txt", old_content, &hunks[1], None).unwrap();

        // Verify workdir is preserved (should still have both changes)
        let workdir_after = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(workdir_after, new_content, "Workdir should be preserved with all changes");

        // Verify index has only hunk 2 staged
        let index_result = git_repo.index_content("test.txt").unwrap();
        let expected_index = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nLINE15\nline16\nline17\nline18\nline19\nline20\n";
        match index_result {
            ContentResult::Text(s) => assert_eq!(s, expected_index, "Index should have only hunk 2 staged"),
            other => panic!("Expected Text, got {:?}", other),
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[test]
    fn test_unstage_hunk_preserves_workdir_and_other_hunks() {
        // Integration test: stage both hunks, then unstage one.
        // The other hunk should remain staged and workdir should be preserved.
        use crate::diff::compute_hunks;

        let tmpdir = std::env::temp_dir().join(format!("unstage_hunk_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmpdir);

        let repo = git2::Repository::init(&tmpdir).expect("init repo");
        repo.config().unwrap().set_str("user.email", "test@test.com").unwrap();
        repo.config().unwrap().set_str("user.name", "Test").unwrap();

        let file_path = tmpdir.join("test.txt");
        let head_content = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nline15\nline16\nline17\nline18\nline19\nline20\n";
        std::fs::write(&file_path, head_content).unwrap();

        // Stage and commit
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("test.txt")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();

        // Write workdir with two changes and stage both
        let workdir_content = "line1\nLINE2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nLINE15\nline16\nline17\nline18\nline19\nline20\n";
        std::fs::write(&file_path, workdir_content).unwrap();

        let git_repo = GitRepo::open(tmpdir.to_str().unwrap()).unwrap();
        git_repo.stage_file("test.txt").unwrap();

        // Index now has both LINE2 and LINE15. Compute the staged diff hunks (HEAD vs index).
        let index_content_before = match git_repo.index_content("test.txt").unwrap() {
            ContentResult::Text(s) => s,
            other => panic!("Expected Text, got {:?}", other),
        };
        let staged_hunks = compute_hunks(head_content, &index_content_before, 3);
        assert_eq!(staged_hunks.len(), 2, "Expected 2 staged hunks");

        // Unstage hunk 1 (the LINE2 change)
        git_repo.unstage_hunk("test.txt", &index_content_before, &staged_hunks[0], None).unwrap();

        // Verify: workdir should be preserved
        let workdir_after = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(workdir_after, workdir_content, "Workdir should be preserved after unstage_hunk");

        // Verify: index should have only hunk 2 (LINE15) still staged
        let index_after = match git_repo.index_content("test.txt").unwrap() {
            ContentResult::Text(s) => s,
            other => panic!("Expected Text, got {:?}", other),
        };
        let expected_index = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nLINE15\nline16\nline17\nline18\nline19\nline20\n";
        assert_eq!(index_after, expected_index, "Index should have only hunk 2 still staged after unstaging hunk 1");

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[test]
    fn test_is_binary_content_with_null_byte() {
        assert!(is_binary_content(b"hello\x00world"));
    }

    #[test]
    fn test_is_binary_content_without_null_byte() {
        assert!(!is_binary_content(b"hello world"));
    }

    #[test]
    fn test_is_binary_content_empty() {
        assert!(!is_binary_content(b""));
    }

    #[test]
    fn test_is_binary_content_null_after_8kb() {
        // Null byte at position 8193 should NOT be detected (beyond 8KB check)
        let mut data = vec![b'a'; 8193];
        data.push(0);
        assert!(!is_binary_content(&data));
    }

    #[test]
    fn test_file_entry_display_status_workdir_preferred() {
        let entry = FileEntry {
            path: "test.rs".to_string(),
            index_status: Some(FileStatus::Added),
            workdir_status: Some(FileStatus::Modified),
        };
        assert_eq!(entry.display_status(), "M");
    }

    #[test]
    fn test_file_entry_display_status_fallback_to_index() {
        let entry = FileEntry {
            path: "test.rs".to_string(),
            index_status: Some(FileStatus::Added),
            workdir_status: None,
        };
        assert_eq!(entry.display_status(), "A");
    }

    #[test]
    fn test_file_entry_display_status_untracked() {
        let entry = FileEntry {
            path: "test.rs".to_string(),
            index_status: None,
            workdir_status: Some(FileStatus::Untracked),
        };
        assert_eq!(entry.display_status(), "?");
    }

    #[test]
    fn test_gitrepo_open_in_git_repo() {
        let repo = GitRepo::open(env!("CARGO_MANIFEST_DIR"));
        assert!(repo.is_ok());
    }

    #[test]
    fn test_gitrepo_open_nonexistent() {
        let repo = GitRepo::open("/nonexistent/path");
        assert!(repo.is_err());
    }

    #[test]
    fn test_gitrepo_changed_files_returns_vec() {
        let repo = GitRepo::open(env!("CARGO_MANIFEST_DIR")).unwrap();
        let files = repo.changed_files();
        assert!(files.is_ok());
    }

    #[test]
    fn test_gitrepo_head_content_existing_file() {
        let repo = GitRepo::open(env!("CARGO_MANIFEST_DIR")).unwrap();
        let content = repo.head_content("CLAUDE.md");
        assert!(content.is_ok());
        match content.unwrap() {
            ContentResult::Text(s) => assert!(!s.is_empty()),
            other => panic!("Expected Text, got {:?}", other),
        }
    }

    #[test]
    fn test_gitrepo_head_content_nonexistent_file() {
        let repo = GitRepo::open(env!("CARGO_MANIFEST_DIR")).unwrap();
        let content = repo.head_content("definitely_does_not_exist_xyz.txt");
        assert!(content.is_ok());
        assert_eq!(content.unwrap(), ContentResult::NotFound);
    }

    #[test]
    fn test_gitrepo_workdir_content_existing_file() {
        let repo = GitRepo::open(env!("CARGO_MANIFEST_DIR")).unwrap();
        let content = repo.workdir_content("CLAUDE.md");
        assert!(content.is_ok());
        match content.unwrap() {
            ContentResult::Text(s) => assert!(!s.is_empty()),
            other => panic!("Expected Text, got {:?}", other),
        }
    }

    #[test]
    fn test_gitrepo_workdir_content_nonexistent_file() {
        let repo = GitRepo::open(env!("CARGO_MANIFEST_DIR")).unwrap();
        let content = repo.workdir_content("definitely_does_not_exist_xyz.txt");
        assert!(content.is_ok());
        assert_eq!(content.unwrap(), ContentResult::NotFound);
    }

    // ---- discard tests ----

    /// Helper: create a temp repo with an initial commit containing a file.
    /// Returns (tmpdir, GitRepo, file_path).
    fn setup_discard_repo(name: &str, content: &str) -> (std::path::PathBuf, GitRepo, std::path::PathBuf) {
        let tmpdir = std::env::temp_dir().join(format!("discard_{}_test_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&tmpdir);

        let repo = git2::Repository::init(&tmpdir).expect("init repo");
        repo.config().unwrap().set_str("user.email", "test@test.com").unwrap();
        repo.config().unwrap().set_str("user.name", "Test").unwrap();

        let file_path = tmpdir.join("test.txt");
        std::fs::write(&file_path, content).unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new("test.txt")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();

        let git_repo = GitRepo::open(tmpdir.to_str().unwrap()).unwrap();
        (tmpdir, git_repo, file_path)
    }

    #[test]
    fn test_discard_file_modified() {
        let original = "line1\nline2\nline3\n";
        let (tmpdir, git_repo, file_path) = setup_discard_repo("modified", original);

        // Modify the file in workdir
        std::fs::write(&file_path, "line1\nCHANGED\nline3\n").unwrap();

        // Discard should restore to index (== HEAD since nothing staged)
        git_repo.discard_file("test.txt").unwrap();

        let after = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(after, original, "File should be restored to index content");

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[test]
    fn test_discard_file_deleted() {
        let original = "hello\nworld\n";
        let (tmpdir, git_repo, file_path) = setup_discard_repo("deleted", original);

        // Delete the file from workdir
        std::fs::remove_file(&file_path).unwrap();
        assert!(!file_path.exists());

        // Discard should recreate the file from index
        git_repo.discard_file("test.txt").unwrap();

        assert!(file_path.exists(), "File should be recreated");
        let after = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(after, original, "Restored content should match original");

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[test]
    fn test_discard_file_untracked() {
        let tmpdir = std::env::temp_dir().join(format!("discard_untracked_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmpdir);

        let repo = git2::Repository::init(&tmpdir).expect("init repo");
        repo.config().unwrap().set_str("user.email", "test@test.com").unwrap();
        repo.config().unwrap().set_str("user.name", "Test").unwrap();

        // Create initial commit with a different file so HEAD exists
        let other_path = tmpdir.join("other.txt");
        std::fs::write(&other_path, "x\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("other.txt")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();

        // Create an untracked file
        let untracked = tmpdir.join("untracked.txt");
        std::fs::write(&untracked, "new file content\n").unwrap();

        let git_repo = GitRepo::open(tmpdir.to_str().unwrap()).unwrap();
        git_repo.discard_file("untracked.txt").unwrap();

        assert!(!untracked.exists(), "Untracked file should be deleted");

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[test]
    fn test_discard_file_preserves_staged_changes() {
        let original = "line1\nline2\nline3\n";
        let (tmpdir, git_repo, file_path) = setup_discard_repo("staged_preserved", original);

        // Stage a change
        let staged_content = "line1\nSTAGED\nline3\n";
        std::fs::write(&file_path, staged_content).unwrap();
        git_repo.stage_file("test.txt").unwrap();

        // Make further workdir changes on top
        std::fs::write(&file_path, "line1\nSTAGED\nWORKDIR\n").unwrap();

        // Discard unstaged changes — should restore workdir to index (staged version)
        git_repo.discard_file("test.txt").unwrap();

        let after = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(after, staged_content, "Workdir should match index (staged), not HEAD");

        // Verify staged content is still in index
        let idx = git_repo.index_content("test.txt").unwrap();
        match idx {
            ContentResult::Text(s) => assert_eq!(s, staged_content),
            other => panic!("Expected Text, got {:?}", other),
        }

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[test]
    fn test_discard_hunk_single_hunk() {
        let original = "a\nb\nc\n";
        let (tmpdir, git_repo, file_path) = setup_discard_repo("hunk_single", original);

        let modified = "a\nX\nc\n";
        std::fs::write(&file_path, modified).unwrap();

        use crate::diff::compute_hunks;
        let hunks = compute_hunks(original, modified, 3);
        assert_eq!(hunks.len(), 1);

        git_repo.discard_hunk("test.txt", modified, &hunks[0]).unwrap();

        let after = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(after, original, "Workdir should be restored to index content");

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[test]
    fn test_discard_hunk_one_of_two() {
        let original = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nline15\nline16\nline17\nline18\nline19\nline20\n";
        let (tmpdir, git_repo, file_path) = setup_discard_repo("hunk_partial", original);

        let modified = "line1\nLINE2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nLINE15\nline16\nline17\nline18\nline19\nline20\n";
        std::fs::write(&file_path, modified).unwrap();

        use crate::diff::compute_hunks;
        let hunks = compute_hunks(original, modified, 3);
        assert_eq!(hunks.len(), 2);

        // Discard only hunk 1 (the LINE2 change) — LINE15 should remain
        git_repo.discard_hunk("test.txt", modified, &hunks[0]).unwrap();

        let after = std::fs::read_to_string(&file_path).unwrap();
        let expected = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nLINE15\nline16\nline17\nline18\nline19\nline20\n";
        assert_eq!(after, expected, "Only hunk 1 should be discarded; hunk 2 should remain");

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[test]
    fn test_discard_hunk_preserves_staged_changes() {
        let original = "line1\nline2\nline3\n";
        let (tmpdir, git_repo, file_path) = setup_discard_repo("hunk_staged", original);

        // Stage a change
        let staged = "line1\nSTAGED\nline3\n";
        std::fs::write(&file_path, staged).unwrap();
        git_repo.stage_file("test.txt").unwrap();

        // Make workdir change on top of staged
        let workdir = "line1\nSTAGED\nWORKDIR\n";
        std::fs::write(&file_path, workdir).unwrap();

        // The unstaged diff is: staged (index) vs workdir
        // Hunk changes line3 → WORKDIR
        use crate::diff::compute_hunks;
        let hunks = compute_hunks(staged, workdir, 3);
        assert_eq!(hunks.len(), 1);

        git_repo.discard_hunk("test.txt", workdir, &hunks[0]).unwrap();

        let after = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(after, staged, "Workdir should match index (staged content)");

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[cfg(unix)]
    fn make_repo_with_symlink(name: &str, target: &str) -> (std::path::PathBuf, GitRepo, std::path::PathBuf) {
        let tmpdir = std::env::temp_dir().join(format!("symlink_{}_test_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&tmpdir);

        let repo = git2::Repository::init(&tmpdir).expect("init repo");
        repo.config().unwrap().set_str("user.email", "test@test.com").unwrap();
        repo.config().unwrap().set_str("user.name", "Test").unwrap();

        let link_path = tmpdir.join("link");
        std::os::unix::fs::symlink(target, &link_path).unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new("link")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();

        let git_repo = GitRepo::open(tmpdir.to_str().unwrap()).unwrap();
        (tmpdir, git_repo, link_path)
    }

    #[cfg(unix)]
    #[test]
    fn test_workdir_content_returns_symlink_target_as_text() {
        let (tmpdir, git_repo, _link_path) = make_repo_with_symlink("content", "target.txt");

        let content = git_repo.workdir_content("link").unwrap();
        assert_eq!(content, ContentResult::Text("target.txt".to_string()));

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[cfg(unix)]
    #[test]
    fn test_snapshot_captures_symlink_target() {
        let (tmpdir, git_repo, _link_path) = make_repo_with_symlink("snap", "original_target");

        let snap = git_repo.snapshot_path("link").unwrap();
        assert!(matches!(snap.workdir, WorkdirSnapshot::Symlink { .. }));

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[cfg(unix)]
    #[test]
    fn test_restore_snapshot_roundtrips_symlink() {
        let (tmpdir, git_repo, link_path) = make_repo_with_symlink("restore", "original_target");
        let snap = git_repo.snapshot_path("link").unwrap();

        // Change the symlink target
        std::fs::remove_file(&link_path).unwrap();
        std::os::unix::fs::symlink("new_target", &link_path).unwrap();

        // Restore should bring back the original symlink
        git_repo.restore_snapshot(&snap).unwrap();
        let target = std::fs::read_link(&link_path).unwrap();
        assert_eq!(target, std::path::PathBuf::from("original_target"));
        assert!(std::fs::symlink_metadata(&link_path).unwrap().file_type().is_symlink());

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[cfg(unix)]
    #[test]
    fn test_restore_snapshot_regular_file_to_symlink() {
        let tmpdir = std::env::temp_dir().join(format!("symlink_file2link_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmpdir);

        let repo = git2::Repository::init(&tmpdir).expect("init repo");
        repo.config().unwrap().set_str("user.email", "test@test.com").unwrap();
        repo.config().unwrap().set_str("user.name", "Test").unwrap();

        // Start with a symlink, snapshot it
        let file_path = tmpdir.join("entry");
        std::os::unix::fs::symlink("link_target", &file_path).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("entry")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();

        let git_repo = GitRepo::open(tmpdir.to_str().unwrap()).unwrap();
        let snap = git_repo.snapshot_path("entry").unwrap();

        // Replace symlink with a regular file
        std::fs::remove_file(&file_path).unwrap();
        std::fs::write(&file_path, "regular content").unwrap();

        // Restore should bring back the symlink
        git_repo.restore_snapshot(&snap).unwrap();
        assert!(std::fs::symlink_metadata(&file_path).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_link(&file_path).unwrap(), std::path::PathBuf::from("link_target"));

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[cfg(unix)]
    #[test]
    fn test_restore_snapshot_symlink_to_regular_file() {
        let tmpdir = std::env::temp_dir().join(format!("symlink_link2file_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmpdir);

        let repo = git2::Repository::init(&tmpdir).expect("init repo");
        repo.config().unwrap().set_str("user.email", "test@test.com").unwrap();
        repo.config().unwrap().set_str("user.name", "Test").unwrap();

        // Start with a regular file, snapshot it
        let file_path = tmpdir.join("entry");
        std::fs::write(&file_path, "regular content").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("entry")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();

        let git_repo = GitRepo::open(tmpdir.to_str().unwrap()).unwrap();
        let snap = git_repo.snapshot_path("entry").unwrap();

        // Replace regular file with a symlink
        std::fs::remove_file(&file_path).unwrap();
        std::os::unix::fs::symlink("some_target", &file_path).unwrap();

        // Restore should bring back the regular file
        git_repo.restore_snapshot(&snap).unwrap();
        assert!(std::fs::symlink_metadata(&file_path).unwrap().file_type().is_file());
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "regular content");

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[cfg(unix)]
    #[test]
    fn test_snapshot_dangling_symlink() {
        let (tmpdir, git_repo, link_path) = make_repo_with_symlink("dangling", "nonexistent_target");

        // The symlink target doesn't exist — it's dangling
        assert!(!std::path::Path::new("nonexistent_target").exists());

        let snap = git_repo.snapshot_path("link").unwrap();
        assert!(matches!(snap.workdir, WorkdirSnapshot::Symlink { .. }));

        // Delete and restore
        std::fs::remove_file(&link_path).unwrap();
        git_repo.restore_snapshot(&snap).unwrap();

        let target = std::fs::read_link(&link_path).unwrap();
        assert_eq!(target, std::path::PathBuf::from("nonexistent_target"));
        assert!(std::fs::symlink_metadata(&link_path).unwrap().file_type().is_symlink());

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[cfg(unix)]
    #[test]
    fn test_discard_hunk_preserves_symlink() {
        let (tmpdir, git_repo, link_path) = make_repo_with_symlink("discard", "original_target");

        // Change the symlink target (this is the "workdir" change)
        std::fs::remove_file(&link_path).unwrap();
        std::os::unix::fs::symlink("modified_target", &link_path).unwrap();

        use crate::diff::compute_hunks;
        let hunks = compute_hunks("original_target", "modified_target", 3);
        assert_eq!(hunks.len(), 1);

        git_repo.discard_hunk("link", "modified_target", &hunks[0]).unwrap();

        let target = std::fs::read_link(&link_path).unwrap();
        assert_eq!(target, std::path::PathBuf::from("original_target"));
        assert!(std::fs::symlink_metadata(&link_path).unwrap().file_type().is_symlink());

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[cfg(unix)]
    #[test]
    fn test_snapshot_captures_executable_bit() {
        use std::os::unix::fs::PermissionsExt;

        let tmpdir = std::env::temp_dir().join(format!("exec_snap_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmpdir);

        {
            let repo = git2::Repository::init(&tmpdir).expect("init repo");
            repo.config().unwrap().set_str("user.email", "test@test.com").unwrap();
            repo.config().unwrap().set_str("user.name", "Test").unwrap();

            let file_path = tmpdir.join("script.sh");
            std::fs::write(&file_path, "#!/bin/sh\necho hello\n").unwrap();
            let mut perms = std::fs::metadata(&file_path).unwrap().permissions();
            perms.set_mode(0o100755);
            std::fs::set_permissions(&file_path, perms).unwrap();

            let mut index = repo.index().unwrap();
            index.add_path(Path::new("script.sh")).unwrap();
            index.write().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            let sig = repo.signature().unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();
        }

        let git_repo = GitRepo::open(tmpdir.to_str().unwrap()).unwrap();
        let file_path = tmpdir.join("script.sh");
        let snap = git_repo.snapshot_path("script.sh").unwrap();

        assert!(matches!(snap.workdir, WorkdirSnapshot::Regular { executable: true, .. }), "snapshot should capture executable bit");
        assert_eq!(snap.index_mode, Some(0o100755), "snapshot should capture index mode");

        // Overwrite with a non-executable regular file and remove from index
        std::fs::write(&file_path, "overwritten\n").unwrap();
        git_repo.unstage_file("script.sh").unwrap();

        // Restore should bring back executable bit in both index and workdir
        git_repo.restore_snapshot(&snap).unwrap();

        let meta = std::fs::metadata(&file_path).unwrap();
        assert!(meta.permissions().mode() & 0o111 != 0, "workdir file should be executable after restore");

        let index = git_repo.repo.index().unwrap();
        let entry = index.get_path(Path::new("script.sh"), 0).expect("entry should exist");
        assert_eq!(entry.mode, 0o100755, "index entry should have executable mode after restore");

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[cfg(unix)]
    #[test]
    fn test_snapshot_non_executable_stays_non_executable() {
        use std::os::unix::fs::PermissionsExt;

        let tmpdir = std::env::temp_dir().join(format!("noexec_snap_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmpdir);

        {
            let repo = git2::Repository::init(&tmpdir).expect("init repo");
            repo.config().unwrap().set_str("user.email", "test@test.com").unwrap();
            repo.config().unwrap().set_str("user.name", "Test").unwrap();

            let file_path = tmpdir.join("data.txt");
            std::fs::write(&file_path, "just data\n").unwrap();

            let mut index = repo.index().unwrap();
            index.add_path(Path::new("data.txt")).unwrap();
            index.write().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            let sig = repo.signature().unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();
        }

        let git_repo = GitRepo::open(tmpdir.to_str().unwrap()).unwrap();
        let file_path = tmpdir.join("data.txt");
        let snap = git_repo.snapshot_path("data.txt").unwrap();

        assert!(matches!(snap.workdir, WorkdirSnapshot::Regular { executable: false, .. }), "non-executable file should not be marked executable");
        assert_eq!(snap.index_mode, Some(0o100644));

        // Make executable, then restore — should go back to non-executable
        let mut perms = std::fs::metadata(&file_path).unwrap().permissions();
        perms.set_mode(0o100755);
        std::fs::set_permissions(&file_path, perms).unwrap();

        git_repo.restore_snapshot(&snap).unwrap();

        let meta = std::fs::metadata(&file_path).unwrap();
        assert_eq!(meta.permissions().mode() & 0o111, 0, "workdir file should not be executable after restore");

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    // ---- pipeline round-trip tests (Phase 1) ----
    // All use compute_hunks to generate inputs — no hand-built hunks.

    #[test]
    fn test_roundtrip_single_line_change() {
        use crate::diff::compute_hunks;
        let old = "alpha\nbeta\ngamma\n";
        let new = "alpha\nBETA\ngamma\n";
        let hunks = compute_hunks(old, new, 3);
        assert_eq!(hunks.len(), 1);
        assert_eq!(apply_hunk_to_content(old, &hunks[0], None), new);
        assert_eq!(reverse_apply_hunk_to_content(new, &hunks[0], None), old);
    }

    #[test]
    fn test_roundtrip_insert_lines() {
        use crate::diff::compute_hunks;
        let old = "a\nc\n";
        let new = "a\nb1\nb2\nc\n";
        let hunks = compute_hunks(old, new, 3);
        assert_eq!(hunks.len(), 1);
        assert_eq!(apply_hunk_to_content(old, &hunks[0], None), new);
        assert_eq!(reverse_apply_hunk_to_content(new, &hunks[0], None), old);
    }

    #[test]
    fn test_roundtrip_delete_lines() {
        use crate::diff::compute_hunks;
        let old = "a\nb\nc\nd\n";
        let new = "a\nd\n";
        let hunks = compute_hunks(old, new, 3);
        assert_eq!(hunks.len(), 1);
        assert_eq!(apply_hunk_to_content(old, &hunks[0], None), new);
        assert_eq!(reverse_apply_hunk_to_content(new, &hunks[0], None), old);
    }

    #[test]
    fn test_roundtrip_multi_hunk_individual_apply() {
        use crate::diff::compute_hunks;
        let old = "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nl10\nl11\nl12\nl13\nl14\nl15\n";
        let new = "l1\nL2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nl10\nl11\nl12\nl13\nl14\nL15\n";
        let hunks = compute_hunks(old, new, 3);
        assert_eq!(hunks.len(), 2);

        let result0 = apply_hunk_to_content(old, &hunks[0], None);
        assert!(result0.contains("L2"), "Hunk 0 should apply L2 change");
        assert!(result0.contains("l15"), "Hunk 0 should leave l15 unchanged");

        let result1 = apply_hunk_to_content(old, &hunks[1], None);
        assert!(result1.contains("l2\n"), "Hunk 1 should leave l2 unchanged");
        assert!(result1.contains("L15"), "Hunk 1 should apply L15 change");

        let rev0 = reverse_apply_hunk_to_content(new, &hunks[0], None);
        assert!(rev0.contains("l2\n"), "Reversing hunk 0 should restore l2");
        assert!(rev0.contains("L15"), "Reversing hunk 0 should keep L15");

        let rev1 = reverse_apply_hunk_to_content(new, &hunks[1], None);
        assert!(rev1.contains("L2"), "Reversing hunk 1 should keep L2");
        assert!(rev1.contains("l15"), "Reversing hunk 1 should restore l15");
    }

    #[test]
    fn test_roundtrip_no_trailing_newline() {
        use crate::diff::compute_hunks;
        let old = "a\nb\nc";
        let new = "a\nX\nc";
        let hunks = compute_hunks(old, new, 3);
        assert_eq!(hunks.len(), 1);
        assert_eq!(apply_hunk_to_content(old, &hunks[0], None), new);
        assert_eq!(reverse_apply_hunk_to_content(new, &hunks[0], None), old);
    }

    #[test]
    fn test_roundtrip_empty_to_content() {
        use crate::diff::compute_hunks;
        let old = "";
        let new = "hello\nworld\n";
        let hunks = compute_hunks(old, new, 3);
        assert_eq!(hunks.len(), 1);
        assert_eq!(apply_hunk_to_content(old, &hunks[0], None), new);
        assert_eq!(reverse_apply_hunk_to_content(new, &hunks[0], None), old);
    }

    #[test]
    fn test_roundtrip_content_to_empty() {
        use crate::diff::compute_hunks;
        let old = "hello\nworld\n";
        let new = "";
        let hunks = compute_hunks(old, new, 3);
        assert_eq!(hunks.len(), 1);
        assert_eq!(apply_hunk_to_content(old, &hunks[0], None), new);
        assert_eq!(reverse_apply_hunk_to_content(new, &hunks[0], None), old);
    }

    #[test]
    fn test_roundtrip_single_line_file() {
        use crate::diff::compute_hunks;
        let old = "only\n";
        let new = "changed\n";
        let hunks = compute_hunks(old, new, 3);
        assert_eq!(hunks.len(), 1);
        assert_eq!(apply_hunk_to_content(old, &hunks[0], None), new);
        assert_eq!(reverse_apply_hunk_to_content(new, &hunks[0], None), old);
    }

    #[test]
    fn test_roundtrip_change_at_first_line() {
        use crate::diff::compute_hunks;
        let old = "first\nsecond\nthird\n";
        let new = "FIRST\nsecond\nthird\n";
        let hunks = compute_hunks(old, new, 3);
        assert_eq!(hunks.len(), 1);
        assert_eq!(apply_hunk_to_content(old, &hunks[0], None), new);
        assert_eq!(reverse_apply_hunk_to_content(new, &hunks[0], None), old);
    }

    #[test]
    fn test_roundtrip_change_at_last_line() {
        use crate::diff::compute_hunks;
        let old = "first\nsecond\nthird\n";
        let new = "first\nsecond\nTHIRD\n";
        let hunks = compute_hunks(old, new, 3);
        assert_eq!(hunks.len(), 1);
        assert_eq!(apply_hunk_to_content(old, &hunks[0], None), new);
        assert_eq!(reverse_apply_hunk_to_content(new, &hunks[0], None), old);
    }

    // ---- edge-case round-trips (Phase 3) ----

    #[test]
    fn test_roundtrip_no_trailing_newline_hunk_adds_one() {
        use crate::diff::compute_hunks;
        let old = "a\nb";
        let new = "a\nb\n";
        let hunks = compute_hunks(old, new, 3);
        assert!(!hunks.is_empty());
        let mut result = old.to_string();
        for h in &hunks {
            result = apply_hunk_to_content(&result, h, None);
        }
        assert_eq!(result, new);
    }

    #[test]
    fn test_roundtrip_trailing_newline_hunk_removes_it() {
        use crate::diff::compute_hunks;
        let old = "a\nb\n";
        let new = "a\nb";
        let hunks = compute_hunks(old, new, 3);
        assert!(!hunks.is_empty());
        let mut result = old.to_string();
        for h in &hunks {
            result = apply_hunk_to_content(&result, h, None);
        }
        assert_eq!(result, new);
    }

    #[test]
    fn test_roundtrip_hunk_covers_entire_file() {
        use crate::diff::compute_hunks;
        let old = "a\nb\nc\n";
        let new = "x\ny\nz\n";
        let hunks = compute_hunks(old, new, 3);
        assert_eq!(hunks.len(), 1);
        assert_eq!(apply_hunk_to_content(old, &hunks[0], None), new);
        assert_eq!(reverse_apply_hunk_to_content(new, &hunks[0], None), old);
    }

    #[test]
    fn test_roundtrip_multi_hunk_line_count_change() {
        use crate::diff::compute_hunks;
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\nn\no\n";
        let new = "a\nb\nX\nY\nZ\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\nn\nO\n";
        let hunks = compute_hunks(old, new, 3);
        assert!(hunks.len() >= 2, "Expected at least 2 hunks, got {}", hunks.len());

        for (i, h) in hunks.iter().enumerate() {
            let result = apply_hunk_to_content(old, h, None);
            assert!(!result.is_empty(), "Hunk {} produced empty result", i);
        }

        for (i, h) in hunks.iter().enumerate() {
            let result = reverse_apply_hunk_to_content(new, h, None);
            assert!(!result.is_empty(), "Reverse hunk {} produced empty result", i);
        }
    }

    #[test]
    fn test_roundtrip_single_line_no_newline() {
        use crate::diff::compute_hunks;
        let old = "x";
        let new = "y";
        let hunks = compute_hunks(old, new, 3);
        assert_eq!(hunks.len(), 1);
        assert_eq!(apply_hunk_to_content(old, &hunks[0], None), new);
        assert_eq!(reverse_apply_hunk_to_content(new, &hunks[0], None), old);
    }

    // ---- end-to-end staging tests with line filtering (Phase 2) ----

    #[test]
    fn test_stage_filtered_delete_only() {
        use crate::diff::compute_hunks;
        let old = "a\nb\nc\n";
        let (tmpdir, git_repo, file_path) = setup_discard_repo("stage_del", old);

        let workdir = "a\nX\nc\n";
        std::fs::write(&file_path, workdir).unwrap();

        let hunks = compute_hunks(old, workdir, 3);
        assert_eq!(hunks.len(), 1);

        let del_idx: Vec<usize> = hunks[0].lines.iter().enumerate()
            .filter(|(_, l)| l.kind == ChangeKind::Delete)
            .map(|(i, _)| i)
            .collect();

        git_repo.stage_hunk("test.txt", old, &hunks[0], Some(&del_idx)).unwrap();

        let index_content = match git_repo.index_content("test.txt").unwrap() {
            ContentResult::Text(s) => s,
            other => panic!("Expected Text, got {:?}", other),
        };
        assert_eq!(index_content, "a\nc\n", "Index should have b removed, X not added");

        let workdir_after = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(workdir_after, workdir, "Workdir should be preserved");

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[test]
    fn test_stage_filtered_insert_only() {
        use crate::diff::compute_hunks;
        let old = "a\nb\nc\n";
        let (tmpdir, git_repo, file_path) = setup_discard_repo("stage_ins", old);

        let workdir = "a\nX\nc\n";
        std::fs::write(&file_path, workdir).unwrap();

        let hunks = compute_hunks(old, workdir, 3);
        assert_eq!(hunks.len(), 1);

        let ins_idx: Vec<usize> = hunks[0].lines.iter().enumerate()
            .filter(|(_, l)| l.kind == ChangeKind::Insert)
            .map(|(i, _)| i)
            .collect();

        git_repo.stage_hunk("test.txt", old, &hunks[0], Some(&ins_idx)).unwrap();

        let index_content = match git_repo.index_content("test.txt").unwrap() {
            ContentResult::Text(s) => s,
            other => panic!("Expected Text, got {:?}", other),
        };
        assert_eq!(index_content, "a\nb\nX\nc\n", "Index should keep b and add X");

        let workdir_after = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(workdir_after, workdir, "Workdir should be preserved");

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[test]
    fn test_stage_filtered_subset_of_multi_insert() {
        use crate::diff::compute_hunks;
        let old = "a\nb\n";
        let (tmpdir, git_repo, file_path) = setup_discard_repo("stage_multi_ins", old);

        let workdir = "a\nX\nY\nZ\nb\n";
        std::fs::write(&file_path, workdir).unwrap();

        let hunks = compute_hunks(old, workdir, 3);
        assert_eq!(hunks.len(), 1);

        let ins_indices: Vec<usize> = hunks[0].lines.iter().enumerate()
            .filter(|(_, l)| l.kind == ChangeKind::Insert)
            .map(|(i, _)| i)
            .collect();
        assert!(ins_indices.len() >= 2, "Expected multiple inserts");

        git_repo.stage_hunk("test.txt", old, &hunks[0], Some(&ins_indices[..1])).unwrap();

        let index_content = match git_repo.index_content("test.txt").unwrap() {
            ContentResult::Text(s) => s,
            other => panic!("Expected Text, got {:?}", other),
        };
        assert_eq!(index_content, "a\nX\nb\n", "Only first insert should be staged");

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[test]
    fn test_unstage_filtered_insert_only() {
        use crate::diff::compute_hunks;
        let head = "a\nb\nc\n";
        let (tmpdir, git_repo, file_path) = setup_discard_repo("unstage_ins", head);

        let workdir = "a\nX\nc\n";
        std::fs::write(&file_path, workdir).unwrap();
        git_repo.stage_file("test.txt").unwrap();

        let index_content = match git_repo.index_content("test.txt").unwrap() {
            ContentResult::Text(s) => s,
            other => panic!("Expected Text, got {:?}", other),
        };
        let staged_hunks = compute_hunks(head, &index_content, 3);
        assert_eq!(staged_hunks.len(), 1);

        let ins_idx: Vec<usize> = staged_hunks[0].lines.iter().enumerate()
            .filter(|(_, l)| l.kind == ChangeKind::Insert)
            .map(|(i, _)| i)
            .collect();

        git_repo.unstage_hunk("test.txt", &index_content, &staged_hunks[0], Some(&ins_idx)).unwrap();

        let index_after = match git_repo.index_content("test.txt").unwrap() {
            ContentResult::Text(s) => s,
            other => panic!("Expected Text, got {:?}", other),
        };
        assert_eq!(index_after, "a\nc\n", "Unstaging insert should leave delete staged");

        let _ = std::fs::remove_dir_all(&tmpdir);
    }
