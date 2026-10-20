use std::fs;
use std::path::PathBuf;

use crate::domain::cursor::CursorTheme;
use crate::domain::error::ApplyError;
use crate::domain::ports::CursorApplier;
use crate::infrastructure::atomic_write::write_atomic;
use crate::infrastructure::envs_conf::parse_env_line;

use super::name::{ensure_no_comma, system_theme_name};

const THEME_KEYS: [&str; 2] = ["XCURSOR_THEME", "HYPRCURSOR_THEME"];

/// Rewrites the theme variables of Hyprland's `envs.conf`, touching nothing else.
pub struct HyprEnvsApplier {
    path: PathBuf,
}

impl HyprEnvsApplier {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

fn rewrite(content: &str, name: &str) -> String {
    let mut out = String::with_capacity(content.len() + 64);
    let mut seen = [false; THEME_KEYS.len()];
    for line in content.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let ending = &line[body.len()..];
        let hit = parse_env_line(body)
            .and_then(|(key, at)| THEME_KEYS.iter().position(|k| *k == key).map(|i| (i, at)));
        match hit {
            Some((i, at)) => {
                seen[i] = true;
                out.push_str(&body[..at]);
                out.push_str(name);
                out.push_str(ending);
            }
            None => out.push_str(line),
        }
    }
    for (key, _) in THEME_KEYS.iter().zip(seen).filter(|(_, seen)| !seen) {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&format!("env = {key},{name}\n"));
    }
    out
}

