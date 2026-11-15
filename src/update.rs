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

// ---------- Update now ----------

#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    /// "1.6.0"
    pub version: String,
    pub notes: String,
    /// The release page.
    pub page: String,
    pub assets: Vec<Asset>,
}

impl Release {
    pub fn is_newer(&self) -> bool {
        newer(&self.version, CURRENT)
    }

    /// The release notes as plain lines: Markdown marks removed, GitHub's
    /// "by @user in <link>" and "Full Changelog" lines dropped.
    pub fn notes_lines(&self, max: usize) -> Vec<String> {
        self.notes
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with("**Full Changelog**") && !l.starts_with("<!--"))
            .filter_map(|l| {
                let l = l.trim_start_matches('#').trim();
                let (bullet, rest) = match l.strip_prefix("* ").or_else(|| l.strip_prefix("- ")) {
                    Some(r) => (true, r),
                    None => (false, l),
                };
                let rest = rest.split(" by @").next().unwrap_or(rest).replace("**", "").replace('`', "");
                (!rest.is_empty()).then(|| if bullet { format!("• {rest}") } else { rest })
            })
            .take(max)
            .collect()
    }
}

/// Downloads only ever come from this project's own releases.
fn trusted_download(url: &str) -> bool {
    url.starts_with(&format!("{REPO}/releases/download/")) && !url.contains("..")
}

fn get(url: &str, out: Option<&std::path::Path>) -> Result<Vec<u8>, String> {
    let agent = format!("AeroClean/{CURRENT}");
    let target = out.map(|p| p.to_string_lossy().to_string());
    let mut curl: Vec<&str> = vec!["-fsSL", "--max-time", "600", "-A", &agent];
    let mut wget: Vec<&str> = vec!["-q", "-T", "60", "-U", &agent];
    match &target {
        Some(t) => {
            curl.extend(["-o", t]);
            wget.extend(["-O", t]);
        }
        None => wget.extend(["-O", "-"]),
    }
    curl.push(url);
    wget.push(url);
    for (tool, args) in [("curl", curl), ("wget", wget)] {
        let Some(bin) = crate::platform::find_tool(tool) else { continue };
        let o = crate::platform::command(bin).args(&args).stderr(Stdio::null()).output().map_err(|e| e.to_string())?;
        if o.status.success() {
            return Ok(o.stdout);
        }
        return Err("download failed (are you online?)".into());
    }
    Err("curl or wget is needed to download updates".into())
}

/// Version, notes and files of the newest release.
pub fn latest_release() -> Result<Release, String> {
    let api = format!("{}/releases/latest", REPO.replacen("https://github.com/", "https://api.github.com/repos/", 1));
    parse_release(&String::from_utf8_lossy(&get(&api, None)?))
}

fn parse_release(json: &str) -> Result<Release, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|_| "GitHub sent something unexpected".to_string())?;
    let tag = v["tag_name"].as_str().filter(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_alphanumeric() || ".-".contains(c))).ok_or("no version found")?;
    let assets = v["assets"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| Some(Asset { name: x["name"].as_str()?.to_string(), url: x["browser_download_url"].as_str()?.to_string(), size: x["size"].as_u64().unwrap_or(0) }))
                .filter(|a| trusted_download(&a.url) && !a.name.contains(['/', '\\']))
                .collect()
        })
        .unwrap_or_default();
    Ok(Release { version: tag.trim_start_matches('v').to_string(), notes: v["body"].as_str().unwrap_or("").trim().to_string(), page: format!("{REPO}/releases/tag/{tag}"), assets })
}

/// How this copy was installed, i.e. which package can replace it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Installer {
    /// The .deb package (/usr/bin/aero-cleaner).
    Deb,
    /// The Windows installer (%LOCALAPPDATA%\Programs\AeroClean).
    WindowsSetup,
    /// The Mac app (an AeroClean.app bundle).
    MacDmg,
    /// The .rpm package (Fedora, openSUSE).
    Rpm,
    /// An AppImage file, replaced in place.
    AppImage,
}

impl Installer {
    fn suffix(self) -> &'static str {
        match self {
            Installer::Deb => "_amd64.deb",
            Installer::WindowsSetup => "-windows-setup.exe",
            Installer::MacDmg => "-macos.dmg",
            Installer::Rpm => ".x86_64.rpm",
            Installer::AppImage => "-x86_64.AppImage",
        }
    }
}

