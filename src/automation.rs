//! Background features: the weekly automatic clean, the low-disk alert,
//! desktop notifications and what runs them: systemd user timers on Linux,
//! Task Scheduler on Windows, launchd agents on macOS.

use crate::cleaner::{self, Kind, format_bytes};
use crate::settings::{self, Settings};
use crate::tools;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use crate::platform::{exe, home};

/// Shows a desktop notification. With `action`, waits for a click and
/// returns true if the action button was pressed (Linux only; Windows toasts
/// and macOS notifications from a background task can't report clicks back).
pub fn notify(title: &str, body: &str, action: Option<&str>) -> bool {
    #[cfg(target_os = "macos")]
    {
        let _ = action;
        crate::macos::notify(title, body);
        return false;
    }
    #[cfg(windows)]
    {
        let _ = action;
        crate::windows::toast(title, body);
        return false;
    }
    #[allow(unreachable_code)]
    let mut cmd = Command::new("notify-send");
    cmd.args(["--app-name=AeroClean", "--icon=aero-cleaner", title, body]);
    if let Some(a) = action {
        cmd.arg(format!("--action=open={a}"));
    }
    match cmd.stderr(Stdio::null()).output() {
        Ok(out) => String::from_utf8_lossy(&out.stdout).trim() == "open",
        Err(_) => false,
    }
}

/// Opens the main window in its own systemd scope, so it outlives the
/// short-lived timer service that launched it.
pub fn launch_gui(args: &[&str]) {
    if !crate::platform::IS_LINUX {
        let _ = Command::new(exe()).args(args).spawn();
        return;
    }
    let ok =
        Command::new("systemd-run").args(["--user", "--collect", "--quiet"]).arg(exe()).args(args).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
    if !ok {
        let _ = Command::new(exe()).args(args).spawn();
    }
}

/// What the automatic clean touches: only user-level, password-free items.
pub fn auto_kinds(settings: &Settings, browsers_open: bool) -> Vec<Kind> {
    let mut kinds = vec![Kind::Thumbnails];
    if !browsers_open {
        kinds.push(Kind::AppCache);
    }
    if settings.auto_trash {
        kinds.push(Kind::Trash);
    }
    kinds
}

/// Runs the automatic clean, records it in the history and returns a summary.
pub fn auto_clean(settings: &Settings) -> String {
    let browsers = cleaner::running_browsers();
    let kinds = auto_kinds(settings, !browsers.is_empty());
    let measure = |kinds: &[Kind]| -> u64 { kinds.iter().filter_map(|k| cleaner::scan(*k, &settings.exclusions).size).sum() };
    let before = measure(&kinds);
    cleaner::clean(&kinds, &settings.exclusions, settings.journal_days, &|_| {});
    let freed = before.saturating_sub(measure(&kinds));
    let names: Vec<&str> = cleaner::CATEGORIES.iter().filter(|c| kinds.contains(&c.kind)).map(|c| c.name).collect();
    settings::record(freed, &format!("Automatic: {}", names.join(", ")));
    let mut summary = format!("Freed {} ({})", format_bytes(freed), names.join(", "));
    if !browsers.is_empty() {
        summary.push_str(&format!(". Browser cache skipped because {} is open.", browsers.join(" and ")));
    }
    summary
}

fn warning_stamp() -> PathBuf {
    crate::platform::data_dir().join("last-disk-warning")
}

/// True when free space is below the threshold and no warning was shown in
/// the last 24 hours.
pub fn disk_is_low(settings: &Settings) -> Option<(u64, u64)> {
    let (total, avail) = tools::disk_usage()?;
    let low = avail * 100 < total * settings.low_disk_percent as u64;
    let recently_warned =
        fs::metadata(warning_stamp()).and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok()).is_some_and(|age| age < Duration::from_secs(86400));
    (low && !recently_warned).then_some((total, avail))
}

