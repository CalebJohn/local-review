use ratatui::crossterm::event::{MouseButton, MouseEvent, MouseEventKind, KeyCode};
use ratatui::layout::Rect;

use crate::app::{App, Focus, Message};

pub fn translate_text_input(focus: Focus, code: KeyCode) -> Option<Message> {
    match focus {
        Focus::CommentInput => match code {
            KeyCode::Char(c) => Some(Message::CommentInputChar(c)),
            KeyCode::Backspace => Some(Message::CommentInputBackspace),
            KeyCode::Enter => Some(Message::CommentInputSubmit),
            KeyCode::Esc => Some(Message::CommentInputCancel),
            _ => None,
        },
        Focus::SearchInput => match code {
            KeyCode::Char(c) => Some(Message::SearchInputChar(c)),
            KeyCode::Backspace => Some(Message::SearchInputBackspace),
            KeyCode::Enter => Some(Message::SearchInputSubmit),
            KeyCode::Esc => Some(Message::SearchInputCancel),
            _ => None,
        },
        _ => None,
    }
}

pub fn translate_diff_common_key(key: KeyCode) -> Option<Message> {
    match key {
        KeyCode::Char('q') => Some(Message::Quit),
        KeyCode::Char('g') => Some(Message::ScrollToTop),
        KeyCode::Char('G') => Some(Message::ScrollToBottom),
        KeyCode::Char('b') => Some(Message::ToggleSidebar),
        KeyCode::Char('f') => Some(Message::ToggleFullFile),
        KeyCode::Char('r') => Some(Message::ReloadDiff),
        KeyCode::Char('h') => Some(Message::SelectSidebar),
        KeyCode::Char('l') => Some(Message::SelectFile),
        KeyCode::Char('z') => Some(Message::Undo),
        KeyCode::Char('Z') => Some(Message::Redo),
        KeyCode::Tab => Some(Message::SwitchFocus),
        _ => None,
    }
}

pub fn translate_visual_key(key: KeyCode) -> Option<Message> {
    match key {
        KeyCode::Char('j') | KeyCode::Down => Some(Message::ExtendSelectionDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Message::ExtendSelectionUp),
        KeyCode::Char('s') => Some(Message::StageSelectedLines),
        KeyCode::Char('u') => Some(Message::UnstageSelectedLines),
        KeyCode::Char('c') => Some(Message::StartComment),
        KeyCode::Esc => Some(Message::ExitVisual),
        _ => None,
    }
}

pub fn translate_mouse(mev: MouseEvent, review_area: Option<Rect>, staged_area: Rect, unstaged_area: Rect, diff: Rect) -> Option<Message> {
    match mev.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(area) = review_area
                && rect_contains(area, mev.column, mev.row)
            {
                let idx = mev.row.saturating_sub(area.y.saturating_add(1)) as usize;
                Some(Message::MouseClickReviewSidebar(idx))
            } else if rect_contains(staged_area, mev.column, mev.row) {
                let idx = mev.row.saturating_sub(staged_area.y.saturating_add(1)) as usize;
                Some(Message::MouseClickStagedSidebar(idx))
            } else if rect_contains(unstaged_area, mev.column, mev.row) {
                let idx = mev.row.saturating_sub(unstaged_area.y.saturating_add(1)) as usize;
                Some(Message::MouseClickUnstagedSidebar(idx))
            } else if rect_contains(diff, mev.column, mev.row) {
                Some(Message::FocusDiff)
            } else {
                None
            }
        }
        MouseEventKind::ScrollDown => {
            if rect_contains(diff, mev.column, mev.row) {
                Some(Message::ScrollDiffDown)
            } else {
                None
            }
        }
        MouseEventKind::ScrollUp => {
            if rect_contains(diff, mev.column, mev.row) {
                Some(Message::ScrollDiffUp)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn translate_diff_mouse(mev: MouseEvent, diff: Rect, app: &App) -> Option<Message> {
    if !rect_contains(diff, mev.column, mev.row) {
        return None;
    }
    let inner_y = diff.y.saturating_add(1);
    let row_offset = mev.row.saturating_sub(inner_y) as usize;
    let rendered_row = (app.diff_scroll as usize).saturating_add(row_offset);
    let line_idx = app.row_to_cursor(rendered_row);
    match mev.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            Some(Message::MouseClickDiffLine(line_idx))
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            Some(Message::MouseDragDiff(line_idx))
        }
        _ => None,
    }
}

