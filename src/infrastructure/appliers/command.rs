use std::io;
use std::process::Command;

use crate::domain::error::ApplyError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub success: bool,
    pub stderr: String,
}

/// Runs an external program. Implementations must never go through a shell.
pub trait CommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, io::Error>;
}

pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, io::Error> {
        let output = Command::new(program).args(args).output()?;
        Ok(CommandOutput {
            success: output.status.success(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}

/// Runs `program args` and maps a spawn failure or non-zero exit to `ApplyError::Command`.
pub fn run_checked(
    runner: &dyn CommandRunner,
    program: &str,
    args: &[&str],
) -> Result<(), ApplyError> {
    let shown = std::iter::once(program)
        .chain(args.iter().copied())
        .collect::<Vec<_>>()
        .join(" ");
    match runner.run(program, args) {
        Ok(out) if out.success => Ok(()),
        Ok(out) => Err(ApplyError::Command(format!(
            "`{shown}` exited with failure: {}",
            out.stderr
        ))),
        Err(e) => Err(ApplyError::Command(format!("cannot run `{shown}`: {e}"))),
    }
}

#[cfg(test)]
pub mod fake {
    use std::cell::RefCell;
    use std::io;
    use std::rc::Rc;

    use super::{CommandOutput, CommandRunner};

    #[derive(Clone)]
    pub enum Behavior {
        Succeed,
        ExitNonZero(String),
        SpawnError,
    }

    /// Records every call as `[program, arg, ...]`; clones share the same log.
    #[derive(Clone)]
    pub struct FakeRunner {
        pub calls: Rc<RefCell<Vec<Vec<String>>>>,
        behavior: Behavior,
    }

    impl FakeRunner {
        pub fn new(behavior: Behavior) -> Self {
            Self {
                calls: Rc::new(RefCell::new(Vec::new())),
                behavior,
            }
        }

        pub fn ok() -> Self {
            Self::new(Behavior::Succeed)
        }
    }

    impl CommandRunner for FakeRunner {
        fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, io::Error> {
            let mut call = vec![program.to_string()];
            call.extend(args.iter().map(|a| a.to_string()));
            self.calls.borrow_mut().push(call);
            match &self.behavior {
                Behavior::Succeed => Ok(CommandOutput {
                    success: true,
                    stderr: String::new(),
                }),
                Behavior::ExitNonZero(stderr) => Ok(CommandOutput {
                    success: false,
                    stderr: stderr.clone(),
                }),
                Behavior::SpawnError => {
                    Err(io::Error::new(io::ErrorKind::NotFound, "no such program"))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{Behavior, FakeRunner};
    use super::*;

    #[test]
    fn system_runner_reports_success_and_failure() {
        assert!(SystemCommandRunner.run("true", &[]).unwrap().success);
        assert!(!SystemCommandRunner.run("false", &[]).unwrap().success);
    }

    #[test]
    fn system_runner_passes_args_without_a_shell() {
        // A shell would split on `;` and run `false`, failing the command.
        let out = SystemCommandRunner.run("true", &["; false"]).unwrap();
        assert!(out.success);
    }

    #[test]
    fn system_runner_captures_stderr() {
        let out = SystemCommandRunner
            .run("ls", &["/definitely/not/a/real/path"])
            .unwrap();
        assert!(!out.success);
        assert!(!out.stderr.is_empty());
    }

    #[test]
    fn system_runner_spawn_error_is_io_error() {
        assert!(
            SystemCommandRunner
                .run("cambia-cursor-no-such-binary", &[])
                .is_err()
        );
    }

    #[test]
    fn run_checked_ok_on_success() {
        let runner = FakeRunner::ok();
        assert!(run_checked(&runner, "tool", &["a", "b"]).is_ok());
        assert_eq!(*runner.calls.borrow(), vec![vec!["tool", "a", "b"]]);
    }

    #[test]
    fn run_checked_includes_stderr_on_non_zero() {
        let runner = FakeRunner::new(Behavior::ExitNonZero("boom".into()));
        let err = run_checked(&runner, "tool", &[]).unwrap_err();
        assert!(matches!(err, ApplyError::Command(_)));
        assert!(err.to_string().contains("boom"));
        assert!(err.to_string().contains("tool"));
    }

    #[test]
    fn run_checked_maps_spawn_error() {
        let runner = FakeRunner::new(Behavior::SpawnError);
        let err = run_checked(&runner, "tool", &[]).unwrap_err();
        assert!(matches!(err, ApplyError::Command(_)));
        assert!(err.to_string().contains("tool"));
    }
}
