    use super::*;
    use crate::diff::types::ChangeKind;
    use crate::git::types::{FileEntry, FileStatus};

    fn staged_only_entry() -> FileEntry {
        FileEntry {
            path: "staged.rs".to_string(),
            index_status: Some(FileStatus::Modified),
            workdir_status: None,
        }
    }

    fn unstaged_entry() -> FileEntry {
        FileEntry {
            path: "unstaged.rs".to_string(),
            index_status: None,
            workdir_status: Some(FileStatus::Modified),
        }
    }

    fn both_entry() -> FileEntry {
        FileEntry {
            path: "both.rs".to_string(),
            index_status: Some(FileStatus::Modified),
            workdir_status: Some(FileStatus::Modified),
        }
    }

    /// Build an App without opening a repo. Used for testing update() logic.
    fn test_app_with_files(files: Vec<FileEntry>) -> App {
        let repo = GitRepo::open(".").expect("repo should open");
        let (staged_files, unstaged_files) = App::partition_files(&files);
        let initial_section = if !staged_files.is_empty() {
            SidebarSection::Staged
        } else {
            SidebarSection::Unstaged
        };
        App {
            repo,
            staged_files,
            unstaged_files,
            selected_index: 0,
            sidebar_section: initial_section,
            diff_content: None,
            diff_scroll: 0,
            focus: Focus::Sidebar,
            should_quit: false,
            styled_diff: None,
            current_hunk_index: None,
            scroll_positions: HashMap::new(),
            diff_stale: false,
            auto_reload: false,
            status_message: None,
            sidebar_collapsed: false,
            pending_discard: None,
            show_full_file: false,
            diff_viewport_height: Cell::new(0),
            undo: UndoManager::new(),
            comment_input: String::new(),
            comment_context: None,
            mode: AppMode::Normal,
            diff_cursor: 0,
            visual_selection: Vec::new(),
            visual_cursor: 0,
            visual_anchor: 0,
            visual_from_mouse: false,
            semantic_filter: false,
            formatting_only_cache: HashMap::new(),
        }
    }

    #[test]
    fn test_partition_files() {
        let files = vec![staged_only_entry(), unstaged_entry(), both_entry()];
        let (staged, unstaged) = App::partition_files(&files);
        // staged_only_entry has index_status, both_entry has index_status
        assert_eq!(staged.len(), 2);
        // unstaged_entry has workdir_status, both_entry has workdir_status
        assert_eq!(unstaged.len(), 2);
    }

    #[test]
    fn test_binary_produces_binary_diff_content() {
        let dc = binary_diff_content("image.png");
        assert!(dc.is_binary);
        assert!(dc.hunks.is_empty());
    }

    #[test]
    fn test_update_move_down_within_section() {
        let mut app = test_app_with_files(vec![
            staged_only_entry(),
            FileEntry {
                path: "staged2.rs".to_string(),
                index_status: Some(FileStatus::Added),
                workdir_status: None,
            },
        ]);
        app.focus = Focus::DiffView;
        assert_eq!(app.sidebar_section, SidebarSection::Staged);
        assert_eq!(app.selected_index, 0);
        app.update(Message::MoveDown);
        assert_eq!(app.selected_index, 1);
        assert_eq!(app.sidebar_section, SidebarSection::Staged);
    }

    #[test]
    fn test_update_move_down_crosses_to_unstaged() {
        let mut app = test_app_with_files(vec![staged_only_entry(), unstaged_entry()]);
        app.focus = Focus::DiffView;
        assert_eq!(app.sidebar_section, SidebarSection::Staged);
        assert_eq!(app.staged_files.len(), 1);
        assert_eq!(app.unstaged_files.len(), 1);
        // At bottom of staged (index 0, len 1), move down should cross
        app.update(Message::MoveDown);
        assert_eq!(app.sidebar_section, SidebarSection::Unstaged);
        assert_eq!(app.selected_index, 0);
    }

    #[test]
    fn test_update_move_up_crosses_to_staged() {
        let mut app = test_app_with_files(vec![staged_only_entry(), unstaged_entry()]);
        app.focus = Focus::DiffView;
        app.sidebar_section = SidebarSection::Unstaged;
        app.selected_index = 0;
        app.update(Message::MoveUp);
        assert_eq!(app.sidebar_section, SidebarSection::Staged);
        assert_eq!(app.selected_index, 0); // last item in staged (len 1, so index 0)
    }

    #[test]
    fn test_update_move_up_at_top_of_staged_stays() {
        let mut app = test_app_with_files(vec![staged_only_entry()]);
        app.focus = Focus::DiffView;
        assert_eq!(app.sidebar_section, SidebarSection::Staged);
        assert_eq!(app.selected_index, 0);
        app.update(Message::MoveUp);
        assert_eq!(app.selected_index, 0);
        assert_eq!(app.sidebar_section, SidebarSection::Staged);
    }

    #[test]
    fn test_update_move_down_at_bottom_of_unstaged_stays() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.focus = Focus::DiffView;
        assert_eq!(app.sidebar_section, SidebarSection::Unstaged);
        app.update(Message::MoveDown);
        assert_eq!(app.selected_index, 0);
        assert_eq!(app.sidebar_section, SidebarSection::Unstaged);
    }

    #[test]
    fn test_update_quit() {
        let mut app = test_app_with_files(vec![]);
        assert!(!app.should_quit);
        app.update(Message::Quit);
        assert!(app.should_quit);
    }

    #[test]
    fn test_update_switch_focus() {
        let mut app = test_app_with_files(vec![]);
        assert_eq!(app.focus, Focus::Sidebar);
        app.update(Message::SwitchFocus);
        assert_eq!(app.focus, Focus::DiffView);
        app.update(Message::SwitchFocus);
        assert_eq!(app.focus, Focus::Sidebar);
    }

    #[test]
    fn test_update_scroll_diff_up_at_zero() {
        let mut app = test_app_with_files(vec![]);
        assert_eq!(app.diff_scroll, 0);
        app.update(Message::ScrollDiffUp);
        assert_eq!(app.diff_scroll, 0);
    }

    #[test]
    fn test_load_diff_for_selected_clears_styled_diff_on_empty_file_list() {
        let mut app = test_app_with_files(vec![]);
        app.styled_diff = Some(StyledDiffContent {
            lines_by_old_lineno: std::collections::HashMap::new(),
            lines_by_new_lineno: std::collections::HashMap::new(),
        });
        app.load_diff_for_selected();
        assert!(app.styled_diff.is_none());
    }

    #[test]
    fn test_next_hunk_no_op_on_empty() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.diff_scroll = 0;
        app.update(Message::NextHunk);
        assert_eq!(app.diff_scroll, 0);
    }

    fn three_hunk_dc() -> DiffContent {
        make_dc(vec![
            DiffHunk { old_start: 1, new_start: 1, lines: vec![dl(ChangeKind::Insert, None, Some(1)); 2], has_header: true, header_context: None },
            DiffHunk { old_start: 3, new_start: 3, lines: vec![dl(ChangeKind::Insert, None, Some(3)); 3], has_header: true, header_context: None },
            DiffHunk { old_start: 8, new_start: 8, lines: vec![dl(ChangeKind::Insert, None, Some(8)); 5], has_header: true, header_context: None },
        ])
    }

    #[test]
    fn test_next_hunk_advances_to_next_start() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.diff_content = Some(three_hunk_dc());
        app.diff_scroll = 0;
        app.update(Message::NextHunk);
        assert_eq!(app.diff_scroll, 3);
        app.update(Message::NextHunk);
        assert_eq!(app.diff_scroll, 7);
        app.update(Message::NextHunk);
        assert_eq!(app.diff_scroll, 7);
    }

    #[test]
    fn test_prev_hunk_no_op_at_first() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.diff_content = Some(three_hunk_dc());
        app.diff_scroll = 0;
        app.update(Message::PrevHunk);
        assert_eq!(app.diff_scroll, 0);
    }

    #[test]
    fn test_prev_hunk_goes_to_previous_start() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.diff_content = Some(three_hunk_dc());
        app.diff_scroll = 7;
        app.update(Message::PrevHunk);
        assert_eq!(app.diff_scroll, 3);
        app.update(Message::PrevHunk);
        assert_eq!(app.diff_scroll, 0);
    }

    #[test]
    fn test_mouse_click_staged_sidebar() {
        let mut app = test_app_with_files(vec![
            staged_only_entry(),
            FileEntry {
                path: "staged2.rs".to_string(),
                index_status: Some(FileStatus::Added),
                workdir_status: None,
            },
        ]);
        app.focus = Focus::DiffView;
        app.update(Message::MouseClickStagedSidebar(1));
        assert_eq!(app.selected_index, 1);
        assert_eq!(app.sidebar_section, SidebarSection::Staged);
        assert_eq!(app.focus, Focus::Sidebar);
    }

    #[test]
    fn test_mouse_click_unstaged_sidebar() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.focus = Focus::DiffView;
        app.update(Message::MouseClickUnstagedSidebar(0));
        assert_eq!(app.selected_index, 0);
        assert_eq!(app.sidebar_section, SidebarSection::Unstaged);
        assert_eq!(app.focus, Focus::Sidebar);
    }

    #[test]
    fn test_mouse_click_staged_out_of_bounds_noop() {
        let mut app = test_app_with_files(vec![staged_only_entry()]);
        app.focus = Focus::DiffView;
        let before = app.selected_index;
        app.update(Message::MouseClickStagedSidebar(99));
        assert_eq!(app.selected_index, before);
        assert_eq!(app.focus, Focus::DiffView);
    }

    #[test]
    fn test_focus_diff_sets_focus_to_diffview() {
        let mut app = test_app_with_files(vec![]);
        assert_eq!(app.focus, Focus::Sidebar);
        app.update(Message::FocusDiff);
        assert_eq!(app.focus, Focus::DiffView);
    }

    #[test]
    fn test_move_down_resets_scroll_for_new_file() {
        let mut app = test_app_with_files(vec![
            staged_only_entry(),
            FileEntry {
                path: "staged2.rs".to_string(),
                index_status: Some(FileStatus::Added),
                workdir_status: None,
            },
        ]);
        // Simulate scrolling down in the first file
        app.diff_scroll = 42;
        // Navigate to the second file
        app.update(Message::MoveDown);
        assert_eq!(app.selected_index, 1);
        // Second file was never visited, scroll should be 0
        assert_eq!(app.diff_scroll, 0);
    }

    #[test]
    fn test_move_up_resets_scroll_for_new_file() {
        let mut app = test_app_with_files(vec![
            staged_only_entry(),
            FileEntry {
                path: "staged2.rs".to_string(),
                index_status: Some(FileStatus::Added),
                workdir_status: None,
            },
        ]);
        app.selected_index = 1;
        app.diff_scroll = 30;
        app.update(Message::MoveUp);
        assert_eq!(app.selected_index, 0);
        assert_eq!(app.diff_scroll, 0);
    }

    #[test]
    fn test_scroll_position_saved_and_restored_on_navigation() {
        let mut app = test_app_with_files(vec![
            staged_only_entry(),
            FileEntry {
                path: "staged2.rs".to_string(),
                index_status: Some(FileStatus::Added),
                workdir_status: None,
            },
        ]);
        // Scroll in first file
        app.diff_scroll = 15;
        // Move to second file — should save 15 for first file
        app.update(Message::MoveDown);
        assert_eq!(app.diff_scroll, 0);
        // Scroll in second file
        app.diff_scroll = 25;
        // Move back to first file — should save 25 for second, restore 15 for first
        app.update(Message::MoveUp);
        assert_eq!(app.diff_scroll, 15);
        // Move to second file again — should restore 25
        app.update(Message::MoveDown);
        assert_eq!(app.diff_scroll, 25);
    }

    #[test]
    fn test_cross_section_resets_scroll_for_new_file() {
        let mut app = test_app_with_files(vec![staged_only_entry(), unstaged_entry()]);
        assert_eq!(app.sidebar_section, SidebarSection::Staged);
        app.diff_scroll = 20;
        // Cross from staged to unstaged
        app.update(Message::MoveDown);
        assert_eq!(app.sidebar_section, SidebarSection::Unstaged);
        assert_eq!(app.diff_scroll, 0);
        // Scroll in unstaged, then cross back
        app.diff_scroll = 10;
        app.update(Message::MoveUp);
        assert_eq!(app.sidebar_section, SidebarSection::Staged);
        assert_eq!(app.diff_scroll, 20);
    }

    #[test]
    fn test_mouse_click_resets_scroll_for_new_file() {
        let mut app = test_app_with_files(vec![
            staged_only_entry(),
            FileEntry {
                path: "staged2.rs".to_string(),
                index_status: Some(FileStatus::Added),
                workdir_status: None,
            },
        ]);
        app.diff_scroll = 50;
        app.update(Message::MouseClickStagedSidebar(1));
        assert_eq!(app.selected_index, 1);
        assert_eq!(app.diff_scroll, 0);
    }

    #[test]
    fn test_mouse_click_cross_section_saves_scroll() {
        let mut app = test_app_with_files(vec![staged_only_entry(), unstaged_entry()]);
        assert_eq!(app.sidebar_section, SidebarSection::Staged);
        app.diff_scroll = 33;
        // Click into unstaged section
        app.update(Message::MouseClickUnstagedSidebar(0));
        assert_eq!(app.diff_scroll, 0);
        // Click back to staged
        app.update(Message::MouseClickStagedSidebar(0));
        assert_eq!(app.diff_scroll, 33);
    }

    // ---- discard confirmation flow tests ----

    #[test]
    fn test_discard_file_noop_in_staged_section() {
        let mut app = test_app_with_files(vec![staged_only_entry()]);
        assert_eq!(app.sidebar_section, SidebarSection::Staged);
        app.update(Message::DiscardFile);
        assert!(app.pending_discard.is_none());
        assert!(app.status_message.is_none());
    }

    #[test]
    fn test_discard_file_first_press_sets_pending() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        assert_eq!(app.sidebar_section, SidebarSection::Unstaged);
        app.update(Message::DiscardFile);
        assert_eq!(
            app.pending_discard,
            Some(PendingDiscard::File { path: "unstaged.rs".to_string() }),
        );
        assert!(app.status_message.is_some());
        assert!(app.status_message.as_ref().unwrap().contains("IRREVERSIBLE"));
    }

    #[test]
    fn test_discard_other_key_clears_pending() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.update(Message::DiscardFile);
        assert!(app.pending_discard.is_some());
        assert!(app.status_message.is_some());
        // Any non-discard message should clear pending
        app.update(Message::ScrollDiffDown);
        assert!(app.pending_discard.is_none());
        assert!(app.status_message.is_none());
    }

    #[test]
    fn test_discard_hunk_noop_in_staged_section() {
        let mut app = test_app_with_files(vec![staged_only_entry()]);
        app.focus = Focus::DiffView;
        app.current_hunk_index = Some(0);
        app.update(Message::DiscardHunk);
        assert!(app.pending_discard.is_none());
    }

    #[test]
    fn test_discard_hunk_first_press_sets_pending() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.focus = Focus::DiffView;
        app.current_hunk_index = Some(0);
        app.update(Message::DiscardHunk);
        assert_eq!(
            app.pending_discard,
            Some(PendingDiscard::Hunk { path: "unstaged.rs".to_string(), hunk_index: 0 }),
        );
        assert!(app.status_message.as_ref().unwrap().contains("IRREVERSIBLE"));
    }

    #[test]
    fn test_discard_hunk_noop_when_no_hunk_selected() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.focus = Focus::DiffView;
        app.current_hunk_index = None;
        app.update(Message::DiscardHunk);
        assert!(app.pending_discard.is_none());
    }

    #[test]
    fn test_discard_file_then_hunk_resets_pending() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.current_hunk_index = Some(0);

        // First: file discard pending
        app.update(Message::DiscardFile);
        assert!(matches!(app.pending_discard, Some(PendingDiscard::File { .. })));

        // Then: hunk discard — should replace pending (not confirm file discard)
        app.update(Message::DiscardHunk);
        assert!(matches!(app.pending_discard, Some(PendingDiscard::Hunk { .. })));
    }

    #[test]
    fn test_discard_hunk_noop_when_diff_stale() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.focus = Focus::DiffView;
        app.current_hunk_index = Some(0);
        app.diff_stale = true;
        app.update(Message::DiscardHunk);
        assert!(app.pending_discard.is_none());
    }

    // ---- full-file toggle tests ----

    use crate::diff::types::{DiffHunk, DiffLine};

    fn make_dc(hunks: Vec<DiffHunk>) -> DiffContent {
        DiffContent { path: "t.rs".to_string(), hunks, is_binary: false }
    }

    fn dl(kind: ChangeKind, old: Option<u32>, new: Option<u32>) -> DiffLine {
        DiffLine { kind, old_lineno: old, new_lineno: new, content: "x\n".to_string(), formatting_only: false }
    }

    #[test]
    fn test_diff_line_at_row_header_resolves_to_first_line() {
        let dc = make_dc(vec![DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![dl(ChangeKind::Equal, Some(1), Some(1)), dl(ChangeKind::Insert, None, Some(2))],
            has_header: true,
            header_context: None,
        }]);
        // row 0 = header → resolves to first line
        assert_eq!(diff_line_at_row(&dc, 0), Some((ChangeKind::Equal, Some(1), Some(1))));
        // row 1 = first content line
        assert_eq!(diff_line_at_row(&dc, 1), Some((ChangeKind::Equal, Some(1), Some(1))));
        // row 2 = second content line
        assert_eq!(diff_line_at_row(&dc, 2), Some((ChangeKind::Insert, None, Some(2))));
    }

    #[test]
    fn test_diff_line_at_row_past_end_returns_last() {
        let dc = make_dc(vec![DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![dl(ChangeKind::Equal, Some(1), Some(1))],
            has_header: true,
            header_context: None,
        }]);
        assert_eq!(diff_line_at_row(&dc, 999), Some((ChangeKind::Equal, Some(1), Some(1))));
    }

    #[test]
    fn test_diff_line_at_row_empty_or_binary_is_none() {
        let empty = make_dc(vec![]);
        assert_eq!(diff_line_at_row(&empty, 0), None);
        let mut bin = make_dc(vec![]);
        bin.is_binary = true;
        assert_eq!(diff_line_at_row(&bin, 0), None);
    }

    #[test]
    fn test_row_for_diff_line_finds_match() {
        let dc = make_dc(vec![DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![
                dl(ChangeKind::Equal, Some(1), Some(1)),
                dl(ChangeKind::Equal, Some(2), Some(2)),
                dl(ChangeKind::Insert, None, Some(3)),
            ],
            has_header: true,
            header_context: None,
        }]);
        // row layout: 0 header, 1 line1, 2 line2, 3 insert
        assert_eq!(row_for_diff_line(&dc, (ChangeKind::Equal, Some(2), Some(2))), Some(2));
        assert_eq!(row_for_diff_line(&dc, (ChangeKind::Insert, None, Some(3))), Some(3));
    }

    #[test]
    fn test_row_for_diff_line_returns_none_when_missing() {
        let dc = make_dc(vec![DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![dl(ChangeKind::Equal, Some(1), Some(1))],
            has_header: true,
            header_context: None,
        }]);
        // line 99 is not present in this diff
        assert_eq!(row_for_diff_line(&dc, (ChangeKind::Equal, Some(99), Some(99))), None);
    }

    #[test]
    fn test_compute_full_diff_includes_all_lines() {
        // 5-line file with one change in the middle: the change hunk's 3 lines
        // of context cover the whole file, so full-file mode produces a single
        // change hunk with no surrounding fillers.
        let old = "a\nb\nc\nd\ne\n";
        let new = "a\nb\nC\nd\ne\n";
        let dc = crate::diff::compute_full_diff_content("t.rs", Some(old), Some(new));
        assert_eq!(dc.hunks.len(), 1);
        let hunk = &dc.hunks[0];
        assert!(hunk.has_header);
        // Equal a, Equal b, Delete c, Insert C, Equal d, Equal e = 6 lines
        assert_eq!(hunk.lines.len(), 6);
        assert!(hunk.lines.iter().any(|l| l.kind == ChangeKind::Delete));
        assert!(hunk.lines.iter().any(|l| l.kind == ChangeKind::Insert));
        // First and last lines are unchanged context, far from the actual change
        assert_eq!(hunk.lines[0].kind, ChangeKind::Equal);
        assert_eq!(hunk.lines[0].old_lineno, Some(1));
        assert_eq!(hunk.lines.last().unwrap().new_lineno, Some(5));
    }

    #[test]
    fn test_compute_full_diff_keeps_change_hunks_in_place_with_fillers() {
        // 200-line file with one change at line 100. Full-file mode should
        // produce: leading filler (no header) + change hunk (with header) +
        // trailing filler (no header). The change hunk must keep its proper
        // position; the gap before/after is filled without hunk headers.
        let old: String = (1..=200).map(|n| format!("line{}\n", n)).collect();
        let new = old.replace("line100\n", "LINE100\n");

        let dc = crate::diff::compute_full_diff_content("t.rs", Some(&old), Some(&new));
        assert_eq!(dc.hunks.len(), 3);

        let leading = &dc.hunks[0];
        assert!(!leading.has_header);
        assert_eq!(leading.old_start, 1);
        assert_eq!(leading.new_start, 1);
        assert!(leading.lines.iter().all(|l| l.kind == ChangeKind::Equal));
        assert_eq!(leading.lines.first().unwrap().new_lineno, Some(1));
        assert_eq!(leading.lines.last().unwrap().new_lineno, Some(96));

        let change = &dc.hunks[1];
        assert!(change.has_header);
        assert_eq!(change.old_start, 97);
        assert_eq!(change.new_start, 97);
        assert!(change.lines.iter().any(|l| l.kind == ChangeKind::Delete));
        assert!(change.lines.iter().any(|l| l.kind == ChangeKind::Insert));

        let trailing = &dc.hunks[2];
        assert!(!trailing.has_header);
        assert!(trailing.lines.iter().all(|l| l.kind == ChangeKind::Equal));
        assert_eq!(trailing.lines.first().unwrap().new_lineno, Some(104));
        assert_eq!(trailing.lines.last().unwrap().new_lineno, Some(200));
    }

    /// Cursor preservation across mode toggle: a line visible in hunk mode
    /// maps to the same line identity in full-file mode.
    #[test]
    fn test_cursor_preservation_across_modes() {
        let old: String = (1..=200).map(|n| format!("line{}\n", n)).collect();
        let new = old.replace("line100\n", "LINE100\n");

        let hunk_dc = crate::diff::compute_diff_content("t.rs", Some(&old), Some(&new));
        let full_dc = crate::diff::compute_full_diff_content("t.rs", Some(&old), Some(&new));

        let target = (ChangeKind::Insert, None, Some(100u32));
        let hunk_row = row_for_diff_line(&hunk_dc, target).expect("insert in hunk mode");
        let full_row = row_for_diff_line(&full_dc, target).expect("insert in full mode");

        let cursor_key = diff_line_at_row(&hunk_dc, hunk_row).expect("cursor key");
        assert_eq!(cursor_key, target);

        let restored = row_for_diff_line(&full_dc, cursor_key).expect("found in full");
        assert_eq!(restored, full_row);

        let resolved = diff_line_at_row(&full_dc, restored);
        assert_eq!(resolved, Some(target));
    }

    /// When cursor is on a context-only line in full-file mode that doesn't
    /// exist in hunk mode, nearest_row_for_line finds the closest line.
    #[test]
    fn test_nearest_row_fallback() {
        let old: String = (1..=200).map(|n| format!("line{}\n", n)).collect();
        let new = old.replace("line100\n", "LINE100\n");

        let hunk_dc = crate::diff::compute_diff_content("t.rs", Some(&old), Some(&new));

        let far_away = (ChangeKind::Equal, Some(5u32), Some(5u32));
        assert!(row_for_diff_line(&hunk_dc, far_away).is_none());

        let nearest = nearest_row_for_line(&hunk_dc, far_away);
        assert!(nearest.is_some());
    }

    /// `Message::ToggleFullFile` flips the flag and reloads the diff (or clears
    /// it when no file is selected).
    #[test]
    fn test_toggle_full_file_flips_flag() {
        let mut app = test_app_with_files(vec![]);
        assert!(!app.show_full_file);
        app.update(Message::ToggleFullFile);
        assert!(app.show_full_file);
        app.update(Message::ToggleFullFile);
        assert!(!app.show_full_file);
    }

    #[test]
    fn test_scroll_positions_are_per_mode() {
        // Saved scroll for hunk mode should not bleed into full-file mode.
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.scroll_positions.insert(
            ("unstaged.rs".to_string(), SidebarSection::Unstaged, false),
            42,
        );
        app.scroll_positions.insert(
            ("unstaged.rs".to_string(), SidebarSection::Unstaged, true),
            7,
        );
        // Hunk mode reads false-keyed entry
        app.show_full_file = false;
        app.load_diff_for_selected();
        assert_eq!(app.diff_scroll, 42);
        // Full-file mode reads true-keyed entry
        app.show_full_file = true;
        app.load_diff_for_selected();
        assert_eq!(app.diff_scroll, 7);
    }

    // ---- active-hunk visibility tests ----

    /// Build a small App state with a single change hunk located somewhere in
    /// the rendered diff.
    #[test]
    fn test_update_hunk_from_cursor_first_hunk() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        let hunk1 = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![dl(ChangeKind::Insert, None, Some(1)); 5],
            has_header: true,
            header_context: None,
        };
        let hunk2 = DiffHunk {
            old_start: 10,
            new_start: 10,
            lines: vec![dl(ChangeKind::Insert, None, Some(10)); 3],
            has_header: true,
            header_context: None,
        };
        app.diff_content = Some(make_dc(vec![hunk1, hunk2]));
        app.diff_cursor = 0;
        app.update_hunk_from_cursor();
        assert_eq!(app.current_hunk_index, Some(0));
    }

    #[test]
    fn test_update_hunk_from_cursor_second_hunk() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        let hunk1 = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![dl(ChangeKind::Insert, None, Some(1)); 5],
            has_header: true,
            header_context: None,
        };
        let hunk2 = DiffHunk {
            old_start: 10,
            new_start: 10,
            lines: vec![dl(ChangeKind::Insert, None, Some(10)); 3],
            has_header: true,
            header_context: None,
        };
        app.diff_content = Some(make_dc(vec![hunk1, hunk2]));
        app.diff_cursor = 6;
        app.update_hunk_from_cursor();
        assert_eq!(app.current_hunk_index, Some(1));
    }

    #[test]
    fn test_update_hunk_from_cursor_last_line_of_last_hunk() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        let hunk1 = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![dl(ChangeKind::Insert, None, Some(1)); 5],
            has_header: true,
            header_context: None,
        };
        let hunk2 = DiffHunk {
            old_start: 10,
            new_start: 10,
            lines: vec![dl(ChangeKind::Insert, None, Some(10)); 3],
            has_header: true,
            header_context: None,
        };
        app.diff_content = Some(make_dc(vec![hunk1, hunk2]));
        app.diff_cursor = 7;
        app.update_hunk_from_cursor();
        assert_eq!(app.current_hunk_index, Some(1));
    }

    #[test]
    fn test_update_hunk_from_cursor_single_hunk() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        let hunk = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![dl(ChangeKind::Insert, None, Some(1)); 3],
            has_header: true,
            header_context: None,
        };
        app.diff_content = Some(make_dc(vec![hunk]));
        app.diff_cursor = 2;
        app.update_hunk_from_cursor();
        assert_eq!(app.current_hunk_index, Some(0));
    }

    #[test]
    fn test_update_hunk_from_cursor_no_content() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.diff_content = None;
        app.diff_cursor = 0;
        app.update_hunk_from_cursor();
        assert_eq!(app.current_hunk_index, None);
    }

    #[test]
    fn test_move_cursor_to_hunk() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        let hunk1 = DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![dl(ChangeKind::Insert, None, Some(1)); 5],
            has_header: true,
            header_context: None,
        };
        let hunk2 = DiffHunk {
            old_start: 10,
            new_start: 10,
            lines: vec![dl(ChangeKind::Insert, None, Some(10)); 3],
            has_header: true,
            header_context: None,
        };
        app.diff_content = Some(make_dc(vec![hunk1, hunk2]));
        app.move_cursor_to_hunk(1);
        assert_eq!(app.diff_cursor, 5);
        assert_eq!(app.current_hunk_index, Some(1));
    }

    #[test]
    fn test_stage_hunk_warns_when_no_active_hunk() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.focus = Focus::DiffView;
        app.current_hunk_index = None;
        app.update(Message::StageHunk);
        assert!(app.status_message.as_deref().unwrap_or("").contains("No active hunk"));
    }

    #[test]
    fn test_unstage_hunk_warns_when_no_active_hunk() {
        let mut app = test_app_with_files(vec![staged_only_entry()]);
        app.focus = Focus::DiffView;
        app.current_hunk_index = None;
        app.update(Message::UnstageHunk);
        assert!(app.status_message.as_deref().unwrap_or("").contains("No active hunk"));
    }

    #[test]
    fn test_discard_hunk_warns_and_clears_pending_when_no_active_hunk() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.focus = Focus::DiffView;
        // Simulate a stale pending discard from a prior hunk.
        app.pending_discard = Some(PendingDiscard::Hunk {
            path: "unstaged.rs".to_string(),
            hunk_index: 0,
        });
        app.current_hunk_index = None;
        app.update(Message::DiscardHunk);
        assert!(app.pending_discard.is_none());
        assert!(app.status_message.as_deref().unwrap_or("").contains("No active hunk"));
    }

    // ---- comment update logic tests ----

    #[test]
    fn test_start_comment_captures_context() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.focus = Focus::DiffView;
        app.current_hunk_index = Some(2);
        app.update(Message::StartComment);
        assert_eq!(app.focus, Focus::CommentInput);
        assert!(app.comment_context.is_some());
        let ctx = app.comment_context.as_ref().unwrap();
        assert_eq!(ctx.file_path, "unstaged.rs");
        assert_eq!(ctx.section, SidebarSection::Unstaged);
        assert_eq!(ctx.hunk_index, 2);
        assert!(app.comment_input.is_empty());
    }

    #[test]
    fn test_start_comment_ignored_when_no_hunk() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.focus = Focus::DiffView;
        app.current_hunk_index = None;
        app.update(Message::StartComment);
        assert_eq!(app.focus, Focus::DiffView);
        assert!(app.comment_context.is_none());
    }

    #[test]
    fn test_comment_input_char_and_backspace() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::CommentInput;
        app.update(Message::CommentInputChar('h'));
        app.update(Message::CommentInputChar('i'));
        assert_eq!(app.comment_input, "hi");
        app.update(Message::CommentInputBackspace);
        assert_eq!(app.comment_input, "h");
        app.update(Message::CommentInputBackspace);
        assert!(app.comment_input.is_empty());
        app.update(Message::CommentInputBackspace);
        assert!(app.comment_input.is_empty());
    }

    #[test]
    fn test_comment_input_cancel_clears_state() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.focus = Focus::CommentInput;
        app.comment_input = "partial".to_string();
        app.comment_context = Some(CommentContext {
            file_path: "unstaged.rs".to_string(),
            section: SidebarSection::Unstaged,
            hunk_index: 0,
            selected_lines: None,
        });
        app.update(Message::CommentInputCancel);
        assert_eq!(app.focus, Focus::DiffView);
        assert!(app.comment_input.is_empty());
        assert!(app.comment_context.is_none());
    }

    // ---- visual mode tests ----

    #[test]
    fn test_enter_visual_from_normal_in_diff_view() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.mode = AppMode::Normal;
        app.current_hunk_index = Some(0);
        app.diff_content = Some(make_dc(vec![DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![dl(ChangeKind::Equal, Some(1), Some(1)), dl(ChangeKind::Insert, None, Some(2))],
            has_header: true,
            header_context: None,
        }]));
        app.update(Message::EnterVisual);
        assert_eq!(app.mode, AppMode::Visual);
        assert!(!app.visual_selection.is_empty());
    }

    #[test]
    fn test_enter_visual_ignored_in_sidebar() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::Sidebar;
        app.mode = AppMode::Normal;
        app.update(Message::EnterVisual);
        assert_eq!(app.mode, AppMode::Normal);
        assert!(app.visual_selection.is_empty());
    }

    #[test]
    fn test_enter_visual_ignored_when_already_in_visual() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.mode = AppMode::Visual;
        app.visual_selection = vec![0, 1];
        app.update(Message::EnterVisual);
        assert_eq!(app.visual_selection, vec![0, 1]);
    }

    #[test]
    fn test_exit_visual_clears_selection() {
        let mut app = test_app_with_files(vec![]);
        app.mode = AppMode::Visual;
        app.visual_selection = vec![0, 1, 2];
        app.update(Message::ExitVisual);
        assert_eq!(app.mode, AppMode::Normal);
        assert!(app.visual_selection.is_empty());
    }

    #[test]
    fn test_exit_visual_ignored_in_normal_mode() {
        let mut app = test_app_with_files(vec![]);
        app.mode = AppMode::Normal;
        app.update(Message::ExitVisual);
        assert_eq!(app.mode, AppMode::Normal);
    }

    #[test]
    fn test_extend_selection_down() {
        let mut app = test_app_with_files(vec![]);
        app.mode = AppMode::Visual;
        app.visual_anchor = 0;
        app.visual_cursor = 0;
        app.diff_content = Some(make_dc(vec![DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                dl(ChangeKind::Equal, Some(1), Some(1)),
                dl(ChangeKind::Insert, None, Some(2)),
                dl(ChangeKind::Equal, Some(3), Some(3)),
            ],
            has_header: true,
            header_context: None,
        }]));
        app.update(Message::ExtendSelectionDown);
        assert_eq!(app.visual_cursor, 1);
        assert_eq!(app.visual_selection, vec![0, 1]);
        app.update(Message::ExtendSelectionDown);
        assert_eq!(app.visual_cursor, 2);
        assert_eq!(app.visual_selection, vec![0, 1, 2]);
    }

    #[test]
    fn test_extend_selection_up() {
        let mut app = test_app_with_files(vec![]);
        app.mode = AppMode::Visual;
        app.visual_anchor = 2;
        app.visual_cursor = 2;
        app.diff_content = Some(make_dc(vec![DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                dl(ChangeKind::Equal, Some(1), Some(1)),
                dl(ChangeKind::Insert, None, Some(2)),
                dl(ChangeKind::Equal, Some(3), Some(3)),
            ],
            has_header: true,
            header_context: None,
        }]));
        app.update(Message::ExtendSelectionUp);
        assert_eq!(app.visual_cursor, 1);
        assert_eq!(app.visual_selection, vec![1, 2]);
        app.update(Message::ExtendSelectionUp);
        assert_eq!(app.visual_cursor, 0);
        assert_eq!(app.visual_selection, vec![0, 1, 2]);
    }

    #[test]
    fn test_extend_selection_up_at_zero_stays() {
        let mut app = test_app_with_files(vec![]);
        app.mode = AppMode::Visual;
        app.visual_anchor = 0;
        app.visual_cursor = 0;
        app.diff_content = Some(make_dc(vec![DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![dl(ChangeKind::Equal, Some(1), Some(1))],
            has_header: true,
            header_context: None,
        }]));
        app.update(Message::ExtendSelectionUp);
        assert_eq!(app.visual_cursor, 0);
        assert_eq!(app.visual_selection, vec![0]);
    }

    #[test]
    fn test_extend_selection_down_at_max_stays() {
        let mut app = test_app_with_files(vec![]);
        app.mode = AppMode::Visual;
        app.visual_anchor = 2;
        app.visual_cursor = 2;
        app.diff_content = Some(make_dc(vec![DiffHunk {
            old_start: 1, new_start: 1,
            lines: vec![
                dl(ChangeKind::Equal, Some(1), Some(1)),
                dl(ChangeKind::Insert, None, Some(2)),
                dl(ChangeKind::Equal, Some(3), Some(3)),
            ],
            has_header: true,
            header_context: None,
        }]));
        app.update(Message::ExtendSelectionDown);
        app.update(Message::ExtendSelectionDown);
        assert_eq!(app.visual_cursor, 2);
    }

    #[test]
    fn test_extend_selection_ignored_in_normal_mode() {
        let mut app = test_app_with_files(vec![]);
        app.mode = AppMode::Normal;
        app.visual_cursor = 0;
        app.update(Message::ExtendSelectionDown);
        assert_eq!(app.visual_cursor, 0);
        app.update(Message::ExtendSelectionUp);
        assert_eq!(app.visual_cursor, 0);
    }

    #[test]
    fn test_extend_selection_down_ignored_when_no_diff_content() {
        let mut app = test_app_with_files(vec![]);
        app.mode = AppMode::Visual;
        app.visual_anchor = 0;
        app.visual_cursor = 0;
        app.diff_content = None;
        app.update(Message::ExtendSelectionDown);
        assert_eq!(app.visual_cursor, 0);
        assert!(app.visual_selection.is_empty());
    }

    #[test]
    fn test_extend_selection_up_ignored_when_no_diff_content() {
        let mut app = test_app_with_files(vec![]);
        app.mode = AppMode::Visual;
        app.visual_anchor = 2;
        app.visual_cursor = 2;
        app.diff_content = None;
        app.update(Message::ExtendSelectionUp);
        assert_eq!(app.visual_cursor, 2);
        assert!(app.visual_selection.is_empty());
    }

    // ---- semantic filter tests (Task 6) ----

    #[test]
    fn test_semantic_filter_defaults_to_false() {
        let app = test_app_with_files(vec![]);
        assert!(!app.semantic_filter);
    }

    #[test]
    fn test_toggle_semantic_filter_flips_state() {
        let mut app = test_app_with_files(vec![]);
        assert!(!app.semantic_filter);
        app.update(Message::ToggleSemanticFilter);
        assert!(app.semantic_filter);
        app.update(Message::ToggleSemanticFilter);
        assert!(!app.semantic_filter);
    }

    #[test]
    fn test_toggle_semantic_filter_available_in_sidebar() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::Sidebar;
        app.update(Message::ToggleSemanticFilter);
        assert!(app.semantic_filter);
    }

    #[test]
    fn test_toggle_semantic_filter_available_in_diff_view() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.update(Message::ToggleSemanticFilter);
        assert!(app.semantic_filter);
    }

    // ---- semantic filter navigation tests (Task 7) ----

    fn dl_fmt(kind: ChangeKind, old: Option<u32>, new: Option<u32>, formatting_only: bool) -> DiffLine {
        DiffLine { kind, old_lineno: old, new_lineno: new, content: "x\n".to_string(), formatting_only }
    }

    #[test]
    fn test_change_hunk_starts_skips_formatting_hunks_when_filter_on() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.diff_content = Some(make_dc(vec![
            DiffHunk { old_start: 1, new_start: 1, lines: vec![dl_fmt(ChangeKind::Insert, None, Some(1), true)], has_header: true, header_context: None },
            DiffHunk { old_start: 5, new_start: 5, lines: vec![dl_fmt(ChangeKind::Delete, Some(5), None, false)], has_header: true, header_context: None },
            DiffHunk { old_start: 10, new_start: 10, lines: vec![dl_fmt(ChangeKind::Insert, None, Some(10), true)], has_header: true, header_context: None },
        ]));

        // Filter off: all 3 hunks visible
        app.semantic_filter = false;
        let starts_off = app.change_hunk_starts();
        assert_eq!(starts_off.len(), 3);

        // Filter on: only the semantic hunk (index 1) visible
        app.semantic_filter = true;
        let starts_on = app.change_hunk_starts();
        assert_eq!(starts_on.len(), 1);
        assert_eq!(starts_on[0].0, 1);
    }

    #[test]
    fn test_next_hunk_skips_formatting_hunks_when_filter_on() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.diff_content = Some(make_dc(vec![
            DiffHunk { old_start: 1, new_start: 1, lines: vec![dl_fmt(ChangeKind::Insert, None, Some(1), true); 2], has_header: true, header_context: None },
            DiffHunk { old_start: 5, new_start: 5, lines: vec![dl_fmt(ChangeKind::Delete, Some(5), None, false); 3], has_header: true, header_context: None },
        ]));
        app.semantic_filter = true;
        app.diff_scroll = 0;

        // Next hunk should skip the formatting hunk and go to the semantic one.
        // The formatting hunk is hidden (0 rows), so the semantic hunk starts at row 0.
        app.update(Message::NextHunk);
        assert_eq!(app.diff_scroll, 0);
    }

    #[test]
    fn test_prev_hunk_skips_formatting_hunks_when_filter_on() {
        let mut app = test_app_with_files(vec![]);
        app.focus = Focus::DiffView;
        app.diff_content = Some(make_dc(vec![
            DiffHunk { old_start: 1, new_start: 1, lines: vec![dl_fmt(ChangeKind::Delete, Some(1), None, false); 3], has_header: true, header_context: None },
            DiffHunk { old_start: 5, new_start: 5, lines: vec![dl_fmt(ChangeKind::Insert, None, Some(5), true); 2], has_header: true, header_context: None },
        ]));
        app.semantic_filter = true;
        app.diff_scroll = 4; // past the second hunk

        // Prev hunk should skip the formatting hunk and go to the semantic one
        app.update(Message::PrevHunk);
        assert_eq!(app.diff_scroll, 0);
    }

    // ── hunk_counts tests (Task 8) ──────────────────────────────────

    #[test]
    fn test_hunk_counts_none_when_no_diff_content() {
        let app = test_app_with_files(vec![]);
        assert!(app.hunk_counts().is_none());
    }

    #[test]
    fn test_hunk_counts_none_when_binary() {
        let mut app = test_app_with_files(vec![]);
        app.diff_content = Some(DiffContent {
            path: "img.png".to_string(),
            hunks: vec![],
            is_binary: true,
        });
        assert!(app.hunk_counts().is_none());
    }

    #[test]
    fn test_hunk_counts_none_when_empty_hunks() {
        let mut app = test_app_with_files(vec![]);
        app.diff_content = Some(DiffContent {
            path: "t.rs".to_string(),
            hunks: vec![],
            is_binary: false,
        });
        assert!(app.hunk_counts().is_none());
    }

    #[test]
    fn test_hunk_counts_all_visible_when_filter_off() {
        let mut app = test_app_with_files(vec![]);
        app.diff_content = Some(make_dc(vec![
            DiffHunk { old_start: 1, new_start: 1, lines: vec![dl_fmt(ChangeKind::Insert, None, Some(1), true)], has_header: true, header_context: None },
            DiffHunk { old_start: 5, new_start: 5, lines: vec![dl_fmt(ChangeKind::Delete, Some(5), None, false)], has_header: true, header_context: None },
        ]));
        app.semantic_filter = false;
        assert_eq!(app.hunk_counts(), Some((2, 2, 0)));
    }

    #[test]
    fn test_hunk_counts_hides_formatting_when_filter_on() {
        let mut app = test_app_with_files(vec![]);
        app.diff_content = Some(make_dc(vec![
            DiffHunk { old_start: 1, new_start: 1, lines: vec![dl_fmt(ChangeKind::Insert, None, Some(1), true)], has_header: true, header_context: None },
            DiffHunk { old_start: 5, new_start: 5, lines: vec![dl_fmt(ChangeKind::Delete, Some(5), None, false)], has_header: true, header_context: None },
            DiffHunk { old_start: 10, new_start: 10, lines: vec![dl_fmt(ChangeKind::Insert, None, Some(10), true)], has_header: true, header_context: None },
        ]));
        app.semantic_filter = true;
        assert_eq!(app.hunk_counts(), Some((1, 3, 2)));
    }

    #[test]
    fn test_hunk_counts_all_formatting_hidden() {
        let mut app = test_app_with_files(vec![]);
        app.diff_content = Some(make_dc(vec![
            DiffHunk { old_start: 1, new_start: 1, lines: vec![dl_fmt(ChangeKind::Insert, None, Some(1), true)], has_header: true, header_context: None },
            DiffHunk { old_start: 5, new_start: 5, lines: vec![dl_fmt(ChangeKind::Delete, Some(5), None, true)], has_header: true, header_context: None },
        ]));
        app.semantic_filter = true;
        assert_eq!(app.hunk_counts(), Some((0, 2, 2)));
    }

    // ── Task 4: classify_diff wired into load_diff_for_selected ──────

    #[test]
    fn test_classify_diff_integration_whitespace_change_marked_formatting() {
        // Simulates what load_diff_for_selected does: compute diff, then classify.
        // A whitespace-only change should have formatting_only = true after classification.
        let old = "fn foo() {\nlet x=1;\n}\n";
        let new = "fn foo() {\n    let x = 1;\n}\n";
        let mut dc = crate::diff::compute_diff_content("t.rs", Some(old), Some(new));

        let lang = crate::classify::language_for_extension("rs");
        crate::classify::classify_diff(&mut dc.hunks, old, new, lang, "rs");

        let changed: Vec<_> = dc.hunks[0].lines.iter().filter(|l| l.kind != ChangeKind::Equal).collect();
        assert!(
            !changed.is_empty(),
            "diff should have changed lines"
        );
        assert!(
            changed.iter().all(|l| l.formatting_only),
            "whitespace-only changes should be formatting_only after classification: {:?}",
            changed.iter().map(|l| (&l.content, l.formatting_only)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_classify_diff_integration_semantic_change_not_formatting() {
        let old = "let x = 1;\n";
        let new = "let y = 1;\n";
        let mut dc = crate::diff::compute_diff_content("t.rs", Some(old), Some(new));

        let lang = crate::classify::language_for_extension("rs");
        crate::classify::classify_diff(&mut dc.hunks, old, new, lang, "rs");

        let changed: Vec<_> = dc.hunks[0].lines.iter().filter(|l| l.kind != ChangeKind::Equal).collect();
        assert!(
            changed.iter().all(|l| !l.formatting_only),
            "semantic changes should NOT be formatting_only after classification"
        );
    }

    #[test]
    fn test_classify_diff_integration_unknown_extension_skips() {
        let old = "hello world\n";
        let new = "hello  world\n";
        let mut dc = crate::diff::compute_diff_content("README.txt", Some(old), Some(new));

        let lang = crate::classify::language_for_extension("txt");
        crate::classify::classify_diff(&mut dc.hunks, old, new, lang, "txt");

        // Unknown language: all lines should remain non-formatting
        let changed: Vec<_> = dc.hunks.iter().flat_map(|h| h.lines.iter()).filter(|l| l.kind != ChangeKind::Equal).collect();
        assert!(
            changed.iter().all(|l| !l.formatting_only),
            "unknown language should leave all lines as non-formatting"
        );
    }

    // ---- sidebar formatting indicator tests (Task 10) ----

    #[test]
    fn test_formatting_only_cache_populated_for_formatting_changes() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        // Simulate loading a diff with only formatting changes
        let old = "fn foo() {\nlet x=1;\n}\n";
        let new = "fn foo() {\n    let x = 1;\n}\n";
        let mut dc = crate::diff::compute_diff_content("unstaged.rs", Some(old), Some(new));
        let lang = crate::classify::language_for_extension("rs");
        crate::classify::classify_diff(&mut dc.hunks, old, new, lang, "rs");
        app.diff_content = Some(dc);

        // After classification, all hunks should be formatting-only
        let all_formatting = app.diff_content.as_ref().unwrap().hunks.iter().all(|h| h.is_formatting_only());
        assert!(all_formatting, "whitespace-only changes should be formatting-only");

        // Manually populate the cache as load_diff_for_selected would
        app.formatting_only_cache.insert(("unstaged.rs".to_string(), SidebarSection::Unstaged), all_formatting);
        assert_eq!(app.formatting_only_cache.get(&("unstaged.rs".to_string(), SidebarSection::Unstaged)), Some(&true));
    }

    #[test]
    fn test_formatting_only_cache_populated_for_semantic_changes() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        let old = "let x = 1;\n";
        let new = "let y = 2;\n";
        let mut dc = crate::diff::compute_diff_content("unstaged.rs", Some(old), Some(new));
        let lang = crate::classify::language_for_extension("rs");
        crate::classify::classify_diff(&mut dc.hunks, old, new, lang, "rs");
        app.diff_content = Some(dc);

        let all_formatting = app.diff_content.as_ref().unwrap().hunks.iter().all(|h| h.is_formatting_only());
        assert!(!all_formatting, "semantic changes should NOT be formatting-only");

        app.formatting_only_cache.insert(("unstaged.rs".to_string(), SidebarSection::Unstaged), all_formatting);
        assert_eq!(app.formatting_only_cache.get(&("unstaged.rs".to_string(), SidebarSection::Unstaged)), Some(&false));
    }

    #[test]
    fn test_formatting_only_cache_cleared_on_refresh() {
        let mut app = test_app_with_files(vec![unstaged_entry()]);
        app.formatting_only_cache.insert(("unstaged.rs".to_string(), SidebarSection::Unstaged), true);
        assert!(app.formatting_only_cache.contains_key(&("unstaged.rs".to_string(), SidebarSection::Unstaged)));

        app.refresh_file_list();
        assert!(!app.formatting_only_cache.contains_key(&("unstaged.rs".to_string(), SidebarSection::Unstaged)));
    }
