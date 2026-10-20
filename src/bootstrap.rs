//! Composition root: the only place that knows every concrete type.

use std::ffi::OsString;
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use thiserror::Error;

use crate::application::update::{CheckOptions, check_for_update};
use crate::domain::ports::CursorApplier;
use crate::domain::update::{CheckEnv, NO_UPDATE_ENV, disabled_reason, env_flag};
use crate::infrastructure::{
    CommandRunner, EnvsConfCurrentProvider, ExeInstaller, ExePathDetector, ExecRelauncher,
    FsThemeInspector, GithubReleaseSource, GsettingsApplier, GtkSettingsApplier, HyprEnvsApplier,
    HyprctlApplier, IconPathApplier, SystemClock, SystemCommandRunner, TerminalConfirm,
    TomlCursorRepository, TomlUpdateStateStore, UreqHttp, XcursorPreviewLoader, YayPackageManager,
    read_cursor_size, registry_path_from, update_check_enabled,
};
use crate::presentation::Services;
use crate::presentation::cli::{Cli, execute};
use crate::presentation::exit_code::Exit;
use crate::presentation::update::UpdateServices;

const CHECK_TIMEOUT: Duration = Duration::from_secs(2);
const MANUAL_TIMEOUT: Duration = Duration::from_secs(10);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BootstrapError {
    #[error("cannot locate your home directory: set HOME (or XDG_CONFIG_HOME and XDG_DATA_HOME)")]
    NoHome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub registry: PathBuf,
    pub config_file: PathBuf,
    pub update_state: PathBuf,
    pub hypr_envs: PathBuf,
    pub gtk_settings: PathBuf,
    pub icon_search_dirs: Vec<PathBuf>,
    pub icon_link_dir: PathBuf,
}

impl Paths {
    pub fn from_env(env: impl Fn(&str) -> Option<OsString>) -> Result<Paths, BootstrapError> {
        let non_empty = |key: &str| env(key).filter(|v| !v.is_empty()).map(PathBuf::from);
        let home = non_empty("HOME");
        let config = non_empty("XDG_CONFIG_HOME")
            .or_else(|| home.as_ref().map(|h| h.join(".config")))
            .ok_or(BootstrapError::NoHome)?;
        let data = non_empty("XDG_DATA_HOME")
            .or_else(|| home.as_ref().map(|h| h.join(".local/share")))
            .ok_or(BootstrapError::NoHome)?;
        let registry = registry_path_from(&env).ok_or(BootstrapError::NoHome)?;

        let icon_link_dir = data.join("icons");
        let mut icon_search_dirs = vec![PathBuf::from("/usr/share/icons"), icon_link_dir.clone()];
        icon_search_dirs.extend(home.map(|h| h.join(".icons")));

        Ok(Paths {
            config_file: registry.with_file_name("config.toml"),
            update_state: registry.with_file_name("update-state.toml"),
            registry,
            hypr_envs: config.join("hypr/envs.conf"),
            gtk_settings: config.join("gtk-3.0/settings.ini"),
            icon_search_dirs,
            icon_link_dir,
        })
    }
}

/// Wires the concrete adapters. Applier order matters: persistent changes
/// first, the live session change last.
pub fn build_services<R: CommandRunner + Clone + 'static>(
    paths: &Paths,
    runner: R,
    update: UpdateServices,
) -> Services {
    let size = read_cursor_size(&paths.hypr_envs);
    let appliers: Vec<Box<dyn CursorApplier>> = vec![
        Box::new(IconPathApplier::new(
            paths.icon_search_dirs.clone(),
            paths.icon_link_dir.clone(),
        )),
        Box::new(HyprEnvsApplier::new(paths.hypr_envs.clone())),
        Box::new(GtkSettingsApplier::new(paths.gtk_settings.clone())),
        Box::new(GsettingsApplier::new(runner.clone())),
        Box::new(HyprctlApplier::new(runner.clone(), size)),
    ];
    Services {
        repo: Box::new(TomlCursorRepository::new(paths.registry.clone())),
        inspector: Box::new(FsThemeInspector),
        current: Box::new(EnvsConfCurrentProvider::new(paths.hypr_envs.clone())),
        preview: Box::new(XcursorPreviewLoader::new()),
        appliers,
        searcher: Box::new(YayPackageManager::new(runner.clone())),
        packages: Box::new(YayPackageManager::new(runner)),
        confirm: Box::new(TerminalConfirm),
        update,
    }
}

/// Collects the facts that decide whether the startup check may run.
pub fn check_env_from(
    env: &impl Fn(&str) -> Option<OsString>,
    flag_opt_out: bool,
    interactive: bool,
    config_enabled: bool,
) -> CheckEnv {
    let text = |key: &str| env(key).map(|v| v.to_string_lossy().into_owned());
    CheckEnv {
        flag_opt_out,
        env_opt_out: env_flag(text(NO_UPDATE_ENV).as_deref()),
        config_opt_out: !config_enabled,
        ci: env_flag(text("CI").as_deref()),
        interactive,
    }
}

