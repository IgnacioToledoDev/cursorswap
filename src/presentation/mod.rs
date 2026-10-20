use crate::domain::ports::{
    Confirm, CurrentCursorProvider, CursorApplier, CursorRepository, PackageManager, PackageSearch,
    PreviewLoader, ThemeInspector,
};

pub mod cli;
pub mod exit_code;
pub mod output;
pub mod tui;

/// Everything the presentation layer needs, injected by the composition root.
pub struct Services {
    pub repo: Box<dyn CursorRepository>,
    pub inspector: Box<dyn ThemeInspector>,
    pub current: Box<dyn CurrentCursorProvider>,
    pub preview: Box<dyn PreviewLoader>,
    pub appliers: Vec<Box<dyn CursorApplier>>,
    pub searcher: Box<dyn PackageSearch>,
    pub packages: Box<dyn PackageManager>,
    pub confirm: Box<dyn Confirm>,
}
