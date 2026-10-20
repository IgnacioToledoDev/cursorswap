use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::domain::cursor::{Alias, CursorTheme};
use crate::domain::error::{
    ApplyError, ConfirmError, CurrentCursorError, PackageError, PreviewError, RepositoryError,
};
use crate::domain::package::PackageInfo;
use crate::domain::ports::{
    Confirm, CurrentCursorProvider, CursorApplier, CursorRepository, PackageManager, PackageSearch,
    Preview, PreviewLoader, ThemeInspector,
};

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

/// Inspector that accepts only the listed directories and names them by their last component.
pub struct PathInspector(pub Vec<PathBuf>);

impl ThemeInspector for PathInspector {
    fn is_valid_theme_dir(&self, path: &Path) -> bool {
        self.0.iter().any(|p| p == path)
    }

    fn dir_name(&self, path: &Path) -> Option<String> {
        path.file_name().map(|n| n.to_string_lossy().into_owned())
    }
}

/// Package search and installer double; every call is appended to `log`.
pub struct FakePackages {
    pub log: CallLog,
    pub results: Vec<PackageInfo>,
    pub files: Vec<PathBuf>,
    pub missing_tool: bool,
    pub install_fails: bool,
    pub search_fails: bool,
}

impl FakePackages {
    pub fn new(log: &CallLog) -> Self {
        FakePackages {
            log: Rc::clone(log),
            results: Vec::new(),
            files: Vec::new(),
            missing_tool: false,
            install_fails: false,
            search_fails: false,
        }
    }

    pub fn with_files(log: &CallLog, files: &[&str]) -> Self {
        FakePackages {
            files: files.iter().map(PathBuf::from).collect(),
            ..Self::new(log)
        }
    }

    fn record(&self, entry: String) {
        self.log.borrow_mut().push(entry);
    }
}

impl PackageSearch for FakePackages {
    fn search(&self, term: &str) -> Result<Vec<PackageInfo>, PackageError> {
        self.record(format!("search:{term}"));
        if self.search_fails {
            return Err(PackageError::Command("search failed".to_string()));
        }
        Ok(self.results.clone())
    }
}

impl PackageManager for FakePackages {
    fn ensure_available(&self) -> Result<(), PackageError> {
        self.record("ensure".to_string());
        if self.missing_tool {
            return Err(PackageError::ToolMissing {
                tool: "yay".to_string(),
                hint: "install it".to_string(),
            });
        }
        Ok(())
    }

    fn install(&self, package: &str) -> Result<(), PackageError> {
        self.record(format!("install:{package}"));
        if self.install_fails {
            return Err(PackageError::Command("install failed".to_string()));
        }
        Ok(())
    }

    fn installed_files(&self, package: &str) -> Result<Vec<PathBuf>, PackageError> {
        self.record(format!("files:{package}"));
        Ok(self.files.clone())
    }
}

/// Confirmation double that records each prompt as `confirm:<prompt>`.
pub struct FakeConfirm {
    pub answer: Result<bool, ConfirmError>,
    pub log: CallLog,
}

impl FakeConfirm {
    pub fn answering(answer: bool, log: &CallLog) -> Self {
        FakeConfirm {
            answer: Ok(answer),
            log: Rc::clone(log),
        }
    }
}

impl Confirm for FakeConfirm {
    fn confirm(&self, prompt: &str) -> Result<bool, ConfirmError> {
        self.log.borrow_mut().push(format!("confirm:{prompt}"));
        self.answer.clone()
    }
}

/// Package-related services for tests that never exercise them.
pub fn idle_package_services() -> (
    Box<dyn PackageSearch>,
    Box<dyn PackageManager>,
    Box<dyn Confirm>,
) {
    let log = CallLog::default();
    (
        Box::new(FakePackages::new(&log)),
        Box::new(FakePackages::new(&log)),
        Box::new(FakeConfirm::answering(false, &log)),
    )
}
