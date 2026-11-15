//! The privileged helper (Linux). Installed packages ship a small D-Bus
//! system service: the system bus starts `aero-cleaner --dbus-helper` as
//! root when AeroClean asks for a system clean, and it quits after a minute
//! of quiet.
//!
//! Safety:
//! * It does a fixed list of operations, each with its own polkit action
//!   (so an admin can allow the safe ones without a password).
//! * No shell runs as root: each operation is fixed commands with checked
//!   arguments, or plain file deletions written in Rust.
//! * The only inputs are the journal's days (1 to 3650) and a service name
//!   (validated, and never a system-critical service).
//!
//! When the helper isn't installed (built from source, AppImage),
//! AeroClean falls back to one pkexec prompt for the whole clean.

use crate::distro::{self, Pkg};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const BUS_NAME: &str = "org.aeroclean.Helper1";
pub const OBJECT_PATH: &str = "/org/aeroclean/Helper1";
pub const INTERFACE: &str = "org.aeroclean.Helper1";
const SERVICE_FILE: &str = "/usr/share/dbus-1/system-services/org.aeroclean.Helper1.service";

/// The operations, as named on the bus.
#[cfg(test)]
pub const OPS: &[&str] = &["package-cache", "unused-packages", "journal", "rotated-logs", "crash-reports", "old-temp", "old-snaps", "service-enable", "service-disable"];

/// The polkit action that guards `op`.
pub fn action_id(op: &str) -> Option<&'static str> {
    Some(match op {
        "package-cache" => "org.aeroclean.helper.clean-package-cache",
        "unused-packages" => "org.aeroclean.helper.remove-unused-packages",
        "journal" => "org.aeroclean.helper.clean-journal",
        "rotated-logs" => "org.aeroclean.helper.clean-rotated-logs",
        "crash-reports" => "org.aeroclean.helper.clean-crash-reports",
        "old-temp" => "org.aeroclean.helper.clean-old-temp",
        "old-snaps" => "org.aeroclean.helper.clean-old-snaps",
        "service-enable" | "service-disable" => "org.aeroclean.helper.manage-services",
        _ => return None,
    })
}

/// One thing an operation does.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// A command: program (looked up in the system folders) and arguments.
    Run {
        argv: Vec<String>,
        env: Vec<(String, String)>,
    },
    DeleteRotatedLogs,
    DeleteCrashReports,
    DeleteOldTemp,
    /// Nothing to do, with a line to say so.
    Say(String),
}

fn run(argv: &[&str]) -> Step {
    Step::Run { argv: argv.iter().map(|s| s.to_string()).collect(), env: vec![] }
}

/// Package names as package managers print them; anything else is refused.
fn package_name_ok(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('-') && name.chars().all(|c| c.is_ascii_alphanumeric() || "+-._@:".contains(c))
}

