//! Cleaning backend: what can be cleaned, how big it is, and how to clean it.
//!
//! User-level items are deleted directly from Rust. System-level items need
//! one password prompt: on Linux they run as a shell script through `pkexec`
//! (on macOS through osascript's administrator prompt); on Windows an
//! elevated copy of AeroClean (one UAC prompt) deletes them natively, from a
//! fixed list of categories.

use std::fs;
#[cfg(target_os = "linux")]
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
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Journal,
    OldLogs,
    CrashReports,
    VarTmp,
    /// Windows: Delivery Optimization cache.
    #[cfg_attr(not(windows), allow(dead_code))]
    DeliveryOpt,
    /// Windows: blue screen dumps and system error reports. macOS: system
    /// crash reports in /Library/Logs/DiagnosticReports.
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    SystemCrash,
}

pub struct Category {
    pub kind: Kind,
    pub name: &'static str,
    pub desc: &'static str,
    pub icon: &'static str,
    pub root: bool,
    pub default_on: bool,
}

#[cfg(not(any(windows, target_os = "macos")))]
pub const CATEGORIES: &[Category] = &[
    Category { kind: Kind::Trash, name: "Trash", desc: "Files you already moved to the trash", icon: "🗑", root: false, default_on: true },
    Category { kind: Kind::Thumbnails, name: "Thumbnail cache", desc: "Image previews, rebuilt automatically when needed", icon: "🖼", root: false, default_on: true },
    Category { kind: Kind::AppCache, name: "App & browser cache", desc: "~/.cache and Flatpak app caches (close your browser first)", icon: "🌐", root: false, default_on: true },
    Category {
        kind: Kind::DevCache,
        name: "Developer caches",
        desc: "Cargo, npm, pnpm, Yarn, Go, Maven and Gradle downloads (re-downloaded on demand)",
        icon: "🔧",
        root: false,
        default_on: false,
    },
    Category { kind: Kind::RecentFiles, name: "Recent files history", desc: "The \"recently opened\" list (privacy)", icon: "🕘", root: false, default_on: false },
    Category { kind: Kind::Flatpak, name: "Unused Flatpak runtimes", desc: "Runtimes no installed app needs anymore", icon: "📦", root: false, default_on: true },
    Category { kind: Kind::AptCache, name: "APT package cache", desc: "Downloaded .deb installers that are already installed", icon: "💿", root: true, default_on: true },
    Category {
        kind: Kind::Autoremove,
        name: "Unused packages & old kernels",
        desc: "Packages nothing depends on anymore (apt autoremove)",
        icon: "🐧",
        root: true,
        default_on: false,
    },
    Category { kind: Kind::Journal, name: "System journal", desc: "Old system logs, keeps the last {} days", icon: "📜", root: true, default_on: true },
    Category { kind: Kind::OldLogs, name: "Rotated log files", desc: "Old compressed logs in /var/log (*.gz, *.1, *.old)", icon: "📄", root: true, default_on: true },
    Category { kind: Kind::CrashReports, name: "Crash reports", desc: "Crash dumps in /var/crash", icon: "💥", root: true, default_on: true },
    Category { kind: Kind::VarTmp, name: "Old temporary files", desc: "Files in /var/tmp untouched for 7+ days", icon: "⏳", root: true, default_on: true },
];

