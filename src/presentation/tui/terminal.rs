//! Terminal setup, teardown and hand-over. This is the only place that touches
//! the real terminal, so the real calls are exercised by hand; the ordering
//! guarantees live in `with_suspended` and are unit tested.

use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, enable_raw_mode};

use ratatui::DefaultTerminal;
use ratatui_image::picker::Picker;

/// Owns the raw-mode alternate screen; restores the terminal on drop, so every
/// exit path (including errors from the event loop) leaves it usable.
pub struct TerminalSession {
    pub terminal: DefaultTerminal,
    pub picker: Picker,
}

impl TerminalSession {
    pub fn start() -> io::Result<TerminalSession> {
        // `try_init` also installs a panic hook that restores the terminal.
        let terminal = ratatui::try_init()?;
        let mut session = TerminalSession {
            terminal,
            picker: Picker::halfblocks(),
        };
        // The query talks to the terminal, so it must run after entering the
        // alternate screen and before any event is read; failures fall back
        // to halfblocks.
        if let Ok(picker) = Picker::from_query_stdio() {
            session.picker = picker;
        }
        Ok(session)
    }

    /// Gives the terminal back to `run` (an interactive child such as yay) and
    /// takes it over again afterwards, on every exit path of `run`.
    pub fn suspend<R>(&mut self, run: impl FnOnce() -> R) -> io::Result<R> {
        let result = with_suspended(ratatui::try_restore, enter, run)?;
        // The child drew over the screen: forget what ratatui believes is on it.
        self.terminal.clear()?;
        Ok(result)
    }
}

fn enter() -> io::Result<()> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)
}

/// Runs `run` between `leave` and `enter`. When `leave` fails, `run` is skipped
/// and nothing is re-entered. Otherwise `enter` runs even if `run` panics, and
/// the panic continues afterwards.
fn with_suspended<R>(
    leave: impl FnOnce() -> io::Result<()>,
    enter: impl FnOnce() -> io::Result<()>,
    run: impl FnOnce() -> R,
) -> io::Result<R> {
    leave()?;
    let outcome = catch_unwind(AssertUnwindSafe(run));
    let entered = enter();
    match outcome {
        Ok(result) => entered.map(|()| result),
        Err(panic) => resume_unwind(panic),
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        ratatui::restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    type Steps = RefCell<Vec<&'static str>>;

    fn step(steps: &Steps, name: &'static str) -> io::Result<()> {
        steps.borrow_mut().push(name);
        Ok(())
    }

    #[test]
    fn leaves_then_runs_then_re_enters_and_returns_the_result() {
        let steps = Steps::default();
        let value = with_suspended(
            || step(&steps, "leave"),
            || step(&steps, "enter"),
            || {
                steps.borrow_mut().push("run");
                42
            },
        )
        .unwrap();
        assert_eq!(value, 42);
        assert_eq!(*steps.borrow(), ["leave", "run", "enter"]);
    }

    #[test]
    fn a_failing_run_still_re_enters() {
        let steps = Steps::default();
        let result: io::Result<Result<(), &str>> = with_suspended(
            || step(&steps, "leave"),
            || step(&steps, "enter"),
            || Err("yay failed"),
        );
        assert_eq!(result.unwrap(), Err("yay failed"));
        assert_eq!(*steps.borrow(), ["leave", "enter"]);
    }

    #[test]
    fn a_panicking_run_re_enters_before_the_panic_continues() {
        let steps = Steps::default();
        let caught = catch_unwind(AssertUnwindSafe(|| {
            let _ = with_suspended(
                || step(&steps, "leave"),
                || step(&steps, "enter"),
                || -> () { panic!("boom") },
            );
        }));
        assert!(caught.is_err());
        assert_eq!(*steps.borrow(), ["leave", "enter"]);
    }

    #[test]
    fn a_failing_leave_skips_the_run_and_the_re_entry() {
        let steps = Steps::default();
        let result = with_suspended(
            || Err(io::Error::other("no tty")),
            || step(&steps, "enter"),
            || steps.borrow_mut().push("run"),
        );
        assert!(result.is_err());
        assert!(steps.borrow().is_empty());
    }

    #[test]
    fn a_failing_re_entry_is_reported() {
        let result = with_suspended(|| Ok(()), || Err(io::Error::other("no tty")), || 1);
        assert!(result.is_err());
    }
}
