mod app;
mod diff;
mod git;
mod ui;

use app::{App, Focus, Message};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = ratatui::init();
    let result = run(&mut terminal);
    ratatui::restore();
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new()?;
    loop {
        terminal.draw(|frame| ui::view(frame, &app))?;

        if let Event::Key(key) = event::read()? {
            // CRITICAL: Filter for KeyEventKind::Press to avoid duplicate events
            if key.kind == KeyEventKind::Press {
                let msg = match app.focus {
                    Focus::Sidebar => match key.code {
                        KeyCode::Char('q') => Some(Message::Quit),
                        KeyCode::Char('j') | KeyCode::Down => Some(Message::MoveDown),
                        KeyCode::Char('k') | KeyCode::Up => Some(Message::MoveUp),
                        KeyCode::Enter => Some(Message::SelectFile),
                        KeyCode::Tab => Some(Message::SwitchFocus),
                        _ => None,
                    },
                    Focus::DiffView => match key.code {
                        KeyCode::Char('q') => Some(Message::Quit),
                        KeyCode::Char('j') | KeyCode::Down => Some(Message::ScrollDiffDown),
                        KeyCode::Char('k') | KeyCode::Up => Some(Message::ScrollDiffUp),
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

        if app.should_quit {
            break;
        }
    }
    Ok(())
}
