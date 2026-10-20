//! Terminal flow for self-update: the startup prompt and the `update` command.

use std::ffi::OsString;
use std::io::{BufRead, Write};
use std::sync::Arc;

use crate::application::update::{
    CheckOptions, UpdateOutcome, UpdateStatus, apply_update, skip_version,
};
use crate::domain::ports::{BinaryInstaller, Relauncher, ReleaseSource, UpdateStateStore};
use crate::domain::update::{Answer, parse_answer, update_hint};

use super::exit_code::Exit;

pub type StatusCheck = Arc<dyn Fn(CheckOptions) -> UpdateStatus + Send + Sync>;

/// Everything the presentation layer needs for self-update.
pub struct UpdateServices {
    /// Whether the automatic startup check may run at all.
    pub enabled: bool,
    pub installed: String,
    /// Arguments of this run, replayed on the new binary after an update.
    pub args: Vec<OsString>,
    pub check: StatusCheck,
    pub source: Box<dyn ReleaseSource>,
    pub installer: Box<dyn BinaryInstaller>,
    pub store: Box<dyn UpdateStateStore>,
    pub relauncher: Box<dyn Relauncher>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Carry on with the requested command.
    Continue,
    /// The command is finished (or replaced by a restart) with this status.
    Exit(Exit),
}