/// What `op` will do. `query` runs a read-only command and returns its output.
pub fn plan(op: &str, arg: &str, pkg: Pkg, has_paccache: bool, query: &dyn Fn(&[&str]) -> String) -> Result<Vec<Step>, String> {
    let no_arg = || if arg.is_empty() { Ok(()) } else { Err(format!("{op} takes no argument")) };
    Ok(match op {
        "package-cache" => {
            no_arg()?;
            vec![match pkg {
                Pkg::Apt => run(&["apt-get", "clean"]),
                Pkg::Dnf => run(&["dnf", "clean", "packages"]),
                Pkg::Pacman if has_paccache => run(&["paccache", "-rk2"]),
                Pkg::Pacman => run(&["pacman", "-Sc", "--noconfirm"]),
                Pkg::Zypper => run(&["zypper", "-n", "clean"]),
                Pkg::Unknown => return Err("unknown package manager".into()),
            }]
        }
        "unused-packages" => {
            no_arg()?;
            match pkg {
                Pkg::Apt => {
                    vec![Step::Run { argv: ["apt-get", "autoremove", "--purge", "-y"].map(String::from).to_vec(), env: vec![("DEBIAN_FRONTEND".into(), "noninteractive".into())] }]
                }
                Pkg::Dnf => vec![run(&["dnf", "-y", "autoremove"])],
                Pkg::Pacman | Pkg::Zypper => {
                    let names: Vec<String> = if pkg == Pkg::Pacman {
                        query(&["pacman", "-Qdtq"]).lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
                    } else {
                        distro::parse_zypper_unneeded(&query(&["zypper", "-n", "packages", "--unneeded"]))
                    };
                    if names.is_empty() {
                        return Ok(vec![Step::Say("No packages to remove".into())]);
                    }
                    if let Some(bad) = names.iter().find(|n| !package_name_ok(n)) {
                        return Err(format!("unexpected package name: {bad}"));
                    }
                    let mut argv: Vec<String> =
                        if pkg == Pkg::Pacman { vec!["pacman".into(), "-Rns".into(), "--noconfirm".into()] } else { vec!["zypper".into(), "-n".into(), "rm".into()] };
                    argv.extend(names);
                    vec![Step::Run { argv, env: vec![] }]
                }
                Pkg::Unknown => return Err("unknown package manager".into()),
            }
        }
        "journal" => {
            let days: u32 = arg.parse().map_err(|_| "journal: days must be a number".to_string())?;
            if !(1..=3650).contains(&days) {
                return Err("journal: days must be between 1 and 3650".into());
            }
            vec![run(&["journalctl", &format!("--vacuum-time={days}d")])]
        }
        "rotated-logs" => {
            no_arg()?;
            vec![Step::DeleteRotatedLogs]
        }
        "crash-reports" => {
            no_arg()?;
            vec![Step::DeleteCrashReports]
        }
        "old-temp" => {
            no_arg()?;
            vec![Step::DeleteOldTemp]
        }
        "old-snaps" => {
            no_arg()?;
            // parse_snap_list only keeps plain names and numeric revisions.
            let revs = distro::parse_snap_list(&query(&["snap", "list", "--all"]));
            if revs.is_empty() {
                return Ok(vec![Step::Say("No packages to remove".into())]);
            }
            revs.iter().map(|(n, r)| run(&["snap", "remove", n, &format!("--revision={r}")])).collect()
        }
        "service-enable" | "service-disable" => {
            if !crate::boot::valid_unit(arg) || crate::boot::is_critical(arg) {
                return Err(format!("{arg} can't be changed here"));
            }
            let action = if op == "service-enable" { "enable" } else { "disable" };
            vec![run(&["systemctl", action, "--", arg])]
        }
        _ => return Err(format!("unknown operation: {op}")),
    })
}

// ---------- Doing it ----------

/// Runs steps; the test version records them instead.
pub trait Exec: Send + Sync {
    fn query(&self, argv: &[&str]) -> String;
    fn step(&self, step: &Step) -> Result<String, String>;
}

/// Programs come from the system folders only (D-Bus starts us with a
/// minimal environment, and nothing from a user's PATH runs as root).
fn system_tool(name: &str) -> Option<String> {
    ["/usr/sbin", "/usr/bin", "/sbin", "/bin"].iter().map(|d| format!("{d}/{name}")).find(|p| Path::new(p).is_file())
}

pub struct RealExec;

impl Exec for RealExec {
    fn query(&self, argv: &[&str]) -> String {
        let Some(bin) = system_tool(argv[0]) else { return String::new() };
        Command::new(bin)
            .args(&argv[1..])
            .env_clear()
            .env("LC_ALL", "C")
            .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
            .stderr(Stdio::null())
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    }

    fn step(&self, step: &Step) -> Result<String, String> {
        match step {
            Step::Say(s) => Ok(s.clone()),
            Step::Run { argv, env } => {
                let bin = system_tool(&argv[0]).ok_or_else(|| format!("{} isn't installed", argv[0]))?;
                let out = Command::new(bin)
                    .args(&argv[1..])
                    .env_clear()
                    .env("LC_ALL", "C")
                    .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
                    .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
                    .stdin(Stdio::null())
                    .output()
                    .map_err(|e| e.to_string())?;
                let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
                if out.status.success() { Ok(text) } else { Err(text.lines().last().unwrap_or("failed").to_string()) }
            }
            Step::DeleteRotatedLogs => {
                Ok(format!("{} files removed", delete_files(Path::new("/var/log"), &|p, md| !p.starts_with("/var/log/journal") && crate::cleaner::is_rotated_log(p, md))))
            }
            Step::DeleteCrashReports => Ok(format!("{} files removed", delete_files(Path::new("/var/crash"), &|_, _| true))),
            Step::DeleteOldTemp => {
                let n = delete_files(Path::new("/var/tmp"), &|_, md| crate::cleaner::older_than(md, 7));
                remove_empty_dirs(Path::new("/var/tmp"));
                Ok(format!("{n} files removed"))
            }
        }
    }
}