/// The hourly job: records free space, takes today's folder snapshot if
/// there isn't one yet, and warns when the disk is almost full.
pub fn check_disk(settings: &Settings) {
    use crate::diskhistory;
    if let Some((total, avail)) = tools::disk_usage() {
        diskhistory::record(total, avail);
    }
    let today = diskhistory::now() / 86400;
    if !diskhistory::has_snapshot(today) {
        let tree = crate::diskmap::scan(&home(), 20_000_000, &|_| {});
        diskhistory::save_snapshot(&diskhistory::snapshot_from_tree(&tree, today));
    }
    let Some((total, avail)) = disk_is_low(settings) else { return };
    let _ = fs::create_dir_all(warning_stamp().parent().unwrap());
    let _ = fs::write(warning_stamp(), "");
    let mut body = crate::i18n::tr(format!("Only {} free of {}. AeroClean can help you make room.", format_bytes(avail), format_bytes(total)));
    if let Some(line) = diskhistory::forecast_sentence(&diskhistory::forecast(&diskhistory::load(), diskhistory::now())) {
        body = format!("{} {body}", crate::i18n::tr(line));
    }
    if notify(&crate::i18n::tr("Your disk is almost full"), &body, Some(&crate::i18n::tr("Open AeroClean"))) {
        launch_gui(&["--tab", "map"]);
    }
}

// ---------- systemd user timers ----------

pub struct Job {
    pub unit: &'static str,
    pub description: &'static str,
    pub command: &'static str,
    pub schedule: &'static str,
}

pub const AUTO_CLEAN: Job = Job {
    unit: "aero-cleaner-clean",
    description: "AeroClean weekly automatic clean",
    command: "clean --auto",
    schedule: "OnCalendar=weekly\nRandomizedDelaySec=1h\nPersistent=true",
};

pub const DISK_CHECK: Job =
    Job { unit: "aero-cleaner-disk", description: "AeroClean low disk space check", command: "check-disk", schedule: "OnBootSec=10min\nOnUnitActiveSec=1h" };

fn unit_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config")).join("systemd/user")
}

