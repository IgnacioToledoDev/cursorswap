use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::cursor::CursorTheme;
use super::error::{
    ApplyError, ConfirmError, CurrentCursorError, PackageError, PreviewError, RepositoryError,
    UpdateError,
};
use super::package::PackageInfo;
use super::update::{InstallMethod, ReleaseInfo, UpdateState};

pub trait CursorRepository {
    fn list(&self) -> Result<Vec<CursorTheme>, RepositoryError>;
    fn add(&mut self, theme: CursorTheme) -> Result<(), RepositoryError>;
    fn remove(&mut self, name: &str) -> Result<(), RepositoryError>;
}

pub trait CursorApplier {
    fn label(&self) -> &'static str;
    fn apply(&self, theme: &CursorTheme) -> Result<(), ApplyError>;
}

pub trait PreviewLoader {
    fn load(&self, theme: &CursorTheme) -> Result<Preview, PreviewError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub frames: Vec<PreviewFrame>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub delay_ms: u32,
}

/// Decides whether a directory can be registered as a cursor theme.
pub trait ThemeInspector {
    fn is_valid_theme_dir(&self, path: &Path) -> bool;
    fn dir_name(&self, path: &Path) -> Option<String>;
}

/// Reports the name of the cursor theme that is currently active.
pub trait CurrentCursorProvider {
    fn current_name(&self) -> Result<Option<String>, CurrentCursorError>;
}

/// Finds the latest stable release.
pub trait ReleaseSource {
    fn latest_stable(&self) -> Result<ReleaseInfo, UpdateError>;
}

/// Persists what the update check remembers. Loading never fails: missing or
/// unreadable data reads as the default state.
pub trait UpdateStateStore {
    fn load(&self) -> UpdateState;
    fn save(&self, state: &UpdateState) -> Result<(), UpdateError>;
}

/// Downloads a release, verifies its checksum and replaces the running binary.
/// Any failure leaves the existing binary untouched.
pub trait BinaryInstaller {
    fn install(&self, release: &ReleaseInfo) -> Result<(), UpdateError>;
}

pub trait InstallMethodDetector {
    fn detect(&self) -> InstallMethod;
}

/// Replaces the current process with the installed binary. On success it does
/// not return in production; an `Err` means the restart failed.
pub trait Relauncher {
    fn relaunch(&self, args: &[OsString]) -> Result<(), UpdateError>;
}

pub trait Clock {
    fn now_secs(&self) -> u64;
}

/// Searches a package index for packages matching a term.
pub trait PackageSearch {
    fn search(&self, term: &str) -> Result<Vec<PackageInfo>, PackageError>;
}

/// Installs packages and reports what they put on disk.
pub trait PackageManager {
    /// Fails with `PackageError::ToolMissing` when the installer is unavailable.
    fn ensure_available(&self) -> Result<(), PackageError>;
    /// Installs `package` interactively: the installer talks to the user itself.
    fn install(&self, package: &str) -> Result<(), PackageError>;
    /// Lists every path owned by an installed `package`.
    fn installed_files(&self, package: &str) -> Result<Vec<PathBuf>, PackageError>;
}

/// Asks the user a yes/no question. The default answer is no.
pub trait Confirm {
    fn confirm(&self, prompt: &str) -> Result<bool, ConfirmError>;
}
