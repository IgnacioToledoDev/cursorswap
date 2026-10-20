//! File-backed update cache and skip marker.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::domain::error::UpdateError;
use crate::domain::ports::UpdateStateStore;
use crate::domain::update::{ReleaseNotes, UpdateState, release_page_url};

use super::atomic_write::write_atomic;

#[derive(Serialize, Deserialize, Default)]
struct StateFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    checked_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    latest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    skipped: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    release_url: Option<String>,
}

pub struct TomlUpdateStateStore {
    path: PathBuf,
}

impl TomlUpdateStateStore {
    pub fn new(path: PathBuf) -> Self {
        TomlUpdateStateStore { path }
    }
}

impl UpdateStateStore for TomlUpdateStateStore {
    fn load(&self) -> UpdateState {
        let parsed = std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| toml::from_str::<StateFile>(&text).ok())
            .unwrap_or_default();
        UpdateState {
            checked_at: parsed.checked_at,
            latest: parsed.latest,
            skipped: parsed.skipped,
            // The file is user-writable, so it gets the same checks as the network.
            notes: parsed
                .notes
                .as_deref()
                .map(ReleaseNotes::sanitize)
                .filter(|notes| !notes.is_empty()),
            release_url: parsed.release_url.as_deref().and_then(release_page_url),
        }
    }

    fn save(&self, state: &UpdateState) -> Result<(), UpdateError> {
        let file = StateFile {
            checked_at: state.checked_at,
            latest: state.latest.clone(),
            skipped: state.skipped.clone(),
            notes: state.notes.as_ref().map(|n| n.as_str().to_string()),
            release_url: state.release_url.clone(),
        };
        let text = toml::to_string(&file).map_err(|e| UpdateError::Io(e.to_string()))?;
        write_atomic(&self.path, text.as_bytes()).map_err(|e| UpdateError::Io(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::update::ReleaseNotes;
    use tempfile::TempDir;

    fn store(dir: &TempDir) -> TomlUpdateStateStore {
        TomlUpdateStateStore::new(dir.path().join("conf/update-state.toml"))
    }

    #[test]
    fn missing_file_reads_as_default() {
        assert_eq!(
            store(&TempDir::new().unwrap()).load(),
            UpdateState::default()
        );
    }

    #[test]
    fn state_round_trips_and_creates_the_directory() {
        let dir = TempDir::new().unwrap();
        let state = UpdateState {
            checked_at: Some(1_700_000_000),
            latest: Some("0.2.0".into()),
            skipped: Some("0.1.5".into()),
            ..UpdateState::default()
        };
        store(&dir).save(&state).unwrap();
        assert_eq!(store(&dir).load(), state);
    }

    #[test]
    fn notes_and_link_round_trip() {
        let dir = TempDir::new().unwrap();
        let state = UpdateState {
            checked_at: Some(5),
            latest: Some("0.2.0".into()),
            notes: Some(ReleaseNotes::sanitize("## Fixes\n* one\n* two")),
            release_url: Some("https://github.com/o/r/releases/tag/v0.2.0".into()),
            ..UpdateState::default()
        };
        store(&dir).save(&state).unwrap();
        assert_eq!(store(&dir).load(), state);
    }

    #[test]
    fn old_cache_without_notes_still_loads() {
        let dir = TempDir::new().unwrap();
        let s = store(&dir);
        std::fs::create_dir_all(s.path.parent().unwrap()).unwrap();
        std::fs::write(&s.path, "checked_at = 5\nlatest = \"0.2.0\"\n").unwrap();
        let state = s.load();
        assert_eq!(state.latest.as_deref(), Some("0.2.0"));
        assert_eq!((state.notes, state.release_url), (None, None));
    }

    #[test]
    fn a_tampered_cache_cannot_smuggle_escapes_or_foreign_links() {
        let dir = TempDir::new().unwrap();
        let s = store(&dir);
        std::fs::create_dir_all(s.path.parent().unwrap()).unwrap();
        std::fs::write(
            &s.path,
            "latest = \"0.2.0\"\nnotes = \"safe\\u001b[2J\\u001b]52;c;AAAA\\u0007 text\"\nrelease_url = \"https://evil.example/x\"\n",
        )
        .unwrap();
        let state = s.load();
        assert_eq!(state.notes.unwrap().as_str(), "safe text");
        assert_eq!(state.release_url, None);
    }

    #[test]
    fn partial_files_fill_in_defaults() {
        let dir = TempDir::new().unwrap();
        let s = store(&dir);
        std::fs::create_dir_all(s.path.parent().unwrap()).unwrap();
        std::fs::write(&s.path, "skipped = \"0.3.0\"\n").unwrap();
        let state = s.load();
        assert_eq!(state.skipped.as_deref(), Some("0.3.0"));
        assert_eq!(state.checked_at, None);
    }

    #[test]
    fn corrupt_or_mistyped_files_read_as_default() {
        let dir = TempDir::new().unwrap();
        let s = store(&dir);
        std::fs::create_dir_all(s.path.parent().unwrap()).unwrap();
        for content in ["\u{0}\u{1}garbage", "checked_at = \"yesterday\"", "[[[["] {
            std::fs::write(&s.path, content).unwrap();
            assert_eq!(s.load(), UpdateState::default(), "{content:?}");
        }
    }

    #[test]
    fn save_failure_is_reported() {
        let dir = TempDir::new().unwrap();
        let blocker = dir.path().join("file");
        std::fs::write(&blocker, "x").unwrap();
        let s = TomlUpdateStateStore::new(blocker.join("state.toml"));
        assert!(matches!(
            s.save(&UpdateState::default()),
            Err(UpdateError::Io(_))
        ));
    }
}
