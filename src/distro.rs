//! Which Linux package manager this system uses (APT, DNF, pacman,
//! zypper), and how each one names, measures and cleans its package cache
//! and unused packages. Plus old Snap revisions where Snap is installed.

#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::process::{Command, Stdio};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pkg {
    Apt,
    Dnf,
    Pacman,
    Zypper,
    Unknown,
}

/// From /etc/os-release: ID first, then ID_LIKE ("ubuntu debian").
pub fn parse_os_release(text: &str) -> Pkg {
    let get = |k: &str| text.lines().find_map(|l| l.strip_prefix(&format!("{k}="))).map(|v| v.trim_matches('"').to_lowercase()).unwrap_or_default();
    let ids = format!("{} {}", get("ID"), get("ID_LIKE"));
    for id in ids.split_whitespace() {
        match id {
            "debian" | "ubuntu" | "linuxmint" | "pop" | "elementary" | "zorin" | "kali" | "raspbian" => return Pkg::Apt,
            "fedora" | "rhel" | "centos" | "rocky" | "almalinux" | "nobara" => return Pkg::Dnf,
            "arch" | "manjaro" | "endeavouros" | "garuda" | "cachyos" | "artix" => return Pkg::Pacman,
            "opensuse" | "opensuse-tumbleweed" | "opensuse-leap" | "suse" | "sles" => return Pkg::Zypper,
            _ => {}
        }
    }
    Pkg::Unknown
}

fn have(tool: &str) -> bool {
    crate::platform::find_tool(tool).is_some()
}

pub fn detect() -> Pkg {
    static P: OnceLock<Pkg> = OnceLock::new();
    *P.get_or_init(|| {
        let from_file = parse_os_release(&std::fs::read_to_string("/etc/os-release").unwrap_or_default());
        if from_file != Pkg::Unknown {
            return from_file;
        }
        // Unknown distro: go by the tool that's there.
        [("apt-get", Pkg::Apt), ("dnf", Pkg::Dnf), ("pacman", Pkg::Pacman), ("zypper", Pkg::Zypper)].into_iter().find(|(t, _)| have(t)).map(|(_, p)| p).unwrap_or(Pkg::Unknown)
    })
}

pub fn has_snap() -> bool {
    have("snap") && std::path::Path::new("/var/lib/snapd/snaps").is_dir()
}

/// (name, description) of the package cache and of unused packages.
pub fn labels(pkg: Pkg) -> Option<[(&'static str, &'static str); 2]> {
    Some(match pkg {
        Pkg::Apt => [
            ("APT package cache", "Downloaded .deb installers that are already installed"),
            ("Unused packages & old kernels", "Packages nothing depends on anymore (apt autoremove)"),
        ],
        Pkg::Dnf => [("DNF package cache", "Downloaded .rpm packages DNF keeps after installing"), ("Unused packages", "Packages nothing needs anymore (dnf autoremove)")],
        Pkg::Pacman => [
            ("Pacman package cache", "Old package versions, keeping the 2 newest of each"),
            ("Orphan packages", "Installed as dependencies, needed by nothing anymore (pacman -Qdt)"),
        ],
        Pkg::Zypper => [("Zypper package cache", "Downloaded .rpm packages zypper keeps"), ("Unused packages", "Packages nothing needs anymore (zypper)")],
        Pkg::Unknown => return None,
    })
}

/// Where downloaded packages are kept, and which file endings count.
pub fn cache_dir(pkg: Pkg) -> Option<(&'static str, &'static [&'static str])> {
    Some(match pkg {
        Pkg::Apt => ("/var/cache/apt/archives", &[".deb"]),
        Pkg::Dnf => ("/var/cache/dnf", &[".rpm"]),
        Pkg::Pacman => ("/var/cache/pacman/pkg", &[".pkg.tar.zst", ".pkg.tar.xz", ".pkg.tar.gz"]),
        Pkg::Zypper => ("/var/cache/zypp/packages", &[".rpm"]),
        Pkg::Unknown => return None,
    })
}

pub fn cache_preview(pkg: Pkg) -> String {
    match pkg {
        Pkg::Pacman => "Runs: paccache -rk2 (or pacman -Sc without paccache)".into(),
        _ => cache_dir(pkg).map(|(d, _)| format!("Deletes downloaded packages in {d}")).unwrap_or_default(),
    }
}

/// Root shell lines (run through pkexec with the other system items).
pub fn cache_script(pkg: Pkg) -> Option<&'static str> {
    Some(match pkg {
        Pkg::Apt => "echo '>> Cleaning APT cache'; apt-get clean",
        Pkg::Dnf => "echo '>> Cleaning DNF cache'; dnf clean packages",
        Pkg::Pacman => "echo '>> Cleaning pacman cache'; if command -v paccache >/dev/null; then paccache -rk2; else pacman -Sc --noconfirm; fi",
        Pkg::Zypper => "echo '>> Cleaning zypper cache'; zypper -n clean",
        Pkg::Unknown => return None,
    })
}

