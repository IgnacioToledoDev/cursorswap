use std::path::PathBuf;

use crate::domain::cursor::{Alias, CursorTheme, RESERVED_WORDS, ensure_unique};
use crate::domain::ports::{CursorRepository, ThemeInspector};

use super::error::ApplicationError;

pub struct AddInput {
    pub path: PathBuf,
    pub name: Option<String>,
    pub alias: Option<String>,
}

pub fn add(
    repo: &mut dyn CursorRepository,
    inspector: &dyn ThemeInspector,
    input: AddInput,
) -> Result<CursorTheme, ApplicationError> {
    if !inspector.is_valid_theme_dir(&input.path) {
        return Err(ApplicationError::InvalidThemeDir(input.path));
    }
    let name = input
        .name
        .or_else(|| inspector.dir_name(&input.path))
        .unwrap_or_default();
    let alias = input
        .alias
        .as_deref()
        .map(|a| Alias::new_checked(a, &RESERVED_WORDS))
        .transpose()?;
    let theme = CursorTheme::new(&name, alias, input.path)?;
    ensure_unique(&repo.list()?, &theme)?;
    repo.add(theme.clone())?;
    Ok(theme)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::{FakeInspector, FakeRepository, theme};
    use crate::domain::error::DomainError;

    fn inspector() -> FakeInspector {
        FakeInspector {
            valid: true,
            name: Some("Nordzy".to_string()),
        }
    }

    fn input(name: Option<&str>, alias: Option<&str>) -> AddInput {
        AddInput {
            path: PathBuf::from("/themes/Nordzy"),
            name: name.map(str::to_string),
            alias: alias.map(str::to_string),
        }
    }

    #[test]
    fn defaults_name_to_directory_name() {
        let mut repo = FakeRepository::default();
        let stored = add(&mut repo, &inspector(), input(None, None)).unwrap();
        assert_eq!(stored.name, "Nordzy");
        assert_eq!(stored.path, PathBuf::from("/themes/Nordzy"));
        assert_eq!(repo.themes, vec![stored]);
    }

    #[test]
    fn uses_explicit_name_and_alias() {
        let mut repo = FakeRepository::default();
        let stored = add(&mut repo, &inspector(), input(Some("Custom"), Some("cu"))).unwrap();
        assert_eq!(stored.name, "Custom");
        assert_eq!(stored.alias.unwrap().as_str(), "cu");
    }

    #[test]
    fn rejects_invalid_directory() {
        let mut repo = FakeRepository::default();
        let bad = FakeInspector {
            valid: false,
            name: Some("x".to_string()),
        };
        let err = add(&mut repo, &bad, input(None, None)).unwrap_err();
        assert!(matches!(err, ApplicationError::InvalidThemeDir(_)));
        assert!(repo.themes.is_empty());
    }

    #[test]
    fn rejects_reserved_alias() {
        let mut repo = FakeRepository::default();
        let err = add(&mut repo, &inspector(), input(None, Some("list"))).unwrap_err();
        assert!(matches!(
            err,
            ApplicationError::Domain(DomainError::ReservedAlias(_))
        ));
        assert!(repo.themes.is_empty());
    }

    #[test]
    fn rejects_duplicate_name() {
        let mut repo = FakeRepository::with(vec![theme("nordzy", None)]);
        let err = add(&mut repo, &inspector(), input(None, None)).unwrap_err();
        assert!(matches!(
            err,
            ApplicationError::Domain(DomainError::DuplicateName(_))
        ));
        assert_eq!(repo.themes.len(), 1);
    }

    #[test]
    fn rejects_alias_collision() {
        let mut repo = FakeRepository::with(vec![theme("other", Some("nz"))]);
        let err = add(&mut repo, &inspector(), input(None, Some("NZ"))).unwrap_err();
        assert!(matches!(
            err,
            ApplicationError::Domain(DomainError::AliasCollision(_))
        ));
    }

    #[test]
    fn rejects_missing_name_when_directory_has_none() {
        let mut repo = FakeRepository::default();
        let nameless = FakeInspector {
            valid: true,
            name: None,
        };
        let err = add(&mut repo, &nameless, input(None, None)).unwrap_err();
        assert!(matches!(
            err,
            ApplicationError::Domain(DomainError::InvalidName)
        ));
    }

    #[test]
    fn propagates_repository_failure() {
        let mut repo = FakeRepository::failing();
        let err = add(&mut repo, &inspector(), input(None, None)).unwrap_err();
        assert!(matches!(err, ApplicationError::Repository(_)));
    }
}
