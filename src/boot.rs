//! Boot time analysis, boot history and the slowest services. Linux reads
//! systemd-analyze (and can toggle services); Windows reads the boot
//! timings it records itself in the Diagnostics-Performance event log.

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
#[cfg_attr(windows, allow(dead_code))]
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
#[cfg_attr(windows, allow(dead_code))]
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

/// The last boot's times, with an id that's unique per boot (for the history).
pub fn last_boot() -> Option<(BootTimes, String)> {
    #[cfg(windows)]
    {
        let events = windows_events("EventID=100", 1);
        let e = events.first()?;
        let id = e.get("BootStartTime").or_else(|| e.get("SystemBootInstance"))?.clone();
        Some((times_from_event(e, crate::windows::firmware_ms()), id))
    }
    #[cfg(not(windows))]
    {
        let times = parse_times(&run("systemd-analyze", &["time"]))?;
        let id = fs::read_to_string("/proc/sys/kernel/random/boot_id").map(|s| s.trim().to_string()).unwrap_or_default();
        Some((times, id))
    }
}

// ---------- Windows boot events ----------

/// One event from `wevtutil qe /f:xml`: its EventID and `<Data Name='x'>` fields.
#[cfg_attr(not(windows), allow(dead_code))]
type Event = std::collections::HashMap<String, String>;

/// Splits wevtutil's XML into events. Values are plain numbers and names,
/// so a small scanner is enough (and is tested on every system).
#[cfg_attr(not(windows), allow(dead_code))]
fn parse_events(xml: &str) -> Vec<Event> {
    xml.split("<Event ")
        .skip(1)
        .map(|block| {
            let mut ev = Event::new();
            if let Some(id) = block.split("<EventID").nth(1).and_then(|r| r.split('>').nth(1)).and_then(|r| r.split('<').next()) {
                ev.insert("EventID".into(), id.trim().to_string());
            }
            for part in block.split("<Data Name=").skip(1) {
                let quote = part.chars().next().unwrap_or('\'');
                let Some((name, rest)) = part[1..].split_once(quote) else { continue };
                let value = rest.strip_prefix('>').and_then(|r| r.split("</Data>").next()).unwrap_or("");
                ev.insert(name.to_string(), value.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&apos;", "'").replace("&quot;", "\""));
            }
            ev
        })
        .collect()
}

#[cfg_attr(not(windows), allow(dead_code))]
fn ms(ev: &Event, key: &str) -> f32 {
    ev.get(key).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) / 1000.0
}

/// Event 100: firmware (from the registry), then kernel and drivers, then
/// everything until the desktop is ready ("main path").
#[cfg_attr(not(windows), allow(dead_code))]
fn times_from_event(ev: &Event, firmware_ms: Option<u32>) -> BootTimes {
    let main = ms(ev, "MainPathBootTime");
    let kernel = (ms(ev, "BootKernelInitTime") + ms(ev, "BootDriverInitTime") + ms(ev, "BootDevicesInitTime")).min(main);
    BootTimes { firmware: firmware_ms.unwrap_or(0) as f32 / 1000.0, loader: 0.0, kernel, userspace: main - kernel }
}

/// Events 101–103: apps, drivers and services Windows says slowed a boot.
#[cfg_attr(not(windows), allow(dead_code))]
fn slow_items(events: &[Event], n: usize) -> Vec<Service> {
    let mut best: Vec<Service> = Vec::new();
    for e in events.iter().filter(|e| matches!(e.get("EventID").map(String::as_str), Some("101" | "102" | "103"))) {
        let name = e.get("FriendlyName").filter(|f| !f.is_empty()).or_else(|| e.get("Name")).cloned().unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        let seconds = ms(e, "TotalTime");
        match best.iter_mut().find(|s| s.unit == name) {
            Some(s) => s.seconds = s.seconds.max(seconds),
            None => best.push(Service { unit: name, description: e.get("Name").cloned().unwrap_or_default(), seconds, state: "windows".into(), critical: false }),
        }
    }
    best.sort_by(|a, b| b.seconds.total_cmp(&a.seconds));
    best.truncate(n);
    best
}

#[cfg(windows)]
fn windows_events(query: &str, count: u32) -> Vec<Event> {
    let out = crate::platform::command("wevtutil")
        .args(["qe", "Microsoft-Windows-Diagnostics-Performance/Operational", &format!("/q:*[System[({query})]]"), &format!("/c:{count}"), "/rd:true", "/f:xml"])
        .output();
    out.map(|o| parse_events(&String::from_utf8_lossy(&o.stdout))).unwrap_or_default()
}

