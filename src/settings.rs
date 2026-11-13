//! User settings (~/.config/aero-cleaner/settings) and cleaning history
//! (~/.local/share/aero-cleaner/history.tsv), stored as plain text.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config")).join("aero-cleaner")
}

fn data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/share")).join("aero-cleaner")
}

pub struct Settings {
    pub reduce_motion: bool,
    pub exclusions: Vec<PathBuf>,
    /// The weekly automatic clean also empties the Trash.
    pub auto_trash: bool,
    /// Low-disk alert fires below this percentage of free space.
    pub low_disk_percent: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { reduce_motion: false, exclusions: vec![], auto_trash: false, low_disk_percent: 10 }
    }
}

impl Settings {
    pub fn load() -> Self {
        let mut s = Settings::default();
        let Ok(text) = fs::read_to_string(config_dir().join("settings")) else { return s };
        for line in text.lines() {
            match line.split_once('=') {
                Some(("reduce_motion", v)) => s.reduce_motion = v.trim() == "true",
                Some(("auto_trash", v)) => s.auto_trash = v.trim() == "true",
                Some(("low_disk_percent", v)) => s.low_disk_percent = v.trim().parse().unwrap_or(10).clamp(1, 50),
                Some(("exclude", v)) if !v.trim().is_empty() => s.exclusions.push(PathBuf::from(v.trim())),
                _ => {}
            }
        }
        s
    }

    pub fn save(&self) {
        let mut text = format!(
            "reduce_motion={}\nauto_trash={}\nlow_disk_percent={}\n",
            self.reduce_motion, self.auto_trash, self.low_disk_percent
        );
        for x in &self.exclusions {
            text.push_str(&format!("exclude={}\n", x.display()));
        }
        let _ = fs::create_dir_all(config_dir());
        let _ = fs::write(config_dir().join("settings"), text);
    }

    /// Adds an exclusion, expanding a leading "~". Returns an error message
    /// if the path is not usable.
    pub fn add_exclusion(&mut self, raw: &str) -> Result<(), String> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err("Type or pick a folder first.".into());
        }
        let path = match raw.strip_prefix("~") {
            Some(rest) => home().join(rest.trim_start_matches('/')),
            None => PathBuf::from(raw),
        };
        if !path.is_absolute() {
            return Err("Use a full path, like ~/.cache/mozilla".into());
        }
        let path = PathBuf::from(path.to_string_lossy().trim_end_matches('/'));
        if self.exclusions.contains(&path) {
            return Err("Already excluded.".into());
        }
        self.exclusions.push(path);
        self.save();
        Ok(())
    }
}

pub struct HistoryEntry {
    pub when: SystemTime,
    pub freed: u64,
    pub what: String,
}

fn history_file() -> PathBuf {
    data_dir().join("history.tsv")
}

/// All past cleans, newest first.
pub fn history() -> Vec<HistoryEntry> {
    let text = fs::read_to_string(history_file()).unwrap_or_default();
    let mut v: Vec<HistoryEntry> = text
        .lines()
        .filter_map(|l| {
            let mut parts = l.splitn(3, '\t');
            let secs: u64 = parts.next()?.parse().ok()?;
            let freed: u64 = parts.next()?.parse().ok()?;
            Some(HistoryEntry { when: UNIX_EPOCH + Duration::from_secs(secs), freed, what: parts.next().unwrap_or("").to_string() })
        })
        .collect();
    v.reverse();
    v
}

pub fn record(freed: u64, what: &str) {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let _ = fs::create_dir_all(data_dir());
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(history_file()) {
        let _ = writeln!(f, "{secs}\t{freed}\t{}", what.replace(['\t', '\n'], " "));
    }
}

/// Folders people commonly want to keep: browser caches (keeps sites fast).
pub fn browser_cache_dirs() -> Vec<PathBuf> {
    let c = home().join(".cache");
    ["mozilla", "BraveSoftware", "google-chrome", "chromium", "vivaldi", "librewolf"]
        .iter()
        .map(|d| c.join(d))
        .filter(|p| p.exists())
        .collect()
}
