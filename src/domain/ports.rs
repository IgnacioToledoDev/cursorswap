use super::cursor::CursorTheme;
use super::error::{ApplyError, PreviewError, RepositoryError};

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
