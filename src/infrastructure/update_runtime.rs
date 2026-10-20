//! Process-level update adapters: install method, restart and clock.

use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::error::UpdateError;
use crate::domain::ports::{Clock, InstallMethodDetector, Relauncher};
use crate::domain::update::{InstallMethod, NO_UPDATE_ENV, classify_install_path};

/// Classifies the executable path captured at startup. When the path is
/// unknown the install is treated as managed, so it is never overwritten.
pub struct ExePathDetector {
    exe: Option<PathBuf>,
}

impl ExePathDetector {
    pub fn new(exe: Option<PathBuf>) -> Self {
        ExePathDetector { exe }
    }
}

impl InstallMethodDetector for ExePathDetector {
    fn detect(&self) -> InstallMethod {
        self.exe
            .as_deref()
            .map_or(InstallMethod::PackageManager, classify_install_path)
    }
}

/// Replaces the process with the (already updated) executable. The path is
/// captured before the update because the old inode's path disappears.
pub struct ExecRelauncher {
    exe: PathBuf,
}

impl ExecRelauncher {
    pub fn new(exe: PathBuf) -> Self {
        ExecRelauncher { exe }
    }
}

impl Relauncher for ExecRelauncher {
    fn relaunch(&self, args: &[OsString]) -> Result<(), UpdateError> {
        // The new process skips the check so a failed upgrade cannot loop.
        let error = Command::new(&self.exe)
            .args(args)
            .env(NO_UPDATE_ENV, "1")
            .exec();
        Err(UpdateError::Relaunch(error.to_string()))
    }
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_secs(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detector_classifies_the_captured_path() {
        let cases = [
            ("/usr/bin/cursorswap", InstallMethod::PackageManager),
            ("/home/u/.cargo/bin/cursorswap", InstallMethod::Cargo),
            ("/home/u/.local/bin/cursorswap", InstallMethod::Standalone),
        ];
        for (path, expected) in cases {
            let detector = ExePathDetector::new(Some(PathBuf::from(path)));
            assert_eq!(detector.detect(), expected, "{path}");
        }
    }

    #[test]
    fn unknown_path_is_treated_as_managed() {
        assert_eq!(
            ExePathDetector::new(None).detect(),
            InstallMethod::PackageManager
        );
    }

    #[test]
    fn clock_reports_a_plausible_unix_time() {
        assert!(SystemClock.now_secs() > 1_700_000_000);
    }

    #[test]
    fn relaunching_a_missing_binary_reports_an_error() {
        let relauncher = ExecRelauncher::new(PathBuf::from("/nonexistent/cursorswap"));
        assert!(matches!(
            relauncher.relaunch(&[]),
            Err(UpdateError::Relaunch(_))
        ));
    }
}
