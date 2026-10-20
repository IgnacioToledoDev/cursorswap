//! Terminal setup, teardown and hand-over. This is the only place that touches
//! the real terminal, so the real calls are exercised by hand; the ordering
//! guarantees live in `with_suspended` and are unit tested.

use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
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
        // After the query, which reads stdin. `ratatui::restore` does not turn
        // capture off, so `Drop` and the panic hook do.
        execute!(io::stdout(), EnableMouseCapture)?;
        install_panic_hook();
        Ok(session)
    }

    /// Gives the terminal back to `run` (an interactive child such as yay) and
    /// takes it over again afterwards, on every exit path of `run`.
    pub fn suspend<R>(&mut self, run: impl FnOnce() -> R) -> io::Result<R> {
        let result = with_suspended(
            || leave(disable_mouse_capture, ratatui::try_restore),
            enter,
            run,
        )?;
        // The child drew over the screen: forget what ratatui believes is on it.
        self.terminal.clear()?;
        Ok(result)
    }
}

fn enter() -> io::Result<()> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)
}

fn disable_mouse_capture() -> io::Result<()> {
    execute!(io::stdout(), DisableMouseCapture)
}

/// Turns capture off first, then restores. A failed disable must not keep the
/// terminal from being restored, so its error is dropped.
fn leave(
    disable_capture: impl FnOnce() -> io::Result<()>,
    restore: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    let _ = disable_capture();
    restore()
}

/// Chains in front of the hook `try_init` installed (which restores the
/// terminal), so a panic message is not printed with capture still on.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_mouse_capture();
        previous(info);
    }));
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
        let _ = leave(disable_mouse_capture, || {
            ratatui::restore();
            Ok(())
        });
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
    fn mouse_capture_is_disabled_before_the_terminal_is_restored() {
        let steps = Steps::default();
        let result = leave(
            || step(&steps, "disable capture"),
            || step(&steps, "restore"),
        );
        assert!(result.is_ok());
        assert_eq!(*steps.borrow(), ["disable capture", "restore"]);
    }

    #[test]
    fn a_failing_capture_disable_does_not_skip_the_restore() {
        let steps = Steps::default();
        let result = leave(
            || {
                steps.borrow_mut().push("disable capture");
                Err(io::Error::other("closed"))
            },
            || step(&steps, "restore"),
        );
        assert!(result.is_ok());
        assert_eq!(*steps.borrow(), ["disable capture", "restore"]);
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
