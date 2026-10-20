//! Self-update use cases over the update ports.

use crate::domain::error::UpdateError;
use crate::domain::ports::{
    BinaryInstaller, Clock, InstallMethodDetector, ReleaseSource, UpdateStateStore,
};
use crate::domain::update::{
    InstallMethod, ReleaseDetails, cache_is_fresh, changelog_link, is_newer,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateStatus {
    UpToDate {
        latest: String,
    },
    Available {
        latest: String,
        details: ReleaseDetails,
    },
    Skipped {
        latest: String,
        details: ReleaseDetails,
    },
    Managed {
        latest: String,
        method: InstallMethod,
        details: ReleaseDetails,
    },
    Unavailable(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckOptions {
    pub use_cache: bool,
    pub honor_skip: bool,
}

impl CheckOptions {
    /// Startup check: cached for six hours, silent about skipped versions.
    pub const AUTO: CheckOptions = CheckOptions {
        use_cache: true,
        honor_skip: true,
    };
    /// Explicit `update` command: always asks the network, ignores skips.
    pub const MANUAL: CheckOptions = CheckOptions {
        use_cache: false,
        honor_skip: false,
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
    Updated { version: String },
    AlreadyCurrent,
}

pub fn check_for_update(
    source: &dyn ReleaseSource,
    store: &dyn UpdateStateStore,
    detector: &dyn InstallMethodDetector,
    clock: &dyn Clock,
    installed: &str,
    options: CheckOptions,
) -> UpdateStatus {
    let mut state = store.load();
    let cache_usable = options.use_cache && cache_is_fresh(&state, clock.now_secs());
    let latest = match (cache_usable, state.latest.clone()) {
        (true, Some(latest)) => latest,
        _ => match source.latest_stable() {
            Ok(release) => {
                state.checked_at = Some(clock.now_secs());
                state.latest = Some(release.version.clone());
                state.notes = release.notes;
                state.release_url = release.url;
                // A failed cache write only costs a later network call.
                let _ = store.save(&state);
                release.version
            }
            Err(err) => return UpdateStatus::Unavailable(err.to_string()),
        },
    };
    if !is_newer(installed, &latest) {
        return UpdateStatus::UpToDate { latest };
    }
    let details = ReleaseDetails {
        link: changelog_link(
            installed,
            &latest,
            state.notes.as_ref(),
            state.release_url.as_deref(),
        ),
        notes: state.notes,
    };
    if options.honor_skip && state.skipped.as_deref() == Some(latest.as_str()) {
        return UpdateStatus::Skipped { latest, details };
    }
    match detector.detect() {
        InstallMethod::Standalone => UpdateStatus::Available { latest, details },
        method => UpdateStatus::Managed {
            latest,
            method,
            details,
        },
    }
}

/// Downloads and installs the latest release when it is newer than `installed`.
pub fn apply_update(
    source: &dyn ReleaseSource,
    installer: &dyn BinaryInstaller,
    installed: &str,
) -> Result<UpdateOutcome, UpdateError> {
    let release = source.latest_stable()?;
    if !is_newer(installed, &release.version) {
        return Ok(UpdateOutcome::AlreadyCurrent);
    }
    installer.install(&release)?;
    Ok(UpdateOutcome::Updated {
        version: release.version,
    })
}

/// Remembers `version` as skipped until a newer one is released.
pub fn skip_version(store: &dyn UpdateStateStore, version: &str) -> Result<(), UpdateError> {
    let mut state = store.load();
    state.skipped = Some(version.to_string());
    store.save(&state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::fakes::{
        FakeClock, FakeDetector, FakeInstaller, FakeReleaseSource, FakeStateStore,
    };
    use crate::domain::update::{CACHE_TTL_SECS, ReleaseNotes, UpdateState};

    const NOW: u64 = 1_000_000;

    fn cached(latest: &str, age: u64) -> UpdateState {
        UpdateState {
            checked_at: Some(NOW - age),
            latest: Some(latest.into()),
            ..UpdateState::default()
        }
    }

    fn check(
        source: &FakeReleaseSource,
        store: &FakeStateStore,
        method: InstallMethod,
        options: CheckOptions,
    ) -> UpdateStatus {
        check_for_update(
            source,
            store,
            &FakeDetector(method),
            &FakeClock(NOW),
            "0.1.0",
            options,
        )
    }

    fn auto(source: &FakeReleaseSource, store: &FakeStateStore) -> UpdateStatus {
        check(source, store, InstallMethod::Standalone, CheckOptions::AUTO)
    }

    fn page(v: &str) -> String {
        format!("https://github.com/IgnacioToledoDev/cursorswap/releases/tag/v{v}")
    }

    fn details(v: &str) -> ReleaseDetails {
        ReleaseDetails {
            notes: None,
            link: page(v),
        }
    }

    fn available(v: &str) -> UpdateStatus {
        UpdateStatus::Available {
            latest: v.into(),
            details: details(v),
        }
    }

    #[test]
    fn missing_cache_fetches_and_stores_the_result() {
        let source = FakeReleaseSource::latest("0.2.0");
        let store = FakeStateStore::default();
        assert_eq!(auto(&source, &store), available("0.2.0"));
        assert_eq!(source.calls.get(), 1);
        let saved = store.state.borrow().clone();
        assert_eq!(saved.checked_at, Some(NOW));
        assert_eq!(saved.latest.as_deref(), Some("0.2.0"));
    }

    #[test]
    fn fresh_cache_skips_the_network() {
        let source = FakeReleaseSource::failing();
        let store = FakeStateStore::with(cached("0.2.0", 60));
        assert_eq!(auto(&source, &store), available("0.2.0"));
        assert_eq!(source.calls.get(), 0);
        assert_eq!(store.saves.get(), 0);
    }

    #[test]
    fn stale_cache_is_refreshed() {
        let source = FakeReleaseSource::latest("0.3.0");
        let store = FakeStateStore::with(cached("0.2.0", CACHE_TTL_SECS));
        assert_eq!(auto(&source, &store), available("0.3.0"));
        assert_eq!(source.calls.get(), 1);
        assert_eq!(store.state.borrow().latest.as_deref(), Some("0.3.0"));
    }

    #[test]
    fn equal_or_older_latest_is_up_to_date() {
        for latest in ["0.1.0", "0.0.9"] {
            let status = auto(
                &FakeReleaseSource::latest(latest),
                &FakeStateStore::default(),
            );
            assert_eq!(
                status,
                UpdateStatus::UpToDate {
                    latest: latest.into()
                }
            );
        }
    }

    #[test]
    fn prerelease_latest_is_never_offered() {
        let status = auto(
            &FakeReleaseSource::latest("0.2.0-rc.1"),
            &FakeStateStore::default(),
        );
        assert!(matches!(status, UpdateStatus::UpToDate { .. }));
    }

    #[test]
    fn network_failure_is_unavailable_and_leaves_the_cache_alone() {
        let store = FakeStateStore::with(cached("0.2.0", CACHE_TTL_SECS + 5));
        let status = auto(&FakeReleaseSource::failing(), &store);
        assert!(matches!(status, UpdateStatus::Unavailable(_)));
        assert_eq!(store.saves.get(), 0);
        assert_eq!(
            store.state.borrow().checked_at,
            Some(NOW - CACHE_TTL_SECS - 5)
        );
    }

    #[test]
    fn cache_write_failure_does_not_hide_the_result() {
        let store = FakeStateStore {
            fail_save: true,
            ..FakeStateStore::default()
        };
        assert_eq!(
            auto(&FakeReleaseSource::latest("0.2.0"), &store),
            available("0.2.0")
        );
    }

    #[test]
    fn skipped_version_is_silent_until_a_newer_one_appears() {
        let skipped = |latest: &str| UpdateState {
            skipped: Some("0.2.0".into()),
            ..cached(latest, 10)
        };
        let store = FakeStateStore::with(skipped("0.2.0"));
        assert_eq!(
            auto(&FakeReleaseSource::failing(), &store),
            UpdateStatus::Skipped {
                latest: "0.2.0".into(),
                details: details("0.2.0")
            }
        );
        let store = FakeStateStore::with(skipped("0.3.0"));
        assert_eq!(
            auto(&FakeReleaseSource::failing(), &store),
            available("0.3.0")
        );
    }

    #[test]
    fn refresh_keeps_the_skipped_version() {
        let store = FakeStateStore::with(UpdateState {
            skipped: Some("0.2.0".into()),
            ..UpdateState::default()
        });
        auto(&FakeReleaseSource::latest("0.2.0"), &store);
        assert_eq!(store.state.borrow().skipped.as_deref(), Some("0.2.0"));
    }

    #[test]
    fn managed_installs_get_a_notice_instead_of_an_offer() {
        for method in [InstallMethod::PackageManager, InstallMethod::Cargo] {
            let status = check(
                &FakeReleaseSource::latest("0.2.0"),
                &FakeStateStore::default(),
                method,
                CheckOptions::AUTO,
            );
            assert_eq!(
                status,
                UpdateStatus::Managed {
                    latest: "0.2.0".into(),
                    method,
                    details: details("0.2.0")
                }
            );
        }
    }

    #[test]
    fn manual_check_ignores_cache_and_skip() {
        let source = FakeReleaseSource::latest("0.2.0");
        let store = FakeStateStore::with(UpdateState {
            skipped: Some("0.2.0".into()),
            ..cached("0.1.0", 10)
        });
        let status = check(
            &source,
            &store,
            InstallMethod::Standalone,
            CheckOptions::MANUAL,
        );
        assert_eq!(status, available("0.2.0"));
        assert_eq!(source.calls.get(), 1);
    }

    const FOOTER: &str = "## What's Changed\n* feat: thing by @a in https://github.com/o/r/pull/1\n\n**Full Changelog**: https://github.com/IgnacioToledoDev/cursorswap/compare/v{base}...v{latest}";

    fn source_with_notes(latest: &str, base: &str) -> FakeReleaseSource {
        let body = FOOTER.replace("{base}", base).replace("{latest}", latest);
        let mut source = FakeReleaseSource::latest(latest);
        let release = source.result.as_mut().unwrap();
        release.notes = Some(ReleaseNotes::sanitize(&body));
        release.url = Some(page(latest));
        source
    }

    #[test]
    fn fetched_notes_and_link_reach_the_offer_and_the_cache() {
        let source = source_with_notes("0.2.0", "0.1.0");
        let store = FakeStateStore::default();
        let UpdateStatus::Available { details, .. } = auto(&source, &store) else {
            panic!("expected an offer");
        };
        assert!(
            details
                .notes
                .as_ref()
                .unwrap()
                .as_str()
                .contains("feat: thing")
        );
        assert_eq!(details.link, page("0.2.0"));
        let saved = store.state.borrow().clone();
        assert_eq!(saved.notes, details.notes);
        assert_eq!(saved.release_url.as_deref(), Some(page("0.2.0").as_str()));
    }

    #[test]
    fn a_fresh_cache_serves_notes_without_a_request() {
        let source = FakeReleaseSource::failing();
        let notes = ReleaseNotes::sanitize("* cached change");
        let store = FakeStateStore::with(UpdateState {
            notes: Some(notes.clone()),
            release_url: Some(page("0.2.0")),
            ..cached("0.2.0", 60)
        });
        let UpdateStatus::Available { details, .. } = auto(&source, &store) else {
            panic!("expected an offer");
        };
        assert_eq!(details.notes, Some(notes));
        assert_eq!(details.link, page("0.2.0"));
        assert_eq!(source.calls.get(), 0);
    }

    #[test]
    fn an_old_cache_without_notes_shows_only_the_link() {
        let store = FakeStateStore::with(cached("0.2.0", 60));
        assert_eq!(
            auto(&FakeReleaseSource::failing(), &store),
            available("0.2.0")
        );
    }

    #[test]
    fn a_refresh_without_notes_drops_stale_cached_notes() {
        let store = FakeStateStore::with(UpdateState {
            notes: Some(ReleaseNotes::sanitize("* old")),
            release_url: Some(page("0.2.0")),
            ..cached("0.2.0", CACHE_TTL_SECS)
        });
        auto(&FakeReleaseSource::latest("0.3.0"), &store);
        let saved = store.state.borrow().clone();
        assert_eq!((saved.notes, saved.release_url), (None, None));
    }

    #[test]
    fn a_jump_over_versions_offers_the_compare_link() {
        let source = source_with_notes("0.3.0", "0.2.0");
        let UpdateStatus::Available { details, .. } = auto(&source, &FakeStateStore::default())
        else {
            panic!("expected an offer");
        };
        assert_eq!(
            details.link,
            "https://github.com/IgnacioToledoDev/cursorswap/compare/v0.1.0...v0.3.0"
        );
        assert!(details.notes.is_some());
    }

    #[test]
    fn managed_installs_carry_the_notes_too() {
        let status = check(
            &source_with_notes("0.2.0", "0.1.0"),
            &FakeStateStore::default(),
            InstallMethod::Cargo,
            CheckOptions::AUTO,
        );
        let UpdateStatus::Managed { details, .. } = status else {
            panic!("expected a notice");
        };
        assert!(details.notes.is_some());
    }

    #[test]
    fn apply_installs_the_newer_release() {
        let installer = FakeInstaller::default();
        let out = apply_update(&FakeReleaseSource::latest("0.2.0"), &installer, "0.1.0").unwrap();
        assert_eq!(
            out,
            UpdateOutcome::Updated {
                version: "0.2.0".into()
            }
        );
        assert_eq!(installer.installed.borrow().len(), 1);
        assert_eq!(installer.installed.borrow()[0].version, "0.2.0");
    }

    #[test]
    fn apply_does_nothing_when_already_current() {
        let installer = FakeInstaller::default();
        let out = apply_update(&FakeReleaseSource::latest("0.1.0"), &installer, "0.1.0").unwrap();
        assert_eq!(out, UpdateOutcome::AlreadyCurrent);
        assert!(installer.installed.borrow().is_empty());
    }

    #[test]
    fn apply_propagates_source_and_installer_errors() {
        let installer = FakeInstaller::default();
        assert!(apply_update(&FakeReleaseSource::failing(), &installer, "0.1.0").is_err());
        let failing = FakeInstaller {
            fail: true,
            ..FakeInstaller::default()
        };
        let err = apply_update(&FakeReleaseSource::latest("0.2.0"), &failing, "0.1.0").unwrap_err();
        assert!(matches!(err, UpdateError::ChecksumMismatch { .. }));
    }

    #[test]
    fn skip_records_the_version_and_keeps_the_rest() {
        let store = FakeStateStore::with(cached("0.2.0", 5));
        skip_version(&store, "0.2.0").unwrap();
        let state = store.state.borrow().clone();
        assert_eq!(state.skipped.as_deref(), Some("0.2.0"));
        assert_eq!(state.latest.as_deref(), Some("0.2.0"));
    }

    #[test]
    fn skip_reports_a_failed_save() {
        let store = FakeStateStore {
            fail_save: true,
            ..FakeStateStore::default()
        };
        assert!(skip_version(&store, "0.2.0").is_err());
    }
}