pub fn rect_contains(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x
        && col < r.x.saturating_add(r.width)
        && row >= r.y
        && row < r.y.saturating_add(r.height)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn staged_rect() -> Rect { Rect::new(0, 0, 30, 10) }
    fn unstaged_rect() -> Rect { Rect::new(0, 10, 30, 10) }
    fn diff_rect() -> Rect { Rect::new(30, 0, 50, 20) }

    fn mouse(kind: MouseEventKind, col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column: col,
            row,
            modifiers: ratatui::crossterm::event::KeyModifiers::empty(),
        }
    }

    #[test]
    fn test_rect_contains_inside() {
        assert!(rect_contains(staged_rect(), 5, 5));
    }

    #[test]
    fn test_rect_contains_outside_right() {
        assert!(!rect_contains(staged_rect(), 30, 5));
    }

    #[test]
    fn test_rect_contains_at_origin() {
        assert!(rect_contains(staged_rect(), 0, 0));
    }

    #[test]
    fn test_translate_mouse_click_staged_row_3() {
        let m = mouse(MouseEventKind::Down(MouseButton::Left), 5, 3);
        assert_eq!(translate_mouse(m, None, staged_rect(), unstaged_rect(), diff_rect()),
                   Some(Message::MouseClickStagedSidebar(2)));
    }

    #[test]
    fn test_translate_mouse_click_review_sidebar() {
        let review_rect = Rect::new(0, 0, 30, 20);
        let m = mouse(MouseEventKind::Down(MouseButton::Left), 5, 4);
        assert_eq!(translate_mouse(m, Some(review_rect), Rect::ZERO, Rect::ZERO, diff_rect()),
                   Some(Message::MouseClickReviewSidebar(3)));
    }

    #[test]
    fn test_translate_mouse_review_area_wins_over_staged_area() {
        let review_rect = Rect::new(0, 0, 30, 20);
        let m = mouse(MouseEventKind::Down(MouseButton::Left), 5, 4);
        // Even when a staged rect overlaps the same region, the review list wins.
        assert_eq!(translate_mouse(m, Some(review_rect), staged_rect(), unstaged_rect(), diff_rect()),
                   Some(Message::MouseClickReviewSidebar(3)));
    }

    #[test]
    fn test_translate_mouse_click_outside_review_area_ignored() {
        let review_rect = Rect::new(0, 0, 30, 20);
        let m = mouse(MouseEventKind::Down(MouseButton::Left), 200, 200);
        assert_eq!(translate_mouse(m, Some(review_rect), staged_rect(), unstaged_rect(), diff_rect()), None);
    }

    #[test]
    fn test_translate_mouse_click_staged_on_border_is_idx_zero() {
        let m = mouse(MouseEventKind::Down(MouseButton::Left), 5, 0);
        assert_eq!(translate_mouse(m, None, staged_rect(), unstaged_rect(), diff_rect()),
                   Some(Message::MouseClickStagedSidebar(0)));
    }

    #[test]
    fn test_translate_mouse_click_unstaged() {
        let m = mouse(MouseEventKind::Down(MouseButton::Left), 5, 13);
        assert_eq!(translate_mouse(m, None, staged_rect(), unstaged_rect(), diff_rect()),
                   Some(Message::MouseClickUnstagedSidebar(2)));
    }

    #[test]
    fn test_translate_mouse_click_diff_is_focus_diff() {
        let m = mouse(MouseEventKind::Down(MouseButton::Left), 35, 5);
        assert_eq!(translate_mouse(m, None, staged_rect(), unstaged_rect(), diff_rect()),
                   Some(Message::FocusDiff));
    }

    #[test]
    fn test_translate_mouse_scroll_down_on_diff_is_scroll() {
        let m = mouse(MouseEventKind::ScrollDown, 35, 5);
        assert_eq!(translate_mouse(m, None, staged_rect(), unstaged_rect(), diff_rect()),
                   Some(Message::ScrollDiffDown));
    }

    #[test]
    fn test_translate_mouse_scroll_up_on_diff_is_scroll() {
        let m = mouse(MouseEventKind::ScrollUp, 35, 5);
        assert_eq!(translate_mouse(m, None, staged_rect(), unstaged_rect(), diff_rect()),
                   Some(Message::ScrollDiffUp));
    }

    #[test]
    fn test_translate_mouse_scroll_on_sidebar_is_noop() {
        let m = mouse(MouseEventKind::ScrollDown, 5, 5);
        assert_eq!(translate_mouse(m, None, staged_rect(), unstaged_rect(), diff_rect()), None);
    }

    #[test]
    fn test_translate_mouse_drag_is_noop() {
        let m = mouse(MouseEventKind::Drag(MouseButton::Left), 5, 5);
        assert_eq!(translate_mouse(m, None, staged_rect(), unstaged_rect(), diff_rect()), None);
    }

    #[test]
    fn test_translate_mouse_click_outside_all_rects_is_noop() {
        let m = mouse(MouseEventKind::Down(MouseButton::Left), 200, 200);
        assert_eq!(translate_mouse(m, None, staged_rect(), unstaged_rect(), diff_rect()), None);
    }
}
