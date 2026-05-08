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

pub fn render_file_list(
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
        .border_style(Style::default().fg(super::border_color(focused)));

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
