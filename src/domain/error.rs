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
