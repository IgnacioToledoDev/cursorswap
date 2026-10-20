//! Composition root: the only place that knows every concrete type.

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use thiserror::Error;

use crate::domain::ports::CursorApplier;
use crate::infrastructure::{
    CommandRunner, EnvsConfCurrentProvider, FsThemeInspector, GsettingsApplier, GtkSettingsApplier,
    HyprEnvsApplier, HyprctlApplier, IconPathApplier, SystemCommandRunner, TomlCursorRepository,
    XcursorPreviewLoader, read_cursor_size, registry_path_from,
};
use crate::presentation::Services;
use crate::presentation::cli::{Cli, execute};
use crate::presentation::exit_code::Exit;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BootstrapError {
    #[error("cannot locate your home directory: set HOME (or XDG_CONFIG_HOME and XDG_DATA_HOME)")]
    NoHome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub registry: PathBuf,
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
pub fn build_services<R: CommandRunner + Clone + 'static>(paths: &Paths, runner: R) -> Services {
    let size = read_cursor_size(&paths.hypr_envs);
    let appliers: Vec<Box<dyn CursorApplier>> = vec![
        Box::new(IconPathApplier::new(
            paths.icon_search_dirs.clone(),
            paths.icon_link_dir.clone(),
        )),
        Box::new(HyprEnvsApplier::new(paths.hypr_envs.clone())),
        Box::new(GtkSettingsApplier::new(paths.gtk_settings.clone())),
        Box::new(GsettingsApplier::new(runner.clone())),
        Box::new(HyprctlApplier::new(runner, size)),
    ];
    Services {
        repo: Box::new(TomlCursorRepository::new(paths.registry.clone())),
        inspector: Box::new(FsThemeInspector),
        current: Box::new(EnvsConfCurrentProvider::new(paths.hypr_envs.clone())),
        preview: Box::new(XcursorPreviewLoader::new()),
        appliers,
    }
}

/// Parses the command line, wires the real adapters and runs the command.
pub fn run(
    args: impl IntoIterator<Item = OsString>,
    env: impl Fn(&str) -> Option<OsString>,
) -> ExitCode {
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(e) => {
            let exit = Exit::from_clap(&e);
            let _ = e.print();
            return ExitCode::from(exit.code());
        }
    };
    let paths = match Paths::from_env(env) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(Exit::Error.code());
        }
    };
    let mut services = build_services(&paths, SystemCommandRunner);
    let exit = execute(cli, &mut services, &mut io::stdout(), &mut io::stderr());
    ExitCode::from(exit.code())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::FakeRunner;

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
        assert_eq!(paths.registry, p("/cfg/cursorswap/registry.toml"));
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
            p("/home/u/.config/cursorswap/registry.toml")
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
            hypr_envs: p("/nonexistent/envs.conf"),
            gtk_settings: p("/nonexistent/settings.ini"),
            icon_search_dirs: vec![],
            icon_link_dir: p("/nonexistent/icons"),
        }
    }

    #[test]
    fn appliers_run_persistent_steps_before_live_ones() {
        let services = build_services(&test_paths(), FakeRunner::ok());
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
        let services = build_services(&paths, runner.clone());
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
        let services = build_services(&test_paths(), FakeRunner::ok());
        let missing =
            crate::domain::cursor::CursorTheme::new("Ghost", None, p("/nonexistent/Ghost"))
                .unwrap();
        // The Xcursor loader reports a missing cursors directory as unavailable.
        assert!(matches!(
            services.preview.load(&missing),
            Err(crate::domain::error::PreviewError::Unavailable(_))
        ));
    }
}
