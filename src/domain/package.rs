use super::error::DomainError;

/// A package returned by a package search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageInfo {
    pub repository: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub installed: bool,
}

/// Heuristic: a package looks like a cursor theme when its name or
/// description mentions "cursor" (case-insensitive).
pub fn looks_like_cursor_theme(package: &PackageInfo) -> bool {
    let mentions = |text: &str| text.to_ascii_lowercase().contains("cursor");
    mentions(&package.name) || mentions(&package.description)
}

/// Rejects names that are empty, could be read as an option, or contain
/// characters pacman does not allow in package names.
pub fn validate_package_name(name: &str) -> Result<(), DomainError> {
    let valid = !name.starts_with(['-', '.'])
        && !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "@._+-".contains(c));
    if valid {
        Ok(())
    } else {
        Err(DomainError::InvalidPackageName(name.to_string()))
    }
}

/// Rejects empty terms and terms that would be read as an option.
pub fn validate_search_term(term: &str) -> Result<(), DomainError> {
    if term.trim().is_empty() || term.starts_with('-') {
        Err(DomainError::InvalidSearchTerm(term.to_string()))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn heuristic_matches_name_or_description_case_insensitively() {
        assert!(looks_like_cursor_theme(&pkg(
            "nordzy-cursors",
            "Nord theme"
        )));
        assert!(looks_like_cursor_theme(&pkg("BIBATA-Cursor", "")));
        assert!(looks_like_cursor_theme(&pkg(
            "pandwaita",
            "has panda CURSORS"
        )));
    }

    #[test]
    fn heuristic_rejects_unrelated_packages() {
        assert!(!looks_like_cursor_theme(&pkg(
            "htop",
            "Interactive process viewer"
        )));
        assert!(!looks_like_cursor_theme(&pkg("", "")));
    }

    #[test]
    fn accepts_realistic_package_names() {
        for name in [
            "nordzy-cursors",
            "bibata-cursor-theme-bin",
            "a",
            "lib32-x.y_z+1",
            "gtk@2",
        ] {
            assert_eq!(validate_package_name(name), Ok(()), "{name}");
        }
    }

    #[test]
    fn rejects_unsafe_package_names() {
        for name in [
            "",
            "-S",
            "--noconfirm",
            ".hidden",
            "a b",
            "a/b",
            "a;b",
            "a\nb",
            "é",
        ] {
            assert_eq!(
                validate_package_name(name),
                Err(DomainError::InvalidPackageName(name.to_string())),
                "{name:?}"
            );
        }
    }

    #[test]
    fn search_term_must_be_non_empty_and_not_an_option() {
        assert_eq!(validate_search_term("nordzy"), Ok(()));
        assert_eq!(validate_search_term("two words"), Ok(()));
        for term in ["", "   ", "-S", "--help"] {
            assert_eq!(
                validate_search_term(term),
                Err(DomainError::InvalidSearchTerm(term.to_string())),
                "{term:?}"
            );
        }
    }
}
