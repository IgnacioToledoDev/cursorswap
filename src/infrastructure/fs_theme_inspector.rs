use std::path::Path;

use crate::domain::ports::ThemeInspector;

pub struct FsThemeInspector;

impl ThemeInspector for FsThemeInspector {
    fn is_valid_theme_dir(&self, path: &Path) -> bool {
        path.join("index.theme").is_file() && path.join("cursors").is_dir()
    }

    fn dir_name(&self, path: &Path) -> Option<String> {
        path.file_name().map(|n| n.to_string_lossy().into_owned())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::*;

    fn theme_dir(tmp: &TempDir) -> PathBuf {
        let dir = tmp.path().join("Theme");
        fs::create_dir_all(dir.join("cursors")).unwrap();
        fs::write(dir.join("index.theme"), "[Icon Theme]\n").unwrap();
        dir
    }

    #[test]
    fn accepts_valid_theme_dir() {
        let tmp = TempDir::new().unwrap();
        assert!(FsThemeInspector.is_valid_theme_dir(&theme_dir(&tmp)));
    }

    #[test]
    fn rejects_missing_index_theme() {
        let tmp = TempDir::new().unwrap();
        let dir = theme_dir(&tmp);
        fs::remove_file(dir.join("index.theme")).unwrap();
        assert!(!FsThemeInspector.is_valid_theme_dir(&dir));
    }

    #[test]
    fn rejects_missing_cursors_dir() {
        let tmp = TempDir::new().unwrap();
        let dir = theme_dir(&tmp);
        fs::remove_dir(dir.join("cursors")).unwrap();
        assert!(!FsThemeInspector.is_valid_theme_dir(&dir));
    }

    #[test]
    fn rejects_cursors_as_file() {
        let tmp = TempDir::new().unwrap();
        let dir = theme_dir(&tmp);
        fs::remove_dir(dir.join("cursors")).unwrap();
        fs::write(dir.join("cursors"), "x").unwrap();
        assert!(!FsThemeInspector.is_valid_theme_dir(&dir));
    }

    #[test]
    fn rejects_index_theme_as_directory() {
        let tmp = TempDir::new().unwrap();
        let dir = theme_dir(&tmp);
        fs::remove_file(dir.join("index.theme")).unwrap();
        fs::create_dir(dir.join("index.theme")).unwrap();
        assert!(!FsThemeInspector.is_valid_theme_dir(&dir));
    }

    #[test]
    fn rejects_path_that_is_not_a_directory() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("file");
        fs::write(&file, "x").unwrap();
        assert!(!FsThemeInspector.is_valid_theme_dir(&file));
        assert!(!FsThemeInspector.is_valid_theme_dir(&tmp.path().join("missing")));
    }

    #[test]
    fn dir_name_returns_last_component() {
        let name = FsThemeInspector.dir_name(Path::new("/a/b/Nordzy-cursors"));
        assert_eq!(name.as_deref(), Some("Nordzy-cursors"));
    }

    #[test]
    fn dir_name_ignores_trailing_slash() {
        let name = FsThemeInspector.dir_name(Path::new("/a/b/Nordzy-cursors/"));
        assert_eq!(name.as_deref(), Some("Nordzy-cursors"));
    }

    #[test]
    fn dir_name_is_none_without_component() {
        assert_eq!(FsThemeInspector.dir_name(Path::new("/")), None);
    }
}
