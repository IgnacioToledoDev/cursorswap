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
    pub notes: Option<ReleaseNotes>,
    /// The release page, already checked by `release_page_url`.
    pub url: Option<String>,
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
    pub notes: Option<ReleaseNotes>,
    pub release_url: Option<String>,
}

/// What an update offer shows besides the version numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseDetails {
    pub notes: Option<ReleaseNotes>,
    /// Release page, or the compare page when versions were skipped.
    pub link: String,
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

const NOTES_MAX_LINES: usize = 20;
const NOTES_MAX_BYTES: usize = 2048;
const TRUNCATION_MARKER: &str = "... (truncated)";

/// Release notes that are safe to print on a terminal.
///
/// A release body is remote text, so this type can only be built through
/// `sanitize`: escape sequences and control characters never reach a screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseNotes(String);

impl ReleaseNotes {
    /// Strips terminal escapes and control characters, flattens light
    /// markdown and caps the result at 20 lines and about 2 KiB.
    pub fn sanitize(raw: &str) -> ReleaseNotes {
        ReleaseNotes(cap(tidy(&strip_controls(raw))))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Up to `max` content lines (no headings or changelog footer) and
    /// whether more were left out.
    pub fn summary(&self, max: usize) -> (Vec<&str>, bool) {
        let mut content = self.0.lines().filter(|l| {
            !l.trim().is_empty() && !l.ends_with(':') && !l.starts_with("Full Changelog")
        });
        let lines: Vec<&str> = content.by_ref().take(max).collect();
        (lines, content.next().is_some())
    }
}

/// Removes escape sequences, C0 controls except newline, DEL, C1 and bidi
/// overrides; normalizes line endings.
fn strip_controls(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => skip_escape(&mut chars),
            '\u{9b}' => skip_csi(&mut chars),
            '\u{90}' | '\u{98}' | '\u{9d}' | '\u{9e}' | '\u{9f}' => skip_string(&mut chars),
            '\r' => {
                chars.next_if_eq(&'\n');
                out.push('\n');
            }
            '\n' => out.push('\n'),
            '\t' => out.push(' '),
            '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' => {}
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

type Chars<'a> = std::iter::Peekable<std::str::Chars<'a>>;

/// Called after an ESC: consumes the rest of the sequence, if any.
fn skip_escape(chars: &mut Chars) {
    match chars.peek().copied() {
        Some(']' | 'P' | 'X' | '^' | '_') => {
            chars.next();
            skip_string(chars);
        }
        Some('[') => {
            chars.next();
            skip_csi(chars);
        }
        Some(' '..='/') => {
            while chars.next_if(|c| (' '..='/').contains(c)).is_some() {}
            chars.next_if(|c| ('0'..='~').contains(c));
        }
        Some('0'..='~') => {
            chars.next();
        }
        // A lone ESC: drop it and let the next character be handled normally.
        _ => {}
    }
}

fn skip_csi(chars: &mut Chars) {
    while chars.next_if(|c| (' '..='?').contains(c)).is_some() {}
    chars.next_if(|c| ('@'..='~').contains(c));
}

/// Consumes an OSC/DCS/PM/APC/SOS string up to BEL, `ESC \` or the C1 ST.
/// An unterminated string swallows the rest of the text.
fn skip_string(chars: &mut Chars) {
    while let Some(c) = chars.next() {
        match c {
            '\u{7}' | '\u{9c}' => return,
            '\u{1b}' => {
                // `ESC \` is the terminator; any other ESC starts a new sequence.
                if chars.next_if_eq(&'\\').is_none() {
                    skip_escape(chars);
                }
                return;
            }
            _ => {}
        }
    }
}

/// Light markdown handling: headings become `Title:`, `* ` and `- ` become
/// bullets, `**` is removed, HTML comments and surplus blank lines go.
fn tidy(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        let trimmed = line.trim_start();
        if trimmed.starts_with("<!--") && trimmed.ends_with("-->") {
            continue;
        }
        let indent = &line[..line.len() - trimmed.len()];
        let line = if trimmed.starts_with('#') {
            match trimmed.trim_start_matches('#').trim() {
                "" => String::new(),
                title => format!("{title}:"),
            }
        } else if let Some(item) = trimmed
            .strip_prefix("* ")
            .or_else(|| trimmed.strip_prefix("- "))
        {
            format!("{indent}\u{2022} {item}")
        } else {
            line.to_string()
        }
        .replace("**", "");
        let blank = line.trim().is_empty();
        if blank && lines.last().is_none_or(|last| last.is_empty()) {
            continue;
        }
        lines.push(if blank { String::new() } else { line });
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines
}

/// Joins the lines, truncating with a marker past the line or size cap.
fn cap(lines: Vec<String>) -> String {
    let total: usize = lines.iter().map(|l| l.len() + 1).sum();
    if lines.len() <= NOTES_MAX_LINES && total.saturating_sub(1) <= NOTES_MAX_BYTES {
        return lines.join("\n");
    }
    let budget = NOTES_MAX_BYTES - TRUNCATION_MARKER.len() - 1;
    let mut kept: Vec<String> = Vec::new();
    let mut used = 0;
    for line in lines.into_iter().take(NOTES_MAX_LINES - 1) {
        if used + line.len() + 1 > budget {
            if kept.is_empty() {
                let end = (0..=budget.min(line.len()))
                    .rev()
                    .find(|&i| line.is_char_boundary(i))
                    .unwrap_or(0);
                kept.push(line[..end].to_string());
            }
            break;
        }
        used += line.len() + 1;
        kept.push(line);
    }
    kept.push(TRUNCATION_MARKER.to_string());
    kept.join("\n")
}

/// Accepts a release page only on github.com over https, as plain ASCII
/// (so it can neither carry escapes nor spoof another host).
pub fn release_page_url(url: &str) -> Option<String> {
    (url.starts_with("https://github.com/") && url.chars().all(|c| c.is_ascii_graphic()))
        .then(|| url.to_string())
}

/// The GitHub compare page for an update that spans several releases.
pub fn compare_url(installed: &str, latest: &str) -> String {
    format!("https://github.com/{REPOSITORY}/compare/v{installed}...v{latest}")
}

/// Where to read about `latest`: the release page, or the compare page when
/// the notes' own changelog footer shows the release was not the next one
/// after `installed`.
pub fn changelog_link(
    installed: &str,
    latest: &str,
    notes: Option<&ReleaseNotes>,
    release_url: Option<&str>,
) -> String {
    let skipped = parse_version(installed).is_some_and(|v| v.pre.is_empty())
        && notes
            .and_then(|n| changelog_base(n.as_str(), latest))
            .is_some_and(|base| parse_version(base) != parse_version(installed));
    if skipped {
        return compare_url(installed, latest);
    }
    release_url
        .map(str::to_string)
        .unwrap_or_else(|| format!("https://github.com/{REPOSITORY}/releases/tag/v{latest}"))
}

/// The previous tag named by a `.../compare/vA...vLATEST` link in the notes.
fn changelog_base<'a>(notes: &'a str, latest: &str) -> Option<&'a str> {
    let rest = notes.split_once("/compare/v")?.1;
    let (base, tail) = rest.split_once("...v")?;
    let base_ok = !base.is_empty()
        && base
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    (base_ok && tail.starts_with(latest)).then_some(base)
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
            ..UpdateState::default()
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
            ..UpdateState::default()
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