/// Wires the update adapters. The network adapters are only used when the
/// presentation layer decides to check, so a disabled run never touches them.
pub fn build_update_services(
    paths: &Paths,
    enabled: bool,
    args: Vec<OsString>,
    exe: Option<PathBuf>,
) -> UpdateServices {
    let installed = env!("CARGO_PKG_VERSION");
    let state_path = paths.update_state.clone();
    let detected = exe.clone();
    let check = Arc::new(move |options: CheckOptions| {
        let timeout = if options.use_cache {
            CHECK_TIMEOUT
        } else {
            MANUAL_TIMEOUT
        };
        check_for_update(
            &GithubReleaseSource::new(UreqHttp::new(timeout)),
            &TomlUpdateStateStore::new(state_path.clone()),
            &ExePathDetector::new(detected.clone()),
            &SystemClock,
            installed,
            options,
        )
    });
    let exe = exe.unwrap_or_default();
    UpdateServices {
        enabled,
        installed: installed.to_string(),
        args,
        check,
        source: Box::new(GithubReleaseSource::new(UreqHttp::new(MANUAL_TIMEOUT))),
        installer: Box::new(ExeInstaller::new(
            UreqHttp::new(DOWNLOAD_TIMEOUT),
            exe.clone(),
        )),
        store: Box::new(TomlUpdateStateStore::new(paths.update_state.clone())),
        relauncher: Box::new(ExecRelauncher::new(exe)),
    }
}