/// Offers an available update before a command runs. Never fails the command.
pub fn startup_prompt(
    update: &UpdateServices,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Flow {
    if !update.enabled {
        return Flow::Continue;
    }
    match (update.check)(CheckOptions::AUTO) {
        UpdateStatus::Available { latest } => {
            say(out, &announcement(&latest, &update.installed));
            say(out, "Update now? [y]es / [n]o / [s]kip this version ");
            match read_answer(input) {
                Answer::Yes => install_and_relaunch(update, out),
                Answer::Skip => {
                    match skip_version(update.store.as_ref(), &latest) {
                        Ok(()) => say(out, &format!("Skipping {latest}.\n")),
                        Err(e) => say(out, &format!("could not remember the choice: {e}\n")),
                    }
                    Flow::Continue
                }
                Answer::No => Flow::Continue,
            }
        }
        UpdateStatus::Managed { latest, method } => {
            say(out, &announcement(&latest, &update.installed));
            say(out, &format!("{}\n", update_hint(method)));
            Flow::Continue
        }
        _ => Flow::Continue,
    }
}

/// Installs the latest release and restarts on it. `Continue` means the
/// restart did not happen (the reason was already printed).
pub fn install_and_relaunch(update: &UpdateServices, out: &mut dyn Write) -> Flow {
    say(out, "Updating cursorswap...\n");
    match apply_update(
        update.source.as_ref(),
        update.installer.as_ref(),
        &update.installed,
    ) {
        Ok(UpdateOutcome::Updated { version }) => {
            say(out, &format!("Updated to {version}. Restarting...\n"));
            match update.relauncher.relaunch(&update.args) {
                Ok(()) => Flow::Exit(Exit::Success),
                Err(e) => {
                    say(out, &format!("updated, but {e}; run cursorswap again\n"));
                    Flow::Continue
                }
            }
        }
        Ok(UpdateOutcome::AlreadyCurrent) => {
            say(out, "cursorswap is already up to date.\n");
            Flow::Continue
        }
        Err(e) => {
            say(out, &format!("update failed: {e}\n"));
            Flow::Continue
        }
    }
}

/// `cursorswap update [--check] [--yes]`.
pub fn run_update_command(
    update: &UpdateServices,
    check_only: bool,
    yes: bool,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Exit {
    let installed = update.installed.as_str();
    let versions = |latest: &str| format!("installed: {installed}\nlatest: {latest}\n");
    match (update.check)(CheckOptions::MANUAL) {
        UpdateStatus::Unavailable(reason) => {
            say(
                err,
                &format!("error: could not check for updates: {reason}\n"),
            );
            Exit::Error
        }
        UpdateStatus::UpToDate { latest } => {
            say(
                out,
                &format!("{}cursorswap is up to date.\n", versions(&latest)),
            );
            Exit::Success
        }
        UpdateStatus::Managed { latest, method } => {
            let hint = update_hint(method);
            if check_only {
                say(
                    out,
                    &format!("{}An update is available. {hint}\n", versions(&latest)),
                );
                Exit::UpdateAvailable
            } else {
                say(err, &format!("error: cannot update in place. {hint}\n"));
                Exit::Error
            }
        }
        UpdateStatus::Available { latest } | UpdateStatus::Skipped { latest } => {
            if check_only {
                say(
                    out,
                    &format!("{}An update is available.\n", versions(&latest)),
                );
                return Exit::UpdateAvailable;
            }
            if !yes {
                say(
                    out,
                    &format!("Update cursorswap {installed} -> {latest}? [y/N] "),
                );
                if read_answer(input) != Answer::Yes {
                    say(out, "Update cancelled.\n");
                    return Exit::Success;
                }
            }
            match apply_update(update.source.as_ref(), update.installer.as_ref(), installed) {
                Ok(UpdateOutcome::Updated { version }) => {
                    say(out, &format!("Updated cursorswap to {version}.\n"));
                    Exit::Success
                }
                Ok(UpdateOutcome::AlreadyCurrent) => {
                    say(out, "cursorswap is up to date.\n");
                    Exit::Success
                }
                Err(e) => {
                    say(err, &format!("error: {e}\n"));
                    Exit::Error
                }
            }
        }
    }
}

fn announcement(latest: &str, installed: &str) -> String {
    format!("A new version of cursorswap is available: {latest} (you have {installed}).\n")
}

fn say(out: &mut dyn Write, text: &str) {
    // A closed pipe must not turn a finished command into a failure.
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}

fn read_answer(input: &mut dyn BufRead) -> Answer {
    let mut line = String::new();
    // EOF and read errors decline, which is the safe default.
    let _ = input.read_line(&mut line);
    parse_answer(&line)
}

#[cfg(test)]
pub(crate) mod fixture {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Mutex;

    use super::*;
    use crate::application::fakes::{
        FakeInstaller, FakeRelauncher, FakeReleaseSource, FakeStateStore,
    };
    use crate::domain::update::ReleaseInfo;

    /// What the fakes recorded.
    pub struct Probe {
        pub installed: Rc<RefCell<Vec<ReleaseInfo>>>,
        pub relaunches: Rc<RefCell<Vec<Vec<OsString>>>>,
        pub store: FakeStateStore,
        pub checks: Arc<Mutex<Vec<CheckOptions>>>,
    }

    pub struct Fixture {
        pub services: UpdateServices,
        probe: Probe,
    }

    impl std::ops::Deref for Fixture {
        type Target = Probe;

        fn deref(&self) -> &Probe {
            &self.probe
        }
    }

    impl Fixture {
        pub fn into_parts(self) -> (UpdateServices, Probe) {
            (self.services, self.probe)
        }
    }

    pub struct Options {
        pub enabled: bool,
        pub status: UpdateStatus,
        pub install_fails: bool,
        pub relaunch_fails: bool,
        pub args: Vec<&'static str>,
    }

    impl Options {
        pub fn with(status: UpdateStatus) -> Options {
            Options {
                enabled: true,
                status,
                install_fails: false,
                relaunch_fails: false,
                args: vec!["Nordzy"],
            }
        }
    }

    pub fn available() -> UpdateStatus {
        UpdateStatus::Available {
            latest: "0.2.0".into(),
        }
    }

    pub fn build(options: Options) -> Fixture {
        let installer = FakeInstaller {
            fail: options.install_fails,
            ..FakeInstaller::default()
        };
        let relauncher = FakeRelauncher {
            fail: options.relaunch_fails,
            ..FakeRelauncher::default()
        };
        let store = FakeStateStore::default();
        let checks = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&checks);
        let status = options.status;
        Fixture {
            probe: Probe {
                installed: Rc::clone(&installer.installed),
                relaunches: Rc::clone(&relauncher.calls),
                store: store.clone(),
                checks,
            },
            services: UpdateServices {
                enabled: options.enabled,
                installed: "0.1.0".into(),
                args: options.args.iter().map(OsString::from).collect(),
                check: Arc::new(move |opts| {
                    log.lock().unwrap().push(opts);
                    status.clone()
                }),
                source: Box::new(FakeReleaseSource::latest("0.2.0")),
                installer: Box::new(installer),
                store: Box::new(store),
                relauncher: Box::new(relauncher),
            },
        }
    }

    /// Update services that never check, for tests unrelated to updating.
    pub fn disabled() -> UpdateServices {
        let mut options = Options::with(UpdateStatus::Unavailable("disabled".into()));
        options.enabled = false;
        build(options).services
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::{Options, available, build};
    use super::*;
    use crate::domain::update::InstallMethod;

    struct Run {
        flow: Flow,
        out: String,
    }

    fn prompt(options: Options, answer: &str) -> (Run, super::fixture::Fixture) {
        let f = build(options);
        let mut out = Vec::new();
        let flow = startup_prompt(&f.services, &mut answer.as_bytes(), &mut out);
        let run = Run {
            flow,
            out: String::from_utf8(out).unwrap(),
        };
        (run, f)
    }

    fn offer(answer: &str) -> (Run, super::fixture::Fixture) {
        prompt(Options::with(available()), answer)
    }

    #[test]
    fn disabled_startup_check_does_nothing() {
        let mut options = Options::with(available());
        options.enabled = false;
        let (run, f) = prompt(options, "y\n");
        assert_eq!(run.flow, Flow::Continue);
        assert!(run.out.is_empty());
        assert!(f.checks.lock().unwrap().is_empty());
    }

    #[test]
    fn prompt_text_names_both_versions_and_the_choices() {
        let (run, _) = offer("n\n");
        assert!(
            run.out
                .contains("A new version of cursorswap is available: 0.2.0 (you have 0.1.0).")
        );
        assert!(
            run.out
                .contains("Update now? [y]es / [n]o / [s]kip this version")
        );
    }

    #[test]
    fn startup_check_uses_the_cached_automatic_mode() {
        let (_, f) = offer("n\n");
        assert_eq!(*f.checks.lock().unwrap(), [CheckOptions::AUTO]);
    }

    #[test]
    fn yes_installs_and_relaunches_with_the_same_arguments() {
        let (run, f) = offer("y\n");
        assert_eq!(run.flow, Flow::Exit(Exit::Success));
        assert_eq!(f.installed.borrow().len(), 1);
        assert_eq!(*f.relaunches.borrow(), [vec![OsString::from("Nordzy")]]);
    }

    #[test]
    fn relaunch_preserves_multiple_arguments_in_order() {
        let mut options = Options::with(available());
        options.args = vec!["add", "/x/y", "--alias", "n"];
        let (_, f) = prompt(options, "yes\n");
        let expected: Vec<OsString> = ["add", "/x/y", "--alias", "n"].map(OsString::from).into();
        assert_eq!(*f.relaunches.borrow(), [expected]);
    }

    #[test]
    fn no_continues_without_installing_or_skipping() {
        let (run, f) = offer("n\n");
        assert_eq!(run.flow, Flow::Continue);
        assert!(f.installed.borrow().is_empty());
        assert!(f.relaunches.borrow().is_empty());
        assert_eq!(f.store.saves.get(), 0);
    }

    #[test]
    fn skip_stores_the_version_and_continues() {
        let (run, f) = offer("s\n");
        assert_eq!(run.flow, Flow::Continue);
        assert_eq!(f.store.state.borrow().skipped.as_deref(), Some("0.2.0"));
        assert!(f.installed.borrow().is_empty());
    }

    #[test]
    fn garbage_and_eof_decline() {
        for answer in ["maybe\n", "\n", ""] {
            let (run, f) = offer(answer);
            assert_eq!(run.flow, Flow::Continue, "{answer:?}");
            assert!(f.installed.borrow().is_empty());
            assert_eq!(f.store.saves.get(), 0);
        }
    }

    #[test]
    fn failed_install_reports_and_continues_without_relaunching() {
        let mut options = Options::with(available());
        options.install_fails = true;
        let (run, f) = prompt(options, "y\n");
        assert_eq!(run.flow, Flow::Continue);
        assert!(run.out.contains("update failed"), "{}", run.out);
        assert!(f.relaunches.borrow().is_empty());
    }

    #[test]
    fn failed_relaunch_continues_with_the_current_process() {
        let mut options = Options::with(available());
        options.relaunch_fails = true;
        let (run, _) = prompt(options, "y\n");
        assert_eq!(run.flow, Flow::Continue);
        assert!(run.out.contains("restart"), "{}", run.out);
    }

    #[test]
    fn managed_install_gets_a_notice_and_no_prompt() {
        for (method, needle) in [
            (InstallMethod::PackageManager, "package manager"),
            (InstallMethod::Cargo, "cargo install"),
        ] {
            let status = UpdateStatus::Managed {
                latest: "0.2.0".into(),
                method,
            };
            let (run, f) = prompt(Options::with(status), "y\n");
            assert_eq!(run.flow, Flow::Continue);
            assert!(run.out.contains("0.2.0"));
            assert!(run.out.contains(needle), "{}", run.out);
            assert!(!run.out.contains("Update now?"));
            assert!(f.installed.borrow().is_empty());
        }
    }

    #[test]
    fn quiet_statuses_print_nothing() {
        let statuses = [
            UpdateStatus::UpToDate {
                latest: "0.1.0".into(),
            },
            UpdateStatus::Skipped {
                latest: "0.2.0".into(),
            },
            UpdateStatus::Unavailable("offline".into()),
        ];
        for status in statuses {
            let (run, f) = prompt(Options::with(status), "y\n");
            assert_eq!(run.flow, Flow::Continue);
            assert!(run.out.is_empty());
            assert!(f.installed.borrow().is_empty());
        }
    }

    struct Cmd {
        exit: Exit,
        out: String,
        err: String,
    }

    fn command(
        status: UpdateStatus,
        check_only: bool,
        yes: bool,
        answer: &str,
    ) -> (Cmd, super::fixture::Fixture) {
        let f = build(Options::with(status));
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let exit = run_update_command(
            &f.services,
            check_only,
            yes,
            &mut answer.as_bytes(),
            &mut out,
            &mut err,
        );
        let cmd = Cmd {
            exit,
            out: String::from_utf8(out).unwrap(),
            err: String::from_utf8(err).unwrap(),
        };
        (cmd, f)
    }

    fn up_to_date() -> UpdateStatus {
        UpdateStatus::UpToDate {
            latest: "0.1.0".into(),
        }
    }

    fn managed() -> UpdateStatus {
        UpdateStatus::Managed {
            latest: "0.2.0".into(),
            method: InstallMethod::PackageManager,
        }
    }

    #[test]
    fn check_exits_zero_when_up_to_date() {
        let (cmd, f) = command(up_to_date(), true, false, "");
        assert_eq!(cmd.exit, Exit::Success);
        assert!(cmd.out.contains("installed: 0.1.0"));
        assert!(cmd.out.contains("latest: 0.1.0"));
        assert_eq!(*f.checks.lock().unwrap(), [CheckOptions::MANUAL]);
    }

    #[test]
    fn check_exits_ten_when_an_update_is_available() {
        let (cmd, f) = command(available(), true, false, "");
        assert_eq!(cmd.exit, Exit::UpdateAvailable);
        assert_eq!(cmd.exit.code(), 10);
        assert!(cmd.out.contains("latest: 0.2.0"));
        assert!(f.installed.borrow().is_empty());
    }

    #[test]
    fn check_reports_availability_for_managed_installs_with_the_hint() {
        let (cmd, _) = command(managed(), true, false, "");
        assert_eq!(cmd.exit, Exit::UpdateAvailable);
        assert!(cmd.out.contains("package manager"), "{}", cmd.out);
    }

    #[test]
    fn check_exits_one_on_error() {
        let (cmd, _) = command(UpdateStatus::Unavailable("offline".into()), true, false, "");
        assert_eq!(cmd.exit, Exit::Error);
        assert!(cmd.err.contains("offline"));
    }

    #[test]
    fn update_when_current_installs_nothing() {
        let (cmd, f) = command(up_to_date(), false, true, "");
        assert_eq!(cmd.exit, Exit::Success);
        assert!(cmd.out.contains("up to date"));
        assert!(f.installed.borrow().is_empty());
    }

    #[test]
    fn update_yes_installs_without_asking_and_does_not_relaunch() {
        let (cmd, f) = command(available(), false, true, "");
        assert_eq!(cmd.exit, Exit::Success);
        assert_eq!(f.installed.borrow().len(), 1);
        assert!(f.relaunches.borrow().is_empty());
        assert!(cmd.out.contains("0.2.0"));
        assert!(!cmd.out.contains('?'), "{}", cmd.out);
    }

    #[test]
    fn update_asks_and_installs_on_yes() {
        let (cmd, f) = command(available(), false, false, "y\n");
        assert_eq!(cmd.exit, Exit::Success);
        assert!(cmd.out.contains('?'));
        assert_eq!(f.installed.borrow().len(), 1);
    }

    #[test]
    fn update_declined_or_eof_installs_nothing() {
        for answer in ["n\n", "s\n", "what\n", ""] {
            let (cmd, f) = command(available(), false, false, answer);
            assert_eq!(cmd.exit, Exit::Success, "{answer:?}");
            assert!(f.installed.borrow().is_empty(), "{answer:?}");
            assert!(cmd.out.contains("cancelled"));
        }
    }

    #[test]
    fn update_refuses_managed_installs_with_the_hint() {
        let (cmd, f) = command(managed(), false, true, "");
        assert_eq!(cmd.exit, Exit::Error);
        assert!(f.installed.borrow().is_empty());
        assert!(cmd.err.contains("package manager"), "{}", cmd.err);
    }

    #[test]
    fn update_failures_exit_one() {
        let (cmd, _) = command(UpdateStatus::Unavailable("offline".into()), false, true, "");
        assert_eq!(cmd.exit, Exit::Error);
        let mut options = Options::with(available());
        options.install_fails = true;
        let f = build(options);
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let exit = run_update_command(
            &f.services,
            false,
            true,
            &mut "".as_bytes(),
            &mut out,
            &mut err,
        );
        assert_eq!(exit, Exit::Error);
        assert!(String::from_utf8(err).unwrap().contains("checksum"));
    }
}
