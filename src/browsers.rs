//! Per-browser cleaning: cache, cookies and history, chosen separately.
//! Supports Chromium-based browsers (Brave, Chrome, Chromium, Vivaldi, Edge)
//! and Firefox. Firefox history shares a database with bookmarks, so it is
//! cleaned with SQL that keeps every bookmarked page.

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
    /// History paths are Firefox `places.sqlite` databases to clean with
    /// SQL, never files to delete (they also hold the bookmarks).
    pub places_db: bool,
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
    fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.join(marker).exists()).collect()).unwrap_or_default()
}

/// `cache` is the separate cache root (Linux: ~/.cache/<browser>). When None
/// (Windows), the cache lives inside each profile as `<profile>/Cache`: never
/// the profile folder itself, which also holds bookmarks and passwords.
fn chromium(name: &'static str, process: &'static str, config: PathBuf, cache: Option<PathBuf>) -> Option<Browser> {
    let profiles = subdirs_with(&config, "Preferences");
    if profiles.is_empty() {
        return None;
    }
    let mut cache_paths = Vec::new();
    let mut cookies = Vec::new();
    let mut history = Vec::new();
    for prof in &profiles {
        let pname = prof.file_name().unwrap();
        cache_paths.push(match &cache {
            Some(root) => root.join(pname),
            None => prof.join("Cache"),
        });
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
        places_db: false,
    })
}

fn firefox(root: PathBuf, cache_root: PathBuf) -> Option<Browser> {
    let profiles = subdirs_with(&root, "prefs.js");
    if profiles.is_empty() {
        return None;
    }
    let mut cache_paths = Vec::new();
    let mut cookies = Vec::new();
    let mut places = Vec::new();
    for prof in &profiles {
        cache_paths.push(cache_root.join(prof.file_name().unwrap()));
        for f in ["cookies.sqlite", "cookies.sqlite-wal", "cookies.sqlite-shm", "webappsstore.sqlite", "storage/default"] {
            cookies.push(prof.join(f));
        }
        if prof.join("places.sqlite").is_file() {
            places.push(prof.join("places.sqlite"));
        }
    }
    let history_size = places.iter().map(|p| firefox_history::estimate(p)).sum();
    Some(Browser {
        name: "Firefox",
        process: "firefox",
        profiles: profiles.len(),
        parts: vec![(Part::Cache, collect(cache_paths)), (Part::Cookies, collect(cookies)), (Part::History, Some((places, history_size)))],
        places_db: true,
    })
}

