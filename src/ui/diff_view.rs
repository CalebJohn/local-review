use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, AppMode};
use crate::diff::types::{ChangeKind, DiffContent};
use crate::syntax::StyledDiffContent;

fn hunk_header_line(old_start: u32, new_start: u32, header_context: Option<&str>, highlighted: bool) -> Line<'static> {
    let gutter = Span::raw(if highlighted { "│" } else { " " });
    let mut spans = vec![
        gutter,
        Span::styled(
            format!("@@ -{} +{} @@", old_start, new_start),
            Style::default().fg(Color::Cyan),
        ),
    ];
    if let Some(ctx) = header_context {
        spans.push(Span::styled(
            format!(" {ctx}"),
            Style::default().fg(Color::Cyan),
        ));
    }
    Line::from(spans)
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

pub fn diff_lines(
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
            lines.push(hunk_header_line(hunk.old_start, hunk.new_start, hunk.header_context.as_deref(), in_hunk));
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

pub fn render_diff_view(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let focused = matches!(app.focus, crate::app::Focus::DiffView | crate::app::Focus::CommentInput);
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
        .border_style(Style::default().fg(super::border_color(focused)));

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
