use std::path::PathBuf;

use super::error::DomainError;

pub const RESERVED_WORDS: [&str; 7] = [
    "list", "add", "remove", "current", "update", "search", "install",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias(String);

impl Alias {
    pub fn new(value: &str) -> Result<Alias, DomainError> {
        let valid =
            (1..=8).contains(&value.len()) && value.chars().all(|c| c.is_ascii_alphanumeric());
        if valid {
            Ok(Alias(value.to_string()))
        } else {
            Err(DomainError::InvalidAlias(value.to_string()))
        }
    }

    pub fn new_checked(value: &str, reserved: &[&str]) -> Result<Alias, DomainError> {
        let alias = Alias::new(value)?;
        if reserved.iter().any(|word| word.eq_ignore_ascii_case(value)) {
            return Err(DomainError::ReservedAlias(value.to_string()));
        }
        Ok(alias)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorTheme {
    pub name: String,
    pub alias: Option<Alias>,
    pub path: PathBuf,
}

impl CursorTheme {
    pub fn new(name: &str, alias: Option<Alias>, path: PathBuf) -> Result<Self, DomainError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DomainError::InvalidName);
        }
        Ok(CursorTheme {
            name: name.to_string(),
            alias,
            path,
        })
    }

    fn alias_str(&self) -> Option<&str> {
        self.alias.as_ref().map(Alias::as_str)
    }
}

/// Resolves a query by exact name first, then by alias (both case-insensitive).
pub fn resolve<'a>(themes: &'a [CursorTheme], query: &str) -> Result<&'a CursorTheme, DomainError> {
    themes
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case(query))
        .or_else(|| {
            themes
                .iter()
                .find(|t| t.alias_str().is_some_and(|a| a.eq_ignore_ascii_case(query)))
        })
        .ok_or_else(|| DomainError::NotFound(query.to_string()))
}

