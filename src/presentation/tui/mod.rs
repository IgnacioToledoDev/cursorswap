mod app;
mod preview;
mod terminal;
mod view;

use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEventKind};
use ratatui::DefaultTerminal;
use ratatui::layout::Rect;
use thiserror::Error;

use super::Services;
use app::{App, Outcome};
use terminal::TerminalSession;

const TICK: Duration = Duration::from_millis(30);

#[derive(Debug, Error)]
pub enum TuiError {
    #[error("terminal error: {0}")]
    Terminal(#[from] io::Error),
}

/// Runs the interactive TUI until the user quits.
pub fn run(services: &mut Services) -> Result<(), TuiError> {
    let mut session = TerminalSession::start()?;
    let mut app = App::new(session.picker.clone(), services);
    event_loop(&mut session.terminal, &mut app, services)?;
    Ok(())
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    services: &mut Services,
) -> io::Result<()> {
    let mut last_tick = Instant::now();
    let mut redraw = true;
    loop {
        if redraw {
            app.prepare_preview(view::preview_size(Rect::from(terminal.size()?)));
            terminal.draw(|frame| view::render(frame, app))?;
            redraw = false;
        }
        if event::poll(TICK)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if app.handle_key(key, services) == Outcome::Quit {
                        return Ok(());
                    }
                    redraw = true;
                }
                Event::Resize(..) => redraw = true,
                _ => {}
            }
        }
        let now = Instant::now();
        redraw |= app.tick(now.duration_since(last_tick));
        last_tick = now;
    }
}