pub(crate) fn is_critical(unit: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "systemd-",
        "dbus",
        "NetworkManager.service",
        "polkit",
        "udisks2",
        "accounts-daemon",
        "lightdm",
        "gdm",
        "apparmor",
        "user@",
        "getty",
        "plymouth",
        "keyboard-setup",
        "console-setup",
        "wpa_supplicant",
        "upower",
        "power-profiles-daemon",
        "thermald",
        "gpu-manager",
        "switcheroo",
        "ufw",
        "kmod",
        "networkd-dispatcher",
        "rtkit",
        "avahi-daemon",
        "colord",
        "packagekit",
        "fwupd",
        "ubuntu-system-adjustments",
        "setvtrgb",
        "grub",
        "secureboot",
        "snapd.apparmor",
    ];
    PREFIXES.iter().any(|p| unit.starts_with(p))
}

/// The slowest units of the last boot, with their description and state.
pub fn slowest(limit: usize) -> Vec<Service> {
    #[cfg(windows)]
    return slow_items(&windows_events("EventID=101 or EventID=102 or EventID=103", 60), limit);
    #[allow(unreachable_code)]
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

pub(crate) fn valid_unit(unit: &str) -> bool {
    !unit.is_empty() && unit.ends_with(".service") && unit.chars().all(|c| c.is_ascii_alphanumeric() || "@._-".contains(c))
}

/// Enables or disables a service at boot (asks for the password via pkexec).
pub fn set_enabled(unit: &str, enabled: bool) -> Result<(), String> {
    if !valid_unit(unit) || is_critical(unit) {
        return Err(format!("{unit} can't be changed here"));
    }
    let action = if enabled { "enable" } else { "disable" };
    #[cfg(target_os = "linux")]
    if crate::helper::installed() {
        return crate::helper::call(&format!("service-{action}"), unit).map(|_| ()).map_err(|e| if e == "cancelled" { "Password prompt was cancelled".into() } else { e });
    }
    let out = Command::new("pkexec").args(["/usr/bin/systemctl", action, "--", unit]).output().map_err(|e| e.to_string())?;
    match out.status.code() {
        Some(0) => Ok(()),
        Some(126 | 127) => Err("Password prompt was cancelled".into()),
        _ => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
    }
}

// ---------- boot history ----------

fn history_file() -> PathBuf {
    crate::platform::data_dir().join("boots.tsv")
}

/// Saves this boot's times once per boot (keyed by the boot's id).
pub fn record(times: &BootTimes, id: &str) {
    if id.is_empty() || id.contains(['\t', '\n']) {
        return;
    }
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
    fn reads_windows_boot_events() {
        let xml = "<Events><Event xmlns='http://schemas.microsoft.com/win/2004/08/events/event'><System><EventID>100</EventID></System><EventData>\
            <Data Name='BootStartTime'>2026-11-15T08:00:00.5Z</Data><Data Name='MainPathBootTime'>21000</Data>\
            <Data Name='BootKernelInitTime'>900</Data><Data Name='BootDriverInitTime'>2100</Data><Data Name='BootDevicesInitTime'>1000</Data></EventData></Event>\
            <Event xmlns='x'><System><EventID>103</EventID></System><EventData><Data Name='Name'>Spooler</Data><Data Name='FriendlyName'>Print Spooler</Data><Data Name='TotalTime'>3500</Data></EventData></Event>\
            <Event xmlns='x'><System><EventID>101</EventID></System><EventData><Data Name='Name'>OneDrive.exe</Data><Data Name='FriendlyName'>Microsoft OneDrive</Data><Data Name='TotalTime'>8000</Data></EventData></Event>\
            <Event xmlns='x'><System><EventID>103</EventID></System><EventData><Data Name='Name'>Spooler</Data><Data Name='FriendlyName'>Print Spooler</Data><Data Name='TotalTime'>4200</Data></EventData></Event></Events>";
        let events = parse_events(xml);
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].get("EventID").map(String::as_str), Some("100"));
        let t = times_from_event(&events[0], Some(7500));
        assert_eq!(t, BootTimes { firmware: 7.5, loader: 0.0, kernel: 4.0, userspace: 17.0 });
        let slow = slow_items(&events, 10);
        let names: Vec<(&str, f32)> = slow.iter().map(|s| (s.unit.as_str(), s.seconds)).collect();
        assert_eq!(names, vec![("Microsoft OneDrive", 8.0), ("Print Spooler", 4.2)], "sorted, one row per item, longest time kept");
        assert!(slow.iter().all(|s| !s.toggleable()), "read-only on Windows");
    }

    #[test]
    fn parses_systemd_analyze_output() {
        let t = parse_times(
            "Startup finished in 14.130s (firmware) + 6.716s (loader) + 2.111s (kernel) + 9.476s (userspace) = 32.434s\ngraphical.target reached after 9.433s in userspace.",
        )
        .unwrap();
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
