use crate::domain::cursor::CursorTheme;
use crate::domain::error::ApplyError;
use crate::domain::ports::CursorApplier;

use super::command::{CommandRunner, run_checked};
use super::name::system_theme_name;

pub const DEFAULT_CURSOR_SIZE: u32 = 24;

/// Applies the cursor to the running Hyprland session.
pub struct HyprctlApplier<R> {
    runner: R,
    size: u32,
}

impl<R: CommandRunner> HyprctlApplier<R> {
    pub fn new(runner: R, size: u32) -> Self {
        Self { runner, size }
    }
}

impl<R: CommandRunner> CursorApplier for HyprctlApplier<R> {
    fn label(&self) -> &'static str {
        "hyprctl"
    }

    fn apply(&self, theme: &CursorTheme) -> Result<(), ApplyError> {
        let name = system_theme_name(theme)?;
        run_checked(
            &self.runner,
            "hyprctl",
            &["setcursor", &name, &self.size.to_string()],
        )
    }
}

/// Sets the GNOME/GTK interface cursor theme through gsettings.
pub struct GsettingsApplier<R> {
    runner: R,
}

impl<R: CommandRunner> GsettingsApplier<R> {
    pub fn new(runner: R) -> Self {
        Self { runner }
    }
}

impl<R: CommandRunner> CursorApplier for GsettingsApplier<R> {
    fn label(&self) -> &'static str {
        "gsettings"
    }

    fn apply(&self, theme: &CursorTheme) -> Result<(), ApplyError> {
        let name = system_theme_name(theme)?;
        run_checked(
            &self.runner,
            "gsettings",
            &["set", "org.gnome.desktop.interface", "cursor-theme", &name],
        )
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::infrastructure::appliers::command::fake::{Behavior, FakeRunner};

    fn theme(name: &str, path: &str) -> CursorTheme {
        CursorTheme::new(name, None, PathBuf::from(path)).unwrap()
    }

    #[test]
    fn labels_are_stable() {
        assert_eq!(HyprctlApplier::new(FakeRunner::ok(), 24).label(), "hyprctl");
        assert_eq!(GsettingsApplier::new(FakeRunner::ok()).label(), "gsettings");
    }

    #[test]
    fn hyprctl_runs_setcursor_with_name_and_size() {
        let runner = FakeRunner::ok();
        let applier = HyprctlApplier::new(runner.clone(), 32);
        applier
            .apply(&theme("Pretty", "/icons/Nordzy-cursors"))
            .unwrap();
        assert_eq!(
            *runner.calls.borrow(),
            vec![vec!["hyprctl", "setcursor", "Nordzy-cursors", "32"]]
        );
    }

    #[test]
    fn hyprctl_non_zero_exit_is_error_with_stderr() {
        let runner = FakeRunner::new(Behavior::ExitNonZero("not running".into()));
        let err = HyprctlApplier::new(runner, 24)
            .apply(&theme("x", "/icons/x"))
            .unwrap_err();
        assert!(err.to_string().contains("not running"));
    }

    #[test]
    fn hyprctl_spawn_error_is_error() {
        let runner = FakeRunner::new(Behavior::SpawnError);
        assert!(
            HyprctlApplier::new(runner, 24)
                .apply(&theme("x", "/icons/x"))
                .is_err()
        );
    }

    #[test]
    fn hyprctl_rejects_unsafe_name_without_running() {
        let runner = FakeRunner::ok();
        let result = HyprctlApplier::new(runner.clone(), 24).apply(&theme("x", "/icons/-bad"));
        assert!(result.is_err());
        assert!(runner.calls.borrow().is_empty());
    }

    #[test]
    fn gsettings_sets_cursor_theme_key() {
        let runner = FakeRunner::ok();
        GsettingsApplier::new(runner.clone())
            .apply(&theme("Pretty", "/icons/Nordzy-cursors"))
            .unwrap();
        assert_eq!(
            *runner.calls.borrow(),
            vec![vec![
                "gsettings",
                "set",
                "org.gnome.desktop.interface",
                "cursor-theme",
                "Nordzy-cursors"
            ]]
        );
    }

    #[test]
    fn gsettings_non_zero_exit_is_error_with_stderr() {
        let runner = FakeRunner::new(Behavior::ExitNonZero("no schema".into()));
        let err = GsettingsApplier::new(runner)
            .apply(&theme("x", "/icons/x"))
            .unwrap_err();
        assert!(err.to_string().contains("no schema"));
    }

    #[test]
    fn gsettings_spawn_error_is_error() {
        let runner = FakeRunner::new(Behavior::SpawnError);
        assert!(
            GsettingsApplier::new(runner)
                .apply(&theme("x", "/icons/x"))
                .is_err()
        );
    }
}
