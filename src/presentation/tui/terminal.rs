//! Terminal setup and teardown. This is the only place that touches the
//! real terminal, so it is exercised by hand rather than by unit tests.

use std::io;

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
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        ratatui::restore();
    }
}
