use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("invalid alias {0:?}: must be 1 to 8 ascii alphanumeric characters")]
    InvalidAlias(String),
    #[error("invalid name: must not be empty")]
    InvalidName,
    #[error("alias {0:?} is reserved for a subcommand")]
    ReservedAlias(String),
    #[error("a cursor named {0:?} already exists")]
    DuplicateName(String),
    #[error("alias {0:?} collides with an existing name or alias")]
    AliasCollision(String),
    #[error("no cursor matches {0:?}")]
    NotFound(String),
    #[error("invalid package name {0:?}")]
    InvalidPackageName(String),
    #[error("invalid search term {0:?}: it must not be empty or start with '-'")]
    InvalidSearchTerm(String),
}

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("storage failure: {0}")]
    Storage(String),
    #[error("cursor {0:?} not found in registry")]
    NotFound(String),
    #[error("cursor {0:?} already registered")]
    Duplicate(String),
    #[error("registry data is invalid: {0}")]
    Corrupt(String),
}

#[derive(Debug, Error)]
pub enum ApplyError {
    #[error("command failed: {0}")]
    Command(String),
    #[error("io failure: {0}")]
    Io(String),
    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Error)]
pub enum PreviewError {
    #[error("no preview available: {0}")]
    Unavailable(String),
    #[error("could not decode cursor: {0}")]
    Decode(String),
    #[error("io failure: {0}")]
    Io(String),
}

#[derive(Debug, Error)]
pub enum CurrentCursorError {
    #[error("could not determine the active cursor: {0}")]
    Unavailable(String),
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum UpdateError {
    #[error("network failure: {0}")]
    Network(String),
    #[error("unexpected release data: {0}")]
    Parse(String),
    #[error("the release has no downloadable binary for this platform")]
    MissingAssets,
    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("io failure: {0}")]
    Io(String),
    #[error("could not restart cursorswap: {0}")]
    Relaunch(String),
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum PackageError {
    #[error("`{tool}` was not found in your PATH; {hint}")]
    ToolMissing { tool: String, hint: String },
    #[error("{0}")]
    Command(String),
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ConfirmError {
    #[error(
        "confirmation is required but standard input is not a terminal; \
         run this command from an interactive terminal"
    )]
    NotInteractive,
    #[error("could not read the confirmation: {0}")]
    Io(String),
}
