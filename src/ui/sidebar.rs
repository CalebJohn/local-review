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
    let sidebar_focused = app.focus == crate::app::Focus::Sidebar;
    let (staged_area, unstaged_area) =
        super::sidebar_section_areas(area, app.staged_files.len(), app.unstaged_files.len());

    render_file_list(
        frame,
        staged_area,
        &FileListProps {
            files: &app.staged_files,
            title: "Staged",
            focused: sidebar_focused && app.sidebar_section == SidebarSection::Staged,
            selected: if app.sidebar_section == SidebarSection::Staged {
                Some(app.selected_index)
            } else {
                None
            },
            formatting_only_cache: &app.formatting_only_cache,
            section: SidebarSection::Staged,
        },
    );

    render_file_list(
        frame,
        unstaged_area,
        &FileListProps {
            files: &app.unstaged_files,
            title: "Unstaged",
            focused: sidebar_focused && app.sidebar_section == SidebarSection::Unstaged,
            selected: if app.sidebar_section == SidebarSection::Unstaged {
                Some(app.selected_index)
            } else {
                None
            },
            formatting_only_cache: &app.formatting_only_cache,
            section: SidebarSection::Unstaged,
        },
    );
}

pub struct FileListProps<'a> {
    pub files: &'a [FileEntry],
    pub title: &'a str,
    pub focused: bool,
    pub selected: Option<usize>,
    pub formatting_only_cache: &'a HashMap<(String, SidebarSection), bool>,
    pub section: SidebarSection,
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
            let line = Line::from(vec![
                Span::styled(format!("{} ", status), style),
                Span::styled(entry.path.clone(), style),
            ]);
            ListItem::new(line)
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