    // --- release notes ---

    fn notes(raw: &str) -> String {
        ReleaseNotes::sanitize(raw).as_str().to_string()
    }

    fn assert_clean(text: &str) {
        for c in text.chars() {
            assert!(
                c == '\n' || !c.is_control(),
                "control char {c:?} survived in {text:?}"
            );
        }
        assert!(!text.contains('\u{1b}'));
    }

    const GENERATED_BODY: &str = "<!-- Release notes generated using configuration in .github/release.yml at main -->\n\n## What's Changed\n### Bug fixes\n* ci: attach assets when the release already exists by @IgnacioToledoDev in https://github.com/IgnacioToledoDev/cursorswap/pull/19\n\n\n**Full Changelog**: https://github.com/IgnacioToledoDev/cursorswap/compare/v0.3.1...v0.3.2";

    #[test]
    fn real_generated_body_becomes_plain_readable_lines() {
        assert_eq!(
            notes(GENERATED_BODY),
            "What's Changed:\nBug fixes:\n\u{2022} ci: attach assets when the release already exists by @IgnacioToledoDev in https://github.com/IgnacioToledoDev/cursorswap/pull/19\n\nFull Changelog: https://github.com/IgnacioToledoDev/cursorswap/compare/v0.3.1...v0.3.2"
        );
    }

    #[test]
    fn csi_sequences_are_stripped() {
        assert_eq!(notes("a\u{1b}[2Jb\u{1b}[1;31mc\u{1b}[0m"), "abc");
        assert_eq!(notes("x\u{9b}2Jy"), "xy");
    }

