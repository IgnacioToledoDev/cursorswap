//! Replaces the running executable with a verified download.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::domain::error::UpdateError;
use crate::domain::ports::BinaryInstaller;
use crate::domain::update::ReleaseInfo;

use super::http::HttpGet;

const BINARY_LIMIT_BYTES: u64 = 256 * 1024 * 1024;
const CHECKSUM_LIMIT_BYTES: u64 = 4 * 1024;

pub struct ExeInstaller<H: HttpGet> {
    http: H,
    exe: PathBuf,
}

impl<H: HttpGet> ExeInstaller<H> {
    pub fn new(http: H, exe: PathBuf) -> Self {
        ExeInstaller { http, exe }
    }
}

/// Extracts the hex digest from a `sha256sum`-style line.
pub fn parse_checksum(text: &str) -> Result<String, UpdateError> {
    let token = text.split_whitespace().next().unwrap_or_default();
    if token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(token.to_ascii_lowercase())
    } else {
        Err(UpdateError::Parse(
            "checksum file has no sha256 digest".into(),
        ))
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Writes `bytes` next to `target` and renames it into place, executable.
fn replace_executable(target: &Path, bytes: &[u8]) -> Result<(), UpdateError> {
    let io = |e: std::io::Error| UpdateError::Io(e.to_string());
    let dir = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut name = std::ffi::OsString::from(".");
    name.push(target.file_name().unwrap_or_default());
    name.push(format!(".update.{}.tmp", std::process::id()));
    let tmp = dir.join(name);
    let mode = fs::metadata(target)
        .map(|meta| (meta.permissions().mode() & 0o7777) | 0o111)
        .unwrap_or(0o755);
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        file.sync_all()?;
        fs::rename(&tmp, target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(io)
}

impl<H: HttpGet> BinaryInstaller for ExeInstaller<H> {
    fn install(&self, release: &ReleaseInfo) -> Result<(), UpdateError> {
        let checksum = self.http.get(&release.checksum_url, CHECKSUM_LIMIT_BYTES)?;
        let expected = parse_checksum(&String::from_utf8_lossy(&checksum))?;
        let binary = self.http.get(&release.binary_url, BINARY_LIMIT_BYTES)?;
        if binary.is_empty() {
            return Err(UpdateError::Io("downloaded binary is empty".into()));
        }
        let actual = sha256_hex(&binary);
        if actual != expected {
            return Err(UpdateError::ChecksumMismatch { expected, actual });
        }
        replace_executable(&self.exe, &binary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::http::fake::FakeHttp;
    use tempfile::TempDir;

    const NEW: &[u8] = b"#!new-binary";

    fn release() -> ReleaseInfo {
        ReleaseInfo {
            version: "0.2.0".into(),
            binary_url: "https://example.invalid/bin".into(),
            checksum_url: "https://example.invalid/bin.sha256".into(),
        }
    }

    fn http_with(binary: &[u8], checksum_line: &str) -> FakeHttp {
        FakeHttp::default()
            .with("https://example.invalid/bin", binary)
            .with(
                "https://example.invalid/bin.sha256",
                checksum_line.as_bytes(),
            )
    }

    fn installed_exe(dir: &TempDir, mode: u32) -> PathBuf {
        let exe = dir.path().join("cursorswap");
        fs::write(&exe, b"old-binary").unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(mode)).unwrap();
        exe
    }

    fn entries(dir: &TempDir) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn good_line() -> String {
        format!("{}  cursorswap-x86_64-unknown-linux-gnu\n", sha256_hex(NEW))
    }

    #[test]
    fn sha256_matches_the_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn checksum_lines_are_parsed() {
        let hex = sha256_hex(b"abc");
        assert_eq!(parse_checksum(&format!("{hex}  file\n")).unwrap(), hex);
        assert_eq!(parse_checksum(&format!("{hex}\n")).unwrap(), hex);
        assert_eq!(
            parse_checksum(&format!("{}  f", hex.to_uppercase())).unwrap(),
            hex
        );
    }

    #[test]
    fn malformed_checksums_are_rejected() {
        for text in ["", "\n", "zz  file", "abc123  file", &"g".repeat(64)] {
            assert!(
                matches!(parse_checksum(text), Err(UpdateError::Parse(_))),
                "{text:?}"
            );
        }
    }

    #[test]
    fn matching_download_replaces_the_binary_atomically() {
        let dir = TempDir::new().unwrap();
        let exe = installed_exe(&dir, 0o755);
        let installer = ExeInstaller::new(http_with(NEW, &good_line()), exe.clone());
        installer.install(&release()).unwrap();
        assert_eq!(fs::read(&exe).unwrap(), NEW);
        assert_eq!(entries(&dir), ["cursorswap"]);
    }

    #[test]
    fn replaced_binary_is_executable_even_if_the_old_one_was_not() {
        let dir = TempDir::new().unwrap();
        let exe = installed_exe(&dir, 0o644);
        ExeInstaller::new(http_with(NEW, &good_line()), exe.clone())
            .install(&release())
            .unwrap();
        let mode = fs::metadata(&exe).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "{mode:o}");
    }

    #[test]
    fn missing_target_gets_a_default_executable_mode() {
        let dir = TempDir::new().unwrap();
        let exe = dir.path().join("cursorswap");
        ExeInstaller::new(http_with(NEW, &good_line()), exe.clone())
            .install(&release())
            .unwrap();
        assert_eq!(
            fs::metadata(&exe).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }

    #[test]
    fn checksum_mismatch_leaves_the_binary_and_directory_untouched() {
        let dir = TempDir::new().unwrap();
        let exe = installed_exe(&dir, 0o755);
        let wrong = format!("{}  x\n", sha256_hex(b"something else"));
        let err = ExeInstaller::new(http_with(NEW, &wrong), exe.clone())
            .install(&release())
            .unwrap_err();
        assert!(matches!(err, UpdateError::ChecksumMismatch { .. }));
        assert_eq!(fs::read(&exe).unwrap(), b"old-binary");
        assert_eq!(entries(&dir), ["cursorswap"]);
    }

    #[test]
    fn download_failures_leave_everything_untouched() {
        let dir = TempDir::new().unwrap();
        let exe = installed_exe(&dir, 0o755);
        let no_binary = FakeHttp::default()
            .failing("https://example.invalid/bin")
            .with("https://example.invalid/bin.sha256", good_line().as_bytes());
        let no_checksum = FakeHttp::default()
            .with("https://example.invalid/bin", NEW)
            .failing("https://example.invalid/bin.sha256");
        for http in [no_binary, no_checksum] {
            let err = ExeInstaller::new(http, exe.clone())
                .install(&release())
                .unwrap_err();
            assert!(matches!(err, UpdateError::Network(_)));
            assert_eq!(fs::read(&exe).unwrap(), b"old-binary");
            assert_eq!(entries(&dir), ["cursorswap"]);
        }
    }

    #[test]
    fn empty_download_is_rejected_even_with_a_matching_checksum() {
        let dir = TempDir::new().unwrap();
        let exe = installed_exe(&dir, 0o755);
        let line = format!("{}  x\n", sha256_hex(b""));
        let result = ExeInstaller::new(http_with(b"", &line), exe.clone()).install(&release());
        assert!(result.is_err());
        assert_eq!(fs::read(&exe).unwrap(), b"old-binary");
    }

    #[test]
    fn unwritable_target_directory_is_an_io_error_without_leftovers() {
        let dir = TempDir::new().unwrap();
        let exe = dir.path().join("missing-dir/cursorswap");
        let err = ExeInstaller::new(http_with(NEW, &good_line()), exe)
            .install(&release())
            .unwrap_err();
        assert!(matches!(err, UpdateError::Io(_)));
        assert!(entries(&dir).is_empty());
    }

    #[test]
    fn requests_are_size_bounded() {
        let dir = TempDir::new().unwrap();
        let http = http_with(NEW, &good_line());
        let installer = ExeInstaller::new(http, installed_exe(&dir, 0o755));
        installer.install(&release()).unwrap();
        let requests = installer.http.requests.borrow();
        assert_eq!(requests[0].1, CHECKSUM_LIMIT_BYTES);
        assert_eq!(requests[1].1, BINARY_LIMIT_BYTES);
    }
}
