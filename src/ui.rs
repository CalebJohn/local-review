use std::collections::HashMap;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::app::{App, AppMode, Focus, SidebarSection};
use crate::diff::types::{ChangeKind, DiffContent};
use crate::git::types::{FileEntry, FileStatus};
use crate::syntax::StyledDiffContent;

pub fn view(frame: &mut ratatui::Frame, app: &App) {
    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());
    if app.sidebar_collapsed {
        render_diff_view(frame, app, rows[0]);
    } else {
        let cols = Layout::horizontal([Constraint::Length(30), Constraint::Min(1)])
            .split(rows[0]);
        render_sidebar(frame, app, cols[0]);
        render_diff_view(frame, app, cols[1]);
    }
    render_footer(frame, app, rows[1]);
}

/// Returns the Rect areas for staged and unstaged sections within the sidebar.
/// Exported for mouse hit-testing in main.rs.
pub fn sidebar_section_areas(sidebar_area: Rect, staged_count: usize, unstaged_count: usize) -> (Rect, Rect) {
    let total = staged_count + unstaged_count;
    if total == 0 {
        // Split evenly when both empty
        let halves = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(sidebar_area);
        return (halves[0], halves[1]);
    }

    // Each section gets at least 3 rows (border + title + 1 content line)
    // Remaining space is proportional to file count
    let min_rows: u16 = 3;
    let available = sidebar_area.height;

    if available < min_rows * 2 {
        // Not enough space for two sections, split evenly
        let halves = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(sidebar_area);
        return (halves[0], halves[1]);
    }

    let staged_content = staged_count as u16 + 2; // +2 for borders
    let unstaged_content = unstaged_count as u16 + 2;
    let total_wanted = staged_content + unstaged_content;

    let (staged_h, unstaged_h) = if total_wanted <= available {
        // Both fit - give each what it needs, remaining to unstaged (grows with unreviewed files)
        (staged_content, available - staged_content)
    } else {
        // Proportional split with minimum
        let staged_ratio = staged_count as f32 / total as f32;
        let staged_h = ((available as f32 * staged_ratio) as u16).max(min_rows);
        let unstaged_h = available.saturating_sub(staged_h).max(min_rows);
        let staged_h = available.saturating_sub(unstaged_h); // adjust if unstaged took min
        (staged_h, unstaged_h)
    };

    let sections = Layout::vertical([Constraint::Length(staged_h), Constraint::Length(unstaged_h)])
        .split(sidebar_area);
    (sections[0], sections[1])
}

fn status_style(entry: &FileEntry) -> Style {
    // Prefer workdir_status for coloring, fall back to index_status.
    let status = entry
        .workdir_status
        .or(entry.index_status);
    match status {
        Some(FileStatus::Modified) => Style::default().fg(Color::Yellow),
        Some(FileStatus::Added) => Style::default().fg(Color::Green),
        Some(FileStatus::Deleted) => Style::default().fg(Color::Red),
        Some(FileStatus::Renamed) => Style::default().fg(Color::Cyan),
        Some(FileStatus::Untracked) => Style::default().fg(Color::DarkGray),
        None => Style::default(),
    }
}

fn border_color(focused: bool) -> Color {
    if focused {
        Color::Blue
    } else {
        Color::DarkGray
    }
}

fn render_sidebar(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let sidebar_focused = app.focus == Focus::Sidebar;
    let (staged_area, unstaged_area) =
        sidebar_section_areas(area, app.staged_files.len(), app.unstaged_files.len());

    // Staged section
    render_file_list(
        frame,
        &app.staged_files,
        "Staged",
        staged_area,
        sidebar_focused && app.sidebar_section == SidebarSection::Staged,
        if app.sidebar_section == SidebarSection::Staged {
            Some(app.selected_index)
        } else {
            None
        },
        &app.formatting_only_cache,
        SidebarSection::Staged,
    );

    // Unstaged section
    render_file_list(
        frame,
        &app.unstaged_files,
        "Unstaged",
        unstaged_area,
        sidebar_focused && app.sidebar_section == SidebarSection::Unstaged,
        if app.sidebar_section == SidebarSection::Unstaged {
            Some(app.selected_index)
        } else {
            None
        },
        &app.formatting_only_cache,
        SidebarSection::Unstaged,
    );
}

