//! Latest stable release lookup against the GitHub releases API.

use serde::Deserialize;

use crate::domain::error::UpdateError;
use crate::domain::ports::ReleaseSource;
use crate::domain::update::{BINARY_ASSET, CHECKSUM_ASSET, REPOSITORY, ReleaseInfo, parse_version};

use super::http::HttpGet;

const API_LIMIT_BYTES: u64 = 1024 * 1024;

#[derive(Deserialize)]
struct ReleaseDto {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<AssetDto>,
}

#[derive(Deserialize)]
struct AssetDto {
    name: String,
    browser_download_url: String,
}

/// Parses one `releases/latest` response into a stable, installable release.
pub fn parse_release(json: &str) -> Result<ReleaseInfo, UpdateError> {
    let dto: ReleaseDto =
        serde_json::from_str(json).map_err(|e| UpdateError::Parse(e.to_string()))?;
    if dto.draft || dto.prerelease {
        return Err(UpdateError::Parse("release is not a stable release".into()));
    }
    let version = parse_version(&dto.tag_name)
        .filter(|v| v.pre.is_empty())
        .ok_or_else(|| UpdateError::Parse(format!("unsupported tag {:?}", dto.tag_name)))?;
    let find = |wanted: &str| {
        dto.assets
            .iter()
            .find(|a| a.name == wanted)
            .map(|a| a.browser_download_url.clone())
            .ok_or(UpdateError::MissingAssets)
    };
    let binary_url = find(BINARY_ASSET)?;
    let checksum_url = find(CHECKSUM_ASSET)?;
    for url in [&binary_url, &checksum_url] {
        if !url.starts_with("https://") {
            return Err(UpdateError::Parse(format!(
                "refusing non-https url {url:?}"
            )));
        }
    }
    Ok(ReleaseInfo {
        version: version.to_string(),
        binary_url,
        checksum_url,
    })
}

pub struct GithubReleaseSource<H: HttpGet> {
    http: H,
}

impl<H: HttpGet> GithubReleaseSource<H> {
    pub fn new(http: H) -> Self {
        GithubReleaseSource { http }
    }
}

impl<H: HttpGet> ReleaseSource for GithubReleaseSource<H> {
    fn latest_stable(&self) -> Result<ReleaseInfo, UpdateError> {
        let url = format!("https://api.github.com/repos/{REPOSITORY}/releases/latest");
        let body = self.http.get(&url, API_LIMIT_BYTES)?;
        let text = String::from_utf8(body).map_err(|e| UpdateError::Parse(e.to_string()))?;
        parse_release(&text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::http::fake::FakeHttp;

    const BIN: &str =
        "https://github.com/o/r/releases/download/v0.2.0/cursorswap-x86_64-unknown-linux-gnu";

    fn fixture(tag: &str, draft: bool, prerelease: bool, assets: &[(&str, &str)]) -> String {
        let assets: Vec<String> = assets
            .iter()
            .map(|(n, u)| format!(r#"{{"name":"{n}","browser_download_url":"{u}","size":1}}"#))
            .collect();
        format!(
            r#"{{"id":1,"tag_name":"{tag}","draft":{draft},"prerelease":{prerelease},"assets":[{}]}}"#,
            assets.join(",")
        )
    }

    fn both_assets() -> Vec<(&'static str, &'static str)> {
        let sum: &'static str = Box::leak(format!("{BIN}.sha256").into_boxed_str());
        vec![
            ("cursorswap-x86_64-unknown-linux-gnu", BIN),
            ("cursorswap-x86_64-unknown-linux-gnu.sha256", sum),
            ("unrelated.txt", "https://example.com/unrelated.txt"),
        ]
    }

    #[test]
    fn stable_release_is_parsed() {
        let release = parse_release(&fixture("v0.2.0", false, false, &both_assets())).unwrap();
        assert_eq!(release.version, "0.2.0");
        assert_eq!(release.binary_url, BIN);
        assert_eq!(release.checksum_url, format!("{BIN}.sha256"));
    }

    #[test]
    fn draft_and_prerelease_are_rejected() {
        for (draft, pre) in [(true, false), (false, true)] {
            let json = fixture("v0.2.0", draft, pre, &both_assets());
            assert!(matches!(parse_release(&json), Err(UpdateError::Parse(_))));
        }
    }

    #[test]
    fn semver_prerelease_tag_is_rejected_even_when_not_flagged() {
        let json = fixture("v0.3.0-rc.1", false, false, &both_assets());
        assert!(matches!(parse_release(&json), Err(UpdateError::Parse(_))));
    }

    #[test]
    fn malformed_json_and_bad_tags_are_parse_errors() {
        for json in ["", "not json", "{}", r#"{"tag_name":5}"#] {
            assert!(
                matches!(parse_release(json), Err(UpdateError::Parse(_))),
                "{json}"
            );
        }
        let json = fixture("nightly", false, false, &both_assets());
        assert!(matches!(parse_release(&json), Err(UpdateError::Parse(_))));
    }

    #[test]
    fn missing_assets_are_reported() {
        let assets = both_assets();
        for keep in [&assets[..1], &assets[1..2], &assets[2..], &[][..]] {
            let json = fixture("v0.2.0", false, false, keep);
            assert_eq!(parse_release(&json), Err(UpdateError::MissingAssets));
        }
    }

    #[test]
    fn non_https_download_urls_are_rejected() {
        let assets = [
            (
                "cursorswap-x86_64-unknown-linux-gnu",
                "http://evil.example/bin",
            ),
            (
                "cursorswap-x86_64-unknown-linux-gnu.sha256",
                "https://example.com/sum",
            ),
        ];
        let json = fixture("v0.2.0", false, false, &assets);
        assert!(matches!(parse_release(&json), Err(UpdateError::Parse(_))));
    }

    #[test]
    fn source_fetches_the_latest_release_endpoint() {
        let url = "https://api.github.com/repos/IgnacioToledoDev/cursorswap/releases/latest";
        let http = FakeHttp::default().with(
            url,
            fixture("v0.2.0", false, false, &both_assets()).as_bytes(),
        );
        let source = GithubReleaseSource::new(http);
        assert_eq!(source.latest_stable().unwrap().version, "0.2.0");
        let requests = source.http.requests.borrow();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].1, API_LIMIT_BYTES);
    }

    #[test]
    fn source_surfaces_network_and_encoding_failures() {
        let url = "https://api.github.com/repos/IgnacioToledoDev/cursorswap/releases/latest";
        let offline = GithubReleaseSource::new(FakeHttp::default().failing(url));
        assert!(matches!(
            offline.latest_stable(),
            Err(UpdateError::Network(_))
        ));
        let binary = GithubReleaseSource::new(FakeHttp::default().with(url, &[0xff, 0xfe]));
        assert!(matches!(binary.latest_stable(), Err(UpdateError::Parse(_))));
    }
}
