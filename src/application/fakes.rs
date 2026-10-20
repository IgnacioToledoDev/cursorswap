use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::domain::cursor::{Alias, CursorTheme};
use crate::domain::error::{ApplyError, CurrentCursorError, PreviewError, RepositoryError};
use crate::domain::ports::{
    CurrentCursorProvider, CursorApplier, CursorRepository, Preview, PreviewLoader, ThemeInspector,
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