fn systemctl(args: &[&str]) -> Result<String, String> {
    let out = Command::new("systemctl").arg("--user").args(args).env("LC_ALL", "C").output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(String::from_utf8_lossy(&out.stdout).trim().to_string()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

pub fn unit_files(job: &Job) -> (String, String) {
    let service =
        format!("[Unit]\nDescription={}\n\n[Service]\nType=oneshot\nExecStart=\"{}\" {}\nNice=10\nIOSchedulingClass=idle\n", job.description, exe().display(), job.command);
    let timer = format!("[Unit]\nDescription={} (timer)\n\n[Timer]\n{}\n\n[Install]\nWantedBy=timers.target\n", job.description, job.schedule);
    (service, timer)
}

pub fn enable(job: &Job) -> Result<(), String> {
    #[cfg(windows)]
    return crate::windows::task_enable(job.unit, job.command, job.unit == AUTO_CLEAN.unit);
    #[cfg(target_os = "macos")]
    return crate::macos::agent_enable(job.unit, job.command, job.unit == AUTO_CLEAN.unit);
    #[allow(unreachable_code)]
    let dir = unit_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let (service, timer) = unit_files(job);
    fs::write(dir.join(format!("{}.service", job.unit)), service).map_err(|e| e.to_string())?;
    fs::write(dir.join(format!("{}.timer", job.unit)), timer).map_err(|e| e.to_string())?;
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", "--now", &format!("{}.timer", job.unit)]).map(|_| ())
}

pub fn disable(job: &Job) -> Result<(), String> {
    #[cfg(windows)]
    return crate::windows::task_disable(job.unit);
    #[cfg(target_os = "macos")]
    return crate::macos::agent_disable(job.unit);
    #[allow(unreachable_code)]
    let _ = systemctl(&["disable", "--now", &format!("{}.timer", job.unit)]);
    let dir = unit_dir();
    let _ = fs::remove_file(dir.join(format!("{}.service", job.unit)));
    let _ = fs::remove_file(dir.join(format!("{}.timer", job.unit)));
    systemctl(&["daemon-reload"]).map(|_| ())
}

pub fn is_enabled(job: &Job) -> bool {
    #[cfg(windows)]
    return crate::windows::task_enabled(job.unit);
    #[cfg(target_os = "macos")]
    return crate::macos::agent_enabled(job.unit);
    #[allow(unreachable_code)]
    systemctl(&["is-enabled", &format!("{}.timer", job.unit)]).is_ok_and(|s| s == "enabled")
}

/// Human-readable next run time, e.g. "Mon 2026-11-16 00:42:00 CET".
pub fn next_run(job: &Job) -> Option<String> {
    #[cfg(windows)]
    return crate::windows::task_next_run(job.unit);
    // launchd doesn't tell when an agent runs next.
    #[cfg(target_os = "macos")]
    return {
        let _ = job;
        None
    };
    #[allow(unreachable_code)]
    let v = systemctl(&["show", "-P", "NextElapseUSecRealtime", &format!("{}.timer", job.unit)]).ok()?;
    (!v.is_empty() && v != "n/a").then_some(v)
}

/// Adds "Quick clean" and "Disk map" to the right-click menu of the
/// AeroClean launcher (menu, panel and desktop icons).
pub const ICON_SVG: &str = include_str!("../assets/aero-cleaner.svg");

pub fn install_launcher() -> Result<(), String> {
    #[cfg(windows)]
    return crate::windows::install_shortcut();
    // The .app bundle is the launcher.
    #[cfg(target_os = "macos")]
    return Ok(());
    #[allow(unreachable_code)]
    // The icon goes into the user's icon theme folder so "Icon=aero-cleaner" resolves.
    let icon_dir = home().join(".local/share/icons/hicolor/scalable/apps");
    fs::create_dir_all(&icon_dir).map_err(|e| e.to_string())?;
    fs::write(icon_dir.join("aero-cleaner.svg"), ICON_SVG).map_err(|e| e.to_string())?;
    let dir = home().join(".local/share/applications");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let exe = exe();
    let e = exe.display();
    let text = format!(
        "[Desktop Entry]\nName=AeroClean\nComment=Clean caches, logs and junk files\nExec={e}\nIcon=aero-cleaner\nTerminal=false\nType=Application\nCategories=System;\nStartupWMClass=aero-cleaner\nActions=QuickClean;DiskMap;\n\n\
         [Desktop Action QuickClean]\nName=Quick clean\nExec={e} clean --auto\n\n\
         [Desktop Action DiskMap]\nName=Disk map\nExec={e} --tab map\n"
    );
    fs::write(dir.join("aero-cleaner.desktop"), text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    fn units_are_well_formed() {
        let (service, timer) = unit_files(&AUTO_CLEAN);
        assert!(service.contains("Type=oneshot") && service.contains("clean --auto"));
        assert!(service.lines().any(|l| l.starts_with("ExecStart=\"/") && l.ends_with("\" clean --auto")), "absolute, quoted path: {service}");
        assert!(timer.contains("OnCalendar=weekly") && timer.contains("WantedBy=timers.target"));
        let (_, disk_timer) = unit_files(&DISK_CHECK);
        assert!(disk_timer.contains("OnUnitActiveSec=1h"));
    }

    #[test]
    #[cfg(target_os = "linux")] // needs systemd-analyze
    fn systemd_accepts_the_units() {
        let dir = std::env::temp_dir().join(format!("aero-units-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        for job in [&AUTO_CLEAN, &DISK_CHECK] {
            let (service, timer) = unit_files(job);
            fs::write(dir.join(format!("{}.service", job.unit)), service).unwrap();
            fs::write(dir.join(format!("{}.timer", job.unit)), &timer).unwrap();
            // System mode: works on CI machines that have no user session.
            let out = Command::new("systemd-analyze").args(["verify"]).arg(dir.join(format!("{}.timer", job.unit))).output().unwrap();
            let err = String::from_utf8_lossy(&out.stderr);
            assert!(out.status.success(), "systemd rejected {}: {err}", job.unit);
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn auto_clean_never_needs_root() {
        let s = Settings { auto_trash: true, ..Default::default() };
        for k in auto_kinds(&s, false) {
            let cat = cleaner::CATEGORIES.iter().find(|c| c.kind == k).unwrap();
            assert!(!cat.root, "{:?} would need a password", k);
        }
        assert!(!auto_kinds(&s, true).contains(&Kind::AppCache), "browser cache skipped while a browser runs");
    }
}
