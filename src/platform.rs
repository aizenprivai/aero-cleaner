//! Everything that differs between Linux, Windows and macOS lives here (or
//! in `windows.rs` / `macos.rs` for the larger system-specific features).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const IS_WINDOWS: bool = cfg!(windows);
pub const IS_MACOS: bool = cfg!(target_os = "macos");
pub const IS_LINUX: bool = !IS_WINDOWS && !IS_MACOS;

/// The user's home folder (`$HOME`, or `%USERPROFILE%` on Windows).
pub fn home() -> PathBuf {
    let var = if IS_WINDOWS { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// Where settings live: `~/.config/aero-cleaner`, `%APPDATA%\AeroClean`
/// or `~/Library/Application Support/AeroClean`.
pub fn config_dir() -> PathBuf {
    if IS_MACOS {
        home().join("Library/Application Support/AeroClean")
    } else if IS_WINDOWS {
        env_dir("APPDATA").unwrap_or_else(|| home().join("AppData/Roaming")).join("AeroClean")
    } else {
        env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home().join(".config")).join("aero-cleaner")
    }
}

/// Where history and state live: `~/.local/share/aero-cleaner`,
/// `%LOCALAPPDATA%\AeroClean` or `~/Library/Application Support/AeroClean`.
pub fn data_dir() -> PathBuf {
    if IS_MACOS {
        config_dir()
    } else if IS_WINDOWS {
        env_dir("LOCALAPPDATA").unwrap_or_else(|| home().join("AppData/Local")).join("AeroClean")
    } else {
        env_dir("XDG_DATA_HOME").unwrap_or_else(|| home().join(".local/share")).join("aero-cleaner")
    }
}

/// `%LOCALAPPDATA%` (Windows) — used for caches and browser data.
pub fn local_app_data() -> PathBuf {
    env_dir("LOCALAPPDATA").unwrap_or_else(|| home().join("AppData/Local"))
}

/// `%APPDATA%` (Windows roaming app data).
pub fn roaming_app_data() -> PathBuf {
    env_dir("APPDATA").unwrap_or_else(|| home().join("AppData/Roaming"))
}

/// True for paths (relative to home) that belong to apps rather than the
/// user: hidden dot-folders, plus `AppData` on Windows and `Library` on macOS.
pub fn is_app_data(rel: &Path) -> bool {
    rel.components().enumerate().any(|(i, c)| {
        let s = c.as_os_str().to_string_lossy();
        s.starts_with('.') || (IS_WINDOWS && s.eq_ignore_ascii_case("AppData")) || (IS_MACOS && i == 0 && s == "Library")
    })
}

/// Free space on the system drive: (total, available) bytes.
pub fn disk_usage() -> Option<(u64, u64)> {
    #[cfg(windows)]
    {
        crate::windows::disk_usage()
    }
    #[cfg(target_os = "macos")]
    {
        crate::macos::disk_usage()
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let out = Command::new("df").args(["-B1", "--output=size,avail", "/"]).output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let mut nums = text.lines().nth(1)?.split_whitespace().map(|n| n.parse::<u64>());
        Some((nums.next()?.ok()?, nums.next()?.ok()?))
    }
}

/// Names of running programs, lowercase, without ".exe".
pub fn running_processes() -> Vec<String> {
    #[cfg(target_os = "macos")]
    return crate::macos::running_processes();
    #[allow(unreachable_code)]
    if IS_WINDOWS {
        // `tasklist /fo csv /nh` prints "image.exe","pid",... (not localized).
        let out = Command::new("tasklist").args(["/fo", "csv", "/nh"]).stderr(Stdio::null()).output();
        let text = out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
        text.lines().filter_map(|l| l.split(',').next()).map(|n| n.trim_matches('"').to_lowercase().trim_end_matches(".exe").to_string()).collect()
    } else {
        std::fs::read_dir("/proc")
            .map(|rd| rd.flatten().filter_map(|e| std::fs::read_to_string(e.path().join("comm")).ok()).map(|c| c.trim().to_string()).collect())
            .unwrap_or_default()
    }
}

/// Shows the file in the system file manager.
pub fn open_folder(path: &Path) {
    if IS_WINDOWS {
        let _ = Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
    } else if IS_MACOS {
        // Opens Finder with the file selected.
        let _ = Command::new("open").arg("-R").arg(path).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    } else if let Some(dir) = path.parent() {
        let _ = Command::new("xdg-open").arg(dir).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    }
}

/// Opens a folder itself in the system file manager.
pub fn open_dir(dir: &Path) {
    let tool = if IS_WINDOWS {
        "explorer"
    } else if IS_MACOS {
        "open"
    } else {
        "xdg-open"
    };
    let _ = command(tool).arg(dir).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

/// Moves a file to the Trash / Recycle Bin (recoverable).
pub fn move_to_trash(path: &Path) -> Result<(), String> {
    #[cfg(any(windows, target_os = "macos"))]
    {
        trash::delete(path).map_err(|e| e.to_string())
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let out = Command::new("gio").arg("trash").arg("--").arg(path).stderr(Stdio::piped()).output().map_err(|e| e.to_string())?;
        if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
    }
}

/// UI font files (regular, bold), with the face index inside the file.
/// Segoe UI is the original Aero-era font; Lucida Grande is the Aqua one.
pub fn font_files() -> [(&'static str, u32); 2] {
    if IS_WINDOWS {
        [("C:\\Windows\\Fonts\\segoeui.ttf", 0), ("C:\\Windows\\Fonts\\segoeuib.ttf", 0)]
    } else if IS_MACOS {
        [("/System/Library/Fonts/LucidaGrande.ttc", 0), ("/System/Library/Fonts/LucidaGrande.ttc", 1)]
    } else {
        [("/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf", 0), ("/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf", 0)]
    }
}

/// The program's own path, preferring the installed copy on Linux so
/// scheduled jobs keep working after rebuilds.
pub fn exe() -> PathBuf {
    let current = std::env::current_exe().unwrap_or_default();
    if !IS_LINUX {
        return current;
    }
    // Inside an AppImage, current_exe is a temporary mount: use the AppImage file.
    if let Some(app) = std::env::var_os("APPIMAGE").map(PathBuf::from).filter(|p| p.is_file()) {
        return app;
    }
    let installed = home().join(".local/bin/aero-cleaner");
    if installed.exists() { installed } else { current }
}

/// A command that doesn't flash a console window on Windows.
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

/// Finds a command-line tool. Apps started from the Dock or the Start Menu
/// don't get the shell's PATH, so also look where installers put them.
pub fn find_tool(name: &str) -> Option<PathBuf> {
    let file = if IS_WINDOWS { format!("{name}.exe") } else { name.to_string() };
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    if IS_MACOS {
        dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
        dirs.push(home().join(".docker/bin"));
    } else if IS_WINDOWS {
        let pf = env_dir("ProgramFiles").unwrap_or_else(|| PathBuf::from("C:\\Program Files"));
        dirs.push(pf.join("Docker\\Docker\\resources\\bin"));
        dirs.push(pf.join("RedHat\\Podman"));
    }
    dirs.into_iter().map(|d| d.join(&file)).find(|p| p.is_file())
}

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Asks the user for a folder with the system dialog.
pub fn pick_folder(title: &str) -> Option<String> {
    #[cfg(target_os = "macos")]
    return crate::macos::choose_folder(title);
    #[allow(unreachable_code)]
    if IS_WINDOWS {
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description = {}; if ($d.ShowDialog() -eq 'OK') {{ $d.SelectedPath }}",
            ps_quote(title)
        );
        return dialog_output(command("powershell").args(["-NoProfile", "-STA", "-Command", &script]));
    }
    dialog_output(Command::new("zenity").args(["--file-selection", "--directory", &format!("--title={title}")]))
}

/// Asks the user for a file with the system dialog.
pub fn pick_file(title: &str) -> Option<String> {
    #[cfg(target_os = "macos")]
    return crate::macos::choose_file(title);
    #[allow(unreachable_code)]
    if IS_WINDOWS {
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.OpenFileDialog; $d.Title = {}; if ($d.ShowDialog() -eq 'OK') {{ $d.FileName }}",
            ps_quote(title)
        );
        return dialog_output(command("powershell").args(["-NoProfile", "-STA", "-Command", &script]));
    }
    dialog_output(Command::new("zenity").args(["--file-selection", &format!("--title={title}")]))
}

fn dialog_output(cmd: &mut Command) -> Option<String> {
    let out = cmd.stderr(Stdio::null()).output().ok().filter(|o| o.status.success())?;
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!path.is_empty()).then_some(path)
}

