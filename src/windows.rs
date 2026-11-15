//! Windows-only features: disk space, Recycle Bin, UAC elevation, startup
//! apps (registry + Startup folder), Task Scheduler, toast notifications
//! and the Start Menu shortcut.

use crate::platform;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, GetLastError};
use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};
use windows_sys::Win32::UI::Shell::{
    SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND, SHEmptyRecycleBinW, SHQUERYRBINFO, SHQueryRecycleBinW, ShellExecuteExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RegType};
use winreg::{RegKey, RegValue};

/// Don't flash a console window when a GUI app runs a command-line tool.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn command(program: &str) -> Command {
    let mut c = Command::new(program);
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

pub fn system_drive() -> PathBuf {
    let d = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    PathBuf::from(format!("{d}\\"))
}

pub fn windows_dir() -> PathBuf {
    std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| system_drive().join("Windows"))
}

pub fn program_data() -> PathBuf {
    std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(|| system_drive().join("ProgramData"))
}

pub fn disk_usage() -> Option<(u64, u64)> {
    let root = wide(system_drive());
    let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
    // SAFETY: valid NUL-terminated path and valid out-pointers.
    let ok = unsafe { GetDiskFreeSpaceExW(root.as_ptr(), &mut avail, &mut total, &mut free) };
    (ok != 0).then_some((total, avail))
}

/// Bytes currently in the Recycle Bin (all drives).
pub fn recycle_bin_size() -> Option<u64> {
    let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, i64Size: 0, i64NumItems: 0 };
    // SAFETY: null root = all drives; info is a valid, sized struct.
    let hr = unsafe { SHQueryRecycleBinW(std::ptr::null(), &mut info) };
    (hr == 0).then_some(info.i64Size.max(0) as u64)
}

pub fn empty_recycle_bin() -> Result<(), String> {
    // SAFETY: null window/root; flags only suppress UI.
    let hr = unsafe { SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND) };
    // An already empty bin returns E_UNEXPECTED (0x8000FFFF); that's fine.
    if hr == 0 || hr as u32 == 0x8000_FFFF { Ok(()) } else { Err(format!("error 0x{:08X}", hr as u32)) }
}

/// Quotes one argument the way the Windows C runtime parses it.
fn quote_arg(a: &str) -> String {
    if !a.is_empty() && !a.contains([' ', '\t', '"']) {
        return a.to_string();
    }
    let mut out = String::from("\"");
    let mut backslashes = 0;
    for c in a.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes * 2));
    out.push('"');
    out
}

/// Runs this program again with administrator rights (one UAC prompt) and
/// waits for it. Returns its exit code, or an error if the user said no.
pub fn run_elevated(args: &[String]) -> Result<u32, String> {
    let exe = wide(platform::exe());
    let params = wide(args.iter().map(|a| quote_arg(a)).collect::<Vec<_>>().join(" "));
    let verb = wide("runas");
    // SAFETY: zeroed is a valid initial state for this plain C struct.
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = exe.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = SW_HIDE;
    // SAFETY: all string pointers outlive the call.
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        // SAFETY: plain thread-local error read.
        let err = unsafe { GetLastError() };
        return Err(if err == ERROR_CANCELLED { "cancelled".into() } else { format!("error {err}") });
    }
    let mut code = 1u32;
    // SAFETY: hProcess is a valid handle because of SEE_MASK_NOCLOSEPROCESS.
    unsafe {
        WaitForSingleObject(info.hProcess, INFINITE);
        GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
    }
    Ok(code)
}

// ---------- Startup apps ----------

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Where 32-bit programs register for all users (Task Manager shows both views).
const RUN32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
const APPROVED_RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const APPROVED_RUN32: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32";
const APPROVED_FOLDER: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder";

/// Task Manager stores on/off in "StartupApproved": first byte 2 = on, 3 = off.
fn approved(root: &RegKey, key: &str, name: &str) -> bool {
    root.open_subkey(key).and_then(|k| k.get_raw_value(name)).map(|v| v.bytes.first().is_none_or(|b| b % 2 == 0)).unwrap_or(true)
}

fn startup_folder(all_users: bool) -> PathBuf {
    let base = if all_users { program_data() } else { platform::roaming_app_data() };
    base.join(r"Microsoft\Windows\Start Menu\Programs\Startup")
}

pub fn startup_entries() -> Vec<crate::tools::StartupEntry> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let mut out = Vec::new();
    for (root, key, approved_key, all_users) in [(&hkcu, RUN, APPROVED_RUN, false), (&hklm, RUN, APPROVED_RUN, true), (&hklm, RUN32, APPROVED_RUN32, true)] {
        let Ok(run) = root.open_subkey(key) else { continue };
        for (name, value) in run.enum_values().flatten() {
            out.push(crate::tools::StartupEntry {
                file: format!("{}:{name}", if all_users { "hklm" } else { "run" }),
                comment: value.to_string().trim_matches('"').to_string(),
                enabled: approved(root, approved_key, &name),
                system: all_users,
                critical: all_users,
                name,
            });
        }
    }
    for all_users in [false, true] {
        let Ok(rd) = std::fs::read_dir(startup_folder(all_users)) else { continue };
        for e in rd.flatten() {
            let file = e.file_name().to_string_lossy().to_string();
            if file.eq_ignore_ascii_case("desktop.ini") {
                continue;
            }
            let root = if all_users { &hklm } else { &hkcu };
            out.push(crate::tools::StartupEntry {
                name: Path::new(&file).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or(file.clone()),
                comment: e.path().display().to_string(),
                enabled: approved(root, APPROVED_FOLDER, &file),
                system: all_users,
                critical: all_users,
                file: format!("{}:{file}", if all_users { "common" } else { "folder" }),
            });
        }
    }
    out.sort_by_key(|e| (e.critical, e.name.to_lowercase()));
    out
}

