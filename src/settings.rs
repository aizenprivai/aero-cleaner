//! User settings (~/.config/aero-cleaner/settings) and cleaning history
//! (~/.local/share/aero-cleaner/history.tsv), stored as plain text.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::platform::{config_dir, data_dir, home};

pub struct Settings {
    pub reduce_motion: bool,
    pub exclusions: Vec<PathBuf>,
    /// The weekly automatic clean also empties the Trash.
    pub auto_trash: bool,
    /// Low-disk alert fires below this percentage of free space.
    pub low_disk_percent: u8,
    /// Days of system journal kept when cleaning it.
    pub journal_days: u32,
    /// Minimum size for the Big files list, in MB.
    pub big_min_mb: u64,
    /// The Cleaner categories ticked last time (by name), if remembered.
    pub selected: Option<Vec<String>>,
    /// "auto", "en" or "fr".
    pub language: String,
    /// Opt-in daily check for a newer version on GitHub.
    pub check_updates: bool,
    /// Hours cleaned files are kept so a clean can be undone (0 = off).
    pub undo_hours: u32,
    /// Size of text and controls, in percent (100 to 200).
    pub ui_scale: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            reduce_motion: false,
            exclusions: vec![],
            auto_trash: false,
            low_disk_percent: 10,
            journal_days: 3,
            big_min_mb: 100,
            selected: None,
            language: "auto".into(),
            check_updates: false,
            undo_hours: 24,
            ui_scale: 100,
        }
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
                Some(("check_updates", v)) => s.check_updates = v.trim() == "true",
                Some(("ui_scale", v)) => s.ui_scale = v.trim().parse().unwrap_or(100).clamp(100, 200),
                Some(("undo_hours", v)) => s.undo_hours = v.trim().parse().unwrap_or(24).min(24 * 30),
                Some(("low_disk_percent", v)) => s.low_disk_percent = v.trim().parse().unwrap_or(10).clamp(1, 50),
                Some(("journal_days", v)) => s.journal_days = v.trim().parse().unwrap_or(3).clamp(1, 365),
                Some(("big_min_mb", v)) => s.big_min_mb = v.trim().parse().unwrap_or(100).clamp(1, 100_000),
                Some(("selected", v)) => s.selected = Some(v.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()),
                Some(("language", v)) if ["auto", "en", "fr"].contains(&v.trim()) => s.language = v.trim().to_string(),
                Some(("exclude", v)) if !v.trim().is_empty() => s.exclusions.push(PathBuf::from(v.trim())),
                _ => {}
            }
        }
        s
    }

    pub fn save(&self) {
        let mut text = format!(
            "reduce_motion={}\nauto_trash={}\nlow_disk_percent={}\njournal_days={}\nbig_min_mb={}\nlanguage={}\ncheck_updates={}\nundo_hours={}\nui_scale={}\n",
            self.reduce_motion, self.auto_trash, self.low_disk_percent, self.journal_days, self.big_min_mb, self.language, self.check_updates, self.undo_hours, self.ui_scale
        );
        if let Some(sel) = &self.selected {
            text.push_str(&format!("selected={}\n", sel.join(",")));
        }
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
    // On Windows, browser caches aren't part of the Cleaner's "Temporary files".
    if crate::platform::IS_WINDOWS {
        return vec![];
    }
    if crate::platform::IS_MACOS {
        let c = home().join("Library/Caches");
        return ["Firefox", "BraveSoftware", "Google/Chrome", "Chromium", "Vivaldi", "Microsoft Edge"].iter().map(|d| c.join(d)).filter(|p| p.exists()).collect();
    }
    let c = home().join(".cache");
    ["mozilla", "BraveSoftware", "google-chrome", "chromium", "vivaldi", "librewolf"].iter().map(|d| c.join(d)).filter(|p| p.exists()).collect()
}
