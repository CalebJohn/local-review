use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::app::{App, Focus};
use crate::diff::types::{ChangeKind, DiffContent};
use crate::git::types::{FileEntry, FileStatus};

pub fn view(frame: &mut ratatui::Frame, app: &App) {
    let chunks = Layout::horizontal([Constraint::Length(30), Constraint::Min(1)])
        .split(frame.area());
    render_sidebar(frame, app, chunks[0]);
    render_diff_view(frame, app, chunks[1]);
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
    let focused = app.focus == Focus::Sidebar;

    let items: Vec<ListItem> = app
        .files
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

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Files")
        .border_style(Style::default().fg(border_color(focused)));

    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );

    let mut state = ListState::default();
    if !app.files.is_empty() {
        state.select(Some(app.selected_index));
    }

    frame.render_stateful_widget(list, area, &mut state);
}

fn hunk_header_line(old_start: u32, new_start: u32) -> Line<'static> {
    Line::from(Span::styled(
        format!("@@ -{} +{} @@", old_start, new_start),
        Style::default().fg(Color::Cyan),
    ))
}

fn format_lineno(n: Option<u32>) -> String {
    match n {
        Some(v) => format!("{:>4}", v),
        None => "    ".to_string(),
    }
}

fn diff_lines(diff: &DiffContent) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    for hunk in &diff.hunks {
        lines.push(hunk_header_line(hunk.old_start, hunk.new_start));
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

            lines.push(Line::from(vec![lineno_span, body_span]));
        }
    }
    lines
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
            let lines = diff_lines(dc);
            let paragraph = Paragraph::new(lines)
                .block(block)
                .scroll((app.diff_scroll, 0));
            frame.render_widget(paragraph, area);
        }
    }
}
