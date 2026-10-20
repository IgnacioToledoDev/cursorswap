use crate::domain::cursor::CursorTheme;
use crate::domain::ports::{CurrentCursorProvider, CursorRepository};

use super::error::ApplicationError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurrentCursor {
    Registered(CursorTheme),
    Unregistered(String),
    None,
}

pub fn current(
    repo: &dyn CursorRepository,
    provider: &dyn CurrentCursorProvider,
) -> Result<CurrentCursor, ApplicationError> {
    let Some(name) = provider.current_name()? else {
        return Ok(CurrentCursor::None);
    };
    let registered = repo
        .list()?
        .into_iter()
        .find(|t| t.name.eq_ignore_ascii_case(&name));
    Ok(match registered {
        Some(theme) => CurrentCursor::Registered(theme),
        None => CurrentCursor::Unregistered(name),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::{FakeCurrent, FakeRepository, theme};

    fn repo() -> FakeRepository {
        FakeRepository::with(vec![theme("Nordzy", Some("nz"))])
    }

    #[test]
    fn returns_registered_theme() {
        let provider = FakeCurrent(Ok(Some("nordzy".to_string())));
        let result = current(&repo(), &provider).unwrap();
        assert!(matches!(result, CurrentCursor::Registered(t) if t.name == "Nordzy"));
    }

    #[test]
    fn returns_raw_name_when_unregistered() {
        let provider = FakeCurrent(Ok(Some("Adwaita".to_string())));
        assert_eq!(
            current(&repo(), &provider).unwrap(),
            CurrentCursor::Unregistered("Adwaita".to_string())
        );
    }

    #[test]
    fn does_not_match_by_alias() {
        let provider = FakeCurrent(Ok(Some("nz".to_string())));
        assert_eq!(
            current(&repo(), &provider).unwrap(),
            CurrentCursor::Unregistered("nz".to_string())
        );
    }

    #[test]
    fn returns_none_when_nothing_is_active() {
        let provider = FakeCurrent(Ok(None));
        assert_eq!(current(&repo(), &provider).unwrap(), CurrentCursor::None);
    }

    #[test]
    fn propagates_provider_failure() {
        let provider = FakeCurrent(Err("no hyprland".to_string()));
        assert!(matches!(
            current(&repo(), &provider),
            Err(ApplicationError::Current(_))
        ));
    }

    #[test]
    fn propagates_repository_failure() {
        let provider = FakeCurrent(Ok(Some("x".to_string())));
        assert!(matches!(
            current(&FakeRepository::failing(), &provider),
            Err(ApplicationError::Repository(_))
        ));
    }
}