/// Deletes matching files under `root` (never following symlinks, never root itself).
fn delete_files(root: &Path, keep_if_false: &dyn Fn(&Path, &fs::Metadata) -> bool) -> usize {
    let mut n = 0;
    let Ok(rd) = fs::read_dir(root) else { return 0 };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(md) = fs::symlink_metadata(&p) else { continue };
        if md.is_dir() {
            n += delete_files(&p, keep_if_false);
        } else if keep_if_false(&p, &md) && fs::remove_file(&p).is_ok() {
            n += 1;
        }
    }
    n
}

fn remove_empty_dirs(root: &Path) {
    let Ok(rd) = fs::read_dir(root) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if fs::symlink_metadata(&p).is_ok_and(|m| m.is_dir()) {
            remove_empty_dirs(&p);
            let _ = fs::remove_dir(&p); // fails (and that's fine) when not empty
        }
    }
}

// ---------- The D-Bus service ----------

/// Who may run an operation.
pub enum Auth {
    /// Ask polkit about the calling program.
    Polkit,
    /// Tests: allow everything.
    #[cfg(test)]
    AllowAll,
}

struct Helper {
    auth: Auth,
    exec: Box<dyn Exec>,
    pkg: Pkg,
    last_call: Arc<Mutex<Instant>>,
    busy: Arc<AtomicUsize>,
}

async fn polkit_allows(conn: &zbus::Connection, sender: &str, action: &str) -> Result<bool, String> {
    use std::collections::HashMap;
    use zbus::zvariant::Value;
    let proxy =
        zbus::Proxy::new(conn, "org.freedesktop.PolicyKit1", "/org/freedesktop/PolicyKit1/Authority", "org.freedesktop.PolicyKit1.Authority").await.map_err(|e| e.to_string())?;
    let mut who: HashMap<&str, Value> = HashMap::new();
    who.insert("name", Value::from(sender));
    let details: HashMap<&str, &str> = HashMap::new();
    // Flag 1: polkit may ask the user for a password.
    let (allowed, _challenge, _extra): (bool, bool, HashMap<String, String>) =
        proxy.call("CheckAuthorization", &(("system-bus-name", who), action, details, 1u32, "")).await.map_err(|e| e.to_string())?;
    Ok(allowed)
}

#[zbus::interface(name = "org.aeroclean.Helper1")]
impl Helper {
    /// Runs one operation and returns what it printed.
    async fn clean(&self, #[zbus(header)] header: zbus::message::Header<'_>, #[zbus(connection)] conn: &zbus::Connection, op: &str, arg: &str) -> zbus::fdo::Result<String> {
        self.busy.fetch_add(1, Ordering::SeqCst);
        let result = self.clean_inner(header.sender().map(|s| s.to_string()), conn, op, arg).await;
        *self.last_call.lock().unwrap() = Instant::now();
        self.busy.fetch_sub(1, Ordering::SeqCst);
        result
    }
}

impl Helper {
    async fn clean_inner(&self, sender: Option<String>, conn: &zbus::Connection, op: &str, arg: &str) -> zbus::fdo::Result<String> {
        let action = action_id(op).ok_or_else(|| zbus::fdo::Error::InvalidArgs(format!("unknown operation: {op}")))?;
        let allowed = match &self.auth {
            Auth::Polkit => match sender {
                Some(s) => polkit_allows(conn, &s, action).await.map_err(zbus::fdo::Error::Failed)?,
                None => false,
            },
            #[cfg(test)]
            Auth::AllowAll => {
                let _ = (sender, conn);
                true
            }
        };
        if !allowed {
            return Err(zbus::fdo::Error::AccessDenied("not authorized".into()));
        }
        let has_paccache = system_tool("paccache").is_some();
        let steps = plan(op, arg, self.pkg, has_paccache, &|argv| self.exec.query(argv)).map_err(zbus::fdo::Error::InvalidArgs)?;
        let mut out = String::new();
        for s in &steps {
            out.push_str(&self.exec.step(s).map_err(zbus::fdo::Error::Failed)?);
            out.push('\n');
        }
        Ok(out)
    }
}

fn running_as_root() -> bool {
    fs::read_to_string("/proc/self/status")
        .unwrap_or_default()
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .and_then(|v| v.split_whitespace().nth(1).map(|e| e == "0"))
        .unwrap_or(false)
}

