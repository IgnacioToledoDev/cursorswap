use std::io::Write;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::application::add::AddInput;
use crate::application::error::ApplicationError;
use crate::application::{add, apply, current, list, remove};

use super::Services;
use super::exit_code::Exit;
use super::{output, tui};

const AFTER_HELP: &str = "\
A theme named like a subcommand (list, add, remove, current) cannot be \
selected by name; select it through its alias instead.

Exit codes:
  0  success (every step applied, or a non-apply command succeeded)
  1  error (I/O, invalid input, registry error, or every step failed)
  2  theme not found
  3  partial apply (some steps failed, some succeeded)";

#[derive(Debug, Parser, PartialEq, Eq)]
#[command(
    name = "cambia-cursor",
    version,
    about = "Switch the system cursor theme by name or alias",
    args_conflicts_with_subcommands = true,
    after_help = AFTER_HELP
)]
pub struct Cli {
    /// Theme name or alias to apply; with no arguments the TUI opens
    pub query: Option<String>,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    /// List registered themes
    List,
    /// Register a theme directory
    Add {
        /// Theme directory (must contain index.theme and cursors/)
        path: PathBuf,
        /// Theme name (defaults to the directory name)
        #[arg(long)]
        name: Option<String>,
        /// Short alias: 1-8 ASCII alphanumeric characters
        #[arg(long)]
        alias: Option<String>,
    },
    /// Unregister a theme by name or alias
    Remove { query: String },
    /// Show the active theme
    Current,
}

/// Runs the parsed command line against `services`, writing results to `out`
/// and diagnostics to `err`.
pub fn execute(
    cli: Cli,
    services: &mut Services,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Exit {
    let result = match (cli.command, cli.query) {
        (Some(command), _) => run_command(command, services, out),
        (None, Some(query)) => apply_query(&query, services, out),
        (None, None) => tui::run(services).map(|()| Exit::Success).map_err(|e| {
            // The TUI has no application-level error type; report and map to a generic failure.
            (Exit::Error, e.to_string())
        }),
    };
    match result {
        Ok(exit) => exit,
        Err((exit, message)) => {
            emit(err, &format!("error: {message}\n"));
            exit
        }
    }
}

type Outcome = Result<Exit, (Exit, String)>;

fn fail(e: ApplicationError) -> (Exit, String) {
    (Exit::from_error(&e), e.to_string())
}

fn emit(out: &mut dyn Write, text: &str) {
    // A closed pipe must not turn a finished command into a failure.
    let _ = out.write_all(text.as_bytes());
}

fn apply_query(query: &str, services: &mut Services, out: &mut dyn Write) -> Outcome {
    let report = apply::apply(services.repo.as_ref(), &services.appliers, query).map_err(fail)?;
    emit(out, &output::format_apply(&report));
    Ok(Exit::from_apply(report.status()))
}

fn run_command(command: Command, services: &mut Services, out: &mut dyn Write) -> Outcome {
    match command {
        Command::List => {
            let themes = list::list(services.repo.as_ref()).map_err(fail)?;
            // The active marker is cosmetic: an unreadable config must not hide the list.
            let active = services.current.current_name().ok().flatten();
            emit(out, &output::format_list(&themes, active.as_deref()));
        }
        Command::Current => {
            let current = current::current(services.repo.as_ref(), services.current.as_ref())
                .map_err(fail)?;
            emit(out, &format!("{}\n", output::format_current(&current)));
        }
        Command::Add { path, name, alias } => {
            let input = AddInput { path, name, alias };
            let theme = add::add(services.repo.as_mut(), services.inspector.as_ref(), input)
                .map_err(fail)?;
            emit(out, &format!("{}\n", output::format_added(&theme)));
        }
        Command::Remove { query } => {
            let theme = remove::remove(services.repo.as_mut(), &query).map_err(fail)?;
            emit(out, &format!("{}\n", output::format_removed(&theme)));
        }
    }
    Ok(Exit::Success)
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("cambia-cursor").chain(args.iter().copied()))
    }

    #[test]
    fn no_args_selects_nothing() {
        let cli = parse(&[]).unwrap();
        assert_eq!(cli.query, None);
        assert_eq!(cli.command, None);
    }

    #[test]
    fn positional_query() {
        let cli = parse(&["Nordzy"]).unwrap();
        assert_eq!(cli.query.as_deref(), Some("Nordzy"));
        assert_eq!(cli.command, None);
    }

    #[test]
    fn list_subcommand() {
        assert_eq!(parse(&["list"]).unwrap().command, Some(Command::List));
    }

    #[test]
    fn current_subcommand() {
        assert_eq!(parse(&["current"]).unwrap().command, Some(Command::Current));
    }

    #[test]
    fn add_with_path_only() {
        assert_eq!(
            parse(&["add", "/x/y"]).unwrap().command,
            Some(Command::Add {
                path: PathBuf::from("/x/y"),
                name: None,
                alias: None
            })
        );
    }

    #[test]
    fn add_with_flags() {
        let cli = parse(&["add", "/x/y", "--name", "N", "--alias", "n"]).unwrap();
        assert_eq!(
            cli.command,
            Some(Command::Add {
                path: PathBuf::from("/x/y"),
                name: Some("N".into()),
                alias: Some("n".into())
            })
        );
    }

    #[test]
    fn add_without_path_is_rejected() {
        assert!(parse(&["add"]).is_err());
        assert!(parse(&["add", "--alias", "n"]).is_err());
    }

    #[test]
    fn remove_takes_a_query() {
        assert_eq!(
            parse(&["remove", "nz"]).unwrap().command,
            Some(Command::Remove { query: "nz".into() })
        );
        assert!(parse(&["remove"]).is_err());
    }

    #[test]
    fn query_and_subcommand_cannot_be_mixed() {
        assert!(parse(&["Nordzy", "list"]).is_err());
    }

    #[test]
    fn unknown_flag_is_rejected() {
        assert!(parse(&["--bogus"]).is_err());
    }

    #[test]
    fn help_footer_documents_exit_codes_and_reserved_names() {
        let help = parse(&["--help"]).unwrap_err().to_string();
        for needle in ["Exit codes", "3  ", "alias"] {
            assert!(help.contains(needle), "missing {needle:?}");
        }
    }
}

