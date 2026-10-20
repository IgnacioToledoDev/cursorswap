use thiserror::Error;

use super::Services;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TuiError {
    #[error("the interactive TUI is not implemented yet; pass a theme name or use a subcommand")]
    NotImplemented,
}

/// Placeholder entry point; replaced by the real TUI in a later task.
pub fn run(_services: &mut Services) -> Result<(), TuiError> {
    Err(TuiError::NotImplemented)
}
