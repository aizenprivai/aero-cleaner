//! Extra tools: big file finder, startup apps manager and disk usage.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn home() -> PathBuf {
    crate::platform::home()
}

// ---------- Disk usage ----------

/// (total, available) bytes on the system drive.
pub fn disk_usage() -> Option<(u64, u64)> {
    crate::platform::disk_usage()
}

// ---------- Big files ----------

pub struct BigFile {
    pub path: PathBuf,
    pub size: u64,
    pub modified: Option<SystemTime>,
}

/// Walks the home folder in parallel (without following symlinks) and
/// returns the largest files of at least `min` bytes, biggest first.
pub fn find_big_files(min: u64, limit: usize, progress: &(dyn Fn(u64) + Sync)) -> Vec<BigFile> {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};
    let home = home();
    let skip = [home.join(".local/share/Trash"), home.join(".Trash")];
    let found = Mutex::new(Vec::new());
    let scanned = AtomicU64::new(0);
    crate::walk::par_walk(&home, &|dir| !skip.iter().any(|s| s == dir), &|path, md| {
        let n = scanned.fetch_add(1, Ordering::Relaxed) + 1;
        if n.is_multiple_of(2000) {
            progress(n);
        }
        if md.len() >= min {
            found.lock().unwrap().push(BigFile { path: path.to_path_buf(), size: md.len(), modified: md.modified().ok() });
        }
    });
    progress(scanned.load(Ordering::Relaxed));
    let mut found = found.into_inner().unwrap();
    found.sort_by_key(|f| std::cmp::Reverse(f.size));
    found.truncate(limit);
    found
}

/// Moves a file to the Trash / Recycle Bin (recoverable).
pub fn trash(path: &Path) -> Result<(), String> {
    crate::platform::move_to_trash(path)
}

pub fn open_folder(path: &Path) {
    crate::platform::open_folder(path)
}

pub fn age(t: Option<SystemTime>) -> String {
    // Some packaging tools set dates to 1970; showing "56 years ago" would be nonsense.
    let t = t.filter(|t| t.duration_since(std::time::UNIX_EPOCH).is_ok_and(|d| d.as_secs() > 315_532_800));
    let Some(d) = t.and_then(|t| SystemTime::now().duration_since(t).ok()) else { return String::new() };
    let days = d.as_secs() / 86400;
    let text = match days {
        0 => "today".into(),
        1 => "yesterday".into(),
        2..=59 => format!("{days} days ago"),
        60..=729 => format!("{} months ago", days / 30),
        _ => format!("{} years ago", days / 365),
    };
    crate::i18n::tr(text)
}

// ---------- Startup apps ----------

pub struct StartupEntry {
    pub file: String,
    pub name: String,
    pub comment: String,
    pub enabled: bool,
    /// Provided by the system (/etc/xdg/autostart) rather than the user.
    pub system: bool,
    /// Part of the desktop itself; disabling it could break things.
    pub critical: bool,
}

const SYSTEM_DIR: &str = "/etc/xdg/autostart";

fn user_dir() -> PathBuf {
    home().join(".config/autostart")
}

/// Key/value pairs of the [Desktop Entry] section.
fn parse_desktop(text: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let mut in_main = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_main = line == "[Desktop Entry]";
        } else if in_main && let Some((k, v)) = line.split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    map
}

fn desktops() -> Vec<String> {
    std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().split(':').map(str::to_string).collect()
}

/// Shown on this desktop according to OnlyShowIn / NotShowIn?
fn shown_here(kv: &BTreeMap<String, String>) -> bool {
    let current = desktops();
    let list = |k: &str| kv.get(k).map(|v| v.split(';').filter(|s| !s.is_empty()).map(str::to_string).collect::<Vec<_>>());
    if let Some(only) = list("OnlyShowIn")
        && !only.iter().any(|d| current.contains(d))
    {
        return false;
    }
    if let Some(not) = list("NotShowIn")
        && not.iter().any(|d| current.contains(d))
    {
        return false;
    }
    true
}

fn is_critical(file: &str) -> bool {
    const NEEDED: &[&str] = &[
        "polkit",
        "keyring",
        "at-spi",
        "cinnamon-settings-daemon",
        "csd-",
        "xdg-user-dirs",
        "pipewire",
        "pulseaudio",
        "nm-applet",
        "gnome-shell",
        "mintupdate",
        "user-dirs",
        "xapp-sn-watcher",
        "geoclue",
    ];
    let f = file.to_ascii_lowercase();
    NEEDED.iter().any(|n| f.contains(n))
}

fn localized(kv: &BTreeMap<String, String>, key: &str) -> Option<String> {
    let fr = crate::i18n::is_french().then(|| kv.get(&format!("{key}[fr_FR]")).or_else(|| kv.get(&format!("{key}[fr]")))).flatten();
    fr.or_else(|| kv.get(key)).cloned()
}

