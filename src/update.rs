//! Opt-in check for a newer AeroClean on GitHub Releases. Off by default.
//! At most once a day it asks GitHub for the latest release number; nothing
//! about the user or the computer is sent. Uses curl (or wget) so the app
//! carries no networking library.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const REPO: &str = env!("CARGO_PKG_REPOSITORY");
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq)]
pub struct Update {
    /// "1.3.0"
    pub version: String,
    /// The release page.
    pub url: String,
}

fn stamp() -> PathBuf {
    crate::platform::data_dir().join("update-check")
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Asks GitHub (or reuses today's answer) and returns the newer release, if any.
pub fn check(force: bool) -> Option<Update> {
    let cached = std::fs::read_to_string(stamp()).ok().and_then(|t| {
        let (when, tag) = t.trim().split_once('\t')?;
        Some((when.parse::<u64>().ok()?, tag.to_string()))
    });
    let tag = match cached {
        Some((when, tag)) if !force && now().saturating_sub(when) < Duration::from_secs(86400).as_secs() => tag,
        _ => {
            let tag = fetch_latest_tag()?;
            let _ = std::fs::create_dir_all(crate::platform::data_dir());
            let _ = std::fs::write(stamp(), format!("{}\t{tag}\n", now()));
            tag
        }
    };
    newer(&tag, CURRENT).then(|| Update { version: tag.trim_start_matches('v').to_string(), url: format!("{REPO}/releases/tag/{tag}") })
}

fn fetch_latest_tag() -> Option<String> {
    let api = format!("{}/releases/latest", REPO.replacen("https://github.com/", "https://api.github.com/repos/", 1));
    let agent = format!("AeroClean/{CURRENT}");
    let attempts: [(&str, Vec<&str>); 2] = [
        ("curl", vec!["-fsSL", "--max-time", "15", "-A", &agent, "-H", "Accept: application/vnd.github+json", &api]),
        ("wget", vec!["-q", "-T", "15", "-U", &agent, "-O", "-", &api]),
    ];
    attempts.into_iter().find_map(|(tool, args)| {
        let out = crate::platform::command(crate::platform::find_tool(tool)?).args(args).stderr(Stdio::null()).output().ok()?;
        if !out.status.success() {
            return None;
        }
        parse_tag(&String::from_utf8_lossy(&out.stdout))
    })
}

/// The `"tag_name": "v1.2.0"` value of a release.
fn parse_tag(json: &str) -> Option<String> {
    let rest = &json[json.find("\"tag_name\"")? + 10..];
    let rest = &rest[rest.find('"')? + 1..];
    let tag = &rest[..rest.find('"')?];
    // Only plain version tags, nothing that could carry surprises into a URL.
    (!tag.is_empty() && tag.chars().all(|c| c.is_ascii_alphanumeric() || ".-".contains(c))).then(|| tag.to_string())
}

fn version(v: &str) -> Vec<u64> {
    v.trim_start_matches('v').split(['.', '-']).map_while(|p| p.parse().ok()).collect()
}

/// Is `latest` a higher version than `current`? ("v1.10.0" > "1.9.2")
fn newer(latest: &str, current: &str) -> bool {
    let (l, c) = (version(latest), version(current));
    !l.is_empty() && l > c
}

/// Opens the release page in the default browser.
pub fn open(url: &str) {
    if !url.starts_with("https://") {
        return;
    }
    let tool = if crate::platform::IS_WINDOWS {
        "explorer"
    } else if crate::platform::IS_MACOS {
        "open"
    } else {
        "xdg-open"
    };
    let _ = crate::platform::command(tool).arg(url).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_numerically() {
        assert!(newer("v1.10.0", "1.9.2"));
        assert!(newer("v1.2.1", "1.2.0"));
        assert!(!newer("v1.2.0", "1.2.0"));
        assert!(!newer("v1.1.9", "1.2.0"));
        assert!(!newer("nightly", "1.2.0"));
    }

    #[test]
    fn reads_the_tag_safely() {
        let json = r#"{"url": "https://api.github.com/x", "tag_name": "v1.3.0", "name": "AeroClean v1.3.0"}"#;
        assert_eq!(parse_tag(json).as_deref(), Some("v1.3.0"));
        assert_eq!(parse_tag(r#"{"tag_name": "v1\"; rm"}"#).as_deref(), None);
        assert_eq!(parse_tag(r#"{"tag_name": "../evil"}"#), None);
        assert_eq!(parse_tag("not json"), None);
        assert!(REPO.starts_with("https://github.com/"));
    }
}