/// Linux layout: profiles in ~/.config/<browser>, caches in ~/.cache/<browser>.
pub fn detect_linux(home: &Path) -> Vec<Browser> {
    let c = |p: &str| home.join(".config").join(p);
    let k = |p: &str| Some(home.join(".cache").join(p));
    [
        chromium("Brave", "brave", c("BraveSoftware/Brave-Browser"), k("BraveSoftware/Brave-Browser")),
        chromium("Chrome", "chrome", c("google-chrome"), k("google-chrome")),
        chromium("Chromium", "chromium", c("chromium"), k("chromium")),
        chromium("Vivaldi", "vivaldi-bin", c("vivaldi"), k("vivaldi")),
        chromium("Edge", "msedge", c("microsoft-edge"), k("microsoft-edge")),
        firefox(home.join(".mozilla/firefox"), home.join(".cache/mozilla/firefox")),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// macOS layout: profiles in ~/Library/Application Support/<vendor>,
/// caches in ~/Library/Caches/<vendor>. Process names are the app names,
/// lowercased. (Safari's data is protected by macOS, so it isn't listed.)
pub fn detect_macos(home: &Path) -> Vec<Browser> {
    let a = |p: &str| home.join("Library/Application Support").join(p);
    let k = |p: &str| Some(home.join("Library/Caches").join(p));
    [
        chromium("Brave", "brave browser", a("BraveSoftware/Brave-Browser"), k("BraveSoftware/Brave-Browser")),
        chromium("Chrome", "google chrome", a("Google/Chrome"), k("Google/Chrome")),
        chromium("Chromium", "chromium", a("Chromium"), k("Chromium")),
        chromium("Vivaldi", "vivaldi", a("Vivaldi"), k("Vivaldi")),
        chromium("Edge", "microsoft edge", a("Microsoft Edge"), k("Microsoft Edge")),
        firefox(a("Firefox/Profiles"), home.join("Library/Caches/Firefox/Profiles")),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Windows layout: profiles (and caches) in %LOCALAPPDATA%\<vendor>\User Data.
pub fn detect_windows(local: &Path, roaming: &Path) -> Vec<Browser> {
    let u = |p: &str| local.join(p).join("User Data");
    [
        chromium("Brave", "brave", u("BraveSoftware/Brave-Browser"), None),
        chromium("Chrome", "chrome", u("Google/Chrome"), None),
        chromium("Chromium", "chrome", u("Chromium"), None),
        chromium("Vivaldi", "vivaldi", u("Vivaldi"), None),
        chromium("Edge", "msedge", u("Microsoft/Edge"), None),
        firefox(roaming.join("Mozilla/Firefox/Profiles"), local.join("Mozilla/Firefox/Profiles")),
    ]
    .into_iter()
    .flatten()
    .collect()
}

pub fn detect() -> Vec<Browser> {
    if crate::platform::IS_MACOS {
        detect_macos(&crate::platform::home())
    } else if crate::platform::IS_WINDOWS {
        detect_windows(&crate::platform::local_app_data(), &crate::platform::roaming_app_data())
    } else {
        detect_linux(&crate::platform::home())
    }
}

pub fn is_running(process: &str) -> bool {
    crate::platform::running_processes().iter().any(|p| p == process)
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
        if *part == Part::History && browser.places_db {
            failed += paths.iter().filter(|p| firefox_history::clear(p).is_err()).count();
            continue;
        }
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

/// Firefox history in `places.sqlite`: visits, address bar data and pages
/// that aren't bookmarked. Bookmarks, their pages and keywords are kept.
/// Only runs while Firefox is closed (its triggers aren't loaded then).
mod firefox_history {
    use rusqlite::{Connection, OpenFlags};
    use std::path::Path;

    /// Pages that must survive: bookmarked, or used by a bookmark keyword.
    const KEPT: &str = "SELECT fk FROM moz_bookmarks WHERE fk IS NOT NULL UNION SELECT place_id FROM moz_keywords WHERE place_id IS NOT NULL";

    fn has_table(db: &Connection, name: &str) -> bool {
        db.query_row("SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1", [name], |r| r.get::<_, i64>(0)).is_ok_and(|n| n > 0)
    }

    fn kept_query(db: &Connection) -> String {
        if has_table(db, "moz_keywords") { KEPT.to_string() } else { "SELECT fk FROM moz_bookmarks WHERE fk IS NOT NULL".to_string() }
    }

    /// Roughly how much space cleaning would free: the file's share of
    /// pages that would go, plus the visit table's share.
    pub fn estimate(path: &Path) -> u64 {
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let Ok(db) = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX) else { return 0 };
        if !has_table(&db, "moz_places") || !has_table(&db, "moz_bookmarks") {
            return 0;
        }
        let count = |sql: &str| db.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap_or(0).max(0) as u64;
        let total = count("SELECT count(*) FROM moz_places");
        if total == 0 {
            return 0;
        }
        let gone = count(&format!("SELECT count(*) FROM moz_places WHERE id NOT IN ({})", kept_query(&db)));
        size * gone / total
    }

    pub fn clear(path: &Path) -> rusqlite::Result<()> {
        let mut db = Connection::open(path)?;
        if !has_table(&db, "moz_places") || !has_table(&db, "moz_bookmarks") {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let kept = kept_query(&db);
        let tables: Vec<&str> =
            ["moz_historyvisits", "moz_inputhistory", "moz_places_metadata", "moz_places_metadata_search_queries", "moz_annos", "moz_origins", "moz_icons_to_pages"]
                .into_iter()
                .filter(|t| has_table(&db, t))
                .collect();
        let tx = db.transaction()?;
        for t in ["moz_historyvisits", "moz_inputhistory", "moz_places_metadata", "moz_places_metadata_search_queries"] {
            if tables.contains(&t) {
                tx.execute(&format!("DELETE FROM {t}"), [])?;
            }
        }
        tx.execute(&format!("DELETE FROM moz_places WHERE id NOT IN ({kept})"), [])?;
        // Bookmarked pages stay, but forget that they were visited.
        tx.execute("UPDATE moz_places SET visit_count = 0, last_visit_date = NULL", [])?;
        if tables.contains(&"moz_annos") {
            tx.execute("DELETE FROM moz_annos WHERE place_id NOT IN (SELECT id FROM moz_places)", [])?;
        }
        if tables.contains(&"moz_origins") {
            tx.execute("DELETE FROM moz_origins WHERE id NOT IN (SELECT origin_id FROM moz_places WHERE origin_id IS NOT NULL)", [])?;
        }
        tx.commit()?;
        // Actually give the space back.
        db.execute_batch("VACUUM")?;
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn keeps_bookmarks_and_their_pages() {
            let dir = std::env::temp_dir().join(format!("aero-places-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("places.sqlite");
            let _ = std::fs::remove_file(&path);
            let db = Connection::open(&path).unwrap();
            db.execute_batch(
                "CREATE TABLE moz_origins (id INTEGER PRIMARY KEY, host TEXT);
                 CREATE TABLE moz_places (id INTEGER PRIMARY KEY, url TEXT, visit_count INTEGER, last_visit_date INTEGER, origin_id INTEGER);
                 CREATE TABLE moz_historyvisits (id INTEGER PRIMARY KEY, place_id INTEGER);
                 CREATE TABLE moz_bookmarks (id INTEGER PRIMARY KEY, fk INTEGER, title TEXT);
                 CREATE TABLE moz_keywords (id INTEGER PRIMARY KEY, keyword TEXT, place_id INTEGER);
                 CREATE TABLE moz_inputhistory (place_id INTEGER, input TEXT);
                 INSERT INTO moz_origins VALUES (1, 'kept.org'), (2, 'gone.com'), (3, 'kw.net');
                 INSERT INTO moz_places VALUES (1, 'https://kept.org', 5, 99, 1), (2, 'https://gone.com', 3, 98, 2), (3, 'https://kw.net/?q=%s', 1, 97, 3);
                 INSERT INTO moz_historyvisits VALUES (1, 1), (2, 2), (3, 2);
                 INSERT INTO moz_bookmarks VALUES (1, NULL, 'Toolbar'), (2, 1, 'Kept site');
                 INSERT INTO moz_keywords VALUES (1, 'kw', 3);
                 INSERT INTO moz_inputhistory VALUES (2, 'go');",
            )
            .unwrap();
            drop(db);

            assert!(estimate(&path) > 0);
            clear(&path).unwrap();
            let db = Connection::open(&path).unwrap();
            let q = |sql: &str| db.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap();
            assert_eq!(q("SELECT count(*) FROM moz_historyvisits"), 0);
            assert_eq!(q("SELECT count(*) FROM moz_inputhistory"), 0);
            assert_eq!(q("SELECT count(*) FROM moz_bookmarks"), 2, "bookmarks untouched");
            let ids: String = db.query_row("SELECT group_concat(id) FROM (SELECT id FROM moz_places ORDER BY id)", [], |r| r.get(0)).unwrap();
            assert_eq!(ids, "1,3", "bookmarked and keyword pages kept, the rest gone");
            assert_eq!(q("SELECT sum(visit_count) FROM moz_places"), 0, "visit counts reset");
            assert_eq!(q("SELECT count(*) FROM moz_origins WHERE host = 'gone.com'"), 0);
            assert_eq!(q("SELECT count(*) FROM moz_origins"), 2);
            drop(db);
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }
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

        let found = detect_linux(&home);
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

    #[test]
    fn macos_layout_finds_profiles_and_caches() {
        let home = std::env::temp_dir().join(format!("aero-brm-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        let prof = home.join("Library/Application Support/Google/Chrome/Default");
        fs::create_dir_all(&prof).unwrap();
        fs::write(prof.join("Preferences"), "{}").unwrap();
        fs::write(prof.join("History"), vec![0u8; 70]).unwrap();
        let cache = home.join("Library/Caches/Google/Chrome/Default/Cache");
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("data_1"), vec![0u8; 250]).unwrap();
        let ff = home.join("Library/Application Support/Firefox/Profiles/abc.default-release");
        fs::create_dir_all(&ff).unwrap();
        fs::write(ff.join("prefs.js"), "").unwrap();
        fs::write(ff.join("cookies.sqlite"), vec![0u8; 30]).unwrap();
        fs::create_dir_all(home.join("Library/Caches/Firefox/Profiles/abc.default-release/cache2")).unwrap();
        fs::write(home.join("Library/Caches/Firefox/Profiles/abc.default-release/cache2/x"), vec![0u8; 90]).unwrap();

        let found = detect_macos(&home);
        let names: Vec<_> = found.iter().map(|b| (b.name, b.process)).collect();
        assert_eq!(names, vec![("Chrome", "google chrome"), ("Firefox", "firefox")]);
        let size = |b: &Browser, part| b.parts.iter().find(|(p, _)| *p == part).unwrap().1.as_ref().map(|d| d.1);
        assert_eq!((size(&found[0], Part::Cache), size(&found[0], Part::History)), (Some(250), Some(70)));
        assert_eq!((size(&found[1], Part::Cache), size(&found[1], Part::Cookies)), (Some(90), Some(30)));
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn windows_layout_targets_cache_inside_profile_only() {
        let base = std::env::temp_dir().join(format!("aero-brw-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let (local, roaming) = (base.join("Local"), base.join("Roaming"));
        let prof = local.join("Google/Chrome/User Data/Default");
        fs::create_dir_all(prof.join("Cache/Cache_Data")).unwrap();
        fs::write(prof.join("Preferences"), "{}").unwrap();
        fs::write(prof.join("Bookmarks"), "keep me").unwrap();
        fs::write(prof.join("Login Data"), "passwords").unwrap();
        fs::write(prof.join("Cache/Cache_Data/f_000001"), vec![0u8; 300]).unwrap();
        let found = detect_windows(&local, &roaming);
        assert_eq!(found.len(), 1);
        let cache = found[0].parts.iter().find(|(p, _)| *p == Part::Cache).unwrap().1.as_ref().unwrap();
        assert!(cache.0.iter().all(|p| p != &prof), "never the whole profile");
        assert!(cache.0.contains(&prof.join("Cache")));
        assert_eq!(cache.1, 300);
        clean(&found[0], &[Part::Cache]).unwrap();
        assert!(prof.join("Bookmarks").exists() && prof.join("Login Data").exists() && prof.join("Preferences").exists());
        assert!(!prof.join("Cache/Cache_Data").exists());
        fs::remove_dir_all(&base).unwrap();
    }
}