    #[test]
    fn osc_strings_are_stripped_for_every_terminator() {
        // OSC 8 hyperlink ended by BEL and by ESC \.
        assert_eq!(
            notes("\u{1b}]8;;https://evil.example\u{7}click\u{1b}]8;;\u{7}"),
            "click"
        );
        assert_eq!(
            notes("\u{1b}]8;;https://evil.example\u{1b}\\click\u{1b}]8;;\u{1b}\\"),
            "click"
        );
        // OSC 52 clipboard write and a window title.
        assert_eq!(notes("a\u{1b}]52;c;ZXZpbA==\u{7}b"), "ab");
        assert_eq!(notes("a\u{1b}]0;title\u{7}b"), "ab");
        // C1 introducer and C1 string terminator.
        assert_eq!(notes("a\u{9d}52;c;ZXZpbA==\u{9c}b"), "ab");
    }

    #[test]
    fn dcs_pm_apc_and_sos_strings_are_stripped() {
        for intro in ["\u{1b}P", "\u{1b}^", "\u{1b}_", "\u{1b}X"] {
            let raw = format!("a{intro}payload\u{1b}\\b");
            assert_eq!(notes(&raw), "ab", "{intro:?}");
        }
        for intro in ["\u{90}", "\u{9e}", "\u{9f}", "\u{98}"] {
            let raw = format!("a{intro}payload\u{9c}b");
            assert_eq!(notes(&raw), "ab", "{intro:?}");
        }
    }

    #[test]
    fn unterminated_strings_swallow_the_rest_instead_of_leaking() {
        assert_eq!(notes("ok\u{1b}]52;c;ZXZpbA=="), "ok");
        assert_eq!(notes("ok\u{1b}Ppayload"), "ok");
    }

    #[test]
    fn a_new_escape_inside_a_string_ends_it() {
        assert_eq!(notes("a\u{1b}]0;t\u{1b}[2Jb"), "ab");
    }

    #[test]
    fn lone_escape_and_short_escapes_are_dropped() {
        assert_eq!(notes("a\u{1b}"), "a");
        assert_eq!(notes("a\u{1b}\nb"), "a\nb");
        assert_eq!(notes("a\u{1b}cb"), "ab");
        assert_eq!(notes("a\u{1b}#8b"), "ab");
        assert_eq!(notes("a\u{1b}\u{1b}\nb"), "a\nb");
    }

    #[test]
    fn other_control_characters_are_dropped() {
        assert_eq!(notes("a\0b\u{7}c\u{8}d\u{7f}e\u{85}f\u{80}g"), "abcdefg");
        assert_eq!(notes("a\tb"), "a b");
    }

    #[test]
    fn bidi_overrides_are_dropped() {
        assert_eq!(notes("a\u{202e}b\u{2066}c"), "abc");
    }

    #[test]
    fn line_endings_are_normalized() {
        assert_eq!(notes("a\r\nb\rc\nd"), "a\nb\nc\nd");
    }

    #[test]
    fn markdown_is_handled_lightly() {
        assert_eq!(notes("## Heading"), "Heading:");
        assert_eq!(notes("# One\n#### Four"), "One:\nFour:");
        assert_eq!(
            notes("* item\n- other\n  * nested"),
            "\u{2022} item\n\u{2022} other\n  \u{2022} nested"
        );
        assert_eq!(notes("**bold** and **more**"), "bold and more");
        assert_eq!(notes("<!-- hidden -->\ntext"), "text");
    }

    #[test]
    fn blank_runs_and_edges_are_collapsed() {
        assert_eq!(notes("\n\n a \n\n\n\n b \n\n"), " a\n\n b");
    }

    #[test]
    fn urls_stay_plain_text() {
        assert_eq!(
            notes("see https://github.com/o/r/pull/1 now"),
            "see https://github.com/o/r/pull/1 now"
        );
    }

    #[test]
    fn empty_and_escape_only_bodies_sanitize_to_nothing() {
        assert!(ReleaseNotes::sanitize("").is_empty());
        assert!(ReleaseNotes::sanitize("\n \u{1b}[2J\n").is_empty());
    }

