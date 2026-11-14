//! Cleaning backend: what can be cleaned, how big it is, and how to clean it.
//!
//! User-level items are deleted directly from Rust. System-level items are
//! collected into one shell script and run through `pkexec`, so the user only
//! sees a single password prompt.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Trash,
    Thumbnails,
    AppCache,
    DevCache,
    RecentFiles,
    Flatpak,
    AptCache,
    Autoremove,
    Journal,
    OldLogs,
    CrashReports,
    VarTmp,
}

pub struct Category {
    pub kind: Kind,
    pub name: &'static str,
    pub desc: &'static str,
    pub icon: &'static str,
    pub root: bool,
    pub default_on: bool,
}

pub const CATEGORIES: &[Category] = &[
    Category { kind: Kind::Trash, name: "Trash", desc: "Files you already moved to the trash", icon: "🗑", root: false, default_on: true },
    Category { kind: Kind::Thumbnails, name: "Thumbnail cache", desc: "Image previews, rebuilt automatically when needed", icon: "🖼", root: false, default_on: true },
    Category { kind: Kind::AppCache, name: "App & browser cache", desc: "~/.cache and Flatpak app caches (close your browser first)", icon: "🌐", root: false, default_on: true },
    Category { kind: Kind::DevCache, name: "Developer caches", desc: "Cargo, npm and Gradle download caches (re-downloaded on demand)", icon: "🔧", root: false, default_on: false },
    Category { kind: Kind::RecentFiles, name: "Recent files history", desc: "The \"recently opened\" list (privacy)", icon: "🕘", root: false, default_on: false },
    Category { kind: Kind::Flatpak, name: "Unused Flatpak runtimes", desc: "Runtimes no installed app needs anymore", icon: "📦", root: false, default_on: true },
    Category { kind: Kind::AptCache, name: "APT package cache", desc: "Downloaded .deb installers that are already installed", icon: "💿", root: true, default_on: true },
    Category { kind: Kind::Autoremove, name: "Unused packages & old kernels", desc: "Packages nothing depends on anymore (apt autoremove)", icon: "🐧", root: true, default_on: false },
    Category { kind: Kind::Journal, name: "System journal", desc: "Old system logs, keeps the last {} days", icon: "📜", root: true, default_on: true },
    Category { kind: Kind::OldLogs, name: "Rotated log files", desc: "Old compressed logs in /var/log (*.gz, *.1, *.old)", icon: "📄", root: true, default_on: true },
    Category { kind: Kind::CrashReports, name: "Crash reports", desc: "Crash dumps in /var/crash", icon: "💥", root: true, default_on: true },
    Category { kind: Kind::VarTmp, name: "Old temporary files", desc: "Files in /var/tmp untouched for 7+ days", icon: "⏳", root: true, default_on: true },
];

pub struct ScanResult {
    /// Bytes that cleaning would free, if it can be measured.
    pub size: Option<u64>,
    /// Extra info shown next to the size ("12 packages", "not installed"...).
    pub note: String,
    pub available: bool,
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/root"))
}

fn user_paths(kind: Kind) -> Vec<PathBuf> {
    let h = home();
    match kind {
        Kind::Trash => vec![h.join(".local/share/Trash/files"), h.join(".local/share/Trash/info"), h.join(".local/share/Trash/expunged")],
        Kind::Thumbnails => vec![h.join(".cache/thumbnails")],
        Kind::AppCache => {
            let mut v = vec![h.join(".cache")];
            if let Ok(rd) = fs::read_dir(h.join(".var/app")) {
                v.extend(rd.flatten().map(|e| e.path().join("cache")));
            }
            v
        }
        Kind::DevCache => vec![h.join(".cargo/registry/cache"), h.join(".npm/_cacache"), h.join(".gradle/caches")],
        Kind::RecentFiles => vec![h.join(".local/share/recently-used.xbel")],
        _ => vec![],
    }
}

/// Paths excluded from a category because another category covers them.
fn user_skips(kind: Kind) -> Vec<PathBuf> {
    match kind {
        Kind::AppCache => vec![home().join(".cache/thumbnails")],
        _ => vec![],
    }
}

fn skips_for(kind: Kind, exclusions: &[PathBuf]) -> Vec<PathBuf> {
    let mut v = user_skips(kind);
    v.extend(exclusions.iter().cloned());
    v
}