/// None when this copy can't update itself (built from source, the
/// portable zip): then the download page is the way.
pub fn installer() -> Option<Installer> {
    let exe = std::env::current_exe().ok()?;
    let s = exe.to_string_lossy().replace('\\', "/");
    if crate::platform::IS_WINDOWS {
        s.to_lowercase().contains("/appdata/local/programs/aeroclean/").then_some(Installer::WindowsSetup)
    } else if crate::platform::IS_MACOS {
        s.contains(".app/Contents/MacOS/").then_some(Installer::MacDmg)
    } else {
        if !cfg!(target_arch = "x86_64") {
            return None;
        }
        if std::env::var_os("APPIMAGE").is_some_and(|a| std::path::Path::new(&a).is_file()) {
            return Some(Installer::AppImage);
        }
        if s != "/usr/bin/aero-cleaner" {
            return None;
        }
        // Which package owns the program? (On Arch it's the AUR's job.)
        let owns = |tool: &str, args: &[&str]| {
            crate::platform::find_tool(tool)
                .is_some_and(|t| std::process::Command::new(t).args(args).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|st| st.success()))
        };
        if owns("dpkg", &["-S", "/usr/bin/aero-cleaner"]) {
            Some(Installer::Deb)
        } else if owns("rpm", &["-qf", "/usr/bin/aero-cleaner"]) {
            Some(Installer::Rpm)
        } else {
            None
        }
    }
}

pub fn pick_asset(release: &Release, kind: Installer) -> Option<&Asset> {
    release.assets.iter().find(|a| a.name.ends_with(kind.suffix()))
}

fn sha256_hex(path: &std::path::Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// The expected hash of `name` in a `sha256sum` file.
fn expected_hash(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (hash, file) = l.split_once(char::is_whitespace)?;
        (file.trim().trim_start_matches('*') == name && hash.len() == 64).then(|| hash.to_lowercase())
    })
}

pub fn downloads_dir() -> PathBuf {
    crate::platform::data_dir().join("updates")
}

/// Downloads the package and checks it against the release's SHA256SUMS.
/// `progress` gets the bytes received so far.
pub fn download(release: &Release, asset: &Asset, progress: &(dyn Fn(u64) + Sync)) -> Result<PathBuf, String> {
    let sums_asset = release.assets.iter().find(|a| a.name == "SHA256SUMS").ok_or("this release has no SHA256SUMS, so the download can't be checked")?;
    let sums = String::from_utf8_lossy(&get(&sums_asset.url, None)?).into_owned();
    let want = expected_hash(&sums, &asset.name).ok_or("the package isn't listed in SHA256SUMS")?;
    let dir = downloads_dir();
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(&asset.name);
    // Report progress from the growing file while curl works.
    let done = std::sync::atomic::AtomicBool::new(false);
    let result = std::thread::scope(|s| {
        s.spawn(|| {
            while !done.load(std::sync::atomic::Ordering::Relaxed) {
                progress(std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0));
                std::thread::sleep(Duration::from_millis(250));
            }
        });
        let r = get(&asset.url, Some(&file));
        done.store(true, std::sync::atomic::Ordering::Relaxed);
        r
    });
    result?;
    if sha256_hex(&file)? != want {
        let _ = std::fs::remove_file(&file);
        return Err("the download doesn't match SHA256SUMS, so it was deleted".into());
    }
    Ok(file)
}

