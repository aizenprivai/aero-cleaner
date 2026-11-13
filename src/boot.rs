//! Boot time analysis (systemd-analyze), boot history and toggling services.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

/// Seconds spent in each boot stage.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct BootTimes {
    pub firmware: f32,
    pub loader: f32,
    pub kernel: f32,
    pub userspace: f32,
}

impl BootTimes {
    pub fn total(&self) -> f32 {
        self.firmware + self.loader + self.kernel + self.userspace
    }
}

pub struct Service {
    pub unit: String,
    pub description: String,
    pub seconds: f32,
    /// Output of `systemctl is-enabled` (enabled, disabled, static...).
    pub state: String,
    /// Part of the system; turning it off could break booting or the desktop.
    pub critical: bool,
}

impl Service {
    pub fn toggleable(&self) -> bool {
        !self.critical && self.unit.ends_with(".service") && (self.state == "enabled" || self.state == "disabled")
    }
}

fn run(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd).args(args).env("LC_ALL", "C").output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

/// Parses durations like "1min 2.345s", "14.130s" or "512ms" into seconds.
pub fn parse_duration(s: &str) -> f32 {
    s.split_whitespace()
        .map(|part| {
            let split = part.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(part.len());
            let n: f32 = part[..split].parse().unwrap_or(0.0);
            match &part[split..] {
                "min" => n * 60.0,
                "ms" => n / 1000.0,
                "us" | "µs" => n / 1_000_000.0,
                "h" => n * 3600.0,
                _ => n,
            }
        })
        .sum()
}

/// Parses "Startup finished in 14.130s (firmware) + 6.716s (loader) + ... = 32.434s".
pub fn parse_times(text: &str) -> Option<BootTimes> {
    let line = text.lines().find(|l| l.starts_with("Startup finished in"))?;
    let body = line.trim_start_matches("Startup finished in").split('=').next()?;
    let mut t = BootTimes::default();
    for part in body.split('+') {
        let part = part.trim();
        let (dur, label) = part.rsplit_once('(').map(|(d, l)| (d.trim(), l.trim_end_matches(')'))).unwrap_or((part, ""));
        let secs = parse_duration(dur);
        match label {
            "firmware" => t.firmware = secs,
            "loader" => t.loader = secs,
            "kernel" => t.kernel = secs,
            "userspace" => t.userspace = secs,
            _ => {}
        }
    }
    Some(t)
}

pub fn last_boot() -> Option<BootTimes> {
    parse_times(&run("systemd-analyze", &["time"]))
}

fn is_critical(unit: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "systemd-", "dbus", "NetworkManager.service", "polkit", "udisks2", "accounts-daemon", "lightdm", "gdm", "apparmor",
        "user@", "getty", "plymouth", "keyboard-setup", "console-setup", "wpa_supplicant", "upower", "power-profiles-daemon",
        "thermald", "gpu-manager", "switcheroo", "ufw", "kmod", "networkd-dispatcher", "rtkit", "avahi-daemon", "colord",
        "packagekit", "fwupd", "ubuntu-system-adjustments", "setvtrgb", "grub", "secureboot", "snapd.apparmor",
    ];
    PREFIXES.iter().any(|p| unit.starts_with(p))
}

/// The slowest units of the last boot, with their description and state.
pub fn slowest(limit: usize) -> Vec<Service> {
    let blame = run("systemd-analyze", &["blame", "--no-pager"]);
    let mut list: Vec<(String, f32)> = blame
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let (dur, unit) = l.rsplit_once(' ')?;
            Some((unit.to_string(), parse_duration(dur)))
        })
        .filter(|(u, _)| u.ends_with(".service"))
        .take(limit)
        .collect();
    list.retain(|(u, _)| !u.is_empty());
    let units: Vec<&str> = list.iter().map(|(u, _)| u.as_str()).collect();
    if units.is_empty() {
        return vec![];
    }
    // `is-enabled` prints one line per unit, in order.
    let mut args = vec!["is-enabled"];
    args.extend(&units);
    let states: Vec<String> = run("systemctl", &args).lines().map(str::to_string).collect();
    let mut args = vec!["show", "--property=Id,Description"];
    args.extend(&units);
    let descs = parse_descriptions(&run("systemctl", &args));
    list.into_iter()
        .enumerate()
        .map(|(i, (unit, seconds))| Service {
            critical: is_critical(&unit),
            description: descs.get(&unit).cloned().unwrap_or_default(),
            state: states.get(i).cloned().unwrap_or_else(|| "unknown".into()),
            unit,
            seconds,
        })
        // Only what really starts at boot: timer/socket-triggered jobs (like
        // update checks that run for minutes later on) would be misleading.
        .filter(|s| s.critical || matches!(s.state.as_str(), "enabled" | "disabled" | "enabled-runtime"))
        .collect()
}

