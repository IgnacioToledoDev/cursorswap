use std::path::PathBuf;

use thiserror::Error;

use crate::domain::error::{CurrentCursorError, DomainError, RepositoryError};

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error(transparent)]
    Current(#[from] CurrentCursorError),
    #[error("{0} is not a valid cursor theme directory")]
    InvalidThemeDir(PathBuf),
}