/// `aero-cleaner --dbus-helper`: started by the system bus. Quits after a
/// minute without calls.
pub fn serve() -> Result<(), String> {
    if !running_as_root() {
        return Err("the helper runs as root, started by D-Bus".into());
    }
    let last_call = Arc::new(Mutex::new(Instant::now()));
    let busy = Arc::new(AtomicUsize::new(0));
    let helper = Helper { auth: Auth::Polkit, exec: Box::new(RealExec), pkg: distro::detect(), last_call: last_call.clone(), busy: busy.clone() };
    let _conn = zbus::blocking::connection::Builder::system()
        .and_then(|b| b.name(BUS_NAME))
        .and_then(|b| b.serve_at(OBJECT_PATH, helper))
        .and_then(|b| b.build())
        .map_err(|e| e.to_string())?;
    loop {
        std::thread::sleep(Duration::from_secs(5));
        if busy.load(Ordering::SeqCst) == 0 && last_call.lock().unwrap().elapsed() > Duration::from_secs(60) {
            return Ok(());
        }
    }
}

// ---------- AeroClean's side ----------

/// Is the helper installed on this system?
pub fn installed() -> bool {
    // From the Flatpak, the host's helper (installed with the .deb or .rpm) is reachable too.
    crate::platform::host_path(SERVICE_FILE).is_file()
}

/// Asks the helper to run `op`. Err("cancelled") when permission was refused.
pub fn call(op: &str, arg: &str) -> Result<String, String> {
    let conn = zbus::blocking::Connection::system().map_err(|e| e.to_string())?;
    call_on(&conn, op, arg)
}

