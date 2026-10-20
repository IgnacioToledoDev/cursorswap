use std::fs;
use std::io;
use std::path::PathBuf;

use crate::domain::error::CurrentCursorError;
use crate::domain::ports::CurrentCursorProvider;

use super::envs_conf::parse_env_line;

/// Reads the active cursor theme from the `XCURSOR_THEME` line of `envs.conf`.
pub struct EnvsConfCurrentProvider {
    path: PathBuf,
}

impl EnvsConfCurrentProvider {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl CurrentCursorProvider for EnvsConfCurrentProvider {
    fn current_name(&self) -> Result<Option<String>, CurrentCursorError> {
        let content = match fs::read_to_string(&self.path) {
            Ok(content) => content,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(CurrentCursorError::Unavailable(format!(
                    "{}: {e}",
                    self.path.display()
                )));
            }
        };
        Ok(content.lines().find_map(|line| {
            let (key, at) = parse_env_line(line)?;
            let value = line[at..].trim();
            (key == "XCURSOR_THEME" && !value.is_empty()).then(|| value.to_string())
        }))
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn current(content: Option<&str>) -> Option<String> {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("envs.conf");
        if let Some(c) = content {
            fs::write(&path, c).unwrap();
        }
        EnvsConfCurrentProvider::new(path).current_name().unwrap()
    }

    #[test]
    fn finds_xcursor_theme() {
        let c = "env = XCURSOR_SIZE,24\nenv = XCURSOR_THEME,Nordzy-cursors\nenv = HYPRCURSOR_THEME,Other\n";
        assert_eq!(current(Some(c)).as_deref(), Some("Nordzy-cursors"));
    }

    #[test]
    fn ignores_commented_line() {
        let c = "# env = XCURSOR_THEME,Old\nenv = XCURSOR_THEME,Real\n";
        assert_eq!(current(Some(c)).as_deref(), Some("Real"));
    }

    #[test]
    fn only_commented_line_means_none() {
        assert_eq!(current(Some("# env = XCURSOR_THEME,Old\n")), None);
    }

    #[test]
    fn missing_key_is_none() {
        assert_eq!(current(Some("env = HYPRCURSOR_THEME,x\n")), None);
    }

    #[test]
    fn missing_file_is_none() {
        assert_eq!(current(None), None);
    }

    #[test]
    fn trims_trailing_whitespace_and_cr() {
        assert_eq!(
            current(Some("env = XCURSOR_THEME,Nord  \r\n")).as_deref(),
            Some("Nord")
        );
    }

    #[test]
    fn empty_value_is_none() {
        assert_eq!(current(Some("env = XCURSOR_THEME,\n")), None);
    }
}
