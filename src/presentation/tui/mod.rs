mod app;
mod preview;
mod terminal;
mod view;

use std::io;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEventKind};
use ratatui::layout::Rect;
use thiserror::Error;

use super::Services;
use crate::application::update::{CheckOptions, UpdateStatus};
use app::{App, Outcome};
use terminal::TerminalSession;

const TICK: Duration = Duration::from_millis(30);

#[derive(Debug, Error)]
pub enum TuiError {
    #[error("terminal error: {0}")]
    Terminal(#[from] io::Error),
}

/// How the TUI ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finish {
    Quit,
    /// The user accepted the update; the terminal is already restored.
    Update(String),
}

/// Runs the interactive TUI until the user quits or accepts an update.
pub fn run(services: &mut Services) -> Result<Finish, TuiError> {
    // Started before the terminal setup so the first draw never waits on it.
    let updates = spawn_update_check(services);
    let mut session = TerminalSession::start()?;
    let mut app = App::new(session.picker.clone(), services);
    Ok(event_loop(&mut session, &mut app, services, &updates)?)
}

/// Runs the update check on its own thread; any failure is a silent no-result.
fn spawn_update_check(services: &Services) -> Receiver<UpdateStatus> {
    let (tx, rx) = mpsc::channel();
    if services.update.enabled {
        let check = std::sync::Arc::clone(&services.update.check);
        thread::spawn(move || {
            let _ = tx.send(check(CheckOptions::AUTO));
        });
    }
    rx
}

/// Runs the install while the terminal belongs to yay, then waits for Enter so
/// its output can be read before the TUI covers the screen again.
fn install_on_terminal(app: &mut App, package: &str, services: &mut Services) {
    app.install_package(package, services);
    println!("\n{}\nPress Enter to return to cursorswap...", app.status);
    let _ = io::stdin().read_line(&mut String::new());
}

fn event_loop(
    session: &mut TerminalSession,
    app: &mut App,
    services: &mut Services,
    updates: &Receiver<UpdateStatus>,
) -> io::Result<Finish> {
    let mut last_tick = Instant::now();
    let mut redraw = true;
    loop {
        if redraw {
            let terminal = &mut session.terminal;
            app.prepare_preview(view::preview_size(Rect::from(terminal.size()?)));
            terminal.draw(|frame| view::render(frame, app))?;
            redraw = false;
        }
        if event::poll(TICK)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    match app.handle_key(key, services) {
                        Outcome::Continue => {}
                        Outcome::Quit => return Ok(Finish::Quit),
                        Outcome::Update(version) => return Ok(Finish::Update(version)),
                        Outcome::Install(package) => {
                            // yay prompts for the PKGBUILD review and sudo itself.
                            session.suspend(|| install_on_terminal(app, &package, services))?;
                        }
                    }
                    redraw = true;
                }
                Event::Resize(..) => redraw = true,
                _ => {}
            }
        }
        redraw |= app.poll_update(updates);
        let now = Instant::now();
        redraw |= app.tick(now.duration_since(last_tick));
        last_tick = now;
    }
}