/// Running on battery (not plugged in)? Checked at most once a minute.
pub fn on_battery() -> bool {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    static CACHE: Mutex<Option<(Instant, bool)>> = Mutex::new(None);
    let mut c = CACHE.lock().unwrap();
    if let Some((when, v)) = *c
        && when.elapsed() < Duration::from_secs(60)
    {
        return v;
    }
    let v = battery_now();
    *c = Some((Instant::now(), v));
    v
}

#[cfg(windows)]
fn battery_now() -> bool {
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut st: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    // ACLineStatus: 0 = on battery, 1 = plugged in, 255 = unknown.
    unsafe { GetSystemPowerStatus(&mut st) != 0 && st.ACLineStatus == 0 }
}

#[cfg(target_os = "macos")]
fn battery_now() -> bool {
    Command::new("pmset").args(["-g", "batt"]).output().map(|o| String::from_utf8_lossy(&o.stdout).contains("'Battery Power'")).unwrap_or(false)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn battery_now() -> bool {
    battery_from_sysfs(Path::new("/sys/class/power_supply"))
}

/// Linux: on battery when there's a battery and no mains supply is online.
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn battery_from_sysfs(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else { return false };
    let (mut battery, mut mains_online) = (false, false);
    for e in rd.flatten() {
        let read = |f: &str| std::fs::read_to_string(e.path().join(f)).unwrap_or_default().trim().to_string();
        match read("type").as_str() {
            "Battery" => battery = true,
            "Mains" | "USB" if read("online") == "1" => mains_online = true,
            _ => {}
        }
    }
    battery && !mains_online
}

#[cfg(test)]
mod tests {
    #[test]
    fn battery_from_sysfs_reads_power_supplies() {
        let base = std::env::temp_dir().join(format!("aero-power-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let supply = |name: &str, ty: &str, online: &str| {
            std::fs::create_dir_all(base.join(name)).unwrap();
            std::fs::write(base.join(name).join("type"), ty).unwrap();
            std::fs::write(base.join(name).join("online"), online).unwrap();
        };
        supply("BAT0", "Battery\n", "");
        supply("AC", "Mains\n", "0\n");
        assert!(super::battery_from_sysfs(&base), "unplugged");
        supply("AC", "Mains\n", "1\n");
        assert!(!super::battery_from_sysfs(&base), "plugged in");
        std::fs::remove_dir_all(&base).unwrap();
        assert!(!super::battery_from_sysfs(&base), "desktop without power info");
    }
}