/// What cleaning `kind` would remove, as short human-readable lines.
pub fn preview(kind: Kind, exclusions: &[PathBuf], journal_days: u32) -> Vec<String> {
    let home = home();
    let pretty = |p: &Path| p.to_string_lossy().replacen(&*home.to_string_lossy(), "~", 1);
    match kind {
        Kind::Trash | Kind::Thumbnails | Kind::AppCache | Kind::DevCache | Kind::RecentFiles => {
            let skips = skips_for(kind, exclusions);
            let mut entries: Vec<(PathBuf, u64)> = Vec::new();
            for root in user_paths(kind) {
                match fs::read_dir(&root) {
                    Ok(rd) => entries.extend(rd.flatten().map(|e| e.path()).filter(|p| !skips.contains(p)).map(|p| {
                        let size = path_size(&p, &skips, &all);
                        (p, size)
                    })),
                    Err(_) if root.is_file() => entries.push((root.clone(), path_size(&root, &[], &all))),
                    Err(_) => {}
                }
            }
            entries.retain(|e| e.1 > 0);
            entries.sort_by(|a, b| b.1.cmp(&a.1));
            let more = entries.len().saturating_sub(6);
            let mut lines: Vec<String> = entries.iter().take(6).map(|(p, s)| format!("{}  ({})", pretty(p), format_bytes(*s))).collect();
            if more > 0 {
                lines.push(format!("…and {more} more"));
            }
            let kept: Vec<String> = exclusions.iter().filter(|x| user_paths(kind).iter().any(|r| x.starts_with(r))).map(|x| format!("kept: {}", pretty(x))).collect();
            lines.extend(kept);
            lines
        }
        Kind::Autoremove => {
            let out = run_c("apt-get", &["-s", "autoremove", "--purge"]);
            let pkgs: Vec<&str> = out.lines().filter_map(|l| l.strip_prefix("Purg ").or_else(|| l.strip_prefix("Remv "))).filter_map(|l| l.split_whitespace().next()).collect();
            if pkgs.is_empty() { vec!["No packages to remove".into()] } else { vec![format!("Removes: {}", pkgs.join(", "))] }
        }
        Kind::Flatpak => vec!["Runs: flatpak uninstall --unused".into()],
        Kind::AptCache => vec!["Deletes downloaded .deb files in /var/cache/apt/archives".into()],
        Kind::Journal => vec![format!("Deletes system log entries older than {journal_days} days")],
        Kind::OldLogs => vec!["Deletes *.gz, *.old and *.1-style files in /var/log".into()],
        Kind::CrashReports => vec!["Deletes everything in /var/crash".into()],
        Kind::VarTmp => vec!["Deletes files in /var/tmp untouched for 7+ days".into()],
    }
}

/// Names of web browsers that are currently running.
pub fn running_browsers() -> Vec<&'static str> {
    const BROWSERS: &[(&str, &str)] = &[
        ("firefox", "Firefox"), ("firefox-bin", "Firefox"), ("librewolf", "LibreWolf"), ("brave", "Brave"),
        ("chrome", "Chrome"), ("chromium", "Chromium"), ("vivaldi-bin", "Vivaldi"), ("opera", "Opera"), ("msedge", "Edge"),
    ];
    let mut found = Vec::new();
    if let Ok(rd) = fs::read_dir("/proc") {
        for e in rd.flatten() {
            if let Ok(comm) = fs::read_to_string(e.path().join("comm")) {
                if let Some((_, name)) = BROWSERS.iter().find(|(c, _)| *c == comm.trim()) {
                    if !found.contains(name) {
                        found.push(*name);
                    }
                }
            }
        }
    }
    found
}

/// Recursive size, never following symlinks. Unreadable entries are skipped.
fn path_size(p: &Path, skips: &[PathBuf], filter: &dyn Fn(&Path, &fs::Metadata) -> bool) -> u64 {
    if skips.iter().any(|s| s == p) {
        return 0;
    }
    let Ok(md) = fs::symlink_metadata(p) else { return 0 };
    if md.is_dir() {
        let Ok(rd) = fs::read_dir(p) else { return 0 };
        rd.flatten().map(|e| path_size(&e.path(), skips, filter)).sum()
    } else if md.is_file() && filter(p, &md) {
        md.len()
    } else {
        0
    }
}

fn all(_: &Path, _: &fs::Metadata) -> bool {
    true
}

fn is_rotated_log(p: &Path, _: &fs::Metadata) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.ends_with(".gz")
        || name.ends_with(".old")
        || name.rsplit('.').next().is_some_and(|ext| !ext.is_empty() && ext.chars().all(|c| c.is_ascii_digit()) && name.contains('.'))
}

fn older_than_7_days(_: &Path, md: &fs::Metadata) -> bool {
    md.modified()
        .ok()
        .and_then(|m| SystemTime::now().duration_since(m).ok())
        .is_some_and(|age| age > Duration::from_secs(7 * 86400))
}

fn have(cmd: &str) -> bool {
    Command::new("sh").args(["-c", &format!("command -v {cmd}")]).stdout(Stdio::null()).status().is_ok_and(|s| s.success())
}

fn run_c(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd)
        .args(args)
        .env("LC_ALL", "C")
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// Parses sizes like "1.2G", "512.0M", "123 kB", "1,234 MB".
fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim().replace(',', "");
    let split = s.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(s.len());
    let num: f64 = s[..split].parse().ok()?;
    let unit = s[split..].trim().to_ascii_uppercase();
    let mult = match unit.chars().next() {
        Some('K') => 1e3,
        Some('M') => 1e6,
        Some('G') => 1e9,
        Some('T') => 1e12,
        _ => 1.0,
    };
    Some((num * mult) as u64)
}

pub fn scan(kind: Kind, exclusions: &[PathBuf]) -> ScanResult {
    let ok = |size: u64| ScanResult { size: Some(size), note: String::new(), available: true };
    match kind {
        Kind::Trash | Kind::Thumbnails | Kind::AppCache | Kind::DevCache | Kind::RecentFiles => {
            let skips = skips_for(kind, exclusions);
            ok(user_paths(kind).iter().map(|p| path_size(p, &skips, &all)).sum())
        }
        Kind::Flatpak => {
            if !have("flatpak") {
                return ScanResult { size: None, note: "Flatpak not installed".into(), available: false };
            }
            ScanResult { size: None, note: "size unknown until removed".into(), available: true }
        }
        Kind::AptCache => ok(path_size(Path::new("/var/cache/apt/archives"), &[], &|p, _| p.extension().is_some_and(|e| e == "deb"))),
        Kind::Autoremove => {
            let out = run_c("apt-get", &["-s", "autoremove", "--purge"]);
            let count = out.lines().filter(|l| l.starts_with("Purg ") || l.starts_with("Remv ")).count();
            let size = out
                .lines()
                .find(|l| l.contains("disk space will be freed"))
                .and_then(|l| l.strip_prefix("After this operation, "))
                .and_then(|l| l.split(" disk space").next())
                .and_then(parse_size);
            ScanResult { size: size.or(Some(0)), note: format!("{count} package{}", if count == 1 { "" } else { "s" }), available: true }
        }
        Kind::Journal => {
            // "Archived and active journals take up 1.2G in the file system."
            let out = run_c("journalctl", &["--disk-usage"]);
            let size = out.split("take up ").nth(1).and_then(|r| r.split(" in the").next()).and_then(parse_size);
            ScanResult { size, note: "up to".into(), available: true }
        }
        Kind::OldLogs => ok(path_size(Path::new("/var/log"), &[PathBuf::from("/var/log/journal")], &is_rotated_log)),
        Kind::CrashReports => ok(path_size(Path::new("/var/crash"), &[], &all)),
        Kind::VarTmp => ok(path_size(Path::new("/var/tmp"), &[], &older_than_7_days)),
    }
}

/// Deletes the contents of a directory (or a single file) without following
/// symlinks. The directory itself is kept. Returns the number of entries that
/// could not be removed (usually files held by running apps).
fn clear(p: &Path, skips: &[PathBuf]) -> usize {
    let Ok(md) = fs::symlink_metadata(p) else { return 0 };
    if !md.is_dir() {
        return fs::remove_file(p).is_err() as usize;
    }
    let Ok(rd) = fs::read_dir(p) else { return 1 };
    let mut failed = 0;
    for e in rd.flatten() {
        let path = e.path();
        if skips.iter().any(|s| *s == path) {
            continue;
        }
        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        // A folder that contains an excluded path: clean around it instead.
        if is_dir && skips.iter().any(|s| s.starts_with(&path)) {
            failed += clear(&path, skips);
            continue;
        }
        // std::fs::remove_dir_all does not follow symlinks.
        let res = if is_dir { fs::remove_dir_all(&path) } else { fs::remove_file(&path) };
        if res.is_err() {
            failed += 1;
        }
    }
    failed
}

fn root_script(kind: Kind, journal_days: u32) -> Option<String> {
    let s = match kind {
        Kind::AptCache => "echo '>> Cleaning APT cache'; apt-get clean",
        Kind::Autoremove => "echo '>> Removing unused packages'; DEBIAN_FRONTEND=noninteractive apt-get autoremove --purge -y",
        Kind::Journal => return Some(format!("echo '>> Shrinking system journal'; journalctl --vacuum-time={}d", journal_days.max(1))),
        Kind::OldLogs => "echo '>> Deleting rotated logs'; find /var/log -type f -not -path '/var/log/journal/*' \\( -name '*.gz' -o -name '*.old' -o -regex '.*\\.[0-9]+' \\) -print -delete | wc -l | sed 's/$/ files removed/'",
        Kind::CrashReports => "echo '>> Deleting crash reports'; find /var/crash -mindepth 1 -delete",
        Kind::VarTmp => "echo '>> Deleting old temp files'; find /var/tmp -mindepth 1 -type f -mtime +7 -delete; find /var/tmp -mindepth 1 -type d -empty -delete",
        _ => return None,
    };
    Some(s.to_string())
}

/// Cleans the given categories, reporting progress lines through `log`.
pub fn clean(kinds: &[Kind], exclusions: &[PathBuf], journal_days: u32, log: &dyn Fn(String)) {
    // Merge stderr into stdout so a single pipe carries everything (no deadlock).
    let mut script = String::from("exec 2>&1\nexport LC_ALL=C\n");
    let mut need_root = false;

    for &kind in kinds {
        let cat = CATEGORIES.iter().find(|c| c.kind == kind).unwrap();
        if let Some(line) = root_script(kind, journal_days) {
            script.push_str(&line);
            script.push('\n');
            need_root = true;
            continue;
        }
        log(format!(">> {}", cat.name));
        if kind == Kind::Flatpak {
            let out = run_c("flatpak", &["uninstall", "--unused", "-y", "--noninteractive"]);
            log(out.lines().last().unwrap_or("done").to_string());
            continue;
        }
        let skips = skips_for(kind, exclusions);
        let failed: usize = user_paths(kind).iter().map(|p| clear(p, &skips)).sum();
        if failed > 0 {
            log(format!("   {failed} item(s) in use, skipped"));
        }
    }

    if !need_root {
        return;
    }
    log("Asking for administrator password...".into());
    let child = Command::new("pkexec").args(["/bin/sh", "-c", &script]).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return log(format!("Could not run pkexec: {e}")),
    };
    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            log(line);
        }
    }
    match child.wait().map(|s| s.code()) {
        Ok(Some(126 | 127)) => log("Password prompt was cancelled, system items were skipped.".into()),
        Ok(Some(0)) => {}
        Ok(code) => log(format!("System cleanup finished with errors (code {code:?})")),
        Err(e) => log(format!("System cleanup failed: {e}")),
    }
}