fn call_on(conn: &zbus::blocking::Connection, op: &str, arg: &str) -> Result<String, String> {
    let dest = if conn.is_bus() { Some(BUS_NAME) } else { None };
    match conn.call_method(dest, OBJECT_PATH, Some(INTERFACE), "Clean", &(op, arg)) {
        Ok(reply) => reply.body().deserialize::<String>().map_err(|e| e.to_string()),
        Err(zbus::Error::MethodError(name, msg, _)) => {
            if name.as_str() == "org.freedesktop.DBus.Error.AccessDenied" {
                Err("cancelled".into())
            } else {
                Err(msg.unwrap_or_else(|| name.to_string()))
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

/// The helper operation (and argument) for a Cleaner category, if any.
pub fn op_for(kind: crate::cleaner::Kind, journal_days: u32) -> Option<(&'static str, String)> {
    use crate::cleaner::Kind;
    Some(match kind {
        Kind::AptCache => ("package-cache", String::new()),
        Kind::Autoremove => ("unused-packages", String::new()),
        Kind::Journal => ("journal", journal_days.max(1).to_string()),
        Kind::OldLogs => ("rotated-logs", String::new()),
        Kind::CrashReports => ("crash-reports", String::new()),
        Kind::VarTmp => ("old-temp", String::new()),
        Kind::Snap => ("old-snaps", String::new()),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan_q(op: &str, arg: &str, pkg: Pkg, out: &str) -> Result<Vec<Step>, String> {
        let out = out.to_string();
        plan(op, arg, pkg, false, &move |_| out.clone())
    }

    fn argv(steps: &[Step]) -> Vec<String> {
        steps
            .iter()
            .map(|s| match s {
                Step::Run { argv, .. } => argv.join(" "),
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn plans_fixed_commands_per_package_manager() {
        assert_eq!(argv(&plan_q("package-cache", "", Pkg::Apt, "").unwrap()), vec!["apt-get clean"]);
        assert_eq!(argv(&plan_q("package-cache", "", Pkg::Zypper, "").unwrap()), vec!["zypper -n clean"]);
        assert_eq!(argv(&plan("package-cache", "", Pkg::Pacman, true, &|_| String::new()).unwrap()), vec!["paccache -rk2"]);
        assert_eq!(argv(&plan_q("unused-packages", "", Pkg::Pacman, "libold\npython-foo\n").unwrap()), vec!["pacman -Rns --noconfirm libold python-foo"]);
        assert_eq!(plan_q("unused-packages", "", Pkg::Pacman, "").unwrap(), vec![Step::Say("No packages to remove".into())]);
        assert!(plan_q("unused-packages", "", Pkg::Pacman, "ok\n--force\n").is_err(), "an option-looking name is refused");
        assert!(plan_q("unused-packages", "", Pkg::Pacman, "a;rm -rf /\n").is_err());
        assert_eq!(argv(&plan_q("journal", "3", Pkg::Apt, "").unwrap()), vec!["journalctl --vacuum-time=3d"]);
        let snaps = "Name Version Rev Tracking Publisher Notes\ncore22 1 1122 latest/stable canonical base,disabled\n";
        assert_eq!(argv(&plan_q("old-snaps", "", Pkg::Apt, snaps).unwrap()), vec!["snap remove core22 --revision=1122"]);
    }

    #[test]
    fn refuses_bad_input() {
        for (op, arg) in [
            ("journal", "0"),
            ("journal", "99999"),
            ("journal", "3; reboot"),
            ("package-cache", "x"),
            ("nope", ""),
            ("service-disable", "dbus.service"),
            ("service-disable", "x.service; rm"),
            ("service-enable", "--now"),
        ] {
            assert!(plan_q(op, arg, Pkg::Apt, "").is_err(), "{op} {arg}");
        }
        assert_eq!(argv(&plan_q("service-disable", "bluetooth.service", Pkg::Apt, "").unwrap()), vec!["systemctl disable -- bluetooth.service"]);
        for op in OPS {
            assert!(action_id(op).is_some(), "{op} has a polkit action");
        }
    }

    #[test]
    fn install_files_match_the_code() {
        let policy = include_str!("../packaging/linux/dbus/org.aeroclean.helper.policy");
        for op in OPS {
            assert!(policy.contains(&format!("\"{}\"", action_id(op).unwrap())), "{op}'s action is in the polkit policy");
        }
        let service = include_str!("../packaging/linux/dbus/org.aeroclean.Helper1.service");
        assert!(service.contains(&format!("Name={BUS_NAME}")) && service.contains("Exec=/usr/bin/aero-cleaner --dbus-helper") && service.contains("User=root"));
        let conf = include_str!("../packaging/linux/dbus/org.aeroclean.Helper1.conf");
        assert!(conf.contains(&format!("own=\"{BUS_NAME}\"")) && conf.contains("<policy user=\"root\">"));
    }

    /// Records steps instead of running them.
    struct Recorder(Arc<Mutex<Vec<Step>>>);
    impl Exec for Recorder {
        fn query(&self, _: &[&str]) -> String {
            String::new()
        }
        fn step(&self, step: &Step) -> Result<String, String> {
            self.0.lock().unwrap().push(step.clone());
            Ok("done".into())
        }
    }

    /// The real D-Bus interface, over a private socket pair (no system bus,
    /// nothing runs): calls reach the helper, bad input comes back as an error.
    #[test]
    #[allow(deprecated)] // unix_stream: fine with the async-io runtime we use
    fn dbus_round_trip() {
        let (a, b) = std::os::unix::net::UnixStream::pair().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let helper = Helper {
            auth: Auth::AllowAll,
            exec: Box::new(Recorder(seen.clone())),
            pkg: Pkg::Dnf,
            last_call: Arc::new(Mutex::new(Instant::now())),
            busy: Arc::new(AtomicUsize::new(0)),
        };
        let server = std::thread::spawn(move || {
            zbus::blocking::connection::Builder::unix_stream(a).server(zbus::Guid::generate()).unwrap().p2p().serve_at(OBJECT_PATH, helper).unwrap().build().unwrap()
        });
        let client = zbus::blocking::connection::Builder::unix_stream(b).p2p().build().unwrap();
        let _server = server.join().unwrap();
        assert_eq!(call_on(&client, "package-cache", "").unwrap().trim(), "done");
        assert!(call_on(&client, "journal", "0").unwrap_err().contains("between 1 and 3650"));
        assert!(call_on(&client, "service-disable", "polkit.service").is_err());
        assert_eq!(*seen.lock().unwrap(), vec![run(&["dnf", "clean", "packages"])], "only the valid call ran");

        // Polkit mode with no caller identity (a private socket): refused.
        let (a, b) = std::os::unix::net::UnixStream::pair().unwrap();
        let strict = Helper {
            auth: Auth::Polkit,
            exec: Box::new(Recorder(seen.clone())),
            pkg: Pkg::Dnf,
            last_call: Arc::new(Mutex::new(Instant::now())),
            busy: Arc::new(AtomicUsize::new(0)),
        };
        let server = std::thread::spawn(move || {
            zbus::blocking::connection::Builder::unix_stream(a).server(zbus::Guid::generate()).unwrap().p2p().serve_at(OBJECT_PATH, strict).unwrap().build().unwrap()
        });
        let client = zbus::blocking::connection::Builder::unix_stream(b).p2p().build().unwrap();
        let _server = server.join().unwrap();
        assert_eq!(call_on(&client, "package-cache", "").unwrap_err(), "cancelled");
        assert_eq!(seen.lock().unwrap().len(), 1, "nothing more ran");
    }
}