/// Starts the installer. Returns a message for the user.
pub fn install(file: &std::path::Path, kind: Installer) -> Result<String, String> {
    match kind {
        Installer::Deb => {
            // apt-get also pulls any new dependency; one password prompt.
            let out = std::process::Command::new("pkexec")
                .args(["apt-get", "install", "-y", "--allow-downgrades"])
                .arg(file)
                .env("DEBIAN_FRONTEND", "noninteractive")
                .output()
                .map_err(|e| e.to_string())?;
            match out.status.code() {
                Some(0) => Ok("Updated. Restart AeroClean to use the new version.".into()),
                Some(126 | 127) => Err("Password prompt was cancelled".into()),
                _ => Err(String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("apt-get failed").to_string()),
            }
        }
        // The installer closes AeroClean itself (and offers to start it again).
        Installer::WindowsSetup => {
            crate::platform::command(file).spawn().map_err(|e| e.to_string())?;
            Ok("The installer is running.".into())
        }
        Installer::MacDmg => {
            std::process::Command::new("open").arg(file).spawn().map_err(|e| e.to_string())?;
            Ok("Drag the new AeroClean into Applications to replace this one, then open it again.".into())
        }
        Installer::Rpm => {
            let mut cmd = std::process::Command::new("pkexec");
            if crate::platform::find_tool("dnf").is_some() {
                cmd.args(["dnf", "install", "-y"]);
            } else {
                cmd.args(["zypper", "-n", "install", "--allow-unsigned-rpm"]);
            }
            let out = cmd.arg(file).output().map_err(|e| e.to_string())?;
            match out.status.code() {
                Some(0) => Ok("Updated. Restart AeroClean to use the new version.".into()),
                Some(126 | 127) => Err("Password prompt was cancelled".into()),
                _ => Err(String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("install failed").to_string()),
            }
        }
        Installer::AppImage => {
            // Swap the AppImage file: copy next to it, then rename over it.
            let target = std::path::PathBuf::from(std::env::var_os("APPIMAGE").ok_or("not running as an AppImage")?);
            let tmp = target.with_extension("new");
            std::fs::copy(file, &tmp).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
            }
            std::fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
            Ok("Updated. Restart AeroClean to use the new version.".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_releases_and_checks_downloads() {
        let json = format!(
            r#"{{"tag_name": "v9.0.0", "body": "- Shiny\n- New", "assets": [
                {{"name": "AeroClean-9.0.0-macos.dmg", "browser_download_url": "{REPO}/releases/download/v9.0.0/AeroClean-9.0.0-macos.dmg", "size": 14000000}},
                {{"name": "aero-cleaner_9.0.0_amd64.deb", "browser_download_url": "{REPO}/releases/download/v9.0.0/aero-cleaner_9.0.0_amd64.deb", "size": 7000000}},
                {{"name": "evil.deb", "browser_download_url": "https://example.com/evil.deb", "size": 1}},
                {{"name": "SHA256SUMS", "browser_download_url": "{REPO}/releases/download/v9.0.0/SHA256SUMS", "size": 300}}]}}"#
        );
        let r = parse_release(&json).unwrap();
        assert_eq!(r.version, "9.0.0");
        assert!(r.is_newer());
        assert_eq!(r.notes, "- Shiny\n- New");
        let gh = Release { notes: "## What's Changed\n* Add **undo** by @someone in https://github.com/x/y/pull/1\n\n**Full Changelog**: v1...v2".into(), ..r.clone() };
        assert_eq!(gh.notes_lines(8), vec!["What's Changed", "• Add undo"]);
        assert_eq!(r.assets.len(), 3, "downloads from elsewhere are dropped");
        assert_eq!(pick_asset(&r, Installer::Deb).unwrap().size, 7_000_000);
        assert_eq!(pick_asset(&r, Installer::MacDmg).unwrap().name, "AeroClean-9.0.0-macos.dmg");
        assert!(pick_asset(&r, Installer::WindowsSetup).is_none());
        let more = Release {
            assets: vec![
                Asset { name: "aero-cleaner-9.0.0-1.x86_64.rpm".into(), url: String::new(), size: 1 },
                Asset { name: "AeroClean-9.0.0-x86_64.AppImage".into(), url: String::new(), size: 2 },
            ],
            ..r.clone()
        };
        assert_eq!(pick_asset(&more, Installer::Rpm).unwrap().size, 1);
        assert_eq!(pick_asset(&more, Installer::AppImage).unwrap().size, 2);

        let sums = "1111111111111111111111111111111111111111111111111111111111111111  aero-cleaner_9.0.0_amd64.deb\nABCDEF0000000000000000000000000000000000000000000000000000000000 *SHA256SUMS\n";
        assert_eq!(expected_hash(sums, "aero-cleaner_9.0.0_amd64.deb").unwrap(), "1".repeat(64));
        assert_eq!(expected_hash(sums, "SHA256SUMS").unwrap(), format!("abcdef{}", "0".repeat(58)));
        assert!(expected_hash(sums, "other.deb").is_none());

        let f = std::env::temp_dir().join(format!("aero-sha-{}", std::process::id()));
        std::fs::write(&f, b"abc").unwrap();
        assert_eq!(sha256_hex(&f).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        std::fs::remove_file(&f).unwrap();
    }

    /// Reads the real latest release from GitHub (needs the internet):
    /// `cargo test live_release -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_release() {
        let r = latest_release().unwrap();
        println!("version {} newer={} assets: {:?}", r.version, r.is_newer(), r.assets.iter().map(|a| (&a.name, a.size)).collect::<Vec<_>>());
        assert!(r.assets.iter().any(|a| a.name == "SHA256SUMS"));
        for k in [Installer::Deb, Installer::WindowsSetup, Installer::MacDmg] {
            assert!(pick_asset(&r, k).is_some(), "{k:?}");
        }
    }

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
