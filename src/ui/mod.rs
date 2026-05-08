mod diff_view;
mod footer;
mod sidebar;

use ratatui::prelude::*;
use ratatui::layout::Constraint;

use crate::app::App;

#[cfg(test)]
pub use diff_view::diff_lines;

pub fn view(frame: &mut ratatui::Frame, app: &App) {
    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());
    if app.sidebar_collapsed {
        diff_view::render_diff_view(frame, app, rows[0]);
    } else {
        let cols = Layout::horizontal([Constraint::Length(30), Constraint::Min(1)])
            .split(rows[0]);
        sidebar::render_sidebar(frame, app, cols[0]);
        diff_view::render_diff_view(frame, app, cols[1]);
    }
    footer::render_footer(frame, app, rows[1]);
}

pub fn sidebar_section_areas(sidebar_area: Rect, staged_count: usize, unstaged_count: usize) -> (Rect, Rect) {
    let total = staged_count + unstaged_count;
    if total == 0 {
        let halves = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(sidebar_area);
        return (halves[0], halves[1]);
    }

    let min_rows: u16 = 3;
    let available = sidebar_area.height;

    if available < min_rows * 2 {
        let halves = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(sidebar_area);
        return (halves[0], halves[1]);
    }

    let staged_content = staged_count as u16 + 2;
    let unstaged_content = unstaged_count as u16 + 2;
    let total_wanted = staged_content + unstaged_content;

    let (staged_h, unstaged_h) = if total_wanted <= available {
        (staged_content, available - staged_content)
    } else {
        let staged_ratio = staged_count as f32 / total as f32;
        let staged_h = ((available as f32 * staged_ratio) as u16).max(min_rows);
        let unstaged_h = available.saturating_sub(staged_h).max(min_rows);
        let staged_h = available.saturating_sub(unstaged_h);
        (staged_h, unstaged_h)
    };

    let sections = Layout::vertical([Constraint::Length(staged_h), Constraint::Length(unstaged_h)])
        .split(sidebar_area);
    (sections[0], sections[1])
}

fn border_color(focused: bool) -> Color {
    if focused {
        Color::Blue
    } else {
        Color::DarkGray
    }
}

#[cfg(test)]
#[path = "../ui_tests.rs"]
mod tests;
