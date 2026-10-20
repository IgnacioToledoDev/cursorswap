use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use crate::domain::cursor::CursorTheme;
use crate::domain::error::ApplyError;
use crate::domain::ports::CursorApplier;

use super::name::system_theme_name;

/// Makes a theme discoverable by symlinking it into `link_dir` when it does not
/// already live in one of the standard icon search directories.
pub struct IconPathApplier {
    search_dirs: Vec<PathBuf>,
    link_dir: PathBuf,
}

impl IconPathApplier {
    pub fn new(search_dirs: Vec<PathBuf>, link_dir: PathBuf) -> Self {
        Self {
            search_dirs,
            link_dir,
        }
    }
}

impl IconPathApplier {
    fn is_in_search_dir(&self, theme_path: &Path) -> bool {
        let Some(parent) = theme_path.parent() else {
            return false;
        };
        let parent = canonical_or_raw(parent);
        self.search_dirs
            .iter()
            .any(|dir| canonical_or_raw(dir) == parent)
    }
}

fn canonical_or_raw(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn points_to(link: &Path, target: &Path) -> bool {
    match fs::read_link(link) {
        Ok(current) => current == target || canonical_or_raw(link) == canonical_or_raw(target),
        Err(_) => false,
    }
}

fn io_error(path: &Path, e: io::Error) -> ApplyError {
    ApplyError::Io(format!("{}: {e}", path.display()))
}

impl CursorApplier for IconPathApplier {
    fn label(&self) -> &'static str {
        "icon-path"
    }

    fn apply(&self, theme: &CursorTheme) -> Result<(), ApplyError> {
        let name = system_theme_name(theme)?;
        if self.is_in_search_dir(&theme.path) {
            return Ok(());
        }
        fs::create_dir_all(&self.link_dir).map_err(|e| io_error(&self.link_dir, e))?;
        let link = self.link_dir.join(name);
        match fs::symlink_metadata(&link) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                symlink(&theme.path, &link).map_err(|e| io_error(&link, e))
            }
            Err(e) => Err(io_error(&link, e)),
            Ok(meta) if meta.file_type().is_symlink() && points_to(&link, &theme.path) => Ok(()),
            Ok(_) => Err(ApplyError::Other(format!(
                "{} already exists and is not a link to {}",
                link.display(),
                theme.path.display()
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use tempfile::TempDir;

    use super::*;

    struct Env {
        _root: TempDir,
        search: PathBuf,
        link_dir: PathBuf,
        outside: PathBuf,
    }

    fn env() -> Env {
        let root = TempDir::new().unwrap();
        let search = root.path().join("usr-icons");
        let outside = root.path().join("dotfiles");
        fs::create_dir_all(&search).unwrap();
        fs::create_dir_all(&outside).unwrap();
        Env {
            link_dir: root.path().join("home/.local/share/icons"),
            search,
            outside,
            _root: root,
        }
    }

    fn theme_in(dir: &Path, dir_name: &str, friendly: &str) -> CursorTheme {
        let path = dir.join(dir_name);
        fs::create_dir_all(&path).unwrap();
        CursorTheme::new(friendly, None, path).unwrap()
    }

    fn applier(e: &Env) -> IconPathApplier {
        IconPathApplier::new(vec![e.search.clone()], e.link_dir.clone())
    }

    #[test]
    fn label_is_stable() {
        assert_eq!(applier(&env()).label(), "icon-path");
    }

    #[test]
    fn theme_inside_search_dir_needs_no_link() {
        let e = env();
        let t = theme_in(&e.search, "Bibata", "Bibata");
        applier(&e).apply(&t).unwrap();
        assert!(!e.link_dir.exists());
    }

    #[test]
    fn search_dir_match_survives_non_canonical_paths() {
        let e = env();
        let t = theme_in(&e.search, "Bibata", "Bibata");
        let dotted = e.search.join("..").join("usr-icons");
        IconPathApplier::new(vec![dotted], e.link_dir.clone())
            .apply(&t)
            .unwrap();
        assert!(!e.link_dir.exists());
    }

    #[test]
    fn theme_outside_search_dirs_is_symlinked_and_dir_created() {
        let e = env();
        let t = theme_in(&e.outside, "Nordzy-cursors", "Pretty");
        applier(&e).apply(&t).unwrap();
        let link = e.link_dir.join("Nordzy-cursors");
        assert_eq!(fs::read_link(&link).unwrap(), t.path);
    }

    #[test]
    fn existing_identical_link_is_idempotent() {
        let e = env();
        let t = theme_in(&e.outside, "Nordzy-cursors", "x");
        applier(&e).apply(&t).unwrap();
        applier(&e).apply(&t).unwrap();
        assert_eq!(
            fs::read_link(e.link_dir.join("Nordzy-cursors")).unwrap(),
            t.path
        );
    }

    #[test]
    fn link_to_a_different_target_is_error_and_untouched() {
        let e = env();
        let t = theme_in(&e.outside, "Nordzy-cursors", "x");
        fs::create_dir_all(&e.link_dir).unwrap();
        let other = e.outside.join("other");
        fs::create_dir(&other).unwrap();
        let link = e.link_dir.join("Nordzy-cursors");
        symlink(&other, &link).unwrap();
        assert!(applier(&e).apply(&t).is_err());
        assert_eq!(fs::read_link(&link).unwrap(), other);
    }

    #[test]
    fn regular_file_in_the_way_is_error_and_untouched() {
        let e = env();
        let t = theme_in(&e.outside, "Nordzy-cursors", "x");
        fs::create_dir_all(&e.link_dir).unwrap();
        let blocker = e.link_dir.join("Nordzy-cursors");
        fs::write(&blocker, "mine").unwrap();
        assert!(applier(&e).apply(&t).is_err());
        assert_eq!(fs::read_to_string(&blocker).unwrap(), "mine");
    }

    #[test]
    fn real_directory_in_the_way_is_error_and_untouched() {
        let e = env();
        let t = theme_in(&e.outside, "Nordzy-cursors", "x");
        let blocker = e.link_dir.join("Nordzy-cursors");
        fs::create_dir_all(&blocker).unwrap();
        fs::write(blocker.join("keep"), "1").unwrap();
        assert!(applier(&e).apply(&t).is_err());
        assert!(blocker.join("keep").exists());
    }

    #[test]
    fn uses_directory_name_for_the_link() {
        let e = env();
        let t = theme_in(&e.outside, "Nordzy-cursors", "Pretty");
        applier(&e).apply(&t).unwrap();
        assert!(e.link_dir.join("Pretty").symlink_metadata().is_err());
    }
}
