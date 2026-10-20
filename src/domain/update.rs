//! Self-update rules: version comparison, cache freshness, install-method
//! classification, opt-out policy and prompt answers. Pure and I/O free.

use std::path::Path;

use semver::Version;

pub const REPOSITORY: &str = "IgnacioToledoDev/cursorswap";
pub const BINARY_ASSET: &str = "cursorswap-x86_64-unknown-linux-gnu";
pub const CHECKSUM_ASSET: &str = "cursorswap-x86_64-unknown-linux-gnu.sha256";
pub const NO_UPDATE_ENV: &str = "CURSORSWAP_NO_UPDATE_CHECK";
pub const CACHE_TTL_SECS: u64 = 6 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub version: String,
    pub binary_url: String,
    pub checksum_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallMethod {
    Standalone,
    PackageManager,
    Cargo,
}

/// What the update check remembers between runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdateState {
    pub checked_at: Option<u64>,
    pub latest: Option<String>,
    pub skipped: Option<String>,
}

/// The user's answer to the update prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Yes,
    No,
    Skip,
}

/// Facts that decide whether the automatic check may run at all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CheckEnv {
    pub flag_opt_out: bool,
    pub env_opt_out: bool,
    pub config_opt_out: bool,
    pub ci: bool,
    pub interactive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disabled {
    NotInteractive,
    Ci,
    Flag,
    Env,
    Config,
}

/// Parses a plain `X.Y.Z` (optionally `vX.Y.Z`) version.
pub fn parse_version(text: &str) -> Option<Version> {
    Version::parse(text.trim().trim_start_matches('v')).ok()
}

/// True when `candidate` is a stable release strictly newer than `installed`.
pub fn is_newer(installed: &str, candidate: &str) -> bool {
    let (Some(installed), Some(candidate)) = (parse_version(installed), parse_version(candidate))
    else {
        return false;
    };
    candidate.pre.is_empty() && candidate > installed
}

pub fn classify_install_path(path: &Path) -> InstallMethod {
    let text = path.to_string_lossy();
    if path.starts_with("/usr") {
        InstallMethod::PackageManager
    } else if text.contains("/.cargo/bin/")
        || text.contains("/target/debug/")
        || text.contains("/target/release/")
    {
        InstallMethod::Cargo
    } else {
        InstallMethod::Standalone
    }
}

pub fn update_hint(method: InstallMethod) -> &'static str {
    match method {
        InstallMethod::Standalone => "Run `cursorswap update` to install it.",
        InstallMethod::PackageManager => {
            "Update it with your package manager (for example: yay -Syu cursorswap)."
        }
        InstallMethod::Cargo => concat!(
            "Update it with: cargo install --git https://github.com/IgnacioToledoDev/cursorswap",
            " --locked --force"
        ),
    }
}

/// A cache entry is fresh for six hours; a timestamp from the future is stale.
pub fn cache_is_fresh(state: &UpdateState, now: u64) -> bool {
    match (state.checked_at, &state.latest) {
        (Some(at), Some(_)) => now >= at && now - at < CACHE_TTL_SECS,
        _ => false,
    }
}

/// Reads an opt-out environment value: set, non-empty and not `0`/`false`.
pub fn env_flag(value: Option<&str>) -> bool {
    value.is_some_and(|v| {
        let v = v.trim();
        !v.is_empty() && v != "0" && !v.eq_ignore_ascii_case("false")
    })
}

pub fn disabled_reason(env: &CheckEnv) -> Option<Disabled> {
    if !env.interactive {
        Some(Disabled::NotInteractive)
    } else if env.ci {
        Some(Disabled::Ci)
    } else if env.flag_opt_out {
        Some(Disabled::Flag)
    } else if env.env_opt_out {
        Some(Disabled::Env)
    } else if env.config_opt_out {
        Some(Disabled::Config)
    } else {
        None
    }
}

