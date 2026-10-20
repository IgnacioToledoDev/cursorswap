use crate::domain::cursor::{CursorTheme, resolve};
use crate::domain::ports::CursorRepository;

use super::error::ApplicationError;

/// Removes the theme matching `query` and returns it.
pub fn remove(
    repo: &mut dyn CursorRepository,
    query: &str,
) -> Result<CursorTheme, ApplicationError> {
    let themes = repo.list()?;
    let theme = resolve(&themes, query)?.clone();
    repo.remove(&theme.name)?;
    Ok(theme)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::{FakeRepository, theme};
    use crate::domain::error::DomainError;

    fn repo() -> FakeRepository {
        FakeRepository::with(vec![theme("Nordzy", Some("nz")), theme("Bibata", None)])
    }

    #[test]
    fn removes_by_name() {
        let mut repo = repo();
        let removed = remove(&mut repo, "bibata").unwrap();
        assert_eq!(removed.name, "Bibata");
        assert_eq!(repo.themes.len(), 1);
    }

    #[test]
    fn removes_by_alias() {
        let mut repo = repo();
        let removed = remove(&mut repo, "NZ").unwrap();
        assert_eq!(removed.name, "Nordzy");
        assert_eq!(repo.themes[0].name, "Bibata");
    }

    #[test]
    fn not_found_is_an_error_and_keeps_registry() {
        let mut repo = repo();
        let err = remove(&mut repo, "nope").unwrap_err();
        assert!(matches!(
            err,
            ApplicationError::Domain(DomainError::NotFound(_))
        ));
        assert_eq!(repo.themes.len(), 2);
    }

    #[test]
    fn propagates_repository_failure() {
        let err = remove(&mut FakeRepository::failing(), "x").unwrap_err();
        assert!(matches!(err, ApplicationError::Repository(_)));
    }
}
