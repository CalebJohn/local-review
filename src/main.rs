mod app;
mod diff;
mod git;
mod syntax;
mod ui;

use std::sync::mpsc;
use std::time::Duration;

use app::{App, Focus, Message};
use notify::Watcher;

enum WatchEvent {
    Workdir,
    Index,
}
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
    MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::layout::{Constraint, Layout, Rect};
use ui::sidebar_section_areas;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = ratatui::init();
    execute!(std::io::stdout(), EnableMouseCapture)?;

    let result = run(&mut terminal);

    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new()?;

    // File watcher: watches workdir recursively + .git/index for external git ops
    let (watch_tx, watch_rx) = mpsc::channel();
    let workdir = app.repo.workdir_path()
        .expect("not a bare repo")
        .to_path_buf();
    let git_dir = app.repo.git_dir().to_path_buf();
    let index_path = git_dir.join("index");

    let watcher_git_dir = git_dir.clone();
    let watcher_index = index_path.clone();
    let mut watcher = notify::recommended_watcher(move |res: Result<notify::Event, _>| {
        if let Ok(ev) = res {
            if !(ev.kind.is_modify() || ev.kind.is_create() || ev.kind.is_remove()) {
                return;
            }
            for path in &ev.paths {
                if path.starts_with(&watcher_git_dir) {
                    if path == &watcher_index {
                        let _ = watch_tx.send(WatchEvent::Index);
                    }
                } else {
                    let _ = watch_tx.send(WatchEvent::Workdir);
                }
            }
        }
    })?;
    watcher.watch(&workdir, notify::RecursiveMode::Recursive)?;

    loop {
        terminal.draw(|frame| ui::view(frame, &app))?;

        // Poll crossterm events with 100ms timeout (non-blocking)
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        let msg = match app.focus {
                            Focus::Sidebar => match key.code {
                                KeyCode::Char('q') => Some(Message::Quit),
                                KeyCode::Char('j') | KeyCode::Down => Some(Message::MoveDown),
                                KeyCode::Char('k') | KeyCode::Up => Some(Message::MoveUp),
                                KeyCode::Char('s') => Some(Message::StageFile),
                                KeyCode::Char('u') => Some(Message::UnstageFile),
                                KeyCode::Enter => Some(Message::SelectFile),
                                KeyCode::Tab => Some(Message::SwitchFocus),
                                _ => None,
                            },
                            Focus::DiffView => match key.code {
                                KeyCode::Char('q') => Some(Message::Quit),
                                KeyCode::Char('j') | KeyCode::Down => Some(Message::ScrollDiffDown),
                                KeyCode::Char('k') | KeyCode::Up => Some(Message::ScrollDiffUp),
                                KeyCode::Char('n') => Some(Message::NextHunk),
                                KeyCode::Char('N') => Some(Message::PrevHunk),
                                KeyCode::Char('s') => Some(Message::StageHunk),
                                KeyCode::Char('u') => Some(Message::UnstageHunk),
                                KeyCode::Char('r') => Some(Message::ReloadDiff),
                                KeyCode::Tab => Some(Message::SwitchFocus),
                                KeyCode::Esc => Some(Message::SwitchFocus),
                                _ => None,
                            },
                        };
                        if let Some(msg) = msg {
                            app.update(msg);
                        }
                    }
                }
                Event::Mouse(mev) => {
                    let size = terminal.size()?;
                    let area = Rect::new(0, 0, size.width, size.height);
                    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)])
                        .split(area);
                    let chunks = Layout::horizontal([Constraint::Length(30), Constraint::Min(1)])
                        .split(rows[0]);
                    let sidebar_rect = chunks[0];
                    let diff_rect = chunks[1];

                    let (staged_area, unstaged_area) = sidebar_section_areas(
                        sidebar_rect,
                        app.staged_files.len(),
                        app.unstaged_files.len(),
                    );

                    let msg = translate_mouse(mev, staged_area, unstaged_area, diff_rect);
                    if let Some(msg) = msg {
                        app.update(msg);
                    }
                }
                _ => {}
            }
        }

        // Drain file watcher events, coalescing into a single update
        let mut saw_workdir = false;
        let mut saw_index = false;
        while let Ok(ev) = watch_rx.try_recv() {
            match ev {
                WatchEvent::Workdir => saw_workdir = true,
                WatchEvent::Index => saw_index = true,
            }
        }
        if saw_index {
            app.update(Message::IndexChanged);
        } else if saw_workdir {
            app.update(Message::WorkdirChanged);
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

fn translate_mouse(mev: MouseEvent, staged_area: Rect, unstaged_area: Rect, diff: Rect) -> Option<Message> {
    match mev.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if rect_contains(staged_area, mev.column, mev.row) {
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

fn rect_contains(r: Rect, col: u16, row: u16) -> bool {
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
        assert_eq!(translate_mouse(m, staged_rect(), unstaged_rect(), diff_rect()),
                   Some(Message::MouseClickStagedSidebar(2)));
    }

    #[test]
    fn test_translate_mouse_click_staged_on_border_is_idx_zero() {
        let m = mouse(MouseEventKind::Down(MouseButton::Left), 5, 0);
        assert_eq!(translate_mouse(m, staged_rect(), unstaged_rect(), diff_rect()),
                   Some(Message::MouseClickStagedSidebar(0)));
    }

    #[test]
    fn test_translate_mouse_click_unstaged() {
        let m = mouse(MouseEventKind::Down(MouseButton::Left), 5, 13);
        assert_eq!(translate_mouse(m, staged_rect(), unstaged_rect(), diff_rect()),
                   Some(Message::MouseClickUnstagedSidebar(2)));
    }

    #[test]
    fn test_translate_mouse_click_diff_is_focus_diff() {
        let m = mouse(MouseEventKind::Down(MouseButton::Left), 35, 5);
        assert_eq!(translate_mouse(m, staged_rect(), unstaged_rect(), diff_rect()),
                   Some(Message::FocusDiff));
    }

    #[test]
    fn test_translate_mouse_scroll_down_on_diff_is_scroll() {
        let m = mouse(MouseEventKind::ScrollDown, 35, 5);
        assert_eq!(translate_mouse(m, staged_rect(), unstaged_rect(), diff_rect()),
                   Some(Message::ScrollDiffDown));
    }

    #[test]
    fn test_translate_mouse_scroll_up_on_diff_is_scroll() {
        let m = mouse(MouseEventKind::ScrollUp, 35, 5);
        assert_eq!(translate_mouse(m, staged_rect(), unstaged_rect(), diff_rect()),
                   Some(Message::ScrollDiffUp));
    }

    #[test]
    fn test_translate_mouse_scroll_on_sidebar_is_noop() {
        let m = mouse(MouseEventKind::ScrollDown, 5, 5);
        assert_eq!(translate_mouse(m, staged_rect(), unstaged_rect(), diff_rect()), None);
    }

    #[test]
    fn test_translate_mouse_drag_is_noop() {
        let m = mouse(MouseEventKind::Drag(MouseButton::Left), 5, 5);
        assert_eq!(translate_mouse(m, staged_rect(), unstaged_rect(), diff_rect()), None);
    }

    #[test]
    fn test_translate_mouse_click_outside_all_rects_is_noop() {
        let m = mouse(MouseEventKind::Down(MouseButton::Left), 200, 200);
        assert_eq!(translate_mouse(m, staged_rect(), unstaged_rect(), diff_rect()), None);
    }
}