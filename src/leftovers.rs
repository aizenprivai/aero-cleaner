//! Leftover app data: folders apps leave in your profile after they're
//! uninstalled. It's a guess, so it's careful: a folder is listed only when
//! nothing installed matches its name, it isn't a desktop/system folder, and
//! nothing inside changed for `MIN_AGE_DAYS`. Folders go to the Trash (so
//! they can be restored), and wrong guesses can be hidden.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime};

pub const MIN_AGE_DAYS: u64 = 90;

#[derive(Clone, Debug, PartialEq)]
pub struct Leftover {
    pub path: PathBuf,
    pub size: u64,
    /// Newest change of anything inside.
    pub last_used: Option<SystemTime>,
}

/// What's installed, as normalized names ("google-chrome" -> "googlechrome")
/// and name parts, plus exact ids (Flatpak, bundle ids) in lowercase.
#[derive(Default, Debug)]
pub struct Installed {
    pub names: HashSet<String>,
    pub ids: HashSet<String>,
}

/// Words that say nothing about which app it is.
const NOISE: &[&str] =
    &["org", "com", "io", "net", "app", "apps", "the", "software", "inc", "ltd", "llc", "gmbh", "corp", "corporation", "desktop", "x64", "x86", "bit", "data", "user", "client"];

fn normalize(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// The whole name plus its meaningful parts.
fn tokens(s: &str) -> Vec<String> {
    let mut v = vec![normalize(s)];
    v.extend(s.split(|c: char| !c.is_alphanumeric()).map(normalize).filter(|p| p.len() >= 3 && !NOISE.contains(&p.as_str())));
    v.retain(|t| !t.is_empty());
    v
}

impl Installed {
    pub fn add(&mut self, name: &str) {
        self.names.extend(tokens(name));
    }

    #[cfg_attr(windows, allow(dead_code))]
    pub fn add_id(&mut self, id: &str) {
        self.ids.insert(id.to_lowercase());
        self.add(id);
    }

    /// Does anything installed look like it owns a folder called `folder`?
    pub fn owns(&self, folder: &str) -> bool {
        let f = folder.trim_start_matches('.');
        if self.ids.contains(&f.to_lowercase()) {
            return true;
        }
        let whole = normalize(f);
        if whole.is_empty() || self.names.contains(&whole) {
            return true;
        }
        // A meaningful part of the folder name is an installed name ("Code - OSS" -> "code").
        if tokens(f).iter().skip(1).any(|p| p.len() >= 4 && self.names.contains(p)) {
            return true;
        }
        // An installed name is inside the folder name ("BraveSoftware" has "brave").
        self.names.iter().any(|n| n.len() >= 5 && whole.contains(n.as_str()))
    }
}

/// Desktop, system and well-known shared folders that never count as leftovers.
fn is_system(name: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "gtk",
        "gnome",
        "kde",
        "plasma",
        "xfce",
        "cinnamon",
        "mate",
        "lxqt",
        "lxde",
        "pulse",
        "pipewire",
        "wireplumber",
        "dconf",
        "ibus",
        "fcitx",
        "systemd",
        "tracker",
        "gvfs",
        "evolution",
        "keyrings",
        "flatpak",
        "snap",
        "xdg",
        "fontconfig",
        "fonts",
        "icons",
        "themes",
        "mime",
        "menus",
        "applications",
        "autostart",
        "sounds",
        "trash",
        "recently",
        "user-dirs",
        "environment",
        "session",
        "desktop-directories",
        "backgrounds",
        "nemo",
        "nautilus",
        "caja",
        "thunar",
        "mint",
        "aero-cleaner",
        "aeroclean",
        "containers",
        "dbus",
        "geoclue",
        "speech",
        "baloo",
        "akonadi",
        "kwallet",
        "zeitgeist",
        "folks",
        "telepathy",
        "ubuntu",
        "com.apple",
        "apple",
        "icloud",
        "cloudkit",
        "clouddocs",
        "fileprovider",
        "mobilesync",
        "callhistory",
        "knowledge",
        "dock",
        "crashreporter",
        "microsoft",
        "packages",
        "temp",
        "programs",
        "connecteddevicesplatform",
        "crashdumps",
        "d3dscache",
        "comms",
        "virtualstore",
        "publishers",
        "placeholdertilelogofolder",
        "nvidia",
        "amd",
        "intel",
        "windows",
        "identities",
        "local",
        "low",
        "history",
        "logs",
        "cache",
        "caches",
    ];
    let n = name.trim_start_matches('.').to_lowercase();
    PREFIXES.iter().any(|p| n.starts_with(p))
}