// On macOS, "AptCache" and "Autoremove" are Homebrew's download cache and
// unused dependencies; both are the user's own, so they need no password.
#[cfg(target_os = "macos")]
pub const CATEGORIES: &[Category] = &[
    Category { kind: Kind::Trash, name: "Trash", desc: "Files you already moved to the Trash (emptied through Finder)", icon: "🗑", root: false, default_on: true },
    Category { kind: Kind::AppCache, name: "App & browser cache", desc: "~/Library/Caches (close your browser first)", icon: "🌐", root: false, default_on: true },
    Category { kind: Kind::Thumbnails, name: "Quick Look thumbnails", desc: "File previews, rebuilt automatically when needed", icon: "🖼", root: false, default_on: true },
    Category { kind: Kind::CrashReports, name: "Crash reports", desc: "App crash reports in ~/Library/Logs/DiagnosticReports", icon: "💥", root: false, default_on: true },
    Category {
        kind: Kind::DevCache,
        name: "Developer caches",
        desc: "Xcode DerivedData, simulator caches, Cargo, npm, pnpm, Go, Maven and Gradle downloads (rebuilt on demand)",
        icon: "🔧",
        root: false,
        default_on: false,
    },
    Category { kind: Kind::AptCache, name: "Homebrew downloads", desc: "Installers Homebrew keeps after installing", icon: "🍺", root: false, default_on: true },
    Category {
        kind: Kind::Autoremove, name: "Unused Homebrew packages", desc: "Dependencies nothing needs anymore (brew autoremove)", icon: "📦", root: false, default_on: false
    },
    Category {
        kind: Kind::RecentFiles, name: "Recent items", desc: "The \"Recent Items\" lists of the Apple menu and apps (privacy)", icon: "🕘", root: false, default_on: false
    },
    Category { kind: Kind::OldLogs, name: "Old system logs", desc: "Rotated logs in /private/var/log (*.gz, *.bz2, *.0...)", icon: "📄", root: true, default_on: true },
    Category { kind: Kind::SystemCrash, name: "System crash reports", desc: "Reports in /Library/Logs/DiagnosticReports", icon: "🔥", root: true, default_on: true },
    Category { kind: Kind::VarTmp, name: "Old temporary files", desc: "Files in /private/var/tmp untouched for 7+ days", icon: "⏳", root: true, default_on: true },
];

#[cfg(windows)]
pub const CATEGORIES: &[Category] = &[
    Category { kind: Kind::Trash, name: "Recycle Bin", desc: "Files you already deleted to the Recycle Bin", icon: "🗑", root: false, default_on: true },
    Category { kind: Kind::AppCache, name: "Temporary files", desc: "Your temp folder and graphics shader caches", icon: "⏳", root: false, default_on: true },
    Category { kind: Kind::Thumbnails, name: "Thumbnail cache", desc: "Explorer image previews, rebuilt automatically when needed", icon: "🖼", root: false, default_on: true },
    Category { kind: Kind::CrashReports, name: "Crash dumps", desc: "App crash dumps and error reports in your profile", icon: "💥", root: false, default_on: true },
    Category {
        kind: Kind::DevCache,
        name: "Developer caches",
        desc: "Cargo, npm, pip, pnpm, Yarn, Go, Maven and Gradle downloads (re-downloaded on demand)",
        icon: "🔧",
        root: false,
        default_on: false,
    },
    Category { kind: Kind::RecentFiles, name: "Recent items", desc: "The \"Recent\" list and jump lists (privacy)", icon: "🕘", root: false, default_on: false },
    Category { kind: Kind::VarTmp, name: "Windows temporary files", desc: "Files in Windows\\Temp untouched for 7+ days", icon: "🖥", root: true, default_on: true },
    Category { kind: Kind::AptCache, name: "Windows Update downloads", desc: "Update files Windows has already installed", icon: "💿", root: true, default_on: true },
    Category { kind: Kind::DeliveryOpt, name: "Delivery Optimization cache", desc: "Update pieces shared with other PCs", icon: "📡", root: true, default_on: true },
    Category { kind: Kind::SystemCrash, name: "System crash dumps", desc: "Blue screen dumps and system error reports", icon: "🔥", root: true, default_on: true },
    Category { kind: Kind::OldLogs, name: "Old Windows logs", desc: "Log files in Windows\\Logs older than 30 days", icon: "📄", root: true, default_on: true },
    Category {
        kind: Kind::Autoremove,
        name: "Superseded Windows components",
        desc: "Old versions of updated system files (DISM, can take several minutes)",
        icon: "📚",
        root: true,
        default_on: false,
    },
];

pub struct ScanResult {
    /// Bytes that cleaning would free, if it can be measured.
    pub size: Option<u64>,
    /// Extra info shown next to the size ("12 packages", "not installed"...).
    pub note: String,
    pub available: bool,
}

fn home() -> PathBuf {
    crate::platform::home()
}

#[cfg(not(any(windows, target_os = "macos")))]
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
        Kind::DevCache => {
            let mut v = dev_cache_paths(&h);
            v.extend([h.join(".local/share/pnpm/store")]);
            v
        }
        Kind::RecentFiles => vec![h.join(".local/share/recently-used.xbel")],
        _ => vec![],
    }
}

