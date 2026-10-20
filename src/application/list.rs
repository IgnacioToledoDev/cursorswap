use crate::domain::cursor::CursorTheme;
use crate::domain::ports::CursorRepository;

use super::error::ApplicationError;

pub fn list(repo: &dyn CursorRepository) -> Result<Vec<CursorTheme>, ApplicationError> {
    Ok(repo.list()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::{FakeRepository, theme};

    #[test]
    fn returns_themes_in_stored_order() {
        let repo = FakeRepository::with(vec![theme("b", None), theme("a", None)]);
        let names: Vec<_> = list(&repo).unwrap().into_iter().map(|t| t.name).collect();
        assert_eq!(names, ["b", "a"]);
    }

    #[test]
    fn returns_empty_when_nothing_registered() {
        assert!(list(&FakeRepository::default()).unwrap().is_empty());
    }

    #[test]
    fn propagates_repository_failure() {
        assert!(matches!(
            list(&FakeRepository::failing()),
            Err(ApplicationError::Repository(_))
        ));
    }
}
