use std::io::{BufRead, Write};
use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::application::add::AddInput;
use crate::application::error::ApplicationError;
use crate::application::{add, apply, current, list, remove};

use super::Services;
use super::exit_code::Exit;
use super::update::{self, Flow};
use super::{output, tui};

const AFTER_HELP: &str = "\
A theme named like a subcommand (list, add, remove, current, update) cannot be \
selected by name; select it through its alias instead.

A new stable release is looked up on startup when run from a terminal; \
disable it with --no-update-check, CURSORSWAP_NO_UPDATE_CHECK=1 or \
update_check = false in config.toml.

Exit codes:
  0  success (every step applied, or a non-apply command succeeded)
  1  error (I/O, invalid input, registry error, or every step failed)
  2  theme not found
  3  partial apply (some steps failed, some succeeded)
  10 update available (only from `update --check`)";

#[derive(Debug, Parser, PartialEq, Eq)]
#[command(
    name = "cursorswap",
    version,
    about = "Switch the system cursor theme by name or alias",
    args_conflicts_with_subcommands = true,
    after_help = AFTER_HELP
)]
pub struct Cli {
    /// Theme name or alias to apply; with no arguments the TUI opens
    pub query: Option<String>,
    /// Do not check for a newer release on startup
    #[arg(long, global = true)]
    pub no_update_check: bool,
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
    /// Check for a newer release and install it
    Update {
        /// Only report whether an update is available (exit 10 if so)
        #[arg(long, conflicts_with = "yes")]
        check: bool,
        /// Install without asking for confirmation
        #[arg(long)]
        yes: bool,
    },
}