/// Parses the command line, wires the real adapters and runs the command.
pub fn run(
    args: impl IntoIterator<Item = OsString>,
    env: impl Fn(&str) -> Option<OsString>,
) -> ExitCode {
    let args: Vec<OsString> = args.into_iter().collect();
    let cli = match Cli::try_parse_from(args.clone()) {
        Ok(cli) => cli,
        Err(e) => {
            let exit = Exit::from_clap(&e);
            let _ = e.print();
            return ExitCode::from(exit.code());
        }
    };
    let paths = match Paths::from_env(&env) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(Exit::Error.code());
        }
    };
    let interactive = io::stdin().is_terminal() && io::stdout().is_terminal();
    let check_env = check_env_from(
        &env,
        cli.no_update_check,
        interactive,
        update_check_enabled(&paths.config_file),
    );
    let update = build_update_services(
        &paths,
        disabled_reason(&check_env).is_none(),
        args.into_iter().skip(1).collect(),
        std::env::current_exe().ok(),
    );
    let mut services = build_services(&paths, SystemCommandRunner, update);
    let exit = execute(
        cli,
        &mut services,
        &mut io::stdin().lock(),
        &mut io::stdout(),
        &mut io::stderr(),
    );
    ExitCode::from(exit.code())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::FakeRunner;
    use crate::presentation::update::fixture;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key| {
            owned
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| OsString::from(v))
        }
    }

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn xdg_variables_take_precedence() {
        let env = env_of(&[
            ("XDG_CONFIG_HOME", "/cfg"),
            ("XDG_DATA_HOME", "/data"),
            ("HOME", "/home/u"),
        ]);
        let paths = Paths::from_env(env).unwrap();
        assert_eq!(paths.registry, p("/cfg/cursorswap/conf/registry.toml"));
        assert_eq!(paths.config_file, p("/cfg/cursorswap/conf/config.toml"));
        assert_eq!(
            paths.update_state,
            p("/cfg/cursorswap/conf/update-state.toml")
        );
        assert_eq!(paths.hypr_envs, p("/cfg/hypr/envs.conf"));
        assert_eq!(paths.gtk_settings, p("/cfg/gtk-3.0/settings.ini"));
        assert_eq!(paths.icon_link_dir, p("/data/icons"));
        assert_eq!(
            paths.icon_search_dirs,
            [p("/usr/share/icons"), p("/data/icons"), p("/home/u/.icons")]
        );
    }

    #[test]
    fn home_only_uses_dot_config_and_local_share() {
        let paths = Paths::from_env(env_of(&[("HOME", "/home/u")])).unwrap();
        assert_eq!(
            paths.registry,
            p("/home/u/.config/cursorswap/conf/registry.toml")
        );
        assert_eq!(paths.hypr_envs, p("/home/u/.config/hypr/envs.conf"));
        assert_eq!(paths.icon_link_dir, p("/home/u/.local/share/icons"));
    }

    #[test]
    fn empty_xdg_values_are_ignored() {
        let env = env_of(&[
            ("XDG_CONFIG_HOME", ""),
            ("XDG_DATA_HOME", ""),
            ("HOME", "/home/u"),
        ]);
        let paths = Paths::from_env(env).unwrap();
        assert_eq!(
            paths.gtk_settings,
            p("/home/u/.config/gtk-3.0/settings.ini")
        );
        assert_eq!(paths.icon_link_dir, p("/home/u/.local/share/icons"));
    }

    #[test]
    fn xdg_without_home_omits_the_dot_icons_dir() {
        let env = env_of(&[("XDG_CONFIG_HOME", "/cfg"), ("XDG_DATA_HOME", "/data")]);
        let paths = Paths::from_env(env).unwrap();
        assert_eq!(
            paths.icon_search_dirs,
            [p("/usr/share/icons"), p("/data/icons")]
        );
    }

    #[test]
    fn no_home_and_no_xdg_is_an_error() {
        assert_eq!(Paths::from_env(env_of(&[])), Err(BootstrapError::NoHome));
        let only_data = env_of(&[("XDG_DATA_HOME", "/data")]);
        assert_eq!(Paths::from_env(only_data), Err(BootstrapError::NoHome));
        let only_cfg = env_of(&[("XDG_CONFIG_HOME", "/cfg")]);
        assert_eq!(Paths::from_env(only_cfg), Err(BootstrapError::NoHome));
    }

    fn test_paths() -> Paths {
        Paths {
            registry: p("/nonexistent/registry.toml"),
            config_file: p("/nonexistent/config.toml"),
            update_state: p("/nonexistent/update-state.toml"),
            hypr_envs: p("/nonexistent/envs.conf"),
            gtk_settings: p("/nonexistent/settings.ini"),
            icon_search_dirs: vec![],
            icon_link_dir: p("/nonexistent/icons"),
        }
    }

    #[test]
    fn appliers_run_persistent_steps_before_live_ones() {
        let services = build_services(&test_paths(), FakeRunner::ok(), fixture::disabled());
        let labels: Vec<_> = services.appliers.iter().map(|a| a.label()).collect();
        assert_eq!(
            labels,
            [
                "icon-path",
                "hypr-envs",
                "gtk-settings",
                "gsettings",
                "hyprctl"
            ]
        );
    }

    #[test]
    fn hyprctl_uses_the_size_from_envs_conf() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut paths = test_paths();
        paths.hypr_envs = dir.path().join("envs.conf");
        std::fs::write(&paths.hypr_envs, "env = XCURSOR_SIZE,40\n").unwrap();
        let runner = FakeRunner::ok();
        let services = build_services(&paths, runner.clone(), fixture::disabled());
        let theme =
            crate::domain::cursor::CursorTheme::new("Nordzy", None, p("/icons/Nordzy")).unwrap();
        services.appliers[4].apply(&theme).unwrap();
        assert_eq!(
            *runner.calls.borrow(),
            [["hyprctl", "setcursor", "Nordzy", "40"]]
        );
    }

    #[test]
    fn services_carry_the_real_preview_loader() {
        let services = build_services(&test_paths(), FakeRunner::ok(), fixture::disabled());
        let missing =
            crate::domain::cursor::CursorTheme::new("Ghost", None, p("/nonexistent/Ghost"))
                .unwrap();
        // The Xcursor loader reports a missing cursors directory as unavailable.
        assert!(matches!(
            services.preview.load(&missing),
            Err(crate::domain::error::PreviewError::Unavailable(_))
        ));
    }

    #[test]
    fn check_env_reads_ci_and_the_opt_out_variable() {
        let base = check_env_from(&env_of(&[]), false, true, true);
        assert_eq!(
            base,
            CheckEnv {
                interactive: true,
                ..CheckEnv::default()
            }
        );
        let ci = check_env_from(&env_of(&[("CI", "true")]), false, true, true);
        assert!(ci.ci && !ci.env_opt_out);
        let opted = check_env_from(
            &env_of(&[("CURSORSWAP_NO_UPDATE_CHECK", "1")]),
            false,
            true,
            true,
        );
        assert!(opted.env_opt_out && !opted.ci);
        let off = check_env_from(
            &env_of(&[("CURSORSWAP_NO_UPDATE_CHECK", "0"), ("CI", "false")]),
            false,
            true,
            true,
        );
        assert!(!off.env_opt_out && !off.ci);
    }

    #[test]
    fn check_env_carries_flag_config_and_terminal_state() {
        let env = check_env_from(&env_of(&[]), true, false, false);
        assert!(env.flag_opt_out && env.config_opt_out && !env.interactive);
    }

    #[test]
    fn update_services_keep_the_run_context() {
        let args = vec![OsString::from("Nordzy")];
        let services = build_update_services(&test_paths(), true, args.clone(), None);
        assert!(services.enabled);
        assert_eq!(services.installed, env!("CARGO_PKG_VERSION"));
        assert_eq!(services.args, args);
        let disabled = build_update_services(&test_paths(), false, vec![], None);
        assert!(!disabled.enabled);
    }

    #[test]
    fn package_services_go_through_the_command_runner() {
        let runner = FakeRunner::ok();
        let services = build_services(&test_paths(), runner.clone(), fixture::disabled());
        services.searcher.search("cursor").unwrap();
        services.packages.install("some-cursors").unwrap();
        services.packages.installed_files("some-cursors").unwrap();
        assert_eq!(
            *runner.calls.borrow(),
            [
                vec!["yay", "-Ss", "cursor"],
                vec!["pacman", "-Ql", "some-cursors"]
            ]
        );
        assert_eq!(
            *runner.interactive_calls.borrow(),
            [vec!["yay", "-S", "some-cursors"]]
        );
    }
}
