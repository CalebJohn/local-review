use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::app::{App, Focus, SidebarSection};
use crate::diff::types::{ChangeKind, DiffContent};
use crate::git::types::{FileEntry, FileStatus};
use crate::syntax::{StyledDiffContent, StyledSpan};

pub fn view(frame: &mut ratatui::Frame, app: &App) {
    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());
    let cols = Layout::horizontal([Constraint::Length(30), Constraint::Min(1)])
        .split(rows[0]);
    render_sidebar(frame, app, cols[0]);
    render_diff_view(frame, app, cols[1]);
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
    );
}

fn render_file_list(
    frame: &mut ratatui::Frame,
    files: &[FileEntry],
    title: &str,
    area: Rect,
    focused: bool,
    selected: Option<usize>,
) {
    let items: Vec<ListItem> = files
        .iter()
        .map(|entry| {
            let status = entry.display_status();
            let line = Line::from(vec![
                Span::styled(format!("{} ", status), status_style(entry)),
                Span::raw(entry.path.clone()),
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

fn diff_lines(diff: &DiffContent, current_hunk_index: Option<usize>) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (hunk_idx, hunk) in diff.hunks.iter().enumerate() {
        let in_hunk = current_hunk_index == Some(hunk_idx);
        lines.push(hunk_header_line(hunk.old_start, hunk.new_start, in_hunk));
        let gutter = if in_hunk { "│" } else { " " };
        for dl in &hunk.lines {
            let content = dl.content.trim_end_matches('\n').to_string();
            let (prefix, content_style) = match dl.kind {
                ChangeKind::Equal => (" ", Style::default()),
                ChangeKind::Insert => ("+", Style::default().fg(Color::Green)),
                ChangeKind::Delete => ("-", Style::default().fg(Color::Red)),
            };

            let lineno_span = Span::styled(
                format!("{} {} ", format_lineno(dl.old_lineno), format_lineno(dl.new_lineno)),
                Style::default().fg(Color::DarkGray),
            );
            let body_span = Span::styled(format!("{}{}", prefix, content), content_style);

            lines.push(Line::from(vec![Span::raw(gutter), lineno_span, body_span]));
        }
    }
    lines
}

fn diff_lines_styled(
    diff: &DiffContent,
    styled: &StyledDiffContent,
    current_hunk_index: Option<usize>,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (hunk_idx, hunk) in diff.hunks.iter().enumerate() {
        let in_hunk = current_hunk_index == Some(hunk_idx);
        lines.push(hunk_header_line(hunk.old_start, hunk.new_start, in_hunk));
        let gutter = if in_hunk { "│" } else { " " };
        for dl in &hunk.lines {
            let content = dl.content.trim_end_matches('\n').to_string();
            let (prefix, content_style) = match dl.kind {
                ChangeKind::Equal  => (" ", Style::default()),
                ChangeKind::Insert => ("+", Style::default().fg(Color::Green)),
                ChangeKind::Delete => ("-", Style::default().fg(Color::Red)),
            };

            let lineno_span = Span::styled(
                format!("{} {} ", format_lineno(dl.old_lineno), format_lineno(dl.new_lineno)),
                Style::default().fg(Color::DarkGray),
            );

            // Only apply syntax styling on Equal lines. Keep +/- as single-span full-line color.
            let styled_line: Option<&Vec<StyledSpan>> = match dl.kind {
                ChangeKind::Equal => dl.new_lineno
                    .and_then(|ln| styled.lines_by_new_lineno.get(&ln))
                    .or_else(|| dl.old_lineno.and_then(|ln| styled.lines_by_old_lineno.get(&ln))),
                _ => None,
            };

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
                let body_span = Span::styled(format!("{}{}", prefix, content), content_style);
                Line::from(vec![gutter_span, lineno_span, body_span])
            };

            lines.push(line);
        }
    }
    lines
}

fn render_footer(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let key_style = Style::default().fg(Color::White).bg(Color::DarkGray).add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::Gray);
    let sep = Span::styled("  ", desc_style);

    let in_staged = app.sidebar_section == SidebarSection::Staged;

    let mut spans: Vec<Span> = Vec::new();
    match app.focus {
        Focus::Sidebar => {
            spans.extend([Span::styled(" j/k ", key_style), Span::styled(" navigate ", desc_style), sep.clone()]);
            if in_staged {
                spans.extend([Span::styled(" u ", key_style), Span::styled(" unstage file ", desc_style), sep.clone()]);
            } else {
                spans.extend([Span::styled(" s ", key_style), Span::styled(" stage file ", desc_style), sep.clone()]);
            }
            spans.extend([Span::styled(" Enter ", key_style), Span::styled(" open diff ", desc_style), sep.clone()]);
            spans.extend([Span::styled(" Tab ", key_style), Span::styled(" switch pane ", desc_style), sep]);
            spans.extend([Span::styled(" q ", key_style), Span::styled(" quit ", desc_style)]);
        }
        Focus::DiffView => {
            spans.extend([Span::styled(" n ", key_style), Span::styled(" next hunk ", desc_style), sep.clone()]);
            spans.extend([Span::styled(" N ", key_style), Span::styled(" prev hunk ", desc_style), sep.clone()]);
            if in_staged {
                spans.extend([Span::styled(" u ", key_style), Span::styled(" unstage hunk ", desc_style), sep.clone()]);
            } else {
                spans.extend([Span::styled(" s ", key_style), Span::styled(" stage hunk ", desc_style), sep.clone()]);
            }
            spans.extend([Span::styled(" j/k ", key_style), Span::styled(" scroll ", desc_style), sep.clone()]);
            spans.extend([Span::styled(" Tab ", key_style), Span::styled(" switch pane ", desc_style), sep]);
            spans.extend([Span::styled(" q ", key_style), Span::styled(" quit ", desc_style)]);
        }
    }

    let footer = Paragraph::new(Line::from(spans));
    frame.render_widget(footer, area);
}

fn render_diff_view(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::DiffView;
    let title: String = app
        .diff_content
        .as_ref()
        .map(|dc| dc.path.clone())
        .unwrap_or_else(|| "Diff".to_string());

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
            let lines = match &app.styled_diff {
                Some(sd) => diff_lines_styled(dc, sd, app.current_hunk_index),
                None    => diff_lines(dc, app.current_hunk_index),
            };
            let paragraph = Paragraph::new(lines)
                .block(block)
                .scroll((app.diff_scroll, 0));
            frame.render_widget(paragraph, area);
        }
    }
}
