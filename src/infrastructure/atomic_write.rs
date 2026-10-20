use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Replaces `path` with `bytes` through a temp file in the same directory.
///
/// A symlink at `path` is resolved first so the link itself survives and the
/// content lands on its target. A dangling link is an error.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let target = resolve_target(path)?;
    let dir = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let tmp = temp_path(dir, &target);
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        if let Ok(meta) = fs::metadata(&target) {
            fs::set_permissions(&tmp, meta.permissions())?;
        }
        file.sync_all()?;
        fs::rename(&tmp, &target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Follows a symlink to its final target; a dangling link is an error.
fn resolve_target(path: &Path) -> io::Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => fs::canonicalize(path),
        Ok(_) => Ok(path.to_path_buf()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(e) => Err(e),
    }
}

fn temp_path(dir: &Path, target: &Path) -> PathBuf {
    let mut name = OsString::from(".");
    name.push(target.file_name().unwrap_or_default());
    name.push(format!(".{}.tmp", std::process::id()));
    dir.join(name)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};

    use tempfile::TempDir;

    use super::*;

    fn leftovers(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect()
    }

    #[test]
    fn creates_new_file_and_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("a/b/file.txt");
        write_atomic(&path, b"hello").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"hello");
    }

    #[test]
    fn overwrites_existing_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("file.txt");
        fs::write(&path, "old content that is longer").unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
    }

    #[test]
    fn preserves_permission_bits() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("file.txt");
        fs::write(&path, "x").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        write_atomic(&path, b"y").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);
    }

    #[test]
    fn leaves_no_temp_file_behind() {
        let dir = TempDir::new().unwrap();
        write_atomic(&dir.path().join("file.txt"), b"x").unwrap();
        assert!(leftovers(dir.path()).is_empty());
    }

    #[test]
    fn preserves_symlink_and_writes_to_target() {
        let dir = TempDir::new().unwrap();
        let real_dir = dir.path().join("dotfiles");
        fs::create_dir(&real_dir).unwrap();
        let real = real_dir.join("settings.ini");
        fs::write(&real, "old").unwrap();
        let link = dir.path().join("settings.ini");
        symlink(&real, &link).unwrap();

        write_atomic(&link, b"new").unwrap();

        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_link(&link).unwrap(), real);
        assert_eq!(fs::read(&real).unwrap(), b"new");
        assert!(leftovers(dir.path()).is_empty());
        assert!(leftovers(&real_dir).is_empty());
    }

    #[test]
    fn follows_relative_symlink_chain() {
        let dir = TempDir::new().unwrap();
        let real = dir.path().join("real.txt");
        fs::write(&real, "old").unwrap();
        symlink("real.txt", dir.path().join("mid")).unwrap();
        symlink("mid", dir.path().join("top")).unwrap();
        write_atomic(&dir.path().join("top"), b"new").unwrap();
        assert_eq!(fs::read(&real).unwrap(), b"new");
        assert!(
            fs::symlink_metadata(dir.path().join("top"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn dangling_symlink_is_an_error_and_left_alone() {
        let dir = TempDir::new().unwrap();
        let link = dir.path().join("dangling");
        symlink(dir.path().join("missing"), &link).unwrap();
        assert!(write_atomic(&link, b"x").is_err());
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(!dir.path().join("missing").exists());
        assert!(leftovers(dir.path()).is_empty());
    }

    #[test]
    fn failure_removes_temp_file() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("a_directory");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("child"), "x").unwrap();
        // Renaming a file over a non-empty directory fails.
        assert!(write_atomic(&target, b"x").is_err());
        assert!(leftovers(dir.path()).is_empty());
    }
}