/// Parses `systemctl show --property=Id,Description` blocks into a map.
fn parse_descriptions(text: &str) -> std::collections::HashMap<String, String> {
    text.split("\n\n")
        .filter_map(|block| {
            let get = |k: &str| block.lines().find_map(|l| l.strip_prefix(k)).map(str::to_string);
            Some((get("Id=")?, get("Description=").unwrap_or_default()))
        })
        .collect()
}

fn valid_unit(unit: &str) -> bool {
    !unit.is_empty() && unit.ends_with(".service") && unit.chars().all(|c| c.is_ascii_alphanumeric() || "@._-".contains(c))
}

/// Enables or disables a service at boot (asks for the password via pkexec).
pub fn set_enabled(unit: &str, enabled: bool) -> Result<(), String> {
    if !valid_unit(unit) || is_critical(unit) {
        return Err(format!("{unit} can't be changed here"));
    }
    let action = if enabled { "enable" } else { "disable" };
    let out = Command::new("pkexec").args(["/usr/bin/systemctl", action, "--", unit]).output().map_err(|e| e.to_string())?;
    match out.status.code() {
        Some(0) => Ok(()),
        Some(126 | 127) => Err("Password prompt was cancelled".into()),
        _ => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
    }
}

// ---------- boot history ----------

fn history_file() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    home.join(".local/share/aero-cleaner/boots.tsv")
}

/// Saves this boot's times once per boot (keyed by the kernel's boot id).
pub fn record(times: &BootTimes) {
    let Ok(id) = fs::read_to_string("/proc/sys/kernel/random/boot_id") else { return };
    let id = id.trim();
    let file = history_file();
    if fs::read_to_string(&file).unwrap_or_default().lines().any(|l| l.starts_with(id)) {
        return;
    }
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let _ = fs::create_dir_all(file.parent().unwrap());
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(file) {
        let _ = writeln!(f, "{id}\t{secs}\t{}\t{}\t{}\t{}", times.firmware, times.loader, times.kernel, times.userspace);
    }
}

/// Past boots, oldest first.
pub fn history() -> Vec<BootTimes> {
    fs::read_to_string(history_file())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let f: Vec<f32> = l.split('\t').skip(2).filter_map(|v| v.parse().ok()).collect();
            (f.len() == 4).then(|| BootTimes { firmware: f[0], loader: f[1], kernel: f[2], userspace: f[3] })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_systemd_analyze_output() {
        let t = parse_times("Startup finished in 14.130s (firmware) + 6.716s (loader) + 2.111s (kernel) + 9.476s (userspace) = 32.434s\ngraphical.target reached after 9.433s in userspace.").unwrap();
        assert_eq!(t, BootTimes { firmware: 14.13, loader: 6.716, kernel: 2.111, userspace: 9.476 });
        assert!((t.total() - 32.433).abs() < 0.01);
        let vm = parse_times("Startup finished in 1.2s (kernel) + 1min 3.5s (userspace) = 1min 4.7s").unwrap();
        assert_eq!((vm.firmware, vm.kernel), (0.0, 1.2));
        assert!((vm.userspace - 63.5).abs() < 0.001);
        assert!((parse_duration("512ms") - 0.512).abs() < 1e-6);
    }

    #[test]
    fn descriptions_match_their_unit() {
        let text = "Id=apt-daily-upgrade.service\nDescription=\n\nId=NetworkManager-wait-online.service\nDescription=Network Manager Wait Online\n";
        let d = parse_descriptions(text);
        assert_eq!(d["NetworkManager-wait-online.service"], "Network Manager Wait Online");
        assert_eq!(d["apt-daily-upgrade.service"], "");
    }

    #[test]
    fn refuses_bad_or_critical_units() {
        assert!(!valid_unit("foo; rm -rf /"));
        assert!(!valid_unit("cups.socket"));
        assert!(valid_unit("cups.service"));
        assert!(set_enabled("dbus.service", false).is_err());
        assert!(set_enabled("x.service; reboot", false).is_err());
    }
}
