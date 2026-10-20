use crate::domain::cursor::CursorTheme;
use crate::domain::error::ApplyError;

/// The icon-theme directory name, which is what the system expects as the theme
/// name (the registry name or alias may be friendlier and is not a valid theme id).
pub fn system_theme_name(theme: &CursorTheme) -> Result<String, ApplyError> {
    let name = theme
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| {
            ApplyError::Other(format!(
                "cannot derive a theme name from path {}",
                theme.path.display()
            ))
        })?;
    if name.chars().any(char::is_control) || name.starts_with('-') {
        return Err(ApplyError::Other(format!(
            "unsafe theme directory name {name:?}"
        )));
    }
    Ok(name.to_string())
}

/// Extra check for formats where a comma separates fields.
pub fn ensure_no_comma(name: &str) -> Result<(), ApplyError> {
    if name.contains(',') {
        return Err(ApplyError::Other(format!(
            "theme name {name:?} contains a comma"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn theme_at(name: &str, path: &str) -> CursorTheme {
        CursorTheme::new(name, None, PathBuf::from(path)).unwrap()
    }

    #[test]
    fn uses_the_directory_name_not_the_registry_name() {
        let t = theme_at("Pretty", "/home/u/dotfiles/cursor/Nordzy-cursors");
        assert_eq!(system_theme_name(&t).unwrap(), "Nordzy-cursors");
    }

    #[test]
    fn ignores_trailing_slash() {
        let t = theme_at("x", "/icons/Bibata/");
        assert_eq!(system_theme_name(&t).unwrap(), "Bibata");
    }

    #[test]
    fn rejects_path_without_final_component() {
        assert!(system_theme_name(&theme_at("x", "/")).is_err());
        assert!(system_theme_name(&theme_at("x", "..")).is_err());
    }

    #[test]
    fn rejects_non_utf8_directory_name() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let path = PathBuf::from("/icons").join(OsStr::from_bytes(b"bad\xff"));
        let t = CursorTheme::new("x", None, path).unwrap();
        assert!(system_theme_name(&t).is_err());
    }

    #[test]
    fn rejects_control_characters() {
        for bad in ["a\nb", "a\rb", "a\0b", "a\tb"] {
            let t = theme_at("x", &format!("/icons/{bad}"));
            assert!(system_theme_name(&t).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn rejects_leading_dash_that_could_pass_as_an_option() {
        assert!(system_theme_name(&theme_at("x", "/icons/-evil")).is_err());
    }

    #[test]
    fn comma_check() {
        assert!(ensure_no_comma("fine-name").is_ok());
        assert!(ensure_no_comma("a,b").is_err());
    }
}
