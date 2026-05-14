use std::collections::HashMap;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use crate::app::SidebarSection;
use crate::git::types::{FileEntry, FileStatus};

pub fn status_style(entry: &FileEntry) -> Style {
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

pub fn render_sidebar(
    frame: &mut ratatui::Frame,
    app: &crate::app::App,
    area: Rect,
) {
    let sidebar_focused = app.focus == crate::app::Focus::Sidebar
        || (app.focus == crate::app::Focus::SearchInput && app.search_origin == crate::app::Focus::Sidebar);
    let (staged_area, unstaged_area) =
        super::sidebar_section_areas(area, app.staged_files.len(), app.unstaged_files.len());
    let search = app.search_pattern.as_ref().map(|p| (p.as_str(), app.search_case_sensitive));

    for (section, files, area, title) in [
        (SidebarSection::Staged, app.staged_files.as_slice(), staged_area, "Staged"),
        (SidebarSection::Unstaged, app.unstaged_files.as_slice(), unstaged_area, "Unstaged"),
    ] {
        render_file_list(frame, area, &FileListProps {
            files,
            title,
            focused: sidebar_focused && app.sidebar_section == section,
            selected: if app.sidebar_section == section { Some(app.selected_index) } else { None },
            formatting_only_cache: &app.formatting_only_cache,
            section,
            search_pattern: search,
        });
    }
}

pub struct FileListProps<'a> {
    pub files: &'a [FileEntry],
    pub title: &'a str,
    pub focused: bool,
    pub selected: Option<usize>,
    pub formatting_only_cache: &'a HashMap<(String, SidebarSection), bool>,
    pub section: SidebarSection,
    pub search_pattern: Option<(&'a str, bool)>,
}

fn split_path_for_search<'a>(
    path: &str,
    base_style: Style,
    pattern: &str,
    case_sensitive: bool,
) -> Vec<Span<'a>> {
    let highlight_style = Style::default().bg(Color::Yellow).fg(Color::Black);
    let match_positions: Vec<(usize, usize)> = if case_sensitive {
        path.match_indices(pattern)
            .map(|(i, m)| (i, i + m.len()))
            .collect()
    } else {
        super::case_insensitive_match_ranges(path, pattern)
    };

    if match_positions.is_empty() {
        return vec![Span::styled(path.to_string(), base_style)];
    }

    let mut spans = Vec::new();
    let mut last = 0;
    for (start, end) in match_positions {
        if start > last {
            spans.push(Span::styled(path[last..start].to_string(), base_style));
        }
        spans.push(Span::styled(path[start..end].to_string(), highlight_style));
        last = end;
    }
    if last < path.len() {
        spans.push(Span::styled(path[last..].to_string(), base_style));
    }
    spans
}

pub fn render_file_list(frame: &mut ratatui::Frame, area: Rect, props: &FileListProps) {
    let items: Vec<ListItem> = props
        .files
        .iter()
        .map(|entry| {
            let status = entry.display_status();
            let is_formatting_only = props
                .formatting_only_cache
                .get(&(entry.path.clone(), props.section))
                .copied()
                .unwrap_or(false);
            let mut style = status_style(entry);
            if is_formatting_only {
                style = style.add_modifier(Modifier::DIM);
            }
            let mut spans = vec![Span::styled(format!("{} ", status), style)];
            if let Some((pattern, case_sensitive)) = props.search_pattern {
                spans.extend(split_path_for_search(&entry.path, style, pattern, case_sensitive));
            } else {
                spans.push(Span::styled(&entry.path, style));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let count = props.files.len();
    let title_text = format!("{} ({})", props.title, count);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title_text)
        .border_style(Style::default().fg(super::border_color(props.focused)));

    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );

    let mut state = ListState::default();
    if props.selected.is_some() {
        state.select(props.selected);
    }

    frame.render_stateful_widget(list, area, &mut state);
}
