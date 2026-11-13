//! Per-browser cleaning: cache, cookies and history, chosen separately.
//! Supports Chromium-based browsers (Brave, Chrome, Chromium, Vivaldi, Edge)
//! and Firefox (cache and cookies; its history shares a file with bookmarks).

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Part {
    Cache,
    Cookies,
    History,
}

impl Part {
    pub const ALL: [Part; 3] = [Part::Cache, Part::Cookies, Part::History];

    pub fn label(self) -> &'static str {
        match self {
            Part::Cache => "Cache",
            Part::Cookies => "Cookies & site data",
            Part::History => "Browsing history",
        }
    }

    pub fn desc(self) -> &'static str {
        match self {
            Part::Cache => "Saved copies of web pages. Safe: pages just load a bit slower once.",
            Part::Cookies => "Signs you out of websites and resets site preferences.",
            Part::History => "Visited pages, top sites and address bar suggestions. Bookmarks are kept.",
        }
    }
}

pub struct Browser {
    pub name: &'static str,
    /// Process name used to detect whether it's running.
    pub process: &'static str,
    pub profiles: usize,
    /// Files and folders per part, with their total size. None = not supported.
    pub parts: Vec<(Part, Option<(Vec<PathBuf>, u64)>)>,
}

fn size_of(p: &Path) -> u64 {
    let Ok(md) = p.symlink_metadata() else { return 0 };
    if md.is_dir() {
        fs::read_dir(p).map(|rd| rd.flatten().map(|e| size_of(&e.path())).sum()).unwrap_or(0)
    } else if md.is_file() {
        md.len()
    } else {
        0
    }
}

fn collect(paths: Vec<PathBuf>) -> Option<(Vec<PathBuf>, u64)> {
    let existing: Vec<PathBuf> = paths.into_iter().filter(|p| p.symlink_metadata().is_ok()).collect();
    let size = existing.iter().map(|p| size_of(p)).sum();
    Some((existing, size))
}

fn subdirs_with(dir: &Path, marker: &str) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.join(marker).exists()).collect())
        .unwrap_or_default()
}

fn chromium(name: &'static str, process: &'static str, config: PathBuf, cache: PathBuf) -> Option<Browser> {
    let profiles = subdirs_with(&config, "Preferences");
    if profiles.is_empty() {
        return None;
    }
    let mut cache_paths = Vec::new();
    let mut cookies = Vec::new();
    let mut history = Vec::new();
    for prof in &profiles {
        let pname = prof.file_name().unwrap();
        cache_paths.push(cache.join(pname));
        for d in ["GPUCache", "Service Worker/CacheStorage", "Service Worker/ScriptCache", "Code Cache"] {
            cache_paths.push(prof.join(d));
        }
        for f in ["Cookies", "Cookies-journal", "Network/Cookies", "Network/Cookies-journal", "Local Storage", "Session Storage", "IndexedDB"] {
            cookies.push(prof.join(f));
        }
        for f in ["History", "History-journal", "Visited Links", "Top Sites", "Top Sites-journal", "Shortcuts", "Shortcuts-journal"] {
            history.push(prof.join(f));
        }
    }
    Some(Browser {
        name,
        process,
        profiles: profiles.len(),
        parts: vec![(Part::Cache, collect(cache_paths)), (Part::Cookies, collect(cookies)), (Part::History, collect(history))],
    })
}

fn firefox(home: &Path) -> Option<Browser> {
    let root = home.join(".mozilla/firefox");
    let profiles = subdirs_with(&root, "prefs.js");
    if profiles.is_empty() {
        return None;
    }
    let mut cache_paths = Vec::new();
    let mut cookies = Vec::new();
    for prof in &profiles {
        cache_paths.push(home.join(".cache/mozilla/firefox").join(prof.file_name().unwrap()));
        for f in ["cookies.sqlite", "cookies.sqlite-wal", "cookies.sqlite-shm", "webappsstore.sqlite", "storage/default"] {
            cookies.push(prof.join(f));
        }
    }
    Some(Browser {
        name: "Firefox",
        process: "firefox",
        profiles: profiles.len(),
        parts: vec![(Part::Cache, collect(cache_paths)), (Part::Cookies, collect(cookies)), (Part::History, None)],
    })
}