    #[test]
    fn long_bodies_are_capped_by_lines_with_a_marker() {
        let raw: String = (1..=50).map(|i| format!("line {i}\n")).collect();
        let text = notes(&raw);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 20);
        assert_eq!(lines[18], "line 19");
        assert_eq!(lines[19], TRUNCATION_MARKER);
    }

    #[test]
    fn exactly_twenty_lines_are_not_truncated() {
        let raw: String = (1..=20).map(|i| format!("line {i}\n")).collect();
        let text = notes(&raw);
        assert_eq!(text.lines().count(), 20);
        assert!(!text.contains(TRUNCATION_MARKER));
    }

    #[test]
    fn a_ten_kilobyte_body_is_capped_by_size() {
        let text = notes(&"x".repeat(10 * 1024));
        assert!(text.len() <= 2048, "{}", text.len());
        assert!(text.ends_with(TRUNCATION_MARKER));
        let wide: String = (0..400)
            .map(|i| format!("entry number {i} is here\n"))
            .collect();
        let text = notes(&wide);
        assert!(text.len() <= 2048 && text.lines().count() <= 20);
        assert!(text.ends_with(TRUNCATION_MARKER));
    }

    #[test]
    fn truncation_respects_char_boundaries() {
        let text = notes(&"\u{e9}".repeat(10 * 1024));
        assert!(text.len() <= 2048);
        assert!(text.ends_with(TRUNCATION_MARKER));
    }

    #[test]
    fn sanitizing_is_idempotent() {
        let raw: String = (1..=50).map(|i| format!("* line **{i}**\n")).collect();
        for input in [GENERATED_BODY, raw.as_str(), "a\u{1b}[2Jb"] {
            let once = ReleaseNotes::sanitize(input);
            assert_eq!(ReleaseNotes::sanitize(once.as_str()), once);
        }
    }

    #[test]
    fn hostile_body_leaks_no_control_bytes() {
        let hostile = "\u{1b}[2J\u{1b}]8;;http://x\u{7}t\u{1b}]52;c;AAAA\u{1b}\\\0\r\u{9b}31m\u{9d}0;x\u{9c}\u{1b}";
        assert_clean(&notes(hostile));
    }

    #[test]
    fn summary_skips_headings_and_the_changelog_line() {
        let n = ReleaseNotes::sanitize(GENERATED_BODY);
        let (lines, truncated) = n.summary(4);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("\u{2022} ci: attach assets"));
        assert!(!truncated);
    }

    #[test]
    fn summary_reports_when_more_lines_exist() {
        let raw: String = (1..=9).map(|i| format!("* change {i}\n")).collect();
        let n = ReleaseNotes::sanitize(&raw);
        let (lines, truncated) = n.summary(4);
        assert_eq!(lines.len(), 4);
        assert!(truncated);
        assert!(!n.summary(9).1);
    }

    // --- links ---

    #[test]
    fn release_urls_must_be_github_https_and_plain_ascii() {
        let ok = "https://github.com/o/r/releases/tag/v1.0.0";
        assert_eq!(release_page_url(ok).as_deref(), Some(ok));
        for bad in [
            "http://github.com/o/r",
            "https://evil.example/o/r",
            "https://github.com.evil.example/o",
            "https://github.com/o r",
            "https://github.com/o\u{1b}[2J",
            "https://github.com/\u{e9}",
            "",
        ] {
            assert_eq!(release_page_url(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn compare_link_spans_installed_to_latest() {
        assert_eq!(
            compare_url("0.1.0", "0.3.0"),
            "https://github.com/IgnacioToledoDev/cursorswap/compare/v0.1.0...v0.3.0"
        );
    }

    fn link(installed: &str, latest: &str, body: Option<&str>, url: Option<&str>) -> String {
        let n = body.map(ReleaseNotes::sanitize);
        changelog_link(installed, latest, n.as_ref(), url)
    }

    const PAGE: &str = "https://github.com/IgnacioToledoDev/cursorswap/releases/tag/v0.3.2";

    #[test]
    fn direct_upgrade_links_the_release_page() {
        assert_eq!(
            link("0.3.1", "0.3.2", Some(GENERATED_BODY), Some(PAGE)),
            PAGE
        );
    }

    #[test]
    fn skipped_versions_link_the_compare_page() {
        assert_eq!(
            link("0.1.0", "0.3.2", Some(GENERATED_BODY), Some(PAGE)),
            "https://github.com/IgnacioToledoDev/cursorswap/compare/v0.1.0...v0.3.2"
        );
    }

    #[test]
    fn without_notes_or_url_the_tag_page_is_derived() {
        assert_eq!(link("0.3.1", "0.3.2", None, None), PAGE);
        assert_eq!(link("0.1.0", "0.3.2", None, Some(PAGE)), PAGE);
    }

    #[test]
    fn prerelease_installs_never_get_a_compare_link() {
        assert_eq!(link("0.1.0-dev", "0.3.2", Some(GENERATED_BODY), None), PAGE);
    }
}