/// Runs the parsed command line against `services`, writing results to `out`
/// and diagnostics to `err`. `input` answers the update prompts.
pub fn execute(
    cli: Cli,
    services: &mut Services,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Exit {
    let wants_startup_check = !matches!(
        (&cli.command, &cli.query),
        (Some(Command::Update { .. }), _) | (None, None)
    );
    // The terminal UI runs its own background check instead.
    if wants_startup_check
        && let Flow::Exit(exit) = update::startup_prompt(&services.update, input, err)
    {
        return exit;
    }
    let result = match (cli.command, cli.query) {
        (Some(Command::Update { check, yes }), _) => Ok(update::run_update_command(
            &services.update,
            check,
            yes,
            input,
            out,
            err,
        )),
        (Some(command), _) => run_command(command, services, out),
        (None, Some(query)) => apply_query(&query, services, out),
        (None, None) => match tui::run(services) {
            Ok(finish) => Ok(finish_tui(finish, &services.update, err)),
            // The TUI has no application-level error type; report and map to a generic failure.
            Err(e) => Err((Exit::Error, e.to_string())),
        },
    };
    match result {
        Ok(exit) => exit,
        Err((exit, message)) => {
            emit(err, &format!("error: {message}\n"));
            exit
        }
    }
}

/// Completes a TUI session. An accepted update installs and restarts the TUI
/// on the new binary, now that the terminal is back to normal.
fn finish_tui(finish: tui::Finish, update: &update::UpdateServices, err: &mut dyn Write) -> Exit {
    match finish {
        tui::Finish::Quit => Exit::Success,
        tui::Finish::Update(_) => match update::install_and_relaunch(update, err) {
            Flow::Exit(exit) => exit,
            Flow::Continue => Exit::Error,
        },
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
        Command::Update { .. } => unreachable!("handled before run_command"),
    }
    Ok(Exit::Success)
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("cursorswap").chain(args.iter().copied()))
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
    fn update_subcommand_and_flags() {
        let update = |check, yes| Some(Command::Update { check, yes });
        assert_eq!(parse(&["update"]).unwrap().command, update(false, false));
        assert_eq!(
            parse(&["update", "--check"]).unwrap().command,
            update(true, false)
        );
        assert_eq!(
            parse(&["update", "--yes"]).unwrap().command,
            update(false, true)
        );
        assert!(parse(&["update", "--check", "--yes"]).is_err());
        assert!(parse(&["update", "--bogus"]).is_err());
    }

    #[test]
    fn no_update_check_flag_works_everywhere() {
        assert!(!parse(&[]).unwrap().no_update_check);
        for args in [
            &["--no-update-check"][..],
            &["--no-update-check", "Nordzy"],
            &["Nordzy", "--no-update-check"],
            &["--no-update-check", "list"],
            &["list", "--no-update-check"],
            &["update", "--no-update-check"],
        ] {
            assert!(parse(args).unwrap().no_update_check, "{args:?}");
        }
    }

    #[test]
    fn help_documents_update_and_its_exit_code() {
        let help = parse(&["--help"]).unwrap_err().to_string();
        for needle in ["update", "--no-update-check", "10"] {
            assert!(help.contains(needle), "missing {needle:?}");
        }
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
        CallLog, FakeCurrent, FakeInspector, FakePreviewLoader, FakeRepository, applier, theme,
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
            preview: Box::new(FakePreviewLoader::failing("no preview in cli tests")),
            appliers,
            update: crate::presentation::update::fixture::disabled(),
        };
        let cli =
            Cli::try_parse_from(std::iter::once("cursorswap").chain(args.iter().copied())).unwrap();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let exit = execute(
            cli,
            &mut services,
            &mut std::io::empty(),
            &mut out,
            &mut err,
        );
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
        let cli = Cli::try_parse_from(["cursorswap", "list"]).unwrap();
        let mut out = Vec::new();
        let exit = execute(
            cli,
            &mut services,
            &mut std::io::empty(),
            &mut out,
            &mut Vec::new(),
        );
        assert_eq!(exit, Exit::Success);
        assert!(!String::from_utf8(out).unwrap().contains('*'));
    }

    #[test]
    fn list_of_empty_registry_prints_the_hint() {
        let r = run_with(&["list"], vec![], &[], None).0;
        assert_eq!(r.exit, Exit::Success);
        assert!(r.out.contains("cursorswap add"));
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
        let cli = Cli::try_parse_from(["cursorswap", "current"]).unwrap();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(
            execute(
                cli,
                &mut services,
                &mut std::io::empty(),
                &mut out,
                &mut err
            ),
            Exit::Error
        );
        assert!(out.is_empty());
        assert!(!err.is_empty());
    }

    #[test]
    fn add_then_list_shows_the_new_theme() {
        let (r, mut services) = run_with(&["add", "/some/dir", "--alias", "dd"], vec![], &[], None);
        assert_eq!(r.exit, Exit::Success);
        assert_eq!(r.out, "added Dir (alias dd)\n");
        let cli = Cli::try_parse_from(["cursorswap", "list"]).unwrap();
        let mut out = Vec::new();
        execute(
            cli,
            &mut services,
            &mut std::io::empty(),
            &mut out,
            &mut Vec::new(),
        );
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
        let cli = Cli::try_parse_from(["cursorswap", "list"]).unwrap();
        let mut out = Vec::new();
        execute(
            cli,
            &mut services,
            &mut std::io::empty(),
            &mut out,
            &mut Vec::new(),
        );
        assert!(!String::from_utf8(out).unwrap().contains("Nordzy"));
    }

    #[test]
    fn remove_unknown_exits_2() {
        let r = run(&["remove", "ghost"], &[]);
        assert_eq!(r.exit, Exit::NotFound);
    }
}

#[cfg(test)]
mod update_flow_tests {
    use std::ffi::OsString;

    use super::*;
    use crate::application::fakes::{
        CallLog, FakeCurrent, FakeInspector, FakePreviewLoader, FakeRepository, applier, theme,
    };
    use crate::application::update::{CheckOptions, UpdateStatus};
    use crate::presentation::update::fixture::{Options, Probe, available, build};

    struct Run {
        exit: Exit,
        out: String,
        err: String,
        log: CallLog,
        probe: Probe,
    }

    fn run(args: &[&str], status: UpdateStatus, answer: &str) -> Run {
        let (update, probe) = build(Options::with(status)).into_parts();
        let log = CallLog::default();
        let mut services = Services {
            repo: Box::new(FakeRepository::with(vec![theme("Nordzy", Some("nz"))])),
            inspector: Box::new(FakeInspector {
                valid: true,
                name: None,
            }),
            current: Box::new(FakeCurrent(Ok(None))),
            preview: Box::new(FakePreviewLoader::failing("none")),
            appliers: vec![applier("a", false, &log)],
            update,
        };
        let cli =
            Cli::try_parse_from(std::iter::once("cursorswap").chain(args.iter().copied())).unwrap();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let exit = execute(
            cli,
            &mut services,
            &mut answer.as_bytes(),
            &mut out,
            &mut err,
        );
        Run {
            exit,
            out: String::from_utf8(out).unwrap(),
            err: String::from_utf8(err).unwrap(),
            log,
            probe,
        }
    }