/// Anything that is not an explicit yes or skip declines, including EOF.
pub fn parse_answer(line: &str) -> Answer {
    match line.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => Answer::Yes,
        "s" | "skip" => Answer::Skip,
        _ => Answer::No,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn newer_equal_older() {
        assert!(is_newer("0.1.0", "0.2.0"));
        assert!(is_newer("0.1.9", "0.1.10"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.3.0", "0.2.9"));
    }

    #[test]
    fn leading_v_is_accepted() {
        assert!(is_newer("0.1.0", "v0.2.0"));
    }

    #[test]
    fn prerelease_candidates_are_ignored() {
        assert!(!is_newer("0.1.0", "0.2.0-rc.1"));
        assert!(!is_newer("0.1.0", "1.0.0-beta"));
    }

    #[test]
    fn unparsable_versions_are_never_newer() {
        assert!(!is_newer("0.1.0", "latest"));
        assert!(!is_newer("garbage", "0.2.0"));
        assert!(!is_newer("0.1.0", ""));
    }

    #[test]
    fn installed_prerelease_is_older_than_its_stable() {
        assert!(is_newer("0.2.0-dev", "0.2.0"));
    }

    fn state(checked_at: Option<u64>) -> UpdateState {
        UpdateState {
            checked_at,
            latest: Some("0.2.0".into()),
            skipped: None,
        }
    }

    #[test]
    fn cache_fresh_within_six_hours() {
        assert!(cache_is_fresh(&state(Some(1_000)), 1_000));
        assert!(cache_is_fresh(
            &state(Some(1_000)),
            1_000 + CACHE_TTL_SECS - 1
        ));
    }

    #[test]
    fn cache_stale_after_six_hours_missing_or_future() {
        assert!(!cache_is_fresh(&state(Some(1_000)), 1_000 + CACHE_TTL_SECS));
        assert!(!cache_is_fresh(&state(None), 5));
        assert!(!cache_is_fresh(&state(Some(2_000)), 1_000));
    }

    #[test]
    fn cache_without_a_version_is_stale() {
        let empty = UpdateState {
            checked_at: Some(10),
            latest: None,
            skipped: None,
        };
        assert!(!cache_is_fresh(&empty, 11));
    }

    #[test]
    fn install_paths_are_classified() {
        let cases = [
            ("/usr/bin/cursorswap", InstallMethod::PackageManager),
            ("/usr/local/bin/cursorswap", InstallMethod::PackageManager),
            ("/home/u/.cargo/bin/cursorswap", InstallMethod::Cargo),
            (
                "/home/u/src/cs/target/debug/cursorswap",
                InstallMethod::Cargo,
            ),
            (
                "/home/u/src/cs/target/release/cursorswap",
                InstallMethod::Cargo,
            ),
            ("/home/u/.local/bin/cursorswap", InstallMethod::Standalone),
            ("/opt/tools/cursorswap", InstallMethod::Standalone),
            ("/home/u/usr/bin/cursorswap", InstallMethod::Standalone),
        ];
        for (path, expected) in cases {
            assert_eq!(
                classify_install_path(&PathBuf::from(path)),
                expected,
                "{path}"
            );
        }
    }

    #[test]
    fn hints_name_the_right_update_path() {
        assert!(update_hint(InstallMethod::PackageManager).contains("package manager"));
        assert!(update_hint(InstallMethod::Cargo).contains("cargo install"));
        assert!(update_hint(InstallMethod::Cargo).contains(REPOSITORY));
    }

    #[test]
    fn env_flag_values() {
        assert!(env_flag(Some("1")));
        assert!(env_flag(Some("true")));
        assert!(!env_flag(Some("")));
        assert!(!env_flag(Some("0")));
        assert!(!env_flag(Some("false")));
        assert!(!env_flag(None));
    }

    fn allowed() -> CheckEnv {
        CheckEnv {
            interactive: true,
            ..CheckEnv::default()
        }
    }

    #[test]
    fn interactive_default_is_enabled() {
        assert_eq!(disabled_reason(&allowed()), None);
    }

    #[test]
    fn every_opt_out_source_disables_the_check() {
        let cases = [
            (
                CheckEnv {
                    interactive: false,
                    ..allowed()
                },
                Disabled::NotInteractive,
            ),
            (
                CheckEnv {
                    ci: true,
                    ..allowed()
                },
                Disabled::Ci,
            ),
            (
                CheckEnv {
                    flag_opt_out: true,
                    ..allowed()
                },
                Disabled::Flag,
            ),
            (
                CheckEnv {
                    env_opt_out: true,
                    ..allowed()
                },
                Disabled::Env,
            ),
            (
                CheckEnv {
                    config_opt_out: true,
                    ..allowed()
                },
                Disabled::Config,
            ),
        ];
        for (env, reason) in cases {
            assert_eq!(disabled_reason(&env), Some(reason));
        }
    }

    #[test]
    fn answers() {
        for yes in ["y", "Y", "yes", " YES \n"] {
            assert_eq!(parse_answer(yes), Answer::Yes, "{yes:?}");
        }
        for no in ["n", "no", "N\n"] {
            assert_eq!(parse_answer(no), Answer::No, "{no:?}");
        }
        for skip in ["s", "skip", "S\n"] {
            assert_eq!(parse_answer(skip), Answer::Skip, "{skip:?}");
        }
        for other in ["", "\n", "maybe", "yy", "ye"] {
            assert_eq!(parse_answer(other), Answer::No, "{other:?}");
        }
    }
}
