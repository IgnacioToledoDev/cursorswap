use crate::application::apply::ApplyStatus;
use crate::application::error::ApplicationError;
use crate::domain::error::DomainError;

/// Process exit status of the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Success,
    Error,
    NotFound,
    Partial,
}

impl Exit {
    pub fn code(self) -> u8 {
        match self {
            Exit::Success => 0,
            Exit::Error => 1,
            Exit::NotFound => 2,
            Exit::Partial => 3,
        }
    }

    pub fn from_apply(status: ApplyStatus) -> Self {
        match status {
            ApplyStatus::AllApplied => Exit::Success,
            ApplyStatus::Partial => Exit::Partial,
            ApplyStatus::AllFailed => Exit::Error,
        }
    }

    /// Help and version requests succeed. Usage errors are generic errors:
    /// clap's own status 2 would collide with "theme not found".
    pub fn from_clap(err: &clap::Error) -> Self {
        if err.use_stderr() {
            Exit::Error
        } else {
            Exit::Success
        }
    }

    pub fn from_error(err: &ApplicationError) -> Self {
        match err {
            ApplicationError::Domain(DomainError::NotFound(_)) => Exit::NotFound,
            _ => Exit::Error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::error::{ConfirmError, PackageError, RepositoryError};
    use std::path::PathBuf;

    #[test]
    fn codes_are_stable() {
        assert_eq!(Exit::Success.code(), 0);
        assert_eq!(Exit::Error.code(), 1);
        assert_eq!(Exit::NotFound.code(), 2);
        assert_eq!(Exit::Partial.code(), 3);
    }

    #[test]
    fn maps_apply_status() {
        assert_eq!(Exit::from_apply(ApplyStatus::AllApplied), Exit::Success);
        assert_eq!(Exit::from_apply(ApplyStatus::Partial), Exit::Partial);
        assert_eq!(Exit::from_apply(ApplyStatus::AllFailed), Exit::Error);
    }

    #[test]
    fn not_found_maps_to_not_found() {
        let err = ApplicationError::Domain(DomainError::NotFound("x".into()));
        assert_eq!(Exit::from_error(&err), Exit::NotFound);
    }

    #[test]
    fn other_errors_map_to_generic_error() {
        let errors = [
            ApplicationError::Domain(DomainError::InvalidName),
            ApplicationError::Repository(RepositoryError::Storage("x".into())),
            ApplicationError::InvalidThemeDir(PathBuf::from("/x")),
            ApplicationError::Package(PackageError::Command("x".into())),
            ApplicationError::Confirm(ConfirmError::NotInteractive),
            ApplicationError::Declined,
            ApplicationError::NoThemeInPackage("p".into()),
            ApplicationError::AliasNeedsSingleTheme {
                package: "p".into(),
                count: 2,
                paths: "/a, /b".into(),
            },
            ApplicationError::Domain(DomainError::InvalidPackageName("-S".into())),
        ];
        for err in &errors {
            assert_eq!(Exit::from_error(err), Exit::Error, "{err}");
        }
    }

    fn clap_error(args: &[&str]) -> clap::Error {
        use clap::Parser;
        crate::presentation::cli::Cli::try_parse_from(args).unwrap_err()
    }

    #[test]
    fn help_and_version_are_success() {
        for args in [["cursorswap", "--help"], ["cursorswap", "--version"]] {
            assert_eq!(Exit::from_clap(&clap_error(&args)), Exit::Success);
        }
    }

    #[test]
    fn usage_errors_are_generic_errors_not_2() {
        for args in [
            ["cursorswap", "--bogus"],
            ["cursorswap", "add"],
            ["cursorswap", "search"],
            ["cursorswap", "install"],
        ] {
            assert_eq!(Exit::from_clap(&clap_error(&args)), Exit::Error);
        }
    }
}
