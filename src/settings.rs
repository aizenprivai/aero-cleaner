//! User settings (~/.config/aero-cleaner/settings.toml) and cleaning history
//! (~/.local/share/aero-cleaner/history.tsv). Settings from 2.1 and before
//! (a `settings` file of `key=value` lines) are migrated on first start.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::daynight::Mode;
use crate::sound::Event;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use toml::Table;

use crate::platform::{config_dir, data_dir, home};

#[derive(Clone, Debug, PartialEq)]
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
    /// Switching between a day theme and a night theme.
    pub auto_theme: crate::daynight::Mode,
    pub day_theme: String,
    pub night_theme: String,
    /// When day and night start on a schedule, in minutes since midnight.
    pub day_at: u32,
    pub night_at: u32,
    /// Seasonal particles, a sun that follows the time of day, and fun dates.
    pub seasons: bool,
    pub live_sky: bool,
    pub fun_dates: bool,
    /// Sounds (off until turned on), their volume in percent, and the ones turned off.
    pub sound_on: bool,
    pub sound_volume: u32,
    pub sound_off: Vec<crate::sound::Event>,
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
            auto_theme: crate::daynight::Mode::Off,
            day_theme: "Frutiger Aero".into(),
            night_theme: "Aurora Night".into(),
            day_at: 7 * 60,
            night_at: 20 * 60,
            seasons: false,
            live_sky: false,
            fun_dates: false,
            sound_on: false,
            sound_volume: 60,
            sound_off: vec![crate::sound::Event::ScanDone],
        }
    }
}

impl Settings {
    /// Your settings: `settings.toml`, or the old 2.1 file migrated, or the defaults.
    pub fn load() -> Self {
        // Tests start from the defaults, whatever your own settings are.
        if cfg!(test) {
            return Settings::default();
        }
        let (s, warnings) = Settings::load_from(&config_dir());
        for w in warnings {
            eprintln!("AeroClean: settings.toml: {w}");
        }
        s
    }

    /// Reads the settings in `dir`, migrating the old file if that's all there is.
    pub fn load_from(dir: &Path) -> (Self, Vec<String>) {
        let new = dir.join("settings.toml");
        let old = dir.join("settings");
        match fs::read_to_string(&new) {
            Ok(text) => {
                let (s, warnings) = Settings::from_toml(&text);
                // A file that isn't TOML at all is kept aside, so saving doesn't lose it.
                if warnings.first().is_some_and(|w| w.starts_with("couldn't read")) {
                    let _ = fs::copy(&new, dir.join("settings.toml.broken"));
                }
                (s, warnings)
            }
            Err(_) => match fs::read_to_string(&old) {
                Ok(text) => {
                    let s = Settings::from_v1(&text);
                    if fs::write(&new, s.to_toml()).is_ok() {
                        let _ = fs::rename(&old, dir.join("settings.old"));
                    }
                    (s, vec![])
                }
                Err(_) => (Settings::default(), vec![]),
            },
        }
    }