fn render_file_list(
    frame: &mut ratatui::Frame,
    files: &[FileEntry],
    title: &str,
    area: Rect,
    focused: bool,
    selected: Option<usize>,
    formatting_only_cache: &HashMap<(String, SidebarSection), bool>,
    section: SidebarSection,
) {
    let items: Vec<ListItem> = files
        .iter()
        .map(|entry| {
            let status = entry.display_status();
            let is_formatting_only = formatting_only_cache.get(&(entry.path.clone(), section)).copied().unwrap_or(false);
            let mut style = status_style(entry);
            if is_formatting_only {
                style = style.add_modifier(Modifier::DIM);
            }
            let line = Line::from(vec![
                Span::styled(format!("{} ", status), style),
                Span::styled(entry.path.clone(), style),
            ]);
            ListItem::new(line)
        })
        .collect();

    let count = files.len();
    let title_text = format!("{} ({})", title, count);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title_text)
        .border_style(Style::default().fg(border_color(focused)));

    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );

    let mut state = ListState::default();
    if selected.is_some() {
        state.select(selected);
    }

    frame.render_stateful_widget(list, area, &mut state);
}

fn hunk_header_line(old_start: u32, new_start: u32, highlighted: bool) -> Line<'static> {
    let gutter = Span::raw(if highlighted { "│" } else { " " });
    Line::from(vec![
        gutter,
        Span::styled(
            format!("@@ -{} +{} @@", old_start, new_start),
            Style::default().fg(Color::Cyan),
        ),
    ])
}

fn format_lineno(n: Option<u32>) -> String {
    match n {
        Some(v) => format!("{:>4}", v),
        None => "    ".to_string(),
    }
}