impl CursorApplier for HyprEnvsApplier {
    fn label(&self) -> &'static str {
        "hypr-envs"
    }

    fn apply(&self, theme: &CursorTheme) -> Result<(), ApplyError> {
        let name = system_theme_name(theme)?;
        ensure_no_comma(&name)?;
        let content = fs::read_to_string(&self.path)
            .map_err(|e| ApplyError::Io(format!("{}: {e}", self.path.display())))?;
        write_atomic(&self.path, rewrite(&content, &name).as_bytes())
            .map_err(|e| ApplyError::Io(format!("{}: {e}", self.path.display())))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::TempDir;

    use super::*;

    fn theme(friendly: &str, dir: &str) -> CursorTheme {
        CursorTheme::new(friendly, None, PathBuf::from(format!("/icons/{dir}"))).unwrap()
    }

    fn apply_to(content: &str, t: &CursorTheme) -> String {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("envs.conf");
        fs::write(&path, content).unwrap();
        HyprEnvsApplier::new(path.clone()).apply(t).unwrap();
        fs::read_to_string(path).unwrap()
    }

    const SAMPLE: &str = "# cursor\nenv = XCURSOR_SIZE,24\nenv = XCURSOR_THEME,Old\nenv = HYPRCURSOR_THEME,Old\nenv = HYPRCURSOR_SIZE,24\nenv = EDITOR,nvim\n";

    #[test]
    fn label_is_stable() {
        assert_eq!(HyprEnvsApplier::new(PathBuf::new()).label(), "hypr-envs");
    }

    #[test]
    fn replaces_both_theme_values_and_nothing_else() {
        let out = apply_to(SAMPLE, &theme("Pretty", "Nordzy-cursors"));
        assert_eq!(
            out,
            "# cursor\nenv = XCURSOR_SIZE,24\nenv = XCURSOR_THEME,Nordzy-cursors\nenv = HYPRCURSOR_THEME,Nordzy-cursors\nenv = HYPRCURSOR_SIZE,24\nenv = EDITOR,nvim\n"
        );
    }

    #[test]
    fn keeps_original_spacing_style() {
        let out = apply_to(
            "env=XCURSOR_THEME ,  Old\nenv = HYPRCURSOR_THEME,Old\n",
            &theme("x", "New"),
        );
        assert_eq!(
            out,
            "env=XCURSOR_THEME ,  New\nenv = HYPRCURSOR_THEME,New\n"
        );
    }

    #[test]
    fn commented_theme_lines_are_left_alone() {
        let src = "# env = XCURSOR_THEME,Commented\nenv = XCURSOR_THEME,Old\nenv = HYPRCURSOR_THEME,Old\n";
        let out = apply_to(src, &theme("x", "New"));
        assert!(out.starts_with("# env = XCURSOR_THEME,Commented\n"));
        assert!(out.contains("env = XCURSOR_THEME,New\n"));
    }

    #[test]
    fn appends_missing_lines_at_the_end() {
        let out = apply_to("env = XCURSOR_SIZE,24\n", &theme("x", "New"));
        assert_eq!(
            out,
            "env = XCURSOR_SIZE,24\nenv = XCURSOR_THEME,New\nenv = HYPRCURSOR_THEME,New\n"
        );
    }

    #[test]
    fn appends_only_the_missing_one() {
        let out = apply_to("env = XCURSOR_THEME,Old\n", &theme("x", "New"));
        assert_eq!(out, "env = XCURSOR_THEME,New\nenv = HYPRCURSOR_THEME,New\n");
    }

    #[test]
    fn appending_adds_newline_when_file_lacks_one() {
        let out = apply_to("env = XCURSOR_SIZE,24", &theme("x", "New"));
        assert_eq!(
            out,
            "env = XCURSOR_SIZE,24\nenv = XCURSOR_THEME,New\nenv = HYPRCURSOR_THEME,New\n"
        );
    }

    #[test]
    fn preserves_missing_trailing_newline_when_replacing() {
        let out = apply_to(
            "env = XCURSOR_THEME,Old\nenv = HYPRCURSOR_THEME,Old",
            &theme("x", "New"),
        );
        assert_eq!(out, "env = XCURSOR_THEME,New\nenv = HYPRCURSOR_THEME,New");
    }

    #[test]
    fn preserves_crlf_line_endings() {
        let out = apply_to(
            "env = XCURSOR_THEME,Old\r\nenv = HYPRCURSOR_THEME,Old\r\n",
            &theme("x", "New"),
        );
        assert_eq!(
            out,
            "env = XCURSOR_THEME,New\r\nenv = HYPRCURSOR_THEME,New\r\n"
        );
    }

    #[test]
    fn empty_file_gets_both_lines() {
        let out = apply_to("", &theme("x", "New"));
        assert_eq!(out, "env = XCURSOR_THEME,New\nenv = HYPRCURSOR_THEME,New\n");
    }

    #[test]
    fn missing_file_is_error_and_not_created() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("envs.conf");
        let err = HyprEnvsApplier::new(path.clone())
            .apply(&theme("x", "New"))
            .unwrap_err();
        assert!(matches!(err, ApplyError::Io(_)));
        assert!(!Path::new(&path).exists());
    }

    #[test]
    fn rejects_comma_without_touching_the_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("envs.conf");
        fs::write(&path, SAMPLE).unwrap();
        let t = theme("x", "a,b");
        assert!(HyprEnvsApplier::new(path.clone()).apply(&t).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), SAMPLE);
    }

    #[test]
    fn newline_in_name_cannot_inject_a_line() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("envs.conf");
        fs::write(&path, SAMPLE).unwrap();
        let t = theme("x", "evil\nexec-once = rm -rf ~");
        assert!(HyprEnvsApplier::new(path.clone()).apply(&t).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), SAMPLE);
    }

    #[cfg(unix)]
    #[test]
    fn writes_through_a_symlinked_config() {
        let dir = TempDir::new().unwrap();
        let real = dir.path().join("real.conf");
        fs::write(&real, SAMPLE).unwrap();
        let link = dir.path().join("envs.conf");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        HyprEnvsApplier::new(link.clone())
            .apply(&theme("x", "New"))
            .unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            fs::read_to_string(&real)
                .unwrap()
                .contains("XCURSOR_THEME,New")
        );
    }
}
