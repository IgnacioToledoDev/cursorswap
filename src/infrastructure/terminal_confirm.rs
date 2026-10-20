use std::io::{self, BufRead, IsTerminal, Write};

use crate::domain::error::ConfirmError;
use crate::domain::ports::Confirm;

/// Asks on the controlling terminal; refuses to guess when stdin is not one.
pub struct TerminalConfirm;

impl Confirm for TerminalConfirm {
    fn confirm(&self, prompt: &str) -> Result<bool, ConfirmError> {
        confirm_with(
            io::stdin().is_terminal(),
            &mut io::stdin().lock(),
            &mut io::stderr(),
            prompt,
        )
    }
}

pub(super) fn confirm_with(
    interactive: bool,
    input: &mut impl BufRead,
    output: &mut impl Write,
    prompt: &str,
) -> Result<bool, ConfirmError> {
    if !interactive {
        return Err(ConfirmError::NotInteractive);
    }
    let io_err = |e: io::Error| ConfirmError::Io(e.to_string());
    write!(output, "{prompt} [y/N] ").map_err(io_err)?;
    output.flush().map_err(io_err)?;
    let mut answer = String::new();
    input.read_line(&mut answer).map_err(io_err)?;
    let answer = answer.trim();
    Ok(answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(interactive: bool, typed: &str) -> (Result<bool, ConfirmError>, String) {
        let mut out = Vec::new();
        let result = confirm_with(interactive, &mut typed.as_bytes(), &mut out, "Install foo?");
        (result, String::from_utf8(out).unwrap())
    }

    #[test]
    fn yes_answers_confirm() {
        for typed in ["y\n", "Y\n", "yes\n", "YES\n", "  y  \n", "y"] {
            assert_eq!(ask(true, typed).0, Ok(true), "{typed:?}");
        }
    }

    #[test]
    fn anything_else_declines() {
        for typed in ["n\n", "\n", "", "no\n", "maybe\n", "yes please\n", "yy\n"] {
            assert_eq!(ask(true, typed).0, Ok(false), "{typed:?}");
        }
    }

    #[test]
    fn prompt_shows_the_question_and_the_no_default() {
        let (_, shown) = ask(true, "n\n");
        assert_eq!(shown, "Install foo? [y/N] ");
    }

    #[test]
    fn without_a_terminal_it_fails_and_prints_nothing() {
        let (result, shown) = ask(false, "y\n");
        assert_eq!(result, Err(ConfirmError::NotInteractive));
        assert!(shown.is_empty());
    }

    #[test]
    fn read_failure_is_reported() {
        struct Broken;
        impl io::Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("boom"))
            }
        }
        let mut input = io::BufReader::new(Broken);
        let result = confirm_with(true, &mut input, &mut Vec::new(), "q");
        assert!(matches!(result, Err(ConfirmError::Io(ref m)) if m.contains("boom")));
    }
}