pub fn unused_script(pkg: Pkg) -> Option<&'static str> {
    Some(match pkg {
        Pkg::Apt => "echo '>> Removing unused packages'; DEBIAN_FRONTEND=noninteractive apt-get autoremove --purge -y",
        Pkg::Dnf => "echo '>> Removing unused packages'; dnf -y autoremove",
        Pkg::Pacman => "echo '>> Removing orphan packages'; orphans=$(pacman -Qdtq); if [ -n \"$orphans\" ]; then pacman -Rns --noconfirm $orphans; else echo 'No orphans'; fi",
        Pkg::Zypper => {
            "echo '>> Removing unused packages'; pkgs=$(zypper -n packages --unneeded | awk -F'|' 'NR > 2 && $1 ~ /i/ {gsub(/ /, \"\", $3); print $3}'); if [ -n \"$pkgs\" ]; then zypper -n rm $pkgs; else echo 'Nothing to remove'; fi"
        }
        Pkg::Unknown => return None,
    })
}

fn run_c(cmd: &str, args: &[&str]) -> String {
    let Some(bin) = crate::platform::find_tool(cmd) else { return String::new() };
    Command::new(bin).args(args).env("LC_ALL", "C").stderr(Stdio::null()).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

/// Packages the unused-packages clean would remove (names), and the space
/// it frees when the tool says.
pub fn unused_packages(pkg: Pkg) -> (Vec<String>, Option<u64>) {
    match pkg {
        Pkg::Apt => parse_apt_simulation(&run_c("apt-get", &["-s", "autoremove", "--purge"])),
        Pkg::Dnf => (run_c("dnf", &["repoquery", "--unneeded", "--qf", "%{name}\n"]).lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect(), None),
        Pkg::Pacman => (run_c("pacman", &["-Qdtq"]).lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect(), None),
        Pkg::Zypper => (parse_zypper_unneeded(&run_c("zypper", &["-n", "packages", "--unneeded"])), None),
        Pkg::Unknown => (vec![], None),
    }
}

fn parse_apt_simulation(out: &str) -> (Vec<String>, Option<u64>) {
    let pkgs = out.lines().filter_map(|l| l.strip_prefix("Purg ").or_else(|| l.strip_prefix("Remv "))).filter_map(|l| l.split_whitespace().next()).map(String::from).collect();
    let size = out
        .lines()
        .find(|l| l.contains("disk space will be freed"))
        .and_then(|l| l.strip_prefix("After this operation, "))
        .and_then(|l| l.split(" disk space").next())
        .and_then(crate::cleaner::parse_size);
    (pkgs, size)
}

/// `zypper packages --unneeded`: "S | Repository | Name | Version | Arch" rows.
fn parse_zypper_unneeded(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|l| {
            let cols: Vec<&str> = l.split('|').map(str::trim).collect();
            (cols.len() >= 3 && cols[0].starts_with('i')).then(|| cols[2].to_string())
        })
        .collect()
}

/// For pacman: what `paccache -dk2` says it would save, if paccache is there.
pub fn paccache_saving() -> Option<u64> {
    let out = run_c("paccache", &["-dk2"]);
    // "==> finished dry run: 12 candidates (disk space saved: 1.2 GiB)"
    let v = out.split("disk space saved: ").nth(1)?.trim_end_matches([')', '\n', ' ']);
    let v = v.trim_end_matches(')');
    let (num, unit) = v.split_once(' ')?;
    let n: f64 = num.parse().ok()?;
    let mult = match unit.trim_end_matches(')') {
        "KiB" => 1024.0,
        "MiB" => 1024.0 * 1024.0,
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        "B" => 1.0,
        _ => return None,
    };
    Some((n * mult) as u64)
}