pub fn startup_entries() -> Vec<StartupEntry> {
    #[cfg(windows)]
    return crate::windows::startup_entries();
    #[cfg(target_os = "macos")]
    return crate::macos::startup_entries();
    #[cfg(target_os = "linux")]
    linux_startup_entries()
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn linux_startup_entries() -> Vec<StartupEntry> {
    let mut files: BTreeMap<String, (Option<String>, Option<String>)> = BTreeMap::new();
    for (dir, is_user) in [(PathBuf::from(SYSTEM_DIR), false), (user_dir(), true)] {
        let Ok(rd) = fs::read_dir(dir) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.ends_with(".desktop") {
                continue;
            }
            let Ok(text) = fs::read_to_string(e.path()) else { continue };
            let slot = files.entry(name).or_default();
            if is_user { slot.1 = Some(text) } else { slot.0 = Some(text) }
        }
    }

    let mut out = Vec::new();
    for (file, (sys, user)) in files {
        let sys_kv = sys.as_deref().map(parse_desktop);
        let kv = user.as_deref().map(parse_desktop).or_else(|| sys_kv.clone()).unwrap_or_default();
        // Hide entries that don't apply to this desktop, judged on the original file.
        if !shown_here(sys_kv.as_ref().unwrap_or(&kv)) {
            continue;
        }
        let truthy = |k: &str| kv.get(k).map(|v| v.eq_ignore_ascii_case("true"));
        let enabled = truthy("Hidden") != Some(true) && truthy("X-GNOME-Autostart-enabled") != Some(false);
        out.push(StartupEntry {
            // Apps ship their own translations (Name[fr]=...); use them in French.
            name: localized(&kv, "Name").unwrap_or_else(|| file.trim_end_matches(".desktop").to_string()),
            comment: localized(&kv, "Comment").unwrap_or_default(),
            enabled,
            system: sys.is_some(),
            critical: is_critical(&file),
            file,
        });
    }
    out.sort_by_key(|e| (e.critical, e.name.to_lowercase()));
    out
}

/// Sets `key=value` inside [Desktop Entry], adding it if missing.
fn set_key(text: &str, key: &str, value: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let start = lines.iter().position(|l| l.trim() == "[Desktop Entry]");
    let Some(start) = start else {
        return format!("[Desktop Entry]\n{key}={value}\n{text}");
    };
    let end = lines.iter().skip(start + 1).position(|l| l.trim().starts_with('[')).map(|p| p + start + 1).unwrap_or(lines.len());
    match lines[start + 1..end].iter().position(|l| l.split_once('=').is_some_and(|(k, _)| k.trim() == key)) {
        Some(i) => lines[start + 1 + i] = format!("{key}={value}"),
        None => lines.insert(end, format!("{key}={value}")),
    }
    lines.join("\n") + "\n"
}

/// Enables or disables an autostart entry for this user only. System files
/// are never modified: a user copy in ~/.config/autostart overrides them.
pub fn set_startup(file: &str, enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    return crate::windows::set_startup(file, enabled);
    #[cfg(target_os = "macos")]
    return crate::macos::set_startup(file, enabled);
    #[cfg(target_os = "linux")]
    linux_set_startup(file, enabled)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn linux_set_startup(file: &str, enabled: bool) -> Result<(), String> {
    let user_path = user_dir().join(file);
    let text = fs::read_to_string(&user_path).or_else(|_| fs::read_to_string(Path::new(SYSTEM_DIR).join(file))).map_err(|e| e.to_string())?;
    let mut text = set_key(&text, "X-GNOME-Autostart-enabled", if enabled { "true" } else { "false" });
    if enabled && parse_desktop(&text).get("Hidden").is_some_and(|v| v.eq_ignore_ascii_case("true")) {
        text = set_key(&text, "Hidden", "false");
    }
    fs::create_dir_all(user_dir()).map_err(|e| e.to_string())?;
    fs::write(&user_path, text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_key_replaces_or_adds_in_main_section() {
        let src = "[Desktop Entry]\nName=Foo\nX-GNOME-Autostart-enabled=true\n[Desktop Action x]\nName=Bar\n";
        let out = set_key(src, "X-GNOME-Autostart-enabled", "false");
        assert!(out.contains("X-GNOME-Autostart-enabled=false") && !out.contains("enabled=true"));
        let out2 = set_key("[Desktop Entry]\nName=Foo\n[Other]\nA=b\n", "Hidden", "false");
        let main = out2.split("[Other]").next().unwrap();
        assert!(main.contains("Hidden=false"), "key must land in the main section: {out2}");
        assert_eq!(parse_desktop(&out2).get("Name").map(String::as_str), Some("Foo"));
    }

    /// Timing of the three big scans on the real home folder (read-only).
    /// `cargo test --release bench_scans -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn bench_scans() {
        let home = home();
        for round in 1..=2 {
            let t = std::time::Instant::now();
            let big = find_big_files(100_000_000, 100, &|_| {});
            let t_big = t.elapsed();
            let t = std::time::Instant::now();
            let groups = crate::dupes::find_duplicates(&home, 1_000_000, false, &|_| {});
            let t_dupes = t.elapsed();
            let t = std::time::Instant::now();
            let hidden = crate::dupes::find_duplicates(&home, 1_000_000, true, &|_| {});
            let t_hidden = t.elapsed();
            let t = std::time::Instant::now();
            let tree = crate::diskmap::scan(&home, 1_000_000, &|_| {});
            let t_map = t.elapsed();
            println!(
                "round {round}: big files {t_big:.2?} ({} found), duplicates {t_dupes:.2?} ({} groups), with hidden {t_hidden:.2?} ({} groups), disk map {t_map:.2?} ({} bytes)",
                big.len(),
                groups.len(),
                hidden.len(),
                tree.size
            );
        }
    }

    #[test]
    fn disk_usage_reads_root() {
        let (total, avail) = disk_usage().expect("df works");
        assert!(total > 0 && avail <= total);
    }
}