pub fn format_bytes(b: u64) -> String {
    // French uses "octets" (o, Ko, Mo...) and a decimal comma.
    let fr = crate::i18n::is_french();
    let units: [&str; 5] = if fr { ["o", "Ko", "Mo", "Go", "To"] } else { ["B", "KB", "MB", "GB", "TB"] };
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1000.0 && i < units.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    let text = if i == 0 { format!("{b} {}", units[0]) } else { format!("{v:.1} {}", units[i]) };
    if fr { text.replace('.', ",") } else { text }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_removes_contents_but_not_symlink_targets() {
        let base = std::env::temp_dir().join(format!("aero-test-{}", std::process::id()));
        let target = base.join("outside");
        let dir = base.join("cache");
        fs::create_dir_all(dir.join("sub/deep")).unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("keep.txt"), "important").unwrap();
        fs::write(dir.join("a.bin"), vec![0u8; 1000]).unwrap();
        fs::write(dir.join("sub/deep/b.bin"), vec![0u8; 500]).unwrap();
        std::os::unix::fs::symlink(&target, dir.join("link")).unwrap();
        fs::create_dir_all(dir.join("skipme")).unwrap();
        fs::write(dir.join("skipme/c"), "x").unwrap();

        assert_eq!(path_size(&dir, &[dir.join("skipme")], &all), 1500);
        assert_eq!(clear(&dir, &[dir.join("skipme")]), 0);
        assert!(dir.exists(), "directory itself is kept");
        assert!(!dir.join("a.bin").exists() && !dir.join("sub").exists() && !dir.join("link").exists());
        assert!(dir.join("skipme/c").exists(), "skipped path untouched");
        assert!(target.join("keep.txt").exists(), "symlink target untouched");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn exclusion_deep_inside_cleaned_folder_is_kept() {
        let base = std::env::temp_dir().join(format!("aero-excl-{}", std::process::id()));
        let keep = base.join("mozilla/firefox/profile");
        fs::create_dir_all(&keep).unwrap();
        fs::write(keep.join("keep.bin"), vec![0u8; 300]).unwrap();
        fs::write(base.join("mozilla/firefox/junk.bin"), vec![0u8; 200]).unwrap();
        fs::create_dir_all(base.join("other")).unwrap();
        fs::write(base.join("other/x"), vec![0u8; 100]).unwrap();

        let skips = vec![keep.clone()];
        assert_eq!(path_size(&base, &skips, &all), 300);
        assert_eq!(clear(&base, &skips), 0);
        assert!(keep.join("keep.bin").exists(), "excluded folder survives");
        assert!(!base.join("mozilla/firefox/junk.bin").exists() && !base.join("other").exists());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn detects_running_browser_by_process_name() {
        let dir = std::env::temp_dir().join(format!("aero-browser-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("firefox");
        fs::copy("/bin/sleep", &fake).unwrap();
        let mut child = Command::new(&fake).arg("5").spawn().unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let found = running_browsers();
        child.kill().unwrap();
        let _ = child.wait();
        fs::remove_dir_all(&dir).unwrap();
        assert!(found.contains(&"Firefox"), "found: {found:?}");
    }

    #[test]
    fn parsing() {
        assert_eq!(parse_size("1.2G"), Some(1_200_000_000));
        assert_eq!(parse_size("1,234 kB"), Some(1_234_000));
        assert!(is_rotated_log(Path::new("/var/log/syslog.1"), &fs::metadata("/").unwrap()));
        assert!(is_rotated_log(Path::new("/var/log/x.log.2.gz"), &fs::metadata("/").unwrap()));
        assert!(!is_rotated_log(Path::new("/var/log/syslog"), &fs::metadata("/").unwrap()));
        assert!(!is_rotated_log(Path::new("/var/log/Xorg.0.log"), &fs::metadata("/").unwrap()));
    }

    #[test]
    fn root_scripts_are_valid_shell() {
        for c in CATEGORIES {
            if let Some(s) = root_script(c.kind, 3) {
                let ok = Command::new("sh").args(["-n", "-c", &s]).status().unwrap().success();
                assert!(ok, "bad shell for {:?}", c.kind);
            }
        }
    }

    #[test]
    fn old_logs_script_works_with_gnu_find() {
        let base = std::env::temp_dir().join(format!("aero-logs-{}", std::process::id()));
        fs::create_dir_all(base.join("journal")).unwrap();
        for f in ["syslog", "syslog.1", "kern.log.2.gz", "Xorg.0.log", "Xorg.0.log.old", "journal/x.journal.1"] {
            fs::write(base.join(f), "x").unwrap();
        }
        let script = root_script(Kind::OldLogs, 3).unwrap()
            .replace("/var/log", base.to_str().unwrap())
            .replace("; find ", "; /usr/bin/find ");
        let out = Command::new("sh").args(["-c", &script]).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
        assert!(text.contains("3 files removed"), "unexpected output: {text}");
        assert!(base.join("syslog").exists() && base.join("Xorg.0.log").exists() && base.join("journal/x.journal.1").exists());
        assert!(!base.join("syslog.1").exists() && !base.join("kern.log.2.gz").exists() && !base.join("Xorg.0.log.old").exists());
        fs::remove_dir_all(&base).unwrap();
    }
}
