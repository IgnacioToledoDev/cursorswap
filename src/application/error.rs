use std::path::PathBuf;

use thiserror::Error;

use crate::domain::error::{
    ConfirmError, CurrentCursorError, DomainError, PackageError, RepositoryError,
};

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
    #[error(transparent)]
    Package(#[from] PackageError),
    #[error(transparent)]
    Confirm(#[from] ConfirmError),
    #[error("installation not confirmed: nothing was installed")]
    Declined,
    #[error(
        "package {0} was installed but provides no cursor theme (a directory with \
         index.theme and cursors/), so nothing was registered"
    )]
    NoThemeInPackage(String),
    #[error(
        "--alias needs a package with exactly one cursor theme, but {package} provides \
         {count}: {paths}; nothing was registered (the package itself is installed). \
         Register them with `cursorswap add <PATH> --alias <ALIAS>`"
    )]
    AliasNeedsSingleTheme {
        package: String,
        count: usize,
        paths: String,
    },
}