/// Size and newest modification time of everything under `p` (no symlinks).
fn size_and_newest(p: &Path) -> (u64, Option<SystemTime>) {
    let Ok(md) = fs::symlink_metadata(p) else { return (0, None) };
    let mut newest = md.modified().ok();
    if !md.is_dir() {
        return (if md.is_file() { md.len() } else { 0 }, newest);
    }
    let mut size = 0;
    if let Ok(rd) = fs::read_dir(p) {
        for e in rd.flatten() {
            let (s, n) = size_and_newest(&e.path());
            size += s;
            newest = newest.max(n);
        }
    }
    (size, newest)
}

/// Leftovers among the folders directly inside `roots`. Folders in
/// `exact_roots` are named by exact app ids (Flatpak's ~/.var/app), so only
/// an exact id match counts as installed there.
pub fn find_in(roots: &[PathBuf], exact_roots: &[PathBuf], installed: &Installed, hidden: &[PathBuf], now: SystemTime) -> Vec<Leftover> {
    let cutoff = now.checked_sub(Duration::from_secs(MIN_AGE_DAYS * 86_400)).unwrap_or(SystemTime::UNIX_EPOCH);
    let mut out = Vec::new();
    for root in roots.iter().chain(exact_roots) {
        let exact = exact_roots.contains(root);
        let Ok(rd) = fs::read_dir(root) else { continue };
        for e in rd.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            let owned = if exact { installed.ids.contains(&name.to_lowercase()) } else { is_system(&name) || installed.owns(&name) };
            if !e.file_type().is_ok_and(|t| t.is_dir()) || owned || hidden.contains(&path) {
                continue;
            }
            let (size, last_used) = size_and_newest(&path);
            if size >= 100_000 && last_used.is_some_and(|t| t <= cutoff) {
                out.push(Leftover { path, size, last_used });
            }
        }
    }
    out.sort_by_key(|l| std::cmp::Reverse(l.size));
    out
}