fn apply_cursor_selection_style(line: Line<'static>, is_cursor: bool, is_selected: bool) -> Line<'static> {
    if is_cursor {
        Line::from(line.spans.into_iter().map(|s| s.style(Style::default().bg(Color::Black))).collect::<Vec<_>>())
    } else if is_selected {
        Line::from(line.spans.into_iter().map(|s| s.style(Style::default().bg(Color::Blue))).collect::<Vec<_>>())
    } else {
        line
    }
}

fn diff_lines(
    diff: &DiffContent,
    styled: Option<&StyledDiffContent>,
    current_hunk_index: Option<usize>,
    visual_selection: &[usize],
    diff_cursor: usize,
    mode: &AppMode,
    semantic_filter: bool,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut global_line_idx: usize = 0;
    for (hunk_idx, hunk) in diff.hunks.iter().enumerate() {
        if semantic_filter && hunk.is_formatting_only() {
            continue;
        }
        let in_hunk = hunk.has_header && current_hunk_index == Some(hunk_idx);
        if hunk.has_header {
            lines.push(hunk_header_line(hunk.old_start, hunk.new_start, in_hunk));
        }
        let gutter = if in_hunk { "│" } else { " " };
        for dl in &hunk.lines {
            let content = dl.content.trim_end_matches('\n').to_string();
            let (prefix, content_style) = match dl.kind {
                ChangeKind::Equal  => (" ", Style::default()),
                ChangeKind::Insert => ("+", Style::default().fg(Color::Green)),
                ChangeKind::Delete => ("-", Style::default().fg(Color::Red)),
            };

            let is_selected = match (visual_selection.first(), visual_selection.last()) {
                (Some(&start), Some(&end)) => global_line_idx >= start && global_line_idx <= end,
                _ => false,
            };
            let is_cursor = mode == &AppMode::Normal && global_line_idx == diff_cursor;
            let gutter_style = if is_selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else if is_cursor {
                Style::default().fg(Color::White).add_modifier(Modifier::REVERSED)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let lineno_span = Span::styled(
                format!("{} {} ", format_lineno(dl.old_lineno), format_lineno(dl.new_lineno)),
                gutter_style,
            );

            let styled_line = styled.and_then(|sd| match dl.kind {
                ChangeKind::Equal => dl.new_lineno
                    .and_then(|ln| sd.lines_by_new_lineno.get(&ln))
                    .or_else(|| dl.old_lineno.and_then(|ln| sd.lines_by_old_lineno.get(&ln))),
                _ => None,
            });

            let gutter_span = Span::raw(gutter);
            let line = if let Some(spans) = styled_line {
                let prefix_span = Span::styled(prefix.to_string(), Style::default());
                let mut parts: Vec<Span<'static>> = Vec::with_capacity(3 + spans.len());
                parts.push(gutter_span);
                parts.push(lineno_span);
                parts.push(prefix_span);
                for sp in spans {
                    parts.push(Span::styled(sp.text.clone(), sp.style));
                }
                Line::from(parts)
            } else {
                let body_style = if dl.formatting_only && dl.kind != ChangeKind::Equal {
                    content_style.add_modifier(Modifier::DIM)
                } else {
                    content_style
                };
                let body_span = Span::styled(format!("{}{}", prefix, content), body_style);
                Line::from(vec![gutter_span, lineno_span, body_span])
            };

            lines.push(apply_cursor_selection_style(line, is_cursor, is_selected));
            global_line_idx += 1;
        }
    }
    lines
}

fn render_footer(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let key_style = Style::default().fg(Color::White).bg(Color::DarkGray).add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::Gray);
    let sep = Span::styled("  ", desc_style);

    if let Some(ref msg) = app.status_message {
        let footer = Paragraph::new(Line::from(vec![Span::styled(msg.clone(), Style::default().fg(Color::Red))]));
        frame.render_widget(footer, area);
        return;
    }

    let in_staged = app.sidebar_section == SidebarSection::Staged;
    let full_file_label = if app.show_full_file { " hunks only " } else { " full file " };

    let mut spans: Vec<Span> = Vec::new();
    match app.focus {
        Focus::Sidebar => {
            spans.extend([Span::styled(" j/k ", key_style), Span::styled(" navigate ", desc_style), sep.clone()]);
            if in_staged {
                spans.extend([Span::styled(" u ", key_style), Span::styled(" unstage file ", desc_style), sep.clone()]);
            } else {
                spans.extend([Span::styled(" s ", key_style), Span::styled(" stage file ", desc_style), sep.clone()]);
                spans.extend([Span::styled(" d ", key_style), Span::styled(" discard file ", desc_style), sep.clone()]);
            }
            spans.extend([Span::styled(" Enter ", key_style), Span::styled(" open diff ", desc_style), sep.clone()]);
            spans.extend([Span::styled(" e ", key_style), Span::styled(" edit ", desc_style), sep.clone()]);
            spans.extend([Span::styled(" f ", key_style), Span::styled(full_file_label, desc_style), sep.clone()]);
            spans.extend([Span::styled(" b ", key_style), Span::styled(" hide sidebar ", desc_style), sep.clone()]);
            spans.extend([Span::styled(" Tab ", key_style), Span::styled(" switch pane ", desc_style), sep]);
            spans.extend([Span::styled(" q ", key_style), Span::styled(" quit ", desc_style)]);
        }
        Focus::DiffView if app.diff_stale => {
            let warn_style = Style::default().fg(Color::Yellow);
            spans.extend([Span::styled("file changed", warn_style), sep.clone()]);
            spans.extend([Span::styled(" r ", key_style), Span::styled(" reload diff ", desc_style), sep.clone()]);
            spans.extend([Span::styled(" b ", key_style), Span::styled(if app.sidebar_collapsed { " show sidebar " } else { " hide sidebar " }, desc_style), sep.clone()]);
            spans.extend([Span::styled(" Tab ", key_style), Span::styled(" switch pane ", desc_style), sep]);
            spans.extend([Span::styled(" q ", key_style), Span::styled(" quit ", desc_style)]);
        }
        Focus::DiffView => {
            if app.mode == AppMode::Visual {
                spans.extend([Span::styled(" [VISUAL] ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)), sep.clone()]);
            }
            spans.extend([Span::styled(" n ", key_style), Span::styled(" next hunk ", desc_style), sep.clone()]);
            spans.extend([Span::styled(" N ", key_style), Span::styled(" prev hunk ", desc_style), sep.clone()]);
            if in_staged {
                spans.extend([Span::styled(" u ", key_style), Span::styled(" unstage hunk ", desc_style), sep.clone()]);
                spans.extend([Span::styled(" U ", key_style), Span::styled(" unstage file ", desc_style), sep.clone()]);
            } else {
                spans.extend([Span::styled(" s ", key_style), Span::styled(" stage hunk ", desc_style), sep.clone()]);
                spans.extend([Span::styled(" S ", key_style), Span::styled(" stage file ", desc_style), sep.clone()]);
                spans.extend([Span::styled(" d ", key_style), Span::styled(" discard hunk ", desc_style), sep.clone()]);
                spans.extend([Span::styled(" D ", key_style), Span::styled(" discard file ", desc_style), sep.clone()]);
            }
            spans.extend([Span::styled(" w ", key_style), Span::styled(if app.semantic_filter { " show all " } else { " whitespace " }, desc_style), sep.clone()]);
            if app.semantic_filter {
                if let Some((visible, total, hidden)) = app.hunk_counts() {
                    spans.extend([Span::styled(
                        format!(" {}/{} ({} hidden) ", visible, total, hidden),
                        Style::default().fg(Color::Cyan),
                    ), sep.clone()]);
                }
            }
            spans.extend([Span::styled(" c ", key_style), Span::styled(" comment ", desc_style), sep.clone()]);
            spans.extend([Span::styled(" j/k ", key_style), Span::styled(" navigate ", desc_style), sep.clone()]);
            spans.extend([Span::styled(" e ", key_style), Span::styled(" edit ", desc_style), sep.clone()]);
            spans.extend([Span::styled(" f ", key_style), Span::styled(full_file_label, desc_style), sep.clone()]);
            spans.extend([Span::styled(" b ", key_style), Span::styled(if app.sidebar_collapsed { " show sidebar " } else { " hide sidebar " }, desc_style), sep.clone()]);
            spans.extend([Span::styled(" Tab ", key_style), Span::styled(" switch pane ", desc_style), sep]);
            spans.extend([Span::styled(" q ", key_style), Span::styled(" quit ", desc_style)]);
        }
        Focus::CommentInput => {
            spans.extend([
                Span::styled("comment: ", Style::default().fg(Color::White)),
                Span::raw(&app.comment_input),
                Span::raw("\u{2588}"),
            ]);
        }
    }

    let footer = Paragraph::new(Line::from(spans));
    frame.render_widget(footer, area);
}

fn render_diff_view(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let focused = matches!(app.focus, Focus::DiffView | Focus::CommentInput);
    let path = app
        .diff_content
        .as_ref()
        .map(|dc| dc.path.clone())
        .unwrap_or_else(|| "Diff".to_string());

    let title = if app.diff_stale {
        Line::from(vec![
            Span::raw(path),
            Span::styled(" file changed -- press r to reload ", Style::default().fg(Color::Yellow)),
        ])
    } else {
        Line::from(path)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(border_color(focused)));

    match &app.diff_content {
        None => {
            let paragraph = Paragraph::new(Line::from(Span::styled(
                "Select a file to view diff",
                Style::default().fg(Color::DarkGray),
            )))
            .block(block)
            .alignment(Alignment::Center);
            frame.render_widget(paragraph, area);
        }
        Some(dc) if dc.is_binary => {
            let paragraph = Paragraph::new(Line::from(Span::styled(
                "Binary file (not shown)",
                Style::default().fg(Color::Yellow),
            )))
            .block(block)
            .alignment(Alignment::Center);
            frame.render_widget(paragraph, area);
        }
        Some(dc) => {
            let inner = block.inner(area);
            app.diff_viewport_height.set(inner.height);
            let lines = diff_lines(dc, app.styled_diff.as_ref(), app.current_hunk_index, &app.visual_selection, app.diff_cursor, &app.mode, app.semantic_filter);
            if lines.is_empty() && app.semantic_filter {
                let paragraph = Paragraph::new(Line::from(Span::styled(
                    "All changes are formatting-only",
                    Style::default().fg(Color::DarkGray),
                )))
                .block(block)
                .alignment(Alignment::Center);
                frame.render_widget(paragraph, area);
            } else {
                let paragraph = Paragraph::new(lines)
                    .block(block)
                    .scroll((app.diff_scroll, 0));
                frame.render_widget(paragraph, area);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::types::{DiffHunk, DiffLine};

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
        }]);
        let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, false);
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
        }]);
        let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, false);
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
        }]);
        let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, false);
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
        }]);
        let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, false);
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
        }]);
        let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, false);
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
        }]);
        let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, false);
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
            },
        ]);
        // With filter off: hunk renders (header + content)
        let lines_off = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, false);
        assert_eq!(lines_off.len(), 2);

        // With filter on: pure-formatting hunk is hidden
        let lines_on = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, true);
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
        }]);
        // Mixed hunk should still show with filter on
        let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, true);
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
        }]);
        let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, true);
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
            },
            DiffHunk {
                old_start: 5,
                new_start: 5,
                lines: vec![
                    dl(ChangeKind::Delete, Some(5), None, false),
                    dl(ChangeKind::Insert, None, Some(5), false),
                ],
                has_header: true,
            },
            DiffHunk {
                old_start: 10,
                new_start: 10,
                lines: vec![
                    dl(ChangeKind::Insert, None, Some(10), true),
                ],
                has_header: true,
            },
        ]);
        // With filter on: only the middle semantic hunk should render
        let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, true);
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
            },
            DiffHunk {
                old_start: 5,
                new_start: 5,
                lines: vec![dl(ChangeKind::Delete, Some(5), None, true)],
                has_header: true,
            },
        ]);
        let lines = diff_lines(&dc, None, Some(0), &[], 99, &AppMode::Normal, true);
        assert!(lines.is_empty(), "all formatting hunks should produce no lines");
    }
}
