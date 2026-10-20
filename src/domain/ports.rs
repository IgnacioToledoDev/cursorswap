use std::path::{Path, PathBuf};

use super::cursor::CursorTheme;
use super::error::{
    ApplyError, ConfirmError, CurrentCursorError, PackageError, PreviewError, RepositoryError,
};
use super::package::PackageInfo;

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
