use std::cell::{Cell, RefCell};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::domain::cursor::{Alias, CursorTheme};
use crate::domain::error::{
    ApplyError, CurrentCursorError, PreviewError, RepositoryError, UpdateError,
};
use crate::domain::ports::{
    BinaryInstaller, Clock, CurrentCursorProvider, CursorApplier, CursorRepository,
    InstallMethodDetector, Preview, PreviewLoader, Relauncher, ReleaseSource, ThemeInspector,
    UpdateStateStore,
};
use crate::domain::update::{InstallMethod, ReleaseInfo, UpdateState};

pub fn theme(name: &str, alias: Option<&str>) -> CursorTheme {
    CursorTheme::new(
        name,
        alias.map(|a| Alias::new(a).unwrap()),
        PathBuf::from(format!("/icons/{name}")),
    )
    .unwrap()
}

#[derive(Default)]
pub struct FakeRepository {
    pub themes: Vec<CursorTheme>,
    pub fail: bool,
}

impl FakeRepository {
    pub fn with(themes: Vec<CursorTheme>) -> Self {
        FakeRepository {
            themes,
            fail: false,
        }
    }

    pub fn failing() -> Self {
        FakeRepository {
            themes: Vec::new(),
            fail: true,
        }
    }

    fn check(&self) -> Result<(), RepositoryError> {
        if self.fail {
            Err(RepositoryError::Storage("boom".to_string()))
        } else {
            Ok(())
        }
    }
}

impl CursorRepository for FakeRepository {
    fn list(&self) -> Result<Vec<CursorTheme>, RepositoryError> {
        self.check()?;
        Ok(self.themes.clone())
    }

    fn add(&mut self, theme: CursorTheme) -> Result<(), RepositoryError> {
        self.check()?;
        self.themes.push(theme);
        Ok(())
    }

    fn remove(&mut self, name: &str) -> Result<(), RepositoryError> {
        self.check()?;
        let before = self.themes.len();
        self.themes.retain(|t| t.name != name);
        if self.themes.len() == before {
            return Err(RepositoryError::NotFound(name.to_string()));
        }
        Ok(())
    }
}

pub type CallLog = Rc<RefCell<Vec<String>>>;

struct FakeApplier {
    label: &'static str,
    fail: bool,
    log: CallLog,
}

pub fn applier(label: &'static str, fail: bool, log: &CallLog) -> Box<dyn CursorApplier> {
    Box::new(FakeApplier {
        label,
        fail,
        log: Rc::clone(log),
    })
}

impl CursorApplier for FakeApplier {
    fn label(&self) -> &'static str {
        self.label
    }

    fn apply(&self, theme: &CursorTheme) -> Result<(), ApplyError> {
        self.log
            .borrow_mut()
            .push(format!("{}:{}", self.label, theme.name));
        if self.fail {
            Err(ApplyError::Command(format!("{} failed", self.label)))
        } else {
            Ok(())
        }
    }
}

pub struct FakeInspector {
    pub valid: bool,
    pub name: Option<String>,
}

impl ThemeInspector for FakeInspector {
    fn is_valid_theme_dir(&self, _path: &Path) -> bool {
        self.valid
    }

    fn dir_name(&self, _path: &Path) -> Option<String> {
        self.name.clone()
    }
}

pub struct FakeCurrent(pub Result<Option<String>, String>);

impl CurrentCursorProvider for FakeCurrent {
    fn current_name(&self) -> Result<Option<String>, CurrentCursorError> {
        self.0.clone().map_err(CurrentCursorError::Unavailable)
    }
}

/// Preview loader that records every requested theme path.
pub struct FakePreviewLoader {
    result: Result<Preview, String>,
    pub loads: Rc<RefCell<Vec<PathBuf>>>,
}

impl FakePreviewLoader {
    pub fn returning(preview: Preview) -> Self {
        FakePreviewLoader {
            result: Ok(preview),
            loads: Rc::default(),
        }
    }

    pub fn failing(message: &str) -> Self {
        FakePreviewLoader {
            result: Err(message.to_string()),
            loads: Rc::default(),
        }
    }
}

impl PreviewLoader for FakePreviewLoader {
    fn load(&self, theme: &CursorTheme) -> Result<Preview, PreviewError> {
        self.loads.borrow_mut().push(theme.path.clone());
        self.result.clone().map_err(PreviewError::Unavailable)
    }
}

pub fn release(version: &str) -> ReleaseInfo {
    ReleaseInfo {
        version: version.to_string(),
        binary_url: format!("https://example.invalid/{version}/bin"),
        checksum_url: format!("https://example.invalid/{version}/bin.sha256"),
    }
}

pub struct FakeReleaseSource {
    pub result: Result<ReleaseInfo, UpdateError>,
    pub calls: Rc<Cell<usize>>,
}

impl FakeReleaseSource {
    pub fn latest(version: &str) -> Self {
        FakeReleaseSource {
            result: Ok(release(version)),
            calls: Rc::default(),
        }
    }

    pub fn failing() -> Self {
        FakeReleaseSource {
            result: Err(UpdateError::Network("offline".into())),
            calls: Rc::default(),
        }
    }
}

impl ReleaseSource for FakeReleaseSource {
    fn latest_stable(&self) -> Result<ReleaseInfo, UpdateError> {
        self.calls.set(self.calls.get() + 1);
        self.result.clone()
    }
}

#[derive(Clone, Default)]
pub struct FakeStateStore {
    pub state: Rc<RefCell<UpdateState>>,
    pub saves: Rc<Cell<usize>>,
    pub fail_save: bool,
}

impl FakeStateStore {
    pub fn with(state: UpdateState) -> Self {
        FakeStateStore {
            state: Rc::new(RefCell::new(state)),
            ..FakeStateStore::default()
        }
    }
}

impl UpdateStateStore for FakeStateStore {
    fn load(&self) -> UpdateState {
        self.state.borrow().clone()
    }

    fn save(&self, state: &UpdateState) -> Result<(), UpdateError> {
        if self.fail_save {
            return Err(UpdateError::Io("read-only".into()));
        }
        self.saves.set(self.saves.get() + 1);
        *self.state.borrow_mut() = state.clone();
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct FakeInstaller {
    pub installed: Rc<RefCell<Vec<ReleaseInfo>>>,
    pub fail: bool,
}

impl BinaryInstaller for FakeInstaller {
    fn install(&self, release: &ReleaseInfo) -> Result<(), UpdateError> {
        if self.fail {
            return Err(UpdateError::ChecksumMismatch {
                expected: "a".into(),
                actual: "b".into(),
            });
        }
        self.installed.borrow_mut().push(release.clone());
        Ok(())
    }
}

pub struct FakeDetector(pub InstallMethod);

impl InstallMethodDetector for FakeDetector {
    fn detect(&self) -> InstallMethod {
        self.0
    }
}

#[derive(Clone, Default)]
pub struct FakeRelauncher {
    pub calls: Rc<RefCell<Vec<Vec<OsString>>>>,
    pub fail: bool,
}

impl Relauncher for FakeRelauncher {
    fn relaunch(&self, args: &[OsString]) -> Result<(), UpdateError> {
        self.calls.borrow_mut().push(args.to_vec());
        if self.fail {
            Err(UpdateError::Relaunch("exec failed".into()))
        } else {
            Ok(())
        }
    }
}

pub struct FakeClock(pub u64);

impl Clock for FakeClock {
    fn now_secs(&self) -> u64 {
        self.0
    }
}