/// Turns a startup entry on or off for this user, exactly like Task Manager.
pub fn set_startup(id: &str, enabled: bool) -> Result<(), String> {
    let (kind, name) = id.split_once(':').ok_or("bad id")?;
    let key = match kind {
        "run" => APPROVED_RUN,
        "folder" => APPROVED_FOLDER,
        _ => return Err("Entries for all users can be changed in Task Manager (needs admin).".into()),
    };
    let mut bytes = vec![if enabled { 2u8 } else { 3u8 }, 0, 0, 0];
    let filetime = if enabled {
        0u64
    } else {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        (secs + 11_644_473_600) * 10_000_000
    };
    bytes.extend_from_slice(&filetime.to_le_bytes());
    let (k, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(key).map_err(|e| e.to_string())?;
    k.set_raw_value(name, &RegValue { bytes, vtype: RegType::REG_BINARY }).map_err(|e| e.to_string())
}

// ---------- Task Scheduler ----------

fn task_name(job: &str) -> String {
    format!("AeroClean\\{job}")
}

/// Creates (or replaces) a scheduled task: weekly or hourly.
pub fn task_enable(job: &str, args: &str, weekly: bool) -> Result<(), String> {
    let tr = format!("\"{}\" {args}", platform::exe().display());
    let schedule = if weekly { ["/SC", "WEEKLY"] } else { ["/SC", "HOURLY"] };
    let out = command("schtasks").args(["/Create", "/F", "/TN", &task_name(job), "/TR", &tr]).args(schedule).output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

pub fn task_disable(job: &str) -> Result<(), String> {
    let _ = command("schtasks").args(["/Delete", "/F", "/TN", &task_name(job)]).output();
    Ok(())
}

pub fn task_enabled(job: &str) -> bool {
    command("schtasks").args(["/Query", "/TN", &task_name(job)]).output().is_ok_and(|o| o.status.success())
}

/// "Next run time" column of `schtasks /Query /FO CSV` (in the system's date format).
pub fn task_next_run(job: &str) -> Option<String> {
    let out = command("schtasks").args(["/Query", "/TN", &task_name(job), "/FO", "CSV", "/NH"]).output().ok()?;
    let line = String::from_utf8_lossy(&out.stdout).lines().next()?.to_string();
    let next = line.split("\",\"").nth(1)?.trim_matches('"').to_string();
    (!next.is_empty() && next != "N/A").then_some(next)
}

// ---------- Notifications & shortcut ----------

fn base64(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |acc, (i, b)| acc | (*b as u32) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() { T[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

/// Runs a PowerShell script without a console window or quoting problems.
fn powershell(script: &str) -> std::io::Result<std::process::Output> {
    let utf16: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    command("powershell").args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand", &base64(&utf16)]).output()
}

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Shows a Windows 10/11 toast notification.
pub fn toast(title: &str, body: &str) {
    let script = format!(
        "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null\n\
         $t = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02)\n\
         $x = $t.GetElementsByTagName('text')\n\
         $x.Item(0).AppendChild($t.CreateTextNode({})) | Out-Null\n\
         $x.Item(1).AppendChild($t.CreateTextNode({})) | Out-Null\n\
         [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}}\\WindowsPowerShell\\v1.0\\powershell.exe').Show([Windows.UI.Notifications.ToastNotification]::new($t))",
        ps_quote(title),
        ps_quote(body)
    );
    let _ = powershell(&script);
}

/// Adds AeroClean to the Start Menu (once).
pub fn install_shortcut() -> Result<(), String> {
    let lnk = platform::roaming_app_data().join(r"Microsoft\Windows\Start Menu\Programs\AeroClean.lnk");
    if lnk.exists() {
        return Ok(());
    }
    let exe = platform::exe().display().to_string();
    let script = format!(
        "$s = (New-Object -ComObject WScript.Shell).CreateShortcut({}); $s.TargetPath = {}; $s.IconLocation = {}; $s.Description = 'Clean caches, temp files and junk'; $s.Save()",
        ps_quote(&lnk.display().to_string()),
        ps_quote(&exe),
        ps_quote(&format!("{exe},0"))
    );
    powershell(&script).map_err(|e| e.to_string()).map(|_| ())
}

/// Time the firmware (BIOS/UEFI) took at the last boot, in milliseconds.
pub fn firmware_ms() -> Option<u32> {
    let key = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey("SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Power").ok()?;
    key.get_value::<u32, _>("FwPOSTTime").ok().filter(|ms| *ms > 0)
}

/// Sends printed output to the terminal the app was started from, if any
/// (the app is a GUI program, so it has no console of its own).
pub fn attach_console() {
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    // SAFETY: no pointers involved; fails harmlessly when there's no parent console.
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_and_base64() {
        assert_eq!(quote_arg("plain"), "plain");
        assert_eq!(quote_arg(r"C:\Users\Jean Dupont\log.txt"), r#""C:\Users\Jean Dupont\log.txt""#);
        assert_eq!(quote_arg(r#"say "hi""#), r#""say \"hi\"""#);
        assert_eq!(base64(b"hello"), "aGVsbG8=");
        assert_eq!(base64(b"hi!"), "aGkh");
    }
}
