//! macOS-only features: disk space, running apps, notifications, one
//! administrator prompt (through osascript), launchd agents for the
//! automation, startup agents and Homebrew.
//!
//! The text-handling parts (plist generation and parsing, `df` and
//! `launchctl` output) are plain functions so they are tested on every
//! system; only the commands they run need a Mac.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use crate::platform::home;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).env("LC_ALL", "C").stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The numeric user id, for launchctl's `gui/<uid>` domain.
fn uid() -> String {
    run("id", &["-u"]).map(|s| s.trim().to_string()).unwrap_or_else(|| "501".into())
}

// ---------- Disk and processes ----------

/// (total, available) bytes of the data volume. On APFS "/" is the sealed,
/// read-only system volume; user files live on /System/Volumes/Data.
pub fn disk_usage() -> Option<(u64, u64)> {
    ["/System/Volumes/Data", "/"].iter().find_map(|v| run("df", &["-k", "-P", v]).and_then(|t| parse_df(&t)))
}

/// Parses POSIX `df -k -P` output: "Filesystem 1024-blocks Used Available Capacity Mounted on".
fn parse_df(text: &str) -> Option<(u64, u64)> {
    let fields: Vec<&str> = text.lines().nth(1)?.split_whitespace().collect();
    let total: u64 = fields.get(1)?.parse().ok()?;
    let avail: u64 = fields.get(3)?.parse().ok()?;
    Some((total * 1024, avail * 1024))
}

/// Names of running programs, lowercase ("google chrome", "firefox"...).
pub fn running_processes() -> Vec<String> {
    run("ps", &["-axco", "comm="]).unwrap_or_default().lines().map(|l| l.trim().to_lowercase()).filter(|l| !l.is_empty()).collect()
}

// ---------- AppleScript ----------

/// Quotes text as an AppleScript string literal.
pub fn applescript_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn osascript(script: &str) -> Result<String, String> {
    let out = Command::new("osascript").args(["-e", script]).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        // -128 is "User canceled".
        Err(if err.contains("(-128)") { "cancelled".into() } else { err })
    }
}

pub fn notify(title: &str, body: &str) {
    let _ = osascript(&format!("display notification {} with title {}", applescript_string(body), applescript_string(title)));
}

/// The AppleScript that runs `script` as root after one password prompt.
fn admin_applescript(script: &str) -> String {
    // Without "altering line endings", output lines would come back joined with \r.
    format!("do shell script {} with administrator privileges without altering line endings", applescript_string(script))
}

/// Runs a shell script as root, asking for the administrator password once.
/// Returns its output, or Err("cancelled") if the prompt was dismissed.
pub fn run_as_admin(script: &str) -> Result<String, String> {
    osascript(&admin_applescript(script))
}

/// Empties the Trash through Finder, like "Empty Trash" in its menu.
/// (Reading ~/.Trash directly needs Full Disk Access; asking Finder doesn't.)
pub fn empty_trash() -> Result<(), String> {
    osascript("tell application \"Finder\" to empty trash").map(|_| ())
}

/// Lets the user pick a folder in the standard macOS dialog.
pub fn choose_folder(prompt: &str) -> Option<String> {
    let path = osascript(&format!("POSIX path of (choose folder with prompt {})", applescript_string(prompt))).ok()?;
    let path = path.trim();
    let path = if path.len() > 1 { path.trim_end_matches('/') } else { path };
    (!path.is_empty()).then(|| path.to_string())
}

/// Lets the user pick a file in the standard macOS dialog.
pub fn choose_file(prompt: &str) -> Option<String> {
    let path = osascript(&format!("POSIX path of (choose file with prompt {})", applescript_string(prompt))).ok()?;
    let path = path.trim();
    (!path.is_empty()).then(|| path.to_string())
}