    #[test]
    fn accepted_update_replaces_the_command_with_a_restart() {
        let r = run(&["nz"], available(), "y\n");
        assert_eq!(r.exit, Exit::Success);
        assert!(
            r.log.borrow().is_empty(),
            "the old binary must not apply the theme"
        );
        assert_eq!(
            *r.probe.relaunches.borrow(),
            [vec![OsString::from("Nordzy")]]
        );
        assert!(r.err.contains("Update now?"));
    }

    fn checks(r: &Run) -> Vec<CheckOptions> {
        r.probe.checks.lock().unwrap().clone()
    }

    #[test]
    fn declined_update_runs_the_requested_command() {
        for answer in ["n\n", "s\n", "huh\n", ""] {
            let r = run(&["nz"], available(), answer);
            assert_eq!(r.exit, Exit::Success, "{answer:?}");
            assert_eq!(*r.log.borrow(), ["a:Nordzy"], "{answer:?}");
            assert!(r.probe.relaunches.borrow().is_empty());
        }
    }

    #[test]
    fn skip_is_remembered() {
        let r = run(&["nz"], available(), "s\n");
        assert_eq!(
            r.probe.store.state.borrow().skipped.as_deref(),
            Some("0.2.0")
        );
    }

    #[test]
    fn non_apply_commands_are_prompted_too() {
        let r = run(&["list"], available(), "n\n");
        assert!(r.err.contains("Update now?"));
        assert_eq!(checks(&r), [CheckOptions::AUTO]);
        assert!(r.out.contains("Nordzy"));
    }

    #[test]
    fn up_to_date_is_silent() {
        let status = UpdateStatus::UpToDate {
            latest: "0.1.0".into(),
        };
        let r = run(&["nz"], status, "");
        assert!(r.err.is_empty());
        assert_eq!(*r.log.borrow(), ["a:Nordzy"]);
    }

    #[test]
    fn update_command_skips_the_startup_prompt() {
        let r = run(&["update", "--check"], available(), "y\n");
        assert_eq!(r.exit, Exit::UpdateAvailable);
        assert_eq!(checks(&r), [CheckOptions::MANUAL]);
        assert!(r.probe.installed.borrow().is_empty());
        assert!(!r.err.contains("Update now?"));
    }

    #[test]
    fn update_yes_installs_through_execute() {
        let r = run(&["update", "--yes"], available(), "");
        assert_eq!(r.exit, Exit::Success);
        assert_eq!(r.probe.installed.borrow().len(), 1);
        assert!(r.out.contains("0.2.0"));
    }

    #[test]
    fn finishing_the_tui_without_an_update_succeeds() {
        let (update, probe) = build(Options::with(available())).into_parts();
        let exit = finish_tui(tui::Finish::Quit, &update, &mut Vec::new());
        assert_eq!(exit, Exit::Success);
        assert!(probe.installed.borrow().is_empty());
    }

    #[test]
    fn accepting_the_tui_update_installs_and_restarts_the_tui() {
        let mut options = Options::with(available());
        options.args = vec![];
        let (update, probe) = build(options).into_parts();
        let mut err = Vec::new();
        let exit = finish_tui(tui::Finish::Update("0.2.0".into()), &update, &mut err);
        assert_eq!(exit, Exit::Success);
        assert_eq!(probe.installed.borrow().len(), 1);
        assert_eq!(*probe.relaunches.borrow(), [Vec::<OsString>::new()]);
    }

    #[test]
    fn a_failed_tui_update_is_an_error_exit_with_a_message() {
        let mut options = Options::with(available());
        options.install_fails = true;
        let (update, probe) = build(options).into_parts();
        let mut err = Vec::new();
        let exit = finish_tui(tui::Finish::Update("0.2.0".into()), &update, &mut err);
        assert_eq!(exit, Exit::Error);
        assert!(String::from_utf8(err).unwrap().contains("update failed"));
        assert!(probe.relaunches.borrow().is_empty());
    }
}
