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
    /// Loads the preview of one cursor state of `theme`.
    fn load_state(&self, theme: &CursorTheme, state: CursorState) -> Result<Preview, PreviewError>;

    /// Loads the default arrow, the cursor shown when no state is chosen.
    fn load(&self, theme: &CursorTheme) -> Result<Preview, PreviewError> {
        self.load_state(theme, CursorState::Default)
    }
}

/// A cursor state a theme can draw differently, such as the link hand or the
/// busy indicator.
// The TUI selector that consumes every state lands in the next change.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CursorState {
    Default,
    Pointer,
    Text,
    Wait,
    Progress,
    NotAllowed,
    Help,
    Grabbing,
    Crosshair,
    Move,
    NwseResize,
    EwResize,
}

#[allow(dead_code)]
impl CursorState {
    /// Every state in selector order: the core states first.
    pub const ALL: [CursorState; 12] = [
        CursorState::Default,
        CursorState::Pointer,
        CursorState::Text,
        CursorState::Wait,
        CursorState::Progress,
        CursorState::NotAllowed,
        CursorState::Help,
        CursorState::Grabbing,
        CursorState::Crosshair,
        CursorState::Move,
        CursorState::NwseResize,
        CursorState::EwResize,
    ];

    /// The CSS-style name shown to the user.
    pub fn label(self) -> &'static str {
        match self {
            CursorState::Default => "default",
            CursorState::Pointer => "pointer",
            CursorState::Text => "text",
            CursorState::Wait => "wait",
            CursorState::Progress => "progress",
            CursorState::NotAllowed => "not-allowed",
            CursorState::Help => "help",
            CursorState::Grabbing => "grabbing",
            CursorState::Crosshair => "crosshair",
            CursorState::Move => "move",
            CursorState::NwseResize => "nwse-resize",
            CursorState::EwResize => "ew-resize",
        }
    }

    /// Cursor file names to try, in order: the CSS/freedesktop name first,
    /// then the legacy aliases themes still ship.
    pub fn candidates(self) -> &'static [&'static str] {
        match self {
            CursorState::Default => &["left_ptr", "default", "arrow", "top_left_arrow"],
            CursorState::Pointer => &["pointer", "hand2", "hand1", "pointing_hand", "hand"],
            CursorState::Text => &["text", "xterm", "ibeam"],
            CursorState::Wait => &["wait", "watch"],
            CursorState::Progress => &["progress", "left_ptr_watch", "half-busy"],
            CursorState::NotAllowed => &["not-allowed", "crossed_circle", "forbidden", "no-drop"],
            CursorState::Help => &["help", "question_arrow"],
            CursorState::Grabbing => &["grabbing", "closedhand", "dnd-move"],
            CursorState::Crosshair => &["crosshair"],
            CursorState::Move => &["move", "fleur"],
            CursorState::NwseResize => &["nwse-resize", "bd_double_arrow", "size_fdiag"],
            CursorState::EwResize => &["ew-resize", "sb_h_double_arrow", "size_hor", "col-resize"],
        }
    }

    /// The next state in selector order, wrapping around.
    pub fn next(self) -> CursorState {
        Self::shifted(self, 1)
    }

    /// The previous state in selector order, wrapping around.
    pub fn prev(self) -> CursorState {
        Self::shifted(self, Self::ALL.len() - 1)
    }

    fn shifted(self, by: usize) -> CursorState {
        let index = Self::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Self::ALL[(index + by) % Self::ALL.len()]
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_candidates_keep_the_historic_order() {
        assert_eq!(
            CursorState::Default.candidates(),
            ["left_ptr", "default", "arrow", "top_left_arrow"]
        );
    }

    #[test]
    fn every_state_has_a_label_and_candidates() {
        for state in CursorState::ALL {
            assert!(!state.label().is_empty());
            assert!(!state.candidates().is_empty(), "{state:?}");
        }
    }

    #[test]
    fn css_names_come_before_legacy_aliases() {
        assert_eq!(CursorState::Pointer.candidates()[..2], ["pointer", "hand2"]);
        assert_eq!(CursorState::Progress.candidates()[0], "progress");
        assert_eq!(CursorState::NotAllowed.candidates()[0], "not-allowed");
    }

    #[test]
    fn next_and_prev_cycle_through_every_state() {
        let mut state = CursorState::Default;
        for expected in CursorState::ALL.iter().skip(1) {
            state = state.next();
            assert_eq!(state, *expected);
        }
        assert_eq!(state.next(), CursorState::Default);
        assert_eq!(CursorState::Default.prev(), CursorState::EwResize);
        assert_eq!(CursorState::Pointer.prev(), CursorState::Default);
    }
}