    /// The 2.1 format: one `key=value` per line.
    pub fn from_v1(text: &str) -> Self {
        let mut s = Settings::default();
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
                Some(("language", v)) if language_ok(v.trim()) => s.language = v.trim().to_string(),
                Some(("auto_theme", v)) => s.auto_theme = Mode::from_code(v).unwrap_or(Mode::Off),
                Some(("day_theme", v)) if !v.trim().is_empty() => s.day_theme = v.trim().to_string(),
                Some(("night_theme", v)) if !v.trim().is_empty() => s.night_theme = v.trim().to_string(),
                Some(("day_at", v)) => s.day_at = v.trim().parse().unwrap_or(7 * 60).min(1439),
                Some(("night_at", v)) => s.night_at = v.trim().parse().unwrap_or(20 * 60).min(1439),
                Some(("seasons", v)) => s.seasons = v.trim() == "true",
                Some(("live_sky", v)) => s.live_sky = v.trim() == "true",
                Some(("fun_dates", v)) => s.fun_dates = v.trim() == "true",
                Some(("sound_on", v)) => s.sound_on = v.trim() == "true",
                Some(("sound_volume", v)) => s.sound_volume = v.trim().parse().unwrap_or(60).min(100),
                Some(("sound_off", v)) => s.sound_off = v.split(',').filter_map(Event::from_code).collect(),
                Some(("exclude", v)) if !v.trim().is_empty() => s.exclusions.push(PathBuf::from(v.trim())),
                _ => {}
            }
        }
        s
    }

    /// Reads `settings.toml`. Each setting that's missing or wrong keeps its
    /// default (with a warning); the others still apply.
    pub fn from_toml(text: &str) -> (Self, Vec<String>) {
        let mut s = Settings::default();
        let mut warn = vec![];
        let table: Table = match text.parse() {
            Ok(t) => t,
            Err(e) => return (s, vec![format!("couldn't read it, using the defaults: {}", e.to_string().trim().replace('\n', " "))]),
        };
        let mut r = Reader { table: &table, warn: &mut warn };
        r.bool("look", "reduce_motion", &mut s.reduce_motion);
        r.int("look", "size", &mut s.ui_scale, 100, 200);
        if let Some(v) = r.string("look", "language")
            && r.check("look", "language", language_ok(&v), "auto, en, fr, es, de or pt")
        {
            s.language = v;
        }
        r.bool("cleaning", "weekly_trash", &mut s.auto_trash);
        r.int("cleaning", "journal_days", &mut s.journal_days, 1, 365);
        r.int("cleaning", "big_files_min_mb", &mut s.big_min_mb, 1, 100_000);
        r.int("cleaning", "undo_hours", &mut s.undo_hours, 0, 24 * 30);
        if let Some(v) = r.strings("cleaning", "selected") {
            s.selected = Some(v);
        }
        if let Some(v) = r.strings("cleaning", "keep") {
            s.exclusions = v.into_iter().filter(|x| !x.trim().is_empty()).map(PathBuf::from).collect();
        }
        r.int("alerts", "low_disk_percent", &mut s.low_disk_percent, 1, 50);
        r.bool("updates", "check", &mut s.check_updates);
        if let Some(v) = r.string("day_and_night", "switch") {
            match Mode::from_code(&v) {
                Some(m) => s.auto_theme = m,
                None => {
                    r.check("day_and_night", "switch", false, "off, system, schedule or sun");
                }
            };
        }
        for (key, field) in [("day_theme", &mut s.day_theme), ("night_theme", &mut s.night_theme)] {
            if let Some(v) = r.string("day_and_night", key).filter(|v| !v.trim().is_empty()) {
                *field = v.trim().to_string();
            }
        }
        for (key, field) in [("day_from", &mut s.day_at), ("night_from", &mut s.night_at)] {
            if let Some(v) = r.string("day_and_night", key) {
                match parse_clock(&v) {
                    Some(m) => *field = m,
                    None => {
                        r.check("day_and_night", key, false, "a time like \"07:30\"");
                    }
                };
            }
        }
        r.bool("touches", "seasons", &mut s.seasons);
        r.bool("touches", "live_sky", &mut s.live_sky);
        r.bool("touches", "fun_dates", &mut s.fun_dates);
        r.bool("sounds", "on", &mut s.sound_on);
        r.int("sounds", "volume", &mut s.sound_volume, 0, 100);
        if let Some(v) = r.strings("sounds", "off") {
            let events: Vec<Event> = v.iter().filter_map(|x| Event::from_code(x)).collect();
            if events.len() != v.len() {
                r.warn.push("sounds.off: only clean, scan, click and warning are known; the others are left out".into());
            }
            s.sound_off = events;
        }
        (s, warn)
    }

    /// The settings as `settings.toml`.
    pub fn to_toml(&self) -> String {
        let q = |v: &str| toml::Value::String(v.to_string()).to_string();
        let list = |v: &[String]| format!("[{}]", v.iter().map(|x| q(x)).collect::<Vec<_>>().join(", "));
        let keep: Vec<String> = self.exclusions.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        let off: Vec<String> = self.sound_off.iter().map(|e| e.code().to_string()).collect();
        let selected = match &self.selected {
            Some(v) => format!("selected = {}\n", list(v)),
            None => String::new(),
        };
        format!(
            r#"# AeroClean settings. You can edit this file while AeroClean is closed;
# a setting that's missing or wrong just uses its default.
version = 2

[look]
reduce_motion = {}
size = {}            # text and controls, in percent (100 to 200)
language = {}   # auto, en, fr, es, de or pt

[cleaning]
weekly_trash = {}   # the weekly automatic clean also empties the Trash
journal_days = {}      # days of system journal kept
big_files_min_mb = {}
undo_hours = {}       # cleaned files are kept this long so a clean can be undone (0 = off)
{}keep = {}   # folders the cleaner never touches

[alerts]
low_disk_percent = {}

[updates]
check = {}

[day_and_night]
switch = {}   # off, system, schedule or sun
day_theme = {}
night_theme = {}
day_from = {}
night_from = {}

[touches]
seasons = {}
live_sky = {}
fun_dates = {}

[sounds]
on = {}
volume = {}
off = {}   # clean, scan, click, warning
"#,
            self.reduce_motion,
            self.ui_scale,
            q(&self.language),
            self.auto_trash,
            self.journal_days,
            self.big_min_mb,
            self.undo_hours,
            selected,
            list(&keep),
            self.low_disk_percent,
            self.check_updates,
            q(self.auto_theme.code()),
            q(&self.day_theme),
            q(&self.night_theme),
            q(&crate::daynight::clock(self.day_at as f64)),
            q(&crate::daynight::clock(self.night_at as f64)),
            self.seasons,
            self.live_sky,
            self.fun_dates,
            self.sound_on,
            self.sound_volume,
            list(&off),
        )
    }

    pub fn save(&self) {
        // Tests never touch the real settings.
        if cfg!(test) {
            return;
        }
        let _ = self.save_to(&config_dir());
    }

    /// Writes `settings.toml` in `dir` (through a temporary file, so a crash never leaves half a file).
    pub fn save_to(&self, dir: &Path) -> std::io::Result<()> {
        fs::create_dir_all(dir)?;
        let tmp = dir.join("settings.toml.tmp");
        fs::write(&tmp, self.to_toml())?;
        fs::rename(&tmp, dir.join("settings.toml"))
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

fn language_ok(v: &str) -> bool {
    v == "auto" || crate::i18n::Lang::from_code(v).is_some_and(|l| l.code() == v)
}

/// "07:30" -> 450.
fn parse_clock(v: &str) -> Option<u32> {
    let (h, m) = v.trim().split_once(':')?;
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// Reads settings one by one, noting the ones that are wrong.
struct Reader<'a> {
    table: &'a Table,
    warn: &'a mut Vec<String>,
}

impl Reader<'_> {
    fn get(&self, section: &str, key: &str) -> Option<&toml::Value> {
        self.table.get(section)?.as_table()?.get(key)
    }

    fn check(&mut self, section: &str, key: &str, ok: bool, expected: &str) -> bool {
        if !ok {
            self.warn.push(format!("{section}.{key} should be {expected}; using the default"));
        }
        ok
    }

    fn bool(&mut self, section: &str, key: &str, out: &mut bool) {
        if let Some(v) = self.get(section, key) {
            match v.as_bool() {
                Some(b) => *out = b,
                None => {
                    self.check(section, key, false, "true or false");
                }
            }
        }
    }

    fn int<T: TryFrom<i64>>(&mut self, section: &str, key: &str, out: &mut T, min: i64, max: i64) {
        if let Some(v) = self.get(section, key) {
            match v.as_integer().filter(|n| (min..=max).contains(n)).and_then(|n| T::try_from(n).ok()) {
                Some(n) => *out = n,
                None => {
                    self.check(section, key, false, &format!("a number from {min} to {max}"));
                }
            }
        }
    }

    fn string(&mut self, section: &str, key: &str) -> Option<String> {
        let v = self.get(section, key)?;
        let s = v.as_str().map(str::to_string);
        if s.is_none() {
            self.check(section, key, false, "text in quotes");
        }
        s
    }

    fn strings(&mut self, section: &str, key: &str) -> Option<Vec<String>> {
        let v = self.get(section, key)?;
        let list = v.as_array().and_then(|a| a.iter().map(|x| x.as_str().map(str::to_string)).collect::<Option<Vec<_>>>());
        if list.is_none() {
            self.check(section, key, false, "a list of texts, like [\"a\", \"b\"]");
        }
        list
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

#[cfg(test)]
mod tests {
    use super::*;

    fn custom() -> Settings {
        Settings {
            reduce_motion: true,
            exclusions: vec![PathBuf::from("/home/ann/.cache/mozilla"), PathBuf::from(r#"C:\Users\Ann "Q"\AppData"#)],
            auto_trash: true,
            low_disk_percent: 15,
            journal_days: 7,
            big_min_mb: 250,
            selected: Some(vec!["Trash".into(), "Thumbnails".into()]),
            language: "fr".into(),
            check_updates: true,
            undo_hours: 48,
            ui_scale: 150,
            auto_theme: Mode::Sun,
            day_theme: "Sakura Spring".into(),
            night_theme: "My \"Night\" theme".into(),
            day_at: 6 * 60 + 30,
            night_at: 21 * 60,
            seasons: true,
            live_sky: true,
            fun_dates: true,
            sound_on: true,
            sound_volume: 80,
            sound_off: vec![Event::Click, Event::Warning],
        }
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("aero-settings-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn toml_round_trip() {
        for s in [Settings::default(), custom()] {
            let text = s.to_toml();
            let (back, warnings) = Settings::from_toml(&text);
            assert!(warnings.is_empty(), "{warnings:?}\n{text}");
            assert_eq!(back, s, "\n{text}");
        }
        // An empty file is all defaults.
        assert_eq!(Settings::from_toml(""), (Settings::default(), vec![]));
    }

    #[test]
    fn the_old_file_is_migrated_once() {
        let dir = temp("migrate");
        let s = custom();
        // The 2.1 format, written the way 2.1 wrote it.
        let mut old = format!(
            "reduce_motion=true\nauto_trash=true\nlow_disk_percent=15\njournal_days=7\nbig_min_mb=250\nlanguage=fr\ncheck_updates=true\nundo_hours=48\nui_scale=150\nauto_theme=sun\nday_theme=Sakura Spring\nnight_theme={}\nday_at=390\nnight_at=1260\nseasons=true\nlive_sky=true\nfun_dates=true\nsound_on=true\nsound_volume=80\nsound_off=click,warning\nselected=Trash,Thumbnails\n",
            s.night_theme
        );
        for x in &s.exclusions {
            old.push_str(&format!("exclude={}\n", x.display()));
        }
        fs::write(dir.join("settings"), &old).unwrap();
        let (loaded, warnings) = Settings::load_from(&dir);
        assert!(warnings.is_empty());
        assert_eq!(loaded, s);
        assert!(dir.join("settings.toml").exists(), "the new file is written");
        assert!(!dir.join("settings").exists() && dir.join("settings.old").exists(), "the old one is kept aside");
        // Next time, the new file is read (and changes to it stick).
        let mut changed = loaded.clone();
        changed.sound_volume = 20;
        changed.save_to(&dir).unwrap();
        assert_eq!(Settings::load_from(&dir).0, changed);
        assert!(!dir.join("settings.toml.tmp").exists());
        // Nothing at all: the defaults, and no file written until a setting changes.
        let empty = temp("empty");
        assert_eq!(Settings::load_from(&empty).0, Settings::default());
        assert!(!empty.join("settings.toml").exists());
        fs::remove_dir_all(&dir).unwrap();
        fs::remove_dir_all(&empty).unwrap();
    }

    #[test]
    fn a_wrong_setting_keeps_its_default_and_the_rest_still_apply() {
        let text = r#"
            [look]
            reduce_motion = "yes"
            size = 500
            language = "klingon"
            [cleaning]
            journal_days = 30
            keep = "/not/a/list"
            [day_and_night]
            switch = "always"
            day_from = "25:00"
            night_from = "22:15"
            [sounds]
            on = true
            off = ["click", "boom"]
        "#;
        let (s, warnings) = Settings::from_toml(text);
        let d = Settings::default();
        assert_eq!((s.reduce_motion, s.ui_scale, s.language.as_str()), (d.reduce_motion, d.ui_scale, d.language.as_str()));
        assert_eq!(s.exclusions, d.exclusions);
        assert_eq!((s.auto_theme, s.day_at), (d.auto_theme, d.day_at));
        assert_eq!((s.journal_days, s.night_at, s.sound_on), (30, 22 * 60 + 15, true), "the good ones apply");
        assert_eq!(s.sound_off, [Event::Click], "the known sounds in a list still apply");
        assert_eq!(warnings.len(), 7, "{warnings:#?}");
        assert!(warnings.iter().any(|w| w.starts_with("sounds.off") && w.contains("left out")));
        assert!(warnings.iter().any(|w| w.contains("look.size") && w.contains("100 to 200")));
        assert!(warnings.iter().any(|w| w.contains("day_and_night.day_from") && w.contains("07:30")));
    }

    #[test]
    fn a_broken_file_is_kept_aside() {
        let dir = temp("broken");
        fs::write(dir.join("settings.toml"), "this isn't [ toml").unwrap();
        let (s, warnings) = Settings::load_from(&dir);
        assert_eq!(s, Settings::default());
        assert!(warnings[0].starts_with("couldn't read"));
        assert_eq!(fs::read_to_string(dir.join("settings.toml.broken")).unwrap(), "this isn't [ toml");
        fs::remove_dir_all(&dir).unwrap();
    }
}