#[cfg(test)]
mod execute_tests {
    use super::*;
    use crate::application::fakes::{
        CallLog, FakeCurrent, FakeInspector, FakeRepository, applier, theme,
    };

    struct Run {
        exit: Exit,
        out: String,
        err: String,
        log: CallLog,
    }

    fn run_with(
        args: &[&str],
        themes: Vec<crate::domain::cursor::CursorTheme>,
        failing: &[bool],
        current: Option<&str>,
    ) -> (Run, Services) {
        let log = CallLog::default();
        let appliers = failing
            .iter()
            .enumerate()
            .map(|(i, f)| applier(["a", "b", "c"][i], *f, &log))
            .collect();
        let mut services = Services {
            repo: Box::new(FakeRepository::with(themes)),
            inspector: Box::new(FakeInspector {
                valid: true,
                name: Some("Dir".to_string()),
            }),
            current: Box::new(FakeCurrent(Ok(current.map(String::from)))),
            appliers,
        };
        let cli = Cli::try_parse_from(std::iter::once("cambia-cursor").chain(args.iter().copied()))
            .unwrap();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let exit = execute(cli, &mut services, &mut out, &mut err);
        let run = Run {
            exit,
            out: String::from_utf8(out).unwrap(),
            err: String::from_utf8(err).unwrap(),
            log,
        };
        (run, services)
    }

    fn themes() -> Vec<crate::domain::cursor::CursorTheme> {
        vec![theme("Nordzy", Some("nz")), theme("Bibata", None)]
    }

    fn run(args: &[&str], failing: &[bool]) -> Run {
        run_with(args, themes(), failing, Some("Bibata")).0
    }

    #[test]
    fn apply_by_name() {
        let r = run(&["bibata"], &[false, false]);
        assert_eq!(r.exit, Exit::Success);
        assert_eq!(*r.log.borrow(), ["a:Bibata", "b:Bibata"]);
        assert!(r.out.contains("ok a\nok b\n"));
        assert!(r.out.contains("applied Bibata: 2 of 2"));
        assert!(r.err.is_empty());
    }

    #[test]
    fn apply_by_alias() {
        let r = run(&["NZ"], &[false]);
        assert_eq!(r.exit, Exit::Success);
        assert_eq!(*r.log.borrow(), ["a:Nordzy"]);
    }

    #[test]
    fn apply_not_found_exits_2_and_runs_nothing() {
        let r = run(&["missing"], &[false]);
        assert_eq!(r.exit, Exit::NotFound);
        assert_eq!(r.exit.code(), 2);
        assert!(r.log.borrow().is_empty());
        assert!(r.out.is_empty());
        assert!(r.err.starts_with("error: "));
        assert!(r.err.contains("missing"));
    }