pub fn detect(home: &Path) -> Vec<Browser> {
    let c = |p: &str| home.join(".config").join(p);
    let k = |p: &str| home.join(".cache").join(p);
    [
        chromium("Brave", "brave", c("BraveSoftware/Brave-Browser"), k("BraveSoftware/Brave-Browser")),
        chromium("Chrome", "chrome", c("google-chrome"), k("google-chrome")),
        chromium("Chromium", "chromium", c("chromium"), k("chromium")),
        chromium("Vivaldi", "vivaldi-bin", c("vivaldi"), k("vivaldi")),
        chromium("Edge", "msedge", c("microsoft-edge"), k("microsoft-edge")),
        firefox(home),
    ]
    .into_iter()
    .flatten()
    .collect()
}

pub fn is_running(process: &str) -> bool {
    fs::read_dir("/proc")
        .map(|rd| rd.flatten().any(|e| fs::read_to_string(e.path().join("comm")).is_ok_and(|c| c.trim() == process)))
        .unwrap_or(false)
}

/// Deletes the given parts. Refuses while the browser runs, because it would
/// rewrite or corrupt its databases. Returns how many items failed.
pub fn clean(browser: &Browser, parts: &[Part]) -> Result<usize, String> {
    if is_running(browser.process) {
        return Err(format!("Close {} first.", browser.name));
    }
    let mut failed = 0;
    for (part, data) in &browser.parts {
        let (Some((paths, _)), true) = (data, parts.contains(part)) else { continue };
        for p in paths {
            let Ok(md) = p.symlink_metadata() else { continue };
            let res = if md.is_dir() {
                // Empty the folder but keep it, like the main cleaner does.
                fs::read_dir(p).map(|rd| {
                    rd.flatten()
                        .map(|e| {
                            let ep = e.path();
                            if ep.symlink_metadata().is_ok_and(|m| m.is_dir()) { fs::remove_dir_all(&ep) } else { fs::remove_file(&ep) }
                        })
                        .filter(|r| r.is_err())
                        .count()
                })
            } else {
                fs::remove_file(p).map(|_| 0)
            };
            failed += res.unwrap_or(1);
        }
    }
    Ok(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chromium_profile_parts_are_separate() {
        let home = std::env::temp_dir().join(format!("aero-br-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        let prof = home.join(".config/google-chrome/Default");
        fs::create_dir_all(prof.join("Network")).unwrap();
        fs::write(prof.join("Preferences"), "{}").unwrap();
        fs::write(prof.join("History"), vec![0u8; 100]).unwrap();
        fs::write(prof.join("Network/Cookies"), vec![0u8; 50]).unwrap();
        fs::write(prof.join("Bookmarks"), "keep me").unwrap();
        fs::create_dir_all(home.join(".cache/google-chrome/Default/Cache")).unwrap();
        fs::write(home.join(".cache/google-chrome/Default/Cache/data_0"), vec![0u8; 400]).unwrap();
        // Not a profile (no Preferences): ignored.
        fs::create_dir_all(home.join(".config/google-chrome/Crashpad")).unwrap();

        let found = detect(&home);
        assert_eq!(found.len(), 1);
        let b = &found[0];
        assert_eq!((b.name, b.profiles), ("Chrome", 1));
        let size = |part| b.parts.iter().find(|(p, _)| *p == part).unwrap().1.as_ref().unwrap().1;
        assert_eq!((size(Part::Cache), size(Part::Cookies), size(Part::History)), (400, 50, 100));

        assert_eq!(clean(b, &[Part::History]).unwrap(), 0);
        assert!(!prof.join("History").exists(), "history deleted");
        assert!(prof.join("Network/Cookies").exists(), "cookies untouched");
        assert!(prof.join("Bookmarks").exists(), "bookmarks untouched");
        assert!(home.join(".cache/google-chrome/Default/Cache/data_0").exists(), "cache untouched");
        clean(b, &[Part::Cache]).unwrap();
        assert!(!home.join(".cache/google-chrome/Default/Cache/data_0").exists());
        assert!(home.join(".cache/google-chrome/Default").exists(), "cache folder itself kept");
        fs::remove_dir_all(&home).unwrap();
    }
}