/// Checks that `candidate` does not clash with any theme in `existing`.
pub fn ensure_unique(existing: &[CursorTheme], candidate: &CursorTheme) -> Result<(), DomainError> {
    for other in existing {
        if other.name.eq_ignore_ascii_case(&candidate.name) {
            return Err(DomainError::DuplicateName(candidate.name.clone()));
        }
        if other
            .alias_str()
            .is_some_and(|a| a.eq_ignore_ascii_case(&candidate.name))
        {
            return Err(DomainError::AliasCollision(candidate.name.clone()));
        }
        if let Some(alias) = candidate.alias_str() {
            let hits_name = other.name.eq_ignore_ascii_case(alias);
            let hits_alias = other
                .alias_str()
                .is_some_and(|a| a.eq_ignore_ascii_case(alias));
            if hits_name || hits_alias {
                return Err(DomainError::AliasCollision(alias.to_string()));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod alias_tests {
    use super::*;

    #[test]
    fn rejects_empty_alias() {
        assert_eq!(
            Alias::new(""),
            Err(DomainError::InvalidAlias(String::new()))
        );
    }

    #[test]
    fn accepts_one_char_alias() {
        assert_eq!(Alias::new("N").unwrap().as_str(), "N");
    }

    #[test]
    fn accepts_eight_char_alias() {
        assert_eq!(Alias::new("abcd1234").unwrap().as_str(), "abcd1234");
    }

    #[test]
    fn rejects_nine_char_alias() {
        assert!(matches!(
            Alias::new("abcd12345"),
            Err(DomainError::InvalidAlias(_))
        ));
    }

    #[test]
    fn rejects_non_alphanumeric_alias() {
        for bad in ["a-b", "a b", "a_b", "é", "a.b"] {
            assert!(
                matches!(Alias::new(bad), Err(DomainError::InvalidAlias(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn rejects_reserved_words_in_any_case() {
        for word in [
            "list", "ADD", "Remove", "cUrRent", "UPDATE", "search", "INSTALL",
        ] {
            assert!(
                matches!(
                    Alias::new_checked(word, &RESERVED_WORDS),
                    Err(DomainError::ReservedAlias(_))
                ),
                "{word}"
            );
        }
    }

    #[test]
    fn checked_accepts_non_reserved_alias() {
        assert_eq!(
            Alias::new_checked("nord", &RESERVED_WORDS)
                .unwrap()
                .as_str(),
            "nord"
        );
    }

    #[test]
    fn checked_still_validates_format() {
        assert!(matches!(
            Alias::new_checked("bad-one", &RESERVED_WORDS),
            Err(DomainError::InvalidAlias(_))
        ));
    }
}

#[cfg(test)]
mod theme_tests {
    use super::*;

    fn theme(name: &str, alias: Option<&str>) -> CursorTheme {
        CursorTheme::new(
            name,
            alias.map(|a| Alias::new(a).unwrap()),
            PathBuf::from(format!("/icons/{name}")),
        )
        .unwrap()
    }

    #[test]
    fn rejects_empty_name() {
        assert_eq!(
            CursorTheme::new("", None, PathBuf::from("/x")),
            Err(DomainError::InvalidName)
        );
    }

    #[test]
    fn rejects_whitespace_name() {
        assert_eq!(
            CursorTheme::new("  \t", None, PathBuf::from("/x")),
            Err(DomainError::InvalidName)
        );
    }

    #[test]
    fn trims_name() {
        let t = CursorTheme::new("  Nordzy  ", None, PathBuf::from("/x")).unwrap();
        assert_eq!(t.name, "Nordzy");
    }

    #[test]
    fn resolve_matches_name_case_insensitively() {
        let themes = vec![theme("Nordzy-cursors", Some("N"))];
        assert_eq!(
            resolve(&themes, "nordzy-CURSORS").unwrap().name,
            "Nordzy-cursors"
        );
    }

    #[test]
    fn resolve_matches_alias_case_insensitively() {
        let themes = vec![theme("Nordzy-cursors", Some("Nz"))];
        assert_eq!(resolve(&themes, "nZ").unwrap().name, "Nordzy-cursors");
    }

    #[test]
    fn resolve_prefers_name_over_alias() {
        let themes = vec![theme("first", Some("second")), theme("second", None)];
        assert_eq!(resolve(&themes, "second").unwrap().name, "second");
    }

    #[test]
    fn resolve_reports_not_found() {
        let themes = vec![theme("a", Some("b"))];
        assert_eq!(
            resolve(&themes, "zzz"),
            Err(DomainError::NotFound("zzz".to_string()))
        );
    }

    #[test]
    fn unique_accepts_distinct_candidate() {
        let existing = vec![theme("one", Some("o"))];
        assert_eq!(ensure_unique(&existing, &theme("two", Some("t"))), Ok(()));
    }

    #[test]
    fn unique_accepts_candidate_without_alias() {
        let existing = vec![theme("one", Some("o"))];
        assert_eq!(ensure_unique(&existing, &theme("two", None)), Ok(()));
    }

    #[test]
    fn unique_rejects_duplicate_name_case_insensitively() {
        let existing = vec![theme("One", None)];
        assert!(matches!(
            ensure_unique(&existing, &theme("oNE", None)),
            Err(DomainError::DuplicateName(_))
        ));
    }

    #[test]
    fn unique_rejects_alias_equal_to_existing_alias() {
        let existing = vec![theme("one", Some("Ab"))];
        assert!(matches!(
            ensure_unique(&existing, &theme("two", Some("aB"))),
            Err(DomainError::AliasCollision(_))
        ));
    }

    #[test]
    fn unique_rejects_alias_equal_to_existing_name() {
        let existing = vec![theme("one", None)];
        assert!(matches!(
            ensure_unique(&existing, &theme("two", Some("ONE"))),
            Err(DomainError::AliasCollision(_))
        ));
    }

    #[test]
    fn unique_rejects_name_equal_to_existing_alias() {
        let existing = vec![theme("one", Some("two"))];
        assert!(matches!(
            ensure_unique(&existing, &theme("TWO", None)),
            Err(DomainError::AliasCollision(_))
        ));
    }

    #[test]
    fn unique_allows_alias_equal_to_own_name() {
        let existing = vec![theme("one", None)];
        assert_eq!(ensure_unique(&existing, &theme("two", Some("two"))), Ok(()));
    }
}