#[cfg(target_os = "macos")]
fn user_paths(kind: Kind) -> Vec<PathBuf> {
    let h = home();
    match kind {
        Kind::Trash => vec![h.join(".Trash")],
        Kind::AppCache => vec![h.join("Library/Caches")],
        Kind::Thumbnails => crate::macos::user_cache_dir().map(|d| d.join("com.apple.QuickLook.thumbnailcache")).into_iter().collect(),
        Kind::CrashReports => vec![h.join("Library/Logs/DiagnosticReports")],
        Kind::DevCache => vec![h.join("Library/Developer/Xcode/DerivedData"), h.join("Library/Developer/CoreSimulator/Caches"), h.join("Library/pnpm/store")]
            .into_iter()
            .chain(dev_cache_paths(&h))
            .collect(),
        Kind::AptCache => vec![crate::macos::brew_cache()],
        // Only the "Recent" lists: the same folder also holds Finder's sidebar favorites.
        Kind::RecentFiles => fs::read_dir(h.join("Library/Application Support/com.apple.sharedfilelist"))
            .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().contains("Recent"))).collect())
            .unwrap_or_default(),
        _ => vec![],
    }
}

#[cfg(windows)]
fn user_paths(kind: Kind) -> Vec<PathBuf> {
    use crate::platform::{local_app_data, roaming_app_data};
    let (h, local) = (home(), local_app_data());
    match kind {
        Kind::AppCache => {
            let temp = std::env::var_os("TEMP").map(PathBuf::from).unwrap_or_else(|| local.join("Temp"));
            let mut v = vec![temp, local.join("D3DSCache")];
            for d in ["NVIDIA\\DXCache", "NVIDIA\\GLCache", "AMD\\DxCache", "AMD\\GLCache", "Intel\\ShaderCache"] {
                v.push(local.join(d));
            }
            v
        }
        // Only the cache databases, never the rest of Explorer's folder.
        Kind::Thumbnails => fs::read_dir(local.join("Microsoft\\Windows\\Explorer"))
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.file_name().is_some_and(|n| {
                            let n = n.to_string_lossy().to_lowercase();
                            (n.starts_with("thumbcache_") || n.starts_with("iconcache_")) && n.ends_with(".db")
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        Kind::CrashReports => vec![local.join("CrashDumps"), local.join("Microsoft\\Windows\\WER\\ReportArchive"), local.join("Microsoft\\Windows\\WER\\ReportQueue")],
        Kind::DevCache => {
            let mut v = dev_cache_paths(&h);
            v.extend([local.join("npm-cache\\_cacache"), local.join("pip\\cache"), local.join("pnpm\\store"), local.join("Yarn\\Cache"), local.join("go-build")]);
            v
        }
        Kind::RecentFiles => vec![roaming_app_data().join("Microsoft\\Windows\\Recent")],
        _ => vec![],
    }
}

/// Windows system folders per category, with which files to delete.
#[cfg(windows)]
fn system_paths(kind: Kind) -> (Vec<PathBuf>, fn(&Path, &fs::Metadata) -> bool) {
    use crate::windows::{program_data, windows_dir};
    let w = windows_dir();
    match kind {
        Kind::VarTmp => (vec![w.join("Temp")], older_than_7_days),
        Kind::AptCache => (vec![w.join("SoftwareDistribution\\Download")], all),
        Kind::DeliveryOpt => (vec![w.join("ServiceProfiles\\NetworkService\\AppData\\Local\\Microsoft\\Windows\\DeliveryOptimization\\Cache")], all),
        Kind::SystemCrash => (
            vec![
                w.join("Minidump"),
                w.join("MEMORY.DMP"),
                program_data().join("Microsoft\\Windows\\WER\\ReportArchive"),
                program_data().join("Microsoft\\Windows\\WER\\ReportQueue"),
            ],
            all,
        ),
        Kind::OldLogs => (vec![w.join("Logs")], older_than_30_days),
        _ => (vec![], all),
    }
}

/// Download caches every package manager re-fills on demand, at the same
/// place on every system (relative to home).
fn dev_cache_paths(h: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> =
        [".cargo/registry/cache", ".cargo/registry/src", ".cargo/git", ".gradle/caches", ".m2/repository", ".yarn/berry/cache"].iter().map(|p| h.join(p)).collect();
    if !crate::platform::IS_WINDOWS {
        v.push(h.join(".npm/_cacache"));
    }
    v.push(go_mod_cache(h));
    v
}

/// Go's module cache ($GOMODCACHE, or $GOPATH/pkg/mod, or ~/go/pkg/mod).
fn go_mod_cache(h: &Path) -> PathBuf {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    env("GOMODCACHE").unwrap_or_else(|| env("GOPATH").unwrap_or_else(|| h.join("go")).join("pkg/mod"))
}

/// Go makes its module cache read-only, so give write permission back to
/// the owner before deleting it (never follows symlinks).
fn make_writable(p: &Path) {
    let Ok(md) = fs::symlink_metadata(p) else { return };
    if md.file_type().is_symlink() {
        return;
    }
    let mut perms = md.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(perms.mode() | 0o200);
    }
    #[cfg(not(unix))]
    {
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
    }
    let _ = fs::set_permissions(p, perms);
    if md.is_dir()
        && let Ok(rd) = fs::read_dir(p)
    {
        rd.flatten().for_each(|e| make_writable(&e.path()));
    }
}

/// Paths excluded from a category because another category covers them.
fn user_skips(kind: Kind) -> Vec<PathBuf> {
    match kind {
        Kind::AppCache if crate::platform::IS_MACOS => vec![home().join("Library/Caches/Homebrew")],
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
        #[cfg(windows)]
        Kind::Trash => vec!["Empties the Recycle Bin on all drives".into()],
        #[cfg(windows)]
        Kind::Autoremove => vec!["Runs: DISM /Online /Cleanup-Image /StartComponentCleanup".into()],
        #[cfg(windows)]
        Kind::VarTmp | Kind::AptCache | Kind::DeliveryOpt | Kind::SystemCrash | Kind::OldLogs => {
            system_paths(kind).0.iter().filter(|p| p.exists()).map(|p| format!("Deletes {}", p.display())).collect()
        }
        k if uses_user_paths(k) => {
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
            entries.sort_by_key(|e| std::cmp::Reverse(e.1));
            let more = entries.len().saturating_sub(6);
            let mut lines: Vec<String> = entries.iter().take(6).map(|(p, s)| format!("{}  ({})", pretty(p), format_bytes(*s))).collect();
            if more > 0 {
                lines.push(format!("…and {more} more"));
            }
            let kept: Vec<String> = exclusions.iter().filter(|x| user_paths(kind).iter().any(|r| x.starts_with(r))).map(|x| format!("kept: {}", pretty(x))).collect();
            lines.extend(kept);
            lines
        }
        #[cfg(target_os = "macos")]
        Kind::Autoremove => {
            let pkgs = crate::macos::brew_unused();
            if pkgs.is_empty() { vec!["No packages to remove".into()] } else { vec![format!("Removes: {}", pkgs.join(", "))] }
        }
        #[cfg(target_os = "macos")]
        Kind::OldLogs => vec!["Deletes *.gz, *.bz2 and *.0-style files in /private/var/log".into()],
        #[cfg(target_os = "macos")]
        Kind::SystemCrash => vec!["Deletes everything in /Library/Logs/DiagnosticReports".into()],
        #[cfg(target_os = "macos")]
        Kind::VarTmp => vec!["Deletes files in /private/var/tmp untouched for 7+ days".into()],
        #[cfg(target_os = "linux")]
        Kind::Autoremove => {
            let out = run_c("apt-get", &["-s", "autoremove", "--purge"]);
            let pkgs: Vec<&str> = out.lines().filter_map(|l| l.strip_prefix("Purg ").or_else(|| l.strip_prefix("Remv "))).filter_map(|l| l.split_whitespace().next()).collect();
            if pkgs.is_empty() { vec!["No packages to remove".into()] } else { vec![format!("Removes: {}", pkgs.join(", "))] }
        }
        Kind::Flatpak => vec!["Runs: flatpak uninstall --unused".into()],
        #[cfg(target_os = "linux")]
        Kind::AptCache => vec!["Deletes downloaded .deb files in /var/cache/apt/archives".into()],
        Kind::Journal => vec![format!("Deletes system log entries older than {journal_days} days")],
        #[cfg(target_os = "linux")]
        Kind::OldLogs => vec!["Deletes *.gz, *.old and *.1-style files in /var/log".into()],
        Kind::CrashReports => vec!["Deletes everything in /var/crash".into()],
        #[cfg(target_os = "linux")]
        Kind::VarTmp => vec!["Deletes files in /var/tmp untouched for 7+ days".into()],
        _ => vec![],
    }
}

/// Categories cleaned by emptying plain folders in the user's profile.
fn uses_user_paths(kind: Kind) -> bool {
    matches!(kind, Kind::Trash | Kind::Thumbnails | Kind::AppCache | Kind::DevCache | Kind::RecentFiles)
        || (!crate::platform::IS_LINUX && kind == Kind::CrashReports)
        || (crate::platform::IS_MACOS && kind == Kind::AptCache)
}

/// Names of web browsers that are currently running.
pub fn running_browsers() -> Vec<&'static str> {
    const BROWSERS: &[(&str, &str)] = &[
        ("firefox", "Firefox"),
        ("firefox-bin", "Firefox"),
        ("librewolf", "LibreWolf"),
        ("brave", "Brave"),
        ("chrome", "Chrome"),
        ("chromium", "Chromium"),
        ("vivaldi-bin", "Vivaldi"),
        ("opera", "Opera"),
        ("msedge", "Edge"),
        // macOS app names (lowercased by platform::running_processes).
        ("google chrome", "Chrome"),
        ("brave browser", "Brave"),
        ("microsoft edge", "Edge"),
        ("vivaldi", "Vivaldi"),
        ("safari", "Safari"),
    ];
    let mut found = Vec::new();
    for proc in crate::platform::running_processes() {
        if let Some((_, name)) = BROWSERS.iter().find(|(c, _)| *c == proc)
            && !found.contains(name)
        {
            found.push(*name);
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

#[cfg_attr(windows, allow(dead_code))]
fn is_rotated_log(p: &Path, _: &fs::Metadata) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.ends_with(".gz")
        || name.ends_with(".bz2")
        || name.ends_with(".old")
        || name.rsplit('.').next().is_some_and(|ext| !ext.is_empty() && ext.chars().all(|c| c.is_ascii_digit()) && name.contains('.'))
}

fn older_than(md: &fs::Metadata, days: u64) -> bool {
    md.modified().ok().and_then(|m| SystemTime::now().duration_since(m).ok()).is_some_and(|age| age > Duration::from_secs(days * 86400))
}

fn older_than_7_days(_: &Path, md: &fs::Metadata) -> bool {
    older_than(md, 7)
}

#[cfg_attr(not(windows), allow(dead_code))]
fn older_than_30_days(_: &Path, md: &fs::Metadata) -> bool {
    older_than(md, 30)
}

fn have(cmd: &str) -> bool {
    Command::new("sh").args(["-c", &format!("command -v {cmd}")]).stdout(Stdio::null()).status().is_ok_and(|s| s.success())
}

fn run_c(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd).args(args).env("LC_ALL", "C").stderr(Stdio::null()).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

/// Parses sizes like "1.2G", "512.0M", "123 kB", "1,234 MB".
pub fn parse_size(s: &str) -> Option<u64> {
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
        #[cfg(windows)]
        Kind::Trash => ScanResult { size: crate::windows::recycle_bin_size(), note: String::new(), available: true },
        #[cfg(windows)]
        Kind::Autoremove => ScanResult { size: None, note: "size unknown until removed".into(), available: true },
        #[cfg(windows)]
        Kind::VarTmp | Kind::AptCache | Kind::DeliveryOpt | Kind::SystemCrash | Kind::OldLogs => {
            // Some of these folders are admin-only, so this is a lower bound.
            let (paths, filter) = system_paths(kind);
            ok(paths.iter().map(|p| path_size(p, &[], &filter)).sum())
        }
        // Reading ~/.Trash needs Full Disk Access, but Finder can still empty it.
        #[cfg(target_os = "macos")]
        Kind::Trash if fs::read_dir(home().join(".Trash")).is_err() => ScanResult { size: None, note: "size hidden by macOS".into(), available: true },
        #[cfg(target_os = "macos")]
        Kind::AptCache | Kind::Autoremove if crate::macos::brew().is_none() => ScanResult { size: None, note: "Homebrew not installed".into(), available: false },
        #[cfg(target_os = "macos")]
        Kind::Autoremove => {
            let count = crate::macos::brew_unused().len();
            ScanResult { size: Some(0), note: format!("{count} package{}", if count == 1 { "" } else { "s" }), available: true }
        }
        #[cfg(target_os = "macos")]
        Kind::OldLogs => ok(path_size(Path::new("/private/var/log"), &[], &is_rotated_log)),
        #[cfg(target_os = "macos")]
        Kind::SystemCrash => ok(path_size(Path::new("/Library/Logs/DiagnosticReports"), &[], &all)),
        #[cfg(target_os = "macos")]
        Kind::VarTmp => ok(path_size(Path::new("/private/var/tmp"), &[], &older_than_7_days)),
        k if uses_user_paths(k) => {
            let skips = skips_for(kind, exclusions);
            ok(user_paths(kind).iter().map(|p| path_size(p, &skips, &all)).sum())
        }
        Kind::Flatpak => {
            if !have("flatpak") {
                return ScanResult { size: None, note: "Flatpak not installed".into(), available: false };
            }
            ScanResult { size: None, note: "size unknown until removed".into(), available: true }
        }
        #[cfg(target_os = "linux")]
        Kind::AptCache => ok(path_size(Path::new("/var/cache/apt/archives"), &[], &|p, _| p.extension().is_some_and(|e| e == "deb"))),
        #[cfg(target_os = "linux")]
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
        #[cfg(target_os = "linux")]
        Kind::OldLogs => ok(path_size(Path::new("/var/log"), &[PathBuf::from("/var/log/journal")], &is_rotated_log)),
        Kind::CrashReports => ok(path_size(Path::new("/var/crash"), &[], &all)),
        #[cfg(target_os = "linux")]
        Kind::VarTmp => ok(path_size(Path::new("/var/tmp"), &[], &older_than_7_days)),
        _ => ScanResult { size: None, note: String::new(), available: false },
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
        if skips.contains(&path) {
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

/// Deletes the files under `p` that match `filter`, then removes folders left
/// empty. Never follows symlinks/junctions. Returns how many items failed.
#[cfg_attr(not(windows), allow(dead_code))]
fn clear_filtered(p: &Path, filter: fn(&Path, &fs::Metadata) -> bool) -> usize {
    let Ok(md) = fs::symlink_metadata(p) else { return 0 };
    if md.is_file() {
        return if filter(p, &md) { fs::remove_file(p).is_err() as usize } else { 0 };
    }
    if !md.is_dir() {
        return 0;
    }
    let Ok(rd) = fs::read_dir(p) else { return 1 };
    let mut failed = 0;
    for e in rd.flatten() {
        let path = e.path();
        failed += clear_filtered(&path, filter);
        if path.symlink_metadata().is_ok_and(|m| m.is_dir()) {
            let _ = fs::remove_dir(&path); // only succeeds when empty
        }
    }
    failed
}

/// Runs inside the elevated copy of AeroClean on Windows: cleans the given
/// system categories and appends progress lines to `log_file`.
#[cfg(windows)]
pub fn elevated_clean(kinds: &[Kind], log_file: &Path) {
    use std::io::Write;
    let mut out = fs::OpenOptions::new().create(true).append(true).open(log_file).ok();
    let mut log = |line: String| {
        if let Some(f) = out.as_mut() {
            let _ = writeln!(f, "{line}");
            let _ = f.flush();
        }
    };
    for &kind in kinds {
        let Some(cat) = CATEGORIES.iter().find(|c| c.kind == kind && c.root) else { continue };
        log(format!(">> {}", cat.name));
        if kind == Kind::Autoremove {
            let ok = crate::windows::command("dism.exe").args(["/Online", "/Cleanup-Image", "/StartComponentCleanup"]).status().is_ok_and(|s| s.success());
            if !ok {
                log("DISM reported an error".into());
            }
            continue;
        }
        let (paths, filter) = system_paths(kind);
        let failed: usize = paths.iter().map(|p| clear_filtered(p, filter)).sum();
        if failed > 0 {
            log(format!("   {failed} item(s) in use, skipped"));
        }
    }
}

/// Category from its name (`format!("{:?}", kind)`), for passing between processes.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn kind_from_name(name: &str) -> Option<Kind> {
    CATEGORIES.iter().map(|c| c.kind).find(|k| format!("{k:?}") == name)
}

#[cfg(target_os = "macos")]
fn root_script(kind: Kind, _journal_days: u32) -> Option<String> {
    macos_root_script(kind)
}

/// macOS system items. Uses only `find` options that BSD and GNU find share,
/// so the test can run them anywhere.
#[cfg(any(target_os = "macos", all(test, unix)))]
fn macos_root_script(kind: Kind) -> Option<String> {
    let s = match kind {
        Kind::OldLogs => {
            "echo '>> Deleting old system logs'; find /private/var/log -type f \\( -name '*.gz' -o -name '*.bz2' -o -name '*.old' -o -name '*.[0-9]' -o -name '*.[0-9][0-9]' \\) -print -delete | wc -l | tr -d ' ' | sed 's/$/ files removed/'"
        }
        Kind::SystemCrash => "echo '>> Deleting system crash reports'; find /Library/Logs/DiagnosticReports -mindepth 1 -delete",
        Kind::VarTmp => "echo '>> Deleting old temp files'; find /private/var/tmp -mindepth 1 -type f -mtime +7 -delete",
        _ => return None,
    };
    Some(s.to_string())
}

#[cfg(not(target_os = "macos"))]
fn root_script(kind: Kind, journal_days: u32) -> Option<String> {
    let s = match kind {
        Kind::AptCache => "echo '>> Cleaning APT cache'; apt-get clean",
        Kind::Autoremove => "echo '>> Removing unused packages'; DEBIAN_FRONTEND=noninteractive apt-get autoremove --purge -y",
        Kind::Journal => return Some(format!("echo '>> Shrinking system journal'; journalctl --vacuum-time={}d", journal_days.max(1))),
        Kind::OldLogs => {
            "echo '>> Deleting rotated logs'; find /var/log -type f -not -path '/var/log/journal/*' \\( -name '*.gz' -o -name '*.old' -o -regex '.*\\.[0-9]+' \\) -print -delete | wc -l | sed 's/$/ files removed/'"
        }
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
        #[cfg(windows)]
        if cat.root {
            need_root = true;
            continue;
        }
        log(format!(">> {}", cat.name));
        #[cfg(windows)]
        if kind == Kind::Trash {
            if let Err(e) = crate::windows::empty_recycle_bin() {
                log(format!("Could not empty the Recycle Bin: {e}"));
            }
            continue;
        }
        // Finder can empty the Trash even without Full Disk Access; if it
        // refuses (Automation permission denied), delete what we can read.
        #[cfg(target_os = "macos")]
        if kind == Kind::Trash && !exclusions.iter().any(|x| x.starts_with(home().join(".Trash"))) && crate::macos::empty_trash().is_ok() {
            continue;
        }
        #[cfg(target_os = "macos")]
        if kind == Kind::Autoremove {
            log(crate::macos::brew_autoremove());
            continue;
        }
        if kind == Kind::Flatpak {
            let out = run_c("flatpak", &["uninstall", "--unused", "-y", "--noninteractive"]);
            log(out.lines().last().unwrap_or("done").to_string());
            continue;
        }
        let skips = skips_for(kind, exclusions);
        if kind == Kind::DevCache {
            make_writable(&go_mod_cache(&home()));
        }
        let failed: usize = user_paths(kind).iter().map(|p| clear(p, &skips)).sum();
        if failed > 0 {
            log(format!("   {failed} item(s) in use, skipped"));
        }
    }

    if !need_root {
        return;
    }
    #[cfg(windows)]
    {
        let _ = (script, journal_days);
        let names: Vec<String> = kinds.iter().filter(|k| CATEGORIES.iter().any(|c| c.kind == **k && c.root)).map(|k| format!("{k:?}")).collect();
        let log_file = std::env::temp_dir().join(format!("aero-cleaner-{}.log", std::process::id()));
        let _ = fs::remove_file(&log_file);
        log("Asking for administrator permission...".into());
        let args = vec!["--elevated-clean".to_string(), log_file.display().to_string(), names.join(",")];
        match crate::windows::run_elevated(&args) {
            Ok(code) => {
                for line in fs::read_to_string(&log_file).unwrap_or_default().lines() {
                    log(line.to_string());
                }
                if code != 0 {
                    log(format!("System cleanup finished with errors (code {code})"));
                }
            }
            Err(e) if e == "cancelled" => log("Permission was refused, system items were skipped.".into()),
            Err(e) => log(format!("System cleanup failed: {e}")),
        }
        let _ = fs::remove_file(&log_file);
    }
    #[cfg(target_os = "macos")]
    {
        log("Asking for administrator password...".into());
        match crate::macos::run_as_admin(&script) {
            Ok(out) => out.lines().filter(|l| !l.trim().is_empty()).for_each(|l| log(l.to_string())),
            Err(e) if e == "cancelled" => log("Password prompt was cancelled, system items were skipped.".into()),
            Err(e) => log(format!("System cleanup failed: {e}")),
        }
    }
    #[cfg(target_os = "linux")]
    unix_root_clean(&script, log);
}

#[cfg(target_os = "linux")]
fn unix_root_clean(script: &str, log: &dyn Fn(String)) {
    log("Asking for administrator password...".into());
    let child = Command::new("pkexec").args(["/bin/sh", "-c", script]).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
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
    #[cfg(unix)] // uses Linux tools or Unix-only file features
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
    fn read_only_go_module_cache_can_be_cleared() {
        let base = std::env::temp_dir().join(format!("aero-gomod-{}", std::process::id()));
        let pkg = base.join("github.com/x/y@v1.0.0");
        fs::create_dir_all(&pkg).unwrap();
        fs::write(pkg.join("go.mod"), "module y").unwrap();
        // Go marks files and folders read-only.
        for p in [pkg.join("go.mod"), pkg.clone()] {
            let mut perms = fs::metadata(&p).unwrap().permissions();
            perms.set_readonly(true);
            fs::set_permissions(&p, perms).unwrap();
        }
        make_writable(&base);
        assert_eq!(clear(&base, &[]), 0);
        assert!(base.exists() && fs::read_dir(&base).unwrap().next().is_none());
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
    #[cfg(target_os = "linux")] // copies /bin/sleep, which macOS code signing may refuse to run
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
    #[cfg(unix)] // uses Linux tools or Unix-only file features
    fn root_scripts_are_valid_shell() {
        for c in CATEGORIES {
            if let Some(s) = root_script(c.kind, 3) {
                let ok = Command::new("sh").args(["-n", "-c", &s]).status().unwrap().success();
                assert!(ok, "bad shell for {:?}", c.kind);
            }
        }
    }

    #[test]
    #[cfg(unix)]
    fn macos_scripts_are_valid_and_spare_current_logs() {
        for k in [Kind::OldLogs, Kind::SystemCrash, Kind::VarTmp] {
            let s = macos_root_script(k).unwrap();
            assert!(Command::new("sh").args(["-n", "-c", &s]).status().unwrap().success(), "bad shell for {k:?}");
        }
        let base = std::env::temp_dir().join(format!("aero-maclogs-{}", std::process::id()));
        fs::create_dir_all(base.join("asl")).unwrap();
        for f in ["system.log", "system.log.0.gz", "wifi.log.1.bz2", "install.log", "fsck_apfs.log.0", "asl/2026.asl"] {
            fs::write(base.join(f), "x").unwrap();
        }
        let script = macos_root_script(Kind::OldLogs).unwrap().replace("/private/var/log", base.to_str().unwrap());
        let out = Command::new("sh").args(["-c", &script]).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        assert!(text.contains("\n3 files removed"), "unexpected output: {text}");
        assert!(base.join("system.log").exists() && base.join("install.log").exists() && base.join("asl/2026.asl").exists());
        assert!(!base.join("system.log.0.gz").exists() && !base.join("wifi.log.1.bz2").exists() && !base.join("fsck_apfs.log.0").exists());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    #[cfg(target_os = "linux")] // GNU find syntax
    fn old_logs_script_works_with_gnu_find() {
        let base = std::env::temp_dir().join(format!("aero-logs-{}", std::process::id()));
        fs::create_dir_all(base.join("journal")).unwrap();
        for f in ["syslog", "syslog.1", "kern.log.2.gz", "Xorg.0.log", "Xorg.0.log.old", "journal/x.journal.1"] {
            fs::write(base.join(f), "x").unwrap();
        }
        let script = root_script(Kind::OldLogs, 3).unwrap().replace("/var/log", base.to_str().unwrap()).replace("; find ", "; /usr/bin/find ");
        let out = Command::new("sh").args(["-c", &script]).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
        assert!(text.contains("3 files removed"), "unexpected output: {text}");
        assert!(base.join("syslog").exists() && base.join("Xorg.0.log").exists() && base.join("journal/x.journal.1").exists());
        assert!(!base.join("syslog.1").exists() && !base.join("kern.log.2.gz").exists() && !base.join("Xorg.0.log.old").exists());
        fs::remove_dir_all(&base).unwrap();
    }
}