// ---------- Snap ----------

/// Disabled (old) revisions: (name, revision).
pub fn parse_snap_list(out: &str) -> Vec<(String, String)> {
    out.lines()
        .skip(1)
        .filter(|l| l.split_whitespace().last().is_some_and(|notes| notes.split(',').any(|n| n == "disabled")))
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            Some((f.first()?.to_string(), f.get(2)?.to_string()))
        })
        .filter(|(n, r)| n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') && r.chars().all(|c| c.is_ascii_digit()))
        .collect()
}

pub fn snap_old_revisions() -> Vec<(String, String)> {
    parse_snap_list(&run_c("snap", &["list", "--all"]))
}

/// Size of the old revisions' .snap files.
pub fn snap_old_size(revs: &[(String, String)]) -> u64 {
    revs.iter().filter_map(|(n, r)| std::fs::metadata(format!("/var/lib/snapd/snaps/{n}_{r}.snap")).ok()).map(|m| m.len()).sum()
}

pub const SNAP_SCRIPT: &str =
    "echo '>> Removing old snap revisions'; snap list --all | awk '$NF ~ /disabled/ {print $1, $3}' | while read -r name rev; do snap remove \"$name\" --revision=\"$rev\"; done";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_the_package_manager() {
        assert_eq!(parse_os_release("NAME=\"Linux Mint\"\nID=linuxmint\nID_LIKE=\"ubuntu debian\"\n"), Pkg::Apt);
        assert_eq!(parse_os_release("ID=fedora\nVERSION_ID=41\n"), Pkg::Dnf);
        assert_eq!(parse_os_release("ID=\"endeavouros\"\nID_LIKE=\"arch\"\n"), Pkg::Pacman);
        assert_eq!(parse_os_release("ID=\"opensuse-tumbleweed\"\nID_LIKE=\"opensuse suse\"\n"), Pkg::Zypper);
        assert_eq!(parse_os_release("ID=somethingnew\nID_LIKE=fedora\n"), Pkg::Dnf, "ID_LIKE counts");
        assert_eq!(parse_os_release("ID=nixos\n"), Pkg::Unknown);
    }

    #[test]
    fn reads_tool_output() {
        let apt = "NOTE: simulation\nRemv libfoo1 [1.0]\nPurg linux-image-6.1 [6.1]\nAfter this operation, 412 MB disk space will be freed.\n";
        let (pkgs, size) = parse_apt_simulation(apt);
        assert_eq!(pkgs, vec!["libfoo1", "linux-image-6.1"]);
        assert_eq!(size, Some(412_000_000));
        let zypper = "Loading repository data...\nS | Repository | Name     | Version | Arch\n--+------------+----------+---------+-------\ni | Main       | libold2  | 2.0-1   | x86_64\ni+| Main       | tool-dep | 1.1-3   | noarch\n";
        assert_eq!(parse_zypper_unneeded(zypper), vec!["libold2", "tool-dep"]);
        let snap = "Name    Version  Rev   Tracking       Publisher   Notes\ncore22  20240111 1122  latest/stable  canonical✓  base,disabled\ncore22  20240408 1380  latest/stable  canonical✓  base\nfirefox 125.0    4173  latest/stable  mozilla✓    disabled\n";
        assert_eq!(parse_snap_list(snap), vec![("core22".into(), "1122".into()), ("firefox".into(), "4173".into())]);
    }

    #[test]
    fn every_manager_has_valid_root_scripts() {
        for pkg in [Pkg::Apt, Pkg::Dnf, Pkg::Pacman, Pkg::Zypper] {
            assert!(labels(pkg).is_some() && cache_dir(pkg).is_some());
            for s in [cache_script(pkg).unwrap(), unused_script(pkg).unwrap(), SNAP_SCRIPT] {
                if cfg!(unix) {
                    let ok = Command::new("sh").args(["-n", "-c", s]).status().unwrap().success();
                    assert!(ok, "bad shell for {pkg:?}: {s}");
                }
            }
        }
        assert!(labels(Pkg::Unknown).is_none());
    }
}
