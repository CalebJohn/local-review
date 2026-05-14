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

fn find_match_ranges(text: &str, pattern: &str, case_sensitive: bool) -> Vec<(usize, usize)> {
    if pattern.is_empty() {
        return vec![];
    }
    if case_sensitive {
        text.match_indices(pattern)
            .map(|(i, m)| (i, i + m.len()))
            .collect()
    } else {
        super::case_insensitive_match_ranges(text, pattern)
    }
}

fn highlight_spans(
    spans: Vec<Span<'static>>,
    ranges: &[(usize, usize)],
) -> Vec<Span<'static>> {
    if ranges.is_empty() {
        return spans;
    }
    let highlight_style = Style::default().bg(Color::Yellow).fg(Color::Black);
    let mut result = Vec::new();
    let mut global_offset: usize = 0;
    let mut range_idx = 0;

    for span in spans {
        let text: String = span.content.to_string();
        let base_style = span.style;
        let span_end = global_offset + text.len();
        let mut local_pos = 0;

        while local_pos < text.len() && range_idx < ranges.len() {
            let (rs, re) = ranges[range_idx];
            if re <= global_offset + local_pos {
                range_idx += 1;
                continue;
            }
            if rs >= span_end {
                break;
            }
            let start_in_span = rs.saturating_sub(global_offset).max(local_pos);
            let end_in_span = re.saturating_sub(global_offset).min(text.len());
            if start_in_span > local_pos {
                result.push(Span::styled(text[local_pos..start_in_span].to_string(), base_style));
            }
            result.push(Span::styled(
                text[start_in_span..end_in_span].to_string(),
                highlight_style,
            ));
            local_pos = end_in_span;
            if global_offset + local_pos >= re {
                range_idx += 1;
            }
        }
        if local_pos < text.len() {
            result.push(Span::styled(text[local_pos..].to_string(), base_style));
        }
        global_offset = span_end;
    }
    result
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

#[allow(clippy::too_many_arguments)]
pub fn diff_lines(
    diff: &DiffContent,
    styled: Option<&StyledDiffContent>,
    current_hunk_index: Option<usize>,
    visual_selection: Option<(usize, usize)>,
    diff_cursor: usize,
    mode: &AppMode,
    semantic_filter: bool,
    search_pattern: Option<(&str, bool)>,
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

            let is_selected = match visual_selection {
                Some((start, end)) => global_line_idx >= start && global_line_idx <= end,
                None => false,
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

            let search_ranges = search_pattern
                .map(|(pat, cs)| find_match_ranges(&content, pat, cs))
                .unwrap_or_default();
            let has_search = !search_ranges.is_empty();
            let adjusted_ranges: Vec<(usize, usize)> = search_ranges
                .iter()
                .map(|(s, e)| (s + 1, e + 1))
                .collect();

            let gutter_span = Span::raw(gutter);
            let mut content_spans = if let Some(spans) = styled_line {
                let mut v: Vec<Span<'static>> = Vec::with_capacity(1 + spans.len());
                v.push(Span::styled(prefix.to_string(), Style::default()));
                for sp in spans {
                    v.push(Span::styled(sp.text.clone(), sp.style));
                }
                v
            } else {
                let body_style = if dl.formatting_only && dl.kind != ChangeKind::Equal {
                    content_style.add_modifier(Modifier::DIM)
                } else {
                    content_style
                };
                vec![Span::styled(format!("{}{}", prefix, content), body_style)]
            };

            if has_search {
                content_spans = highlight_spans(content_spans, &adjusted_ranges);
            }

            let mut parts: Vec<Span<'static>> = Vec::with_capacity(2 + content_spans.len());
            parts.push(gutter_span);
            parts.push(lineno_span);
            parts.extend(content_spans);
            let line = Line::from(parts);

            lines.push(apply_cursor_selection_style(line, is_cursor, is_selected));
            global_line_idx += 1;
        }
    }
    lines
}

pub fn render_diff_view(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let focused = matches!(app.focus, crate::app::Focus::DiffView | crate::app::Focus::CommentInput)
        || (app.focus == crate::app::Focus::SearchInput && app.search_origin == crate::app::Focus::DiffView);
    let path = app
        .diff_content
        .as_ref()
        .map(|dc| dc.path.as_str())
        .unwrap_or("Diff");

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
            let search = app.search_pattern.as_ref().map(|p| (p.as_str(), app.search_case_sensitive));
            let lines = diff_lines(dc, app.styled_diff.as_ref(), app.current_hunk_index, app.visual_selection, app.diff_cursor, &app.mode, app.semantic_filter, search);
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
