mod app;
mod classify;
mod diff;
mod git;
mod input;
mod syntax;
mod ui;
mod undo;

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use app::{App, AppMode, Focus, Message};
use input::{translate_diff_common_key, translate_diff_mouse, translate_mouse, translate_visual_key};
use notify::Watcher;

enum WatchEvent {
    Workdir,
    Index,
}
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
    KeyModifiers,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::{Constraint, Layout, Rect};
use ui::sidebar_section_areas;

static SIGTERM_RECEIVED: AtomicBool = AtomicBool::new(false);

extern "C" fn sigterm_handler(_: libc::c_int) {
    SIGTERM_RECEIVED.store(true, Ordering::Relaxed);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
        ratatui::restore();
        original_hook(info);
    }));

    unsafe {
        libc::signal(libc::SIGTERM, sigterm_handler as *const () as libc::sighandler_t);
    }

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
                Event::Key(key)
                    if key.kind == KeyEventKind::Press => {
                        if key.modifiers.contains(KeyModifiers::CONTROL) {
                            match key.code {
                                KeyCode::Char('z') => {
                                    execute!(std::io::stdout(), DisableMouseCapture, LeaveAlternateScreen, ratatui::crossterm::cursor::Show)?;
                                    disable_raw_mode()?;
                                    unsafe { libc::raise(libc::SIGTSTP); }
                                    enable_raw_mode()?;
                                    execute!(std::io::stdout(), EnterAlternateScreen, EnableMouseCapture, ratatui::crossterm::cursor::Hide)?;
                                    terminal.clear()?;
                                    continue;
                                }
                                KeyCode::Char('c') => {
                                    app.update(Message::Quit);
                                    continue;
                                }
                                _ => {}
                            }
                        }
                        if app.focus == Focus::CommentInput {
                            let msg = match key.code {
                                KeyCode::Char(c) => Some(Message::CommentInputChar(c)),
                                KeyCode::Backspace => Some(Message::CommentInputBackspace),
                                KeyCode::Enter => Some(Message::CommentInputSubmit),
                                KeyCode::Esc => Some(Message::CommentInputCancel),
                                _ => None,
                            };
                            if let Some(msg) = msg {
                                app.update(msg);
                            }
                            continue;
                        }

                        // 'e' is handled directly (needs terminal access), not via Message
                        if key.code == KeyCode::Char('e') {
                            if let Some(rel_path) = app.selected_file_path() {
                                let abs_path = app.repo.workdir_path()
                                    .expect("not a bare repo")
                                    .join(&rel_path);
                                let editor_result = run_editor(terminal, &abs_path);
                                // Drain watcher events that accumulated during editing,
                                // then do a full refresh to pick up all changes at once.
                                while watch_rx.try_recv().is_ok() {}
                                app.refresh_files();
                                if let Err(e) = editor_result {
                                    app.status_message = Some(format!("Editor: {}", e));
                                }
                            }
                            continue;
                        }

                        let msg = match app.focus {
                            Focus::Sidebar => match key.code {
                                KeyCode::Char('q') => Some(Message::Quit),
                                KeyCode::Char('j') | KeyCode::Down => Some(Message::MoveDown),
                                KeyCode::Char('k') | KeyCode::Up => Some(Message::MoveUp),
                                KeyCode::Char(' ') => Some(Message::MoveDown),
                                KeyCode::Char('J') => Some(Message::MoveDown),
                                KeyCode::Char('K') => Some(Message::MoveUp),
                                KeyCode::Char('s') => Some(Message::StageFile),
                                KeyCode::Char('u') => Some(Message::UnstageFile),
                                KeyCode::Char('S') => Some(Message::StageFile),
                                KeyCode::Char('U') => Some(Message::UnstageFile),
                                KeyCode::Char('d') => Some(Message::DiscardFile),
                                KeyCode::Char('D') => Some(Message::DiscardFile),
                                KeyCode::Char('b') => Some(Message::ToggleSidebar),
                                KeyCode::Char('f') => Some(Message::ToggleFullFile),
                                KeyCode::Enter => Some(Message::SelectFile),
                                KeyCode::Char('h') => Some(Message::SelectSidebar),
                                KeyCode::Char('l') => Some(Message::SelectFile),
                                KeyCode::Char('z') => Some(Message::Undo),
                                KeyCode::Char('Z') => Some(Message::Redo),
                                KeyCode::Tab => Some(Message::SwitchFocus),
                                KeyCode::Char('w') => Some(Message::ToggleSemanticFilter),
                                _ => None,
                            },
                            Focus::DiffView => {
                                if app.mode == AppMode::Visual {
                                    translate_visual_key(key.code)
                                        .or(match key.code {
                                            KeyCode::Char('v') => Some(Message::ExitVisual),
                                            _ => None,
                                        })
                                        .or_else(|| translate_diff_common_key(key.code))
                                } else {
                                    match key.code {
                                        KeyCode::Char('j') | KeyCode::Down => Some(Message::MoveCursorDown),
                                        KeyCode::Char('k') | KeyCode::Up => Some(Message::MoveCursorUp),
                                        KeyCode::Char(' ') => Some(Message::MoveDown),
                                        KeyCode::Char('J') => Some(Message::MoveDown),
                                        KeyCode::Char('K') => Some(Message::MoveUp),
                                        KeyCode::Char('n') => Some(Message::NextHunk),
                                        KeyCode::Char('N') => Some(Message::PrevHunk),
                                        KeyCode::Char('s') => Some(Message::StageHunk),
                                        KeyCode::Char('u') => Some(Message::UnstageHunk),
                                        KeyCode::Char('S') => Some(Message::StageFile),
                                        KeyCode::Char('U') => Some(Message::UnstageFile),
                                        KeyCode::Char('d') => Some(Message::DiscardHunk),
                                        KeyCode::Char('D') => Some(Message::DiscardFile),
                                        KeyCode::Char('c') => Some(Message::StartComment),
                                        KeyCode::Char('v') => Some(Message::EnterVisual),
                                        KeyCode::Char('w') => Some(Message::ToggleSemanticFilter),
                                                _ => translate_diff_common_key(key.code),
                                    }
                                }
                            }
                            Focus::CommentInput => unreachable!(),
                        };
                        if let Some(msg) = msg {
                            app.update(msg);
                        }
                    }
                Event::Mouse(mev) => {
                    let size = terminal.size()?;
                    let area = Rect::new(0, 0, size.width, size.height);
                    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)])
                        .split(area);

                    let (staged_area, unstaged_area, diff_rect) = if app.sidebar_collapsed {
                        (Rect::ZERO, Rect::ZERO, rows[0])
                    } else {
                        let chunks = Layout::horizontal([Constraint::Length(30), Constraint::Min(1)])
                            .split(rows[0]);
                        let (staged, unstaged) = sidebar_section_areas(
                            chunks[0],
                            app.staged_files.len(),
                            app.unstaged_files.len(),
                        );
                        (staged, unstaged, chunks[1])
                    };

                    if let Some(msg) = translate_mouse(mev, staged_area, unstaged_area, diff_rect) {
                        app.update(msg);
                    }
                    if let Some(msg) = translate_diff_mouse(mev, diff_rect, &app) {
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

        if app.should_quit || SIGTERM_RECEIVED.load(Ordering::Relaxed) {
            break;
        }
    }
    Ok(())
}

fn resolve_editor() -> Result<String, String> {
    std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .map_err(|_| "$VISUAL and $EDITOR are not set".to_string())
}

fn run_editor(
    terminal: &mut ratatui::DefaultTerminal,
    path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let editor = resolve_editor()?;

    execute!(std::io::stdout(), DisableMouseCapture, LeaveAlternateScreen)?;
    disable_raw_mode()?;

    let result = Command::new("sh")
        .arg("-c")
        .arg(format!("{} \"$1\"", editor))
        .arg("--")
        .arg(path)
        .status();

    // Always restore terminal state regardless of editor success/failure
    enable_raw_mode()?;
    execute!(std::io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    terminal.clear()?;

    match result {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("{} exited with status: {}", editor, status).into()),
        Err(e) => Err(format!("failed to launch {}: {}", editor, e).into()),
    }
}