#[cfg_attr(windows, allow(dead_code))]
fn run(cmd: &str, args: &[&str]) -> String {
    let Some(bin) = crate::platform::find_tool(cmd) else { return String::new() };
    crate::platform::command(bin).args(args).stderr(Stdio::null()).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

/// Commands on the PATH.
fn add_path_commands(installed: &mut Installed) {
    // Inside the Flatpak, PATH is the sandbox's: look at the host's folders instead.
    let dirs: Vec<PathBuf> = if crate::platform::in_flatpak() {
        let home = crate::platform::home();
        ["/usr/bin", "/usr/sbin", "/usr/local/bin", "/usr/games"].iter().map(|d| crate::platform::host_path(d)).chain([home.join(".local/bin"), home.join("bin")]).collect()
    } else {
        std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default()
    };
    {
        for dir in dirs {
            if let Ok(rd) = fs::read_dir(dir) {
                for e in rd.flatten() {
                    let n = e.file_name().to_string_lossy().to_string();
                    installed.add(n.trim_end_matches(".exe"));
                }
            }
        }
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn linux_installed(home: &Path) -> Installed {
    let mut ins = Installed::default();
    let dirs = [
        crate::platform::host_path("/usr/share/applications"),
        crate::platform::host_path("/usr/local/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        PathBuf::from("/var/lib/snapd/desktop/applications"),
        home.join(".local/share/applications"),
        home.join(".local/share/flatpak/exports/share/applications"),
    ];
    // (file name, text) of every launcher.
    let mut launchers: Vec<(String, String)> = vec![];
    for d in dirs {
        let Ok(rd) = fs::read_dir(d) else { continue };
        for e in rd.flatten() {
            launchers.push((e.file_name().to_string_lossy().to_string(), fs::read_to_string(e.path()).unwrap_or_default()));
        }
    }
    // The sandbox can't see /var: read the system-wide Flatpak and Snap launchers on the host.
    if crate::platform::in_flatpak() {
        let script = "for f in /var/lib/flatpak/exports/share/applications/*.desktop /var/lib/snapd/desktop/applications/*.desktop; do [ -f \"$f\" ] && printf '\\036%s\\n' \"${f##*/}\" && cat \"$f\"; done";
        let out = run("sh", &["-c", script]);
        for chunk in out.split('\u{1e}').skip(1) {
            if let Some((name, text)) = chunk.split_once('\n') {
                launchers.push((name.to_string(), text.to_string()));
            }
        }
    }
    for (file, text) in launchers {
        let Some(stem) = file.strip_suffix(".desktop") else { continue };
        ins.add_id(stem);
        for line in text.lines() {
            for key in ["Name=", "StartupWMClass=", "Icon="] {
                if let Some(v) = line.strip_prefix(key) {
                    ins.add(v);
                }
            }
            if let Some(cmd) = line.strip_prefix("Exec=").and_then(|v| v.split_whitespace().next()) {
                ins.add(Path::new(cmd).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default().as_str());
            }
        }
    }
    add_path_commands(&mut ins);
    for pkg in run("dpkg-query", &["-W", "-f=${Package}\\n"]).lines() {
        ins.add(pkg);
    }
    for id in run("flatpak", &["list", "--columns=application"]).lines() {
        ins.add_id(id.trim());
    }
    for line in run("snap", &["list"]).lines().skip(1) {
        if let Some(name) = line.split_whitespace().next() {
            ins.add(name);
        }
    }
    ins
}

#[cfg(target_os = "macos")]
fn macos_installed(home: &Path) -> Installed {
    let mut ins = Installed::default();
    for dir in [PathBuf::from("/Applications"), PathBuf::from("/System/Applications"), PathBuf::from("/Applications/Utilities"), home.join("Applications")] {
        let Ok(rd) = fs::read_dir(dir) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let Some(app) = name.strip_suffix(".app") else { continue };
            ins.add(app);
            let plist = e.path().join("Contents/Info.plist");
            let id = run("plutil", &["-extract", "CFBundleIdentifier", "raw", "-o", "-", &plist.to_string_lossy()]);
            if !id.trim().is_empty() {
                ins.add_id(id.trim());
            }
        }
    }
    add_path_commands(&mut ins);
    ins
}

/// Leftovers in this user's profile, minus the ones hidden before.
pub fn find() -> Vec<Leftover> {
    let home = crate::platform::home();
    let hidden = load_hidden();
    let now = SystemTime::now();
    #[cfg(windows)]
    {
        let mut ins = Installed::default();
        for name in crate::windows::installed_programs() {
            ins.add(&name);
        }
        for var in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(pf) = std::env::var_os(var).and_then(|p| fs::read_dir(p).ok()) {
                pf.flatten().for_each(|e| ins.add(&e.file_name().to_string_lossy()));
            }
        }
        add_path_commands(&mut ins);
        let _ = home;
        find_in(&[crate::platform::roaming_app_data(), crate::platform::local_app_data()], &[], &ins, &hidden, now)
    }
    #[cfg(target_os = "macos")]
    {
        let ins = macos_installed(&home);
        find_in(&[home.join("Library/Application Support")], &[home.join("Library/Containers")], &ins, &hidden, now)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let ins = linux_installed(&home);
        // Without flatpak itself, ~/.var/app can't be judged.
        let flatpak = if crate::platform::find_tool("flatpak").is_some() { vec![home.join(".var/app")] } else { vec![] };
        find_in(&[home.join(".config"), home.join(".local/share")], &flatpak, &ins, &hidden, now)
    }
}

// ---------- Folders the user said aren't leftovers ----------

fn hidden_file() -> PathBuf {
    crate::platform::data_dir().join("leftovers-hidden")
}

pub fn load_hidden() -> Vec<PathBuf> {
    fs::read_to_string(hidden_file()).unwrap_or_default().lines().filter(|l| !l.is_empty()).map(PathBuf::from).collect()
}

pub fn hide(path: &Path) {
    let mut v = load_hidden();
    if !v.iter().any(|p| p == path) {
        v.push(path.to_path_buf());
    }
    let _ = fs::create_dir_all(crate::platform::data_dir());
    let _ = fs::write(hidden_file(), v.iter().map(|p| format!("{}\n", p.display())).collect::<String>());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed(names: &[&str], ids: &[&str]) -> Installed {
        let mut i = Installed::default();
        names.iter().for_each(|n| i.add(n));
        ids.iter().for_each(|n| i.add_id(n));
        i
    }

    /// Why each profile folder is or isn't a leftover, on this machine:
    /// `cargo test explain_leftovers -- --ignored --nocapture`
    #[test]
    #[ignore]
    #[cfg(target_os = "linux")]
    fn explain_leftovers() {
        let home = crate::platform::home();
        let ins = linux_installed(&home);
        let cutoff = SystemTime::now() - Duration::from_secs(MIN_AGE_DAYS * 86_400);
        println!("{} installed names, {} ids", ins.names.len(), ins.ids.len());
        for root in [home.join(".config"), home.join(".local/share"), home.join(".var/app")] {
            let Ok(rd) = fs::read_dir(&root) else { continue };
            for e in rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())) {
                let name = e.file_name().to_string_lossy().to_string();
                let why = if is_system(&name) {
                    "system".to_string()
                } else if ins.owns(&name) {
                    let whole = normalize(name.trim_start_matches('.'));
                    let hit = ins.names.iter().filter(|n| n.len() >= 5 && whole.contains(n.as_str())).take(3).cloned().collect::<Vec<_>>();
                    format!("installed {hit:?}")
                } else {
                    let (size, newest) = size_and_newest(&e.path());
                    format!("CANDIDATE size={size} old={}", newest.is_some_and(|t| t <= cutoff))
                };
                println!("{:<45} {why}", e.path().strip_prefix(&home).unwrap().display());
            }
        }
    }

    #[test]
    fn matches_folders_to_installed_apps() {
        let ins = installed(&["google-chrome", "brave-browser", "code", "Spotify"], &["org.gimp.GIMP"]);
        assert!(ins.owns("google-chrome"));
        assert!(ins.owns("BraveSoftware"), "vendor folder containing an app name");
        assert!(ins.owns("Code - OSS") && ins.owns("Code"));
        assert!(ins.owns("spotify"), "case doesn't matter");
        assert!(ins.owns("org.gimp.GIMP") && ins.owns("GIMP"), "flatpak id and its parts");
        assert!(!ins.owns("discord"), "not installed");
        assert!(!ins.owns("Slack"));
        // Short names don't match by accident: "app" is noise, "co" too short.
        assert!(!installed(&["co", "app"], &[]).owns("Coolapp"));
    }

    #[test]
    fn lists_only_old_unowned_non_system_folders() {
        let home = std::env::temp_dir().join(format!("aero-left-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        let cfg = home.join(".config");
        for d in ["discord/Cache", "google-chrome", "gtk-3.0", "tiny", "hidden-one"] {
            fs::create_dir_all(cfg.join(d)).unwrap();
        }
        fs::write(cfg.join("discord/Cache/data"), vec![0u8; 200_000]).unwrap();
        fs::write(cfg.join("google-chrome/Prefs"), vec![0u8; 200_000]).unwrap();
        fs::write(cfg.join("gtk-3.0/settings.ini"), vec![0u8; 200_000]).unwrap();
        fs::write(cfg.join("tiny/x"), vec![0u8; 10]).unwrap();
        fs::write(cfg.join("hidden-one/x"), vec![0u8; 200_000]).unwrap();
        fs::write(cfg.join("loose-file.conf"), vec![0u8; 200_000]).unwrap();
        let ins = installed(&["google-chrome"], &[]);
        let hidden = vec![cfg.join("hidden-one")];

        assert!(find_in(std::slice::from_ref(&cfg), &[], &ins, &hidden, SystemTime::now()).is_empty(), "recently used: not listed");
        let later = SystemTime::now() + Duration::from_secs((MIN_AGE_DAYS + 1) * 86_400);
        let found = find_in(std::slice::from_ref(&cfg), &[], &ins, &hidden, later);
        assert_eq!(found.iter().map(|l| l.path.clone()).collect::<Vec<_>>(), vec![cfg.join("discord")], "installed, system, tiny, hidden and files skipped");
        assert_eq!(found[0].size, 200_000);

        // Flatpak data: only the exact app id counts, not a similar name.
        let var = home.join(".var/app");
        for id in ["org.mozilla.firefox", "com.spotify.Client"] {
            fs::create_dir_all(var.join(id)).unwrap();
            fs::write(var.join(id).join("x"), vec![0u8; 200_000]).unwrap();
        }
        let ins = installed(&["firefox"], &["com.spotify.Client"]);
        let found = find_in(&[], std::slice::from_ref(&var), &ins, &[], later);
        assert_eq!(found.iter().map(|l| l.path.clone()).collect::<Vec<_>>(), vec![var.join("org.mozilla.firefox")], "a deb Firefox doesn't own the Flatpak's data");
        fs::remove_dir_all(&home).unwrap();
    }
}
