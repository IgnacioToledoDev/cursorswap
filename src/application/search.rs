use crate::domain::package::{PackageInfo, looks_like_cursor_theme, validate_search_term};
use crate::domain::ports::PackageSearch;

use super::error::ApplicationError;

/// Searches packages and keeps only those that look like cursor themes.
pub fn search(
    searcher: &dyn PackageSearch,
    term: &str,
) -> Result<Vec<PackageInfo>, ApplicationError> {
    validate_search_term(term)?;
    let found = searcher.search(term)?;
    Ok(found.into_iter().filter(looks_like_cursor_theme).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::{CallLog, FakePackages};
    use crate::domain::error::{DomainError, PackageError};

    fn pkg(name: &str, description: &str) -> PackageInfo {
        PackageInfo {
            repository: "aur".into(),
            name: name.into(),
            version: "1.0-1".into(),
            description: description.into(),
            installed: false,
        }
    }

    #[test]
    fn keeps_only_cursor_like_packages_in_order() {
        let log = CallLog::default();
        let mut packages = FakePackages::new(&log);
        packages.results = vec![
            pkg("nordzy-cursors", "Nord"),
            pkg("htop", "process viewer"),
            pkg("vim-multi", "Multiple cursors plugin"),
        ];
        let found = search(&packages, "nord").unwrap();
        let names: Vec<_> = found.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["nordzy-cursors", "vim-multi"]);
        assert_eq!(*log.borrow(), ["search:nord"]);
    }

    #[test]
    fn no_matches_is_an_empty_list() {
        let packages = FakePackages::new(&CallLog::default());
        assert!(search(&packages, "zzz").unwrap().is_empty());
    }

    #[test]
    fn invalid_term_is_rejected_before_searching() {
        let log = CallLog::default();
        let packages = FakePackages::new(&log);
        for term in ["", "-S"] {
            let err = search(&packages, term).unwrap_err();
            assert!(matches!(
                err,
                ApplicationError::Domain(DomainError::InvalidSearchTerm(_))
            ));
        }
        assert!(log.borrow().is_empty());
    }

    #[test]
    fn search_failure_propagates() {
        let mut packages = FakePackages::new(&CallLog::default());
        packages.search_fails = true;
        let err = search(&packages, "x").unwrap_err();
        assert!(matches!(
            err,
            ApplicationError::Package(PackageError::Command(_))
        ));
    }
}