    #[test]
    fn apply_partial_exits_3_and_reports_both_kinds() {
        let r = run(&["nz"], &[true, false]);
        assert_eq!(r.exit, Exit::Partial);
        assert!(r.out.contains("failed a: "));
        assert!(r.out.contains("ok b"));
    }

    #[test]
    fn apply_all_failed_exits_1() {
        let r = run(&["nz"], &[true, true]);
        assert_eq!(r.exit, Exit::Error);
        assert_eq!(r.log.borrow().len(), 2);
    }

    #[test]
    fn list_marks_the_active_theme() {
        let r = run(&["list"], &[]);
        assert_eq!(r.exit, Exit::Success);
        assert_eq!(
            r.out,
            "  Nordzy  nz  /icons/Nordzy\n* Bibata  -  /icons/Bibata\n"
        );
    }

    #[test]
    fn list_survives_an_unavailable_current_provider() {
        let mut services = run_with(&["list"], themes(), &[], None).1;
        services.current = Box::new(FakeCurrent(Err("nope".into())));
        let cli = Cli::try_parse_from(["cambia-cursor", "list"]).unwrap();
        let mut out = Vec::new();
        let exit = execute(cli, &mut services, &mut out, &mut Vec::new());
        assert_eq!(exit, Exit::Success);
        assert!(!String::from_utf8(out).unwrap().contains('*'));
    }

    #[test]
    fn list_of_empty_registry_prints_the_hint() {
        let r = run_with(&["list"], vec![], &[], None).0;
        assert_eq!(r.exit, Exit::Success);
        assert!(r.out.contains("cambia-cursor add"));
    }

    #[test]
    fn current_reports_registered_theme() {
        let r = run_with(&["current"], themes(), &[], Some("nordzy")).0;
        assert_eq!(r.out, "Nordzy (alias nz)\n");
        let r = run_with(&["current"], themes(), &[], Some("Adwaita")).0;
        assert_eq!(r.out, "Adwaita (not registered)\n");
        let r = run_with(&["current"], themes(), &[], None).0;
        assert_eq!(r.out, "none\n");
    }

    #[test]
    fn current_provider_failure_is_an_error() {
        let mut services = run_with(&["current"], themes(), &[], None).1;
        services.current = Box::new(FakeCurrent(Err("nope".into())));
        let cli = Cli::try_parse_from(["cambia-cursor", "current"]).unwrap();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(execute(cli, &mut services, &mut out, &mut err), Exit::Error);
        assert!(out.is_empty());
        assert!(!err.is_empty());
    }

    #[test]
    fn add_then_list_shows_the_new_theme() {
        let (r, mut services) = run_with(&["add", "/some/dir", "--alias", "dd"], vec![], &[], None);
        assert_eq!(r.exit, Exit::Success);
        assert_eq!(r.out, "added Dir (alias dd)\n");
        let cli = Cli::try_parse_from(["cambia-cursor", "list"]).unwrap();
        let mut out = Vec::new();
        execute(cli, &mut services, &mut out, &mut Vec::new());
        assert_eq!(String::from_utf8(out).unwrap(), "  Dir  dd  /some/dir\n");
    }

    #[test]
    fn add_failure_goes_to_stderr_with_exit_1() {
        let r = run_with(&["add", "/x", "--alias", "list"], vec![], &[], None).0;
        assert_eq!(r.exit, Exit::Error);
        assert!(r.out.is_empty());
        assert!(r.err.contains("reserved"));
    }

    #[test]
    fn remove_by_alias() {
        let (r, mut services) = run_with(&["remove", "nz"], themes(), &[], None);
        assert_eq!(r.exit, Exit::Success);
        assert_eq!(r.out, "removed Nordzy\n");
        let cli = Cli::try_parse_from(["cambia-cursor", "list"]).unwrap();
        let mut out = Vec::new();
        execute(cli, &mut services, &mut out, &mut Vec::new());
        assert!(!String::from_utf8(out).unwrap().contains("Nordzy"));
    }

    #[test]
    fn remove_unknown_exits_2() {
        let r = run(&["remove", "ghost"], &[]);
        assert_eq!(r.exit, Exit::NotFound);
    }

    #[test]
    fn no_arguments_open_the_tui_placeholder() {
        let r = run(&[], &[false]);
        assert_eq!(r.exit, Exit::Error);
        assert!(r.err.contains("not implemented"));
        assert!(r.log.borrow().is_empty());
    }
}