/// The per-user cache folder (`/var/folders/…/C/`), home of the Quick Look thumbnails.
pub fn user_cache_dir() -> Option<PathBuf> {
    run("getconf", &["DARWIN_USER_CACHE_DIR"]).map(|s| PathBuf::from(s.trim())).filter(|p| p.is_absolute())
}

// ---------- Homebrew ----------

/// Path to `brew`. GUI apps don't get the shell's PATH, so look in the
/// standard places (Apple Silicon, then Intel).
pub fn brew() -> Option<PathBuf> {
    ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"].iter().map(PathBuf::from).find(|p| p.exists())
}

fn brew_cmd(args: &[&str]) -> Option<String> {
    let out = Command::new(brew()?)
        .args(args)
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        .env("HOMEBREW_NO_INSTALL_CLEANUP", "1")
        .stderr(Stdio::null())
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Homebrew's download cache.
pub fn brew_cache() -> PathBuf {
    std::env::var_os("HOMEBREW_CACHE").map(PathBuf::from).unwrap_or_else(|| home().join("Library/Caches/Homebrew"))
}

/// Formulae `brew autoremove` would remove.
pub fn brew_unused() -> Vec<String> {
    parse_autoremove(&brew_cmd(&["autoremove", "--dry-run"]).unwrap_or_default())
}

/// "==> Would autoremove 2 unneeded formulae:\nfoo\nbar" -> ["foo", "bar"].
fn parse_autoremove(text: &str) -> Vec<String> {
    text.lines().skip_while(|l| !l.starts_with("==> Would")).skip(1).take_while(|l| !l.starts_with("==>")).flat_map(|l| l.split_whitespace()).map(str::to_string).collect()
}

pub fn brew_autoremove() -> String {
    brew_cmd(&["autoremove"]).unwrap_or_default().lines().last().unwrap_or("done").to_string()
}

// ---------- Property lists ----------

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// The value of a top-level `key` in an XML plist: a string, the first
/// string of an array, or "true"/"false".
fn plist_value(xml: &str, key: &str) -> Option<String> {
    let rest = &xml[xml.find(&format!("<key>{key}</key>"))? + key.len() + 11..];
    let rest = rest.trim_start();
    if rest.starts_with("<true/>") {
        return Some("true".into());
    }
    if rest.starts_with("<false/>") {
        return Some("false".into());
    }
    let rest = if rest.starts_with("<array>") { &rest[rest.find("<string>")?..] } else { rest };
    let rest = rest.strip_prefix("<string>")?;
    Some(xml_unescape(&rest[..rest.find("</string>")?]))
}

/// Reads any plist (XML or binary) as XML.
fn plist_xml(path: &Path) -> Option<String> {
    let path = path.to_str()?;
    run("plutil", &["-convert", "xml1", "-o", "-", path])
}

// ---------- launchd agents (automation) ----------

fn agent_label(job: &str) -> String {
    format!("io.github.aero-cleaner.{}", job.trim_start_matches("aero-cleaner-"))
}

fn agent_path(job: &str) -> PathBuf {
    home().join("Library/LaunchAgents").join(format!("{}.plist", agent_label(job)))
}

/// A launchd agent that runs AeroClean with `args`: weekly (Monday noon,
/// caught up after sleep) or every hour.
pub fn agent_plist(job: &str, exe: &Path, args: &str, weekly: bool) -> String {
    let mut argv = vec![xml_escape(&exe.to_string_lossy())];
    argv.extend(args.split_whitespace().map(xml_escape));
    let argv: String = argv.iter().map(|a| format!("\t\t<string>{a}</string>\n")).collect();
    let schedule = if weekly {
        "\t<key>StartCalendarInterval</key>\n\t<dict>\n\t\t<key>Weekday</key>\n\t\t<integer>1</integer>\n\t\t<key>Hour</key>\n\t\t<integer>12</integer>\n\t\t<key>Minute</key>\n\t\t<integer>0</integer>\n\t</dict>\n"
    } else {
        "\t<key>StartInterval</key>\n\t<integer>3600</integer>\n"
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n\
         \t<key>Label</key>\n\t<string>{}</string>\n\
         \t<key>ProgramArguments</key>\n\t<array>\n{argv}\t</array>\n\
         {schedule}\
         \t<key>ProcessType</key>\n\t<string>Background</string>\n\
         \t<key>LowPriorityIO</key>\n\t<true/>\n\
         \t<key>Nice</key>\n\t<integer>10</integer>\n\
         </dict>\n</plist>\n",
        agent_label(job)
    )
}

fn launchctl(args: &[&str]) -> Result<(), String> {
    let out = Command::new("launchctl").args(args).output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

pub fn agent_enable(job: &str, args: &str, weekly: bool) -> Result<(), String> {
    let path = agent_path(job);
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(&path, agent_plist(job, &crate::platform::exe(), args, weekly)).map_err(|e| e.to_string())?;
    let domain = format!("gui/{}", uid());
    let p = path.to_string_lossy();
    // Reload if it was already loaded (e.g. the app moved).
    let _ = launchctl(&["bootout", &domain, &p]);
    launchctl(&["bootstrap", &domain, &p])
}

pub fn agent_disable(job: &str) -> Result<(), String> {
    let path = agent_path(job);
    let _ = launchctl(&["bootout", &format!("gui/{}", uid()), &path.to_string_lossy()]);
    match fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
        _ => Ok(()),
    }
}

pub fn agent_enabled(job: &str) -> bool {
    agent_path(job).exists()
}

// ---------- Startup agents ----------

const SYSTEM_AGENTS: &str = "/Library/LaunchAgents";

/// `launchctl print-disabled` lines: `"com.foo.bar" => disabled` (or
/// `=> true` on older macOS). Returns label -> enabled.
fn parse_print_disabled(text: &str) -> Vec<(String, bool)> {
    text.lines()
        .filter_map(|l| {
            let (label, state) = l.trim().split_once("=>")?;
            let label = label.trim().trim_matches('"').to_string();
            let enabled = match state.trim() {
                "disabled" | "true" => false,
                "enabled" | "false" => true,
                _ => return None,
            };
            Some((label, enabled))
        })
        .collect()
}

/// Agents that start at login: the user's (~/Library/LaunchAgents) and
/// those installed for all users (/Library/LaunchAgents). Apple's own
/// agents live in /System and aren't listed.
pub fn startup_entries() -> Vec<crate::tools::StartupEntry> {
    let overrides = parse_print_disabled(&run("launchctl", &["print-disabled", &format!("gui/{}", uid())]).unwrap_or_default());
    let mut out = Vec::new();
    for (dir, system) in [(home().join("Library/LaunchAgents"), false), (PathBuf::from(SYSTEM_AGENTS), true)] {
        let Ok(rd) = fs::read_dir(dir) else { continue };
        for e in rd.flatten() {
            let path = e.path();
            if path.extension().is_none_or(|x| x != "plist") {
                continue;
            }
            let Some(xml) = plist_xml(&path) else { continue };
            let Some(label) = plist_value(&xml, "Label") else { continue };
            if label.starts_with("io.github.aero-cleaner.") || out.iter().any(|x: &crate::tools::StartupEntry| x.file == label) {
                continue;
            }
            let program = plist_value(&xml, "Program").or_else(|| plist_value(&xml, "ProgramArguments")).unwrap_or_default();
            let disabled_key = plist_value(&xml, "Disabled").is_some_and(|v| v == "true");
            let enabled = overrides.iter().find(|(l, _)| *l == label).map(|(_, on)| *on).unwrap_or(!disabled_key);
            out.push(crate::tools::StartupEntry {
                name: friendly_name(&label, &program),
                comment: program,
                enabled,
                system,
                critical: label.starts_with("com.apple."),
                file: label,
            });
        }
    }
    out.sort_by_key(|e| (e.critical, e.name.to_lowercase()));
    out
}

/// "com.google.keystone.agent" + ".../GoogleSoftwareUpdateAgent" -> "GoogleSoftwareUpdateAgent".
fn friendly_name(label: &str, program: &str) -> String {
    let exe = Path::new(program).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if exe.is_empty() { label.to_string() } else { exe }
}

/// Turns an agent on or off for this user only: launchctl keeps the choice
/// in its own per-user override list, the plist file is never edited.
/// Takes effect at the next login.
pub fn set_startup(label: &str, enabled: bool) -> Result<(), String> {
    if !label.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) {
        return Err("invalid name".into());
    }
    launchctl(&[if enabled { "enable" } else { "disable" }, &format!("gui/{}/{label}", uid())])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_df() {
        let text = "Filesystem     1024-blocks      Used Available Capacity  Mounted on\n/dev/disk3s5     482797652 300000000 150000000    67%    /System/Volumes/Data\n";
        assert_eq!(parse_df(text), Some((482797652 * 1024, 150000000 * 1024)));
        assert_eq!(parse_df("garbage"), None);
    }

    #[test]
    fn applescript_strings_are_escaped() {
        assert_eq!(applescript_string(r#"say "hi" \ bye"#), r#""say \"hi\" \\ bye""#);
        let s = admin_applescript("echo \"a\"; find /x -name '*.gz'");
        assert!(s.starts_with("do shell script \"echo \\\"a\\\"; find /x -name '*.gz'\" with administrator privileges"), "{s}");
    }

    #[test]
    fn agent_plist_round_trips() {
        let xml = agent_plist("aero-cleaner-clean", Path::new("/Applications/Aero & Clean.app/Contents/MacOS/aero-cleaner"), "clean --auto", true);
        assert_eq!(plist_value(&xml, "Label").as_deref(), Some("io.github.aero-cleaner.clean"));
        assert_eq!(plist_value(&xml, "ProgramArguments").as_deref(), Some("/Applications/Aero & Clean.app/Contents/MacOS/aero-cleaner"));
        assert!(xml.contains("<string>clean</string>") && xml.contains("<string>--auto</string>"));
        assert!(xml.contains("StartCalendarInterval") && !xml.contains("StartInterval<"));
        assert_eq!(plist_value(&xml, "LowPriorityIO").as_deref(), Some("true"));
        let hourly = agent_plist("aero-cleaner-disk", Path::new("/x"), "check-disk", false);
        assert!(hourly.contains("<key>StartInterval</key>\n\t<integer>3600</integer>"));
        // Valid XML for plutil, when we're on a Mac.
        if Path::new("/usr/bin/plutil").exists() && cfg!(target_os = "macos") {
            let f = std::env::temp_dir().join(format!("aero-plist-{}.plist", std::process::id()));
            fs::write(&f, &xml).unwrap();
            let ok = Command::new("plutil").arg("-lint").arg(&f).status().unwrap().success();
            fs::remove_file(&f).unwrap();
            assert!(ok, "plutil rejected the agent");
        }
    }

    #[test]
    fn parses_launchctl_and_brew_output() {
        let text = "disabled services = {\n\t\"com.google.keystone.agent\" => disabled\n\t\"com.foo\" => enabled\n\t\"com.old\" => true\n}\n";
        assert_eq!(parse_print_disabled(text), vec![("com.google.keystone.agent".into(), false), ("com.foo".into(), true), ("com.old".into(), false)]);
        let brew = "==> Would autoremove 2 unneeded formulae:\nlibfoo\nlibbar\n";
        assert_eq!(parse_autoremove(brew), vec!["libfoo", "libbar"]);
        assert!(parse_autoremove("").is_empty());
        assert_eq!(friendly_name("com.x.agent", "/Library/X/GoogleSoftwareUpdateAgent"), "GoogleSoftwareUpdateAgent");
        assert_eq!(friendly_name("com.x.agent", ""), "com.x.agent");
    }
}
