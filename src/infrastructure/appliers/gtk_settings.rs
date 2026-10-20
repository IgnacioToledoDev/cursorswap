use std::fs;
use std::io;
use std::path::PathBuf;

use crate::domain::cursor::CursorTheme;
use crate::domain::error::ApplyError;
use crate::domain::ports::CursorApplier;
use crate::infrastructure::atomic_write::write_atomic;

use super::name::system_theme_name;

const KEY: &str = "gtk-cursor-theme-name";
const SECTION: &str = "[Settings]";

/// Sets `gtk-cursor-theme-name` in a GTK 3 `settings.ini`, preserving the rest.
pub struct GtkSettingsApplier {
    path: PathBuf,
}

impl GtkSettingsApplier {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

fn is_key_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    !trimmed.starts_with(['#', ';'])
        && trimmed
            .split_once('=')
            .is_some_and(|(key, _)| key.trim() == KEY)
}

fn set_cursor_theme(content: &str, name: &str) -> String {
    let mut lines: Vec<String> = content.split_inclusive('\n').map(String::from).collect();
    let Some(start) = lines.iter().position(|l| l.trim() == SECTION) else {
        let mut out = content.to_string();
        if !out.is_empty() {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
        }
        out.push_str(&format!("{SECTION}\n{KEY}={name}\n"));
        return out;
    };
    let end = lines[start + 1..]
        .iter()
        .position(|l| l.trim_start().starts_with('['))
        .map_or(lines.len(), |i| start + 1 + i);

    let existing = (start + 1..end).find(|&i| is_key_line(&lines[i]));
    match existing {
        Some(i) => {
            let body = lines[i].trim_end_matches(['\n', '\r']);
            let ending = &lines[i][body.len()..];
            let eq = body.find('=').unwrap_or(0) + 1;
            let value_at = body.len() - body[eq..].trim_start().len();
            lines[i] = format!("{}{name}{ending}", &body[..value_at]);
        }
        None => {
            let last = (start..end)
                .rev()
                .find(|&i| !lines[i].trim().is_empty())
                .unwrap_or(start);
            if !lines[last].ends_with('\n') {
                lines[last].push('\n');
            }
            lines.insert(last + 1, format!("{KEY}={name}\n"));
        }
    }
    lines.concat()
}

impl CursorApplier for GtkSettingsApplier {
    fn label(&self) -> &'static str {
        "gtk-settings"
    }

    fn apply(&self, theme: &CursorTheme) -> Result<(), ApplyError> {
        let name = system_theme_name(theme)?;
        let io_err = |e: io::Error| ApplyError::Io(format!("{}: {e}", self.path.display()));
        let content = match fs::read_to_string(&self.path) {
            Ok(content) => content,
            Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(io_err(e)),
        };
        write_atomic(&self.path, set_cursor_theme(&content, &name).as_bytes()).map_err(io_err)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn theme(friendly: &str, dir: &str) -> CursorTheme {
        CursorTheme::new(friendly, None, PathBuf::from(format!("/icons/{dir}"))).unwrap()
    }

    fn apply_to(content: &str, t: &CursorTheme) -> String {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.ini");
        fs::write(&path, content).unwrap();
        GtkSettingsApplier::new(path.clone()).apply(t).unwrap();
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn label_is_stable() {
        assert_eq!(
            GtkSettingsApplier::new(PathBuf::new()).label(),
            "gtk-settings"
        );
    }

    #[test]
    fn replaces_existing_key_in_place() {
        let src = "[Settings]\ngtk-theme-name=Adwaita\ngtk-cursor-theme-name=Old\ngtk-cursor-theme-size=24\n";
        assert_eq!(
            apply_to(src, &theme("Pretty", "Nordzy-cursors")),
            "[Settings]\ngtk-theme-name=Adwaita\ngtk-cursor-theme-name=Nordzy-cursors\ngtk-cursor-theme-size=24\n"
        );
    }

    #[test]
    fn keeps_spaced_assignment_style() {
        let src = "[Settings]\ngtk-cursor-theme-name = Old\n";
        assert_eq!(
            apply_to(src, &theme("x", "New")),
            "[Settings]\ngtk-cursor-theme-name = New\n"
        );
    }

    #[test]
    fn inserts_at_end_of_settings_section_before_next_section() {
        let src = "[Settings]\ngtk-theme-name=Adwaita\n\n[Other]\nfoo=bar\n";
        assert_eq!(
            apply_to(src, &theme("x", "New")),
            "[Settings]\ngtk-theme-name=Adwaita\ngtk-cursor-theme-name=New\n\n[Other]\nfoo=bar\n"
        );
    }

    #[test]
    fn inserts_when_settings_is_the_last_section_without_trailing_newline() {
        let src = "[Settings]\ngtk-theme-name=Adwaita";
        assert_eq!(
            apply_to(src, &theme("x", "New")),
            "[Settings]\ngtk-theme-name=Adwaita\ngtk-cursor-theme-name=New\n"
        );
    }

    #[test]
    fn creates_section_when_missing() {
        let src = "[Other]\nfoo=bar\n";
        assert_eq!(
            apply_to(src, &theme("x", "New")),
            "[Other]\nfoo=bar\n\n[Settings]\ngtk-cursor-theme-name=New\n"
        );
    }

    #[test]
    fn empty_file_gets_section() {
        assert_eq!(
            apply_to("", &theme("x", "New")),
            "[Settings]\ngtk-cursor-theme-name=New\n"
        );
    }

    #[test]
    fn ignores_same_key_in_another_section_and_comments() {
        let src =
            "[Other]\ngtk-cursor-theme-name=Keep\n[Settings]\n# gtk-cursor-theme-name=Commented\n";
        assert_eq!(
            apply_to(src, &theme("x", "New")),
            "[Other]\ngtk-cursor-theme-name=Keep\n[Settings]\n# gtk-cursor-theme-name=Commented\ngtk-cursor-theme-name=New\n"
        );
    }

    #[test]
    fn preserves_other_keys_and_comments_byte_for_byte() {
        let src = "# top\n[Settings]\ngtk-cursor-theme-size=32\ngtk-application-prefer-dark-theme=1\ngtk-cursor-theme-name=Old\n";
        let out = apply_to(src, &theme("x", "New"));
        assert_eq!(
            out,
            "# top\n[Settings]\ngtk-cursor-theme-size=32\ngtk-application-prefer-dark-theme=1\ngtk-cursor-theme-name=New\n"
        );
    }

    #[test]
    fn missing_file_is_created_with_parent_dir() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("gtk-3.0/settings.ini");
        GtkSettingsApplier::new(path.clone())
            .apply(&theme("x", "New"))
            .unwrap();
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "[Settings]\ngtk-cursor-theme-name=New\n"
        );
    }

    #[test]
    fn newline_in_name_cannot_inject_a_key() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.ini");
        fs::write(&path, "[Settings]\n").unwrap();
        let t = theme("x", "a\ngtk-modules=evil");
        assert!(GtkSettingsApplier::new(path.clone()).apply(&t).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "[Settings]\n");
    }

    #[cfg(unix)]
    #[test]
    fn writes_through_a_symlinked_ini() {
        let dir = TempDir::new().unwrap();
        let real = dir.path().join("real.ini");
        fs::write(&real, "[Settings]\ngtk-cursor-theme-name=Old\n").unwrap();
        let link = dir.path().join("settings.ini");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        GtkSettingsApplier::new(link.clone())
            .apply(&theme("x", "New"))
            .unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(fs::read_to_string(&real).unwrap().contains("=New"));
    }
}
