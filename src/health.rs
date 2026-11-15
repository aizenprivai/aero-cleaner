//! Disk health, read without a password: udisks2 on Linux (SMART for SATA
//! drives, NVMe health and wear), Storage Spaces on Windows, diskutil on
//! macOS. Each source gives what it can; missing values stay `None`.

use std::process::Stdio;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct DiskHealth {
    pub model: String,
    pub size: u64,
    /// Some(false) when the drive itself reports a problem.
    pub healthy: Option<bool>,
    /// Short reason when not healthy ("reports failing", "spare space low"...).
    pub problem: String,
    /// SSD wear: share of its rated life used, in % (can exceed 100).
    pub wear_percent: Option<u8>,
    pub temperature_c: Option<f32>,
    pub power_on_hours: Option<u64>,
    pub bytes_written: Option<u64>,
}

fn run(cmd: &str, args: &[&str]) -> String {
    let Some(bin) = crate::platform::find_tool(cmd) else { return String::new() };
    crate::platform::command(bin).args(args).env("LC_ALL", "C").stderr(Stdio::null()).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

pub fn read() -> Vec<DiskHealth> {
    if crate::platform::IS_WINDOWS {
        parse_windows(&run("powershell", &["-NoProfile", "-NonInteractive", "-Command", WINDOWS_QUERY]))
    } else if crate::platform::IS_MACOS {
        read_macos()
    } else {
        read_linux()
    }
}

// ---------- Linux: udisks2 ----------

/// One drive object from `udisksctl dump`: its D-Bus path and the
/// properties of its Drive, Drive.Ata and NVMe.Controller interfaces.
#[derive(Debug, Default)]
struct UdisksDrive {
    object: String,
    props: std::collections::HashMap<String, String>,
    nvme: bool,
    ata: bool,
}

fn parse_udisks_dump(text: &str) -> Vec<UdisksDrive> {
    let mut drives: Vec<UdisksDrive> = Vec::new();
    let mut iface = String::new();
    let mut in_drive = false;
    for line in text.lines() {
        if !line.starts_with(' ') {
            // A new object, e.g. "/org/freedesktop/UDisks2/drives/Micron_...:".
            iface.clear();
            let obj = line.strip_suffix(':').filter(|o| o.starts_with("/org/freedesktop/UDisks2/drives/"));
            in_drive = obj.is_some();
            if let Some(obj) = obj {
                drives.push(UdisksDrive { object: obj.to_string(), ..Default::default() });
            }
            continue;
        }
        let Some(d) = drives.last_mut().filter(|_| in_drive) else { continue };
        let t = line.trim();
        if line.starts_with("  ") && !line.starts_with("    ") {
            iface = t.trim_end_matches(':').to_string();
            d.nvme |= iface == "org.freedesktop.UDisks2.NVMe.Controller";
            d.ata |= iface == "org.freedesktop.UDisks2.Drive.Ata";
            continue;
        }
        if let Some((k, v)) = t.split_once(':') {
            let short = iface.trim_start_matches("org.freedesktop.UDisks2.");
            d.props.insert(format!("{short}.{}", k.trim()), v.trim().to_string());
        }
    }
    drives
}

/// `busctl call ... SmartGetAttributes` output: `a{sv} 15 "avail_spare" y 100 ...`.
fn parse_busctl_attrs(text: &str) -> std::collections::HashMap<String, u64> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let mut out = std::collections::HashMap::new();
    for w in tokens.windows(3) {
        if let Some(name) = w[0].strip_prefix('"').and_then(|n| n.strip_suffix('"'))
            && matches!(w[1], "y" | "q" | "u" | "t")
            && let Ok(v) = w[2].parse()
        {
            out.insert(name.to_string(), v);
        }
    }
    out
}

fn kelvin(v: &str) -> Option<f32> {
    v.parse::<f32>().ok().filter(|k| *k > 200.0).map(|k| k - 273.15)
}

fn drive_from_udisks(d: &UdisksDrive, nvme_attrs: Option<&std::collections::HashMap<String, u64>>) -> Option<DiskHealth> {
    let p = |k: &str| d.props.get(k).map(String::as_str).unwrap_or("");
    // Skip USB sticks and card readers: removable media.
    if p("Drive.Removable") == "true" || p("Drive.MediaRemovable") == "true" || !(d.nvme || d.ata) {
        return None;
    }
    let mut h = DiskHealth { model: p("Drive.Model").replace('_', " "), size: p("Drive.Size").parse().unwrap_or(0), ..Default::default() };
    if d.nvme {
        let warning = p("NVMe.Controller.SmartCriticalWarning");
        h.healthy = Some(warning.is_empty());
        if !warning.is_empty() {
            h.problem = format!("drive warning: {warning}");
        }
        h.temperature_c = kelvin(p("NVMe.Controller.SmartTemperature"));
        h.power_on_hours = p("NVMe.Controller.SmartPowerOnHours").parse().ok();
        if let Some(a) = nvme_attrs {
            h.wear_percent = a.get("percent_used").map(|v| (*v).min(255) as u8);
            // NVMe counts data in units of 1000 blocks of 512 bytes; udisks already converts to bytes.
            h.bytes_written = a.get("total_data_written").copied();
            if a.get("media_errors").is_some_and(|e| *e > 0) && h.problem.is_empty() {
                h.problem = "media errors reported".into();
            }
        }
    } else if p("Drive.Ata.SmartSupported") == "true" && p("Drive.Ata.SmartEnabled") == "true" {
        let failing = p("Drive.Ata.SmartFailing") == "true";
        let bad = p("Drive.Ata.SmartNumBadSectors").parse::<u64>().unwrap_or(0);
        h.healthy = Some(!failing);
        h.problem = if failing {
            "the drive reports it is failing".into()
        } else if bad > 0 {
            format!("{bad} bad sectors")
        } else {
            String::new()
        };
        h.temperature_c = kelvin(p("Drive.Ata.SmartTemperature"));
        h.power_on_hours = p("Drive.Ata.SmartPowerOnSeconds").parse::<u64>().ok().map(|s| s / 3600);
    }
    Some(h)
}

fn read_linux() -> Vec<DiskHealth> {
    parse_udisks_dump(&run("udisksctl", &["dump"]))
        .iter()
        .filter_map(|d| {
            let attrs = d.nvme.then(|| {
                parse_busctl_attrs(&run(
                    "busctl",
                    &["--system", "call", "org.freedesktop.UDisks2", &d.object, "org.freedesktop.UDisks2.NVMe.Controller", "SmartGetAttributes", "a{sv}", "0"],
                ))
            });
            drive_from_udisks(d, attrs.as_ref())
        })
        .collect()
}

// ---------- Windows: Storage Spaces ----------

/// HealthStatus works for everyone; wear and temperature need admin rights,
/// so they're simply empty for normal users.
const WINDOWS_QUERY: &str = "Get-PhysicalDisk | Where-Object BusType -ne 'USB' | ForEach-Object { $r = $_ | Get-StorageReliabilityCounter -ErrorAction SilentlyContinue; \
     '{0}|{1}|{2}|{3}|{4}|{5}' -f $_.FriendlyName, $_.Size, $_.HealthStatus, $r.Wear, $r.Temperature, $r.PowerOnHours }";

fn parse_windows(text: &str) -> Vec<DiskHealth> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.trim().split('|').collect();
            if f.len() < 6 || f[0].is_empty() {
                return None;
            }
            let status = f[2].trim();
            let num = |s: &str| s.trim().parse::<u64>().ok();
            Some(DiskHealth {
                model: f[0].trim().to_string(),
                size: num(f[1]).unwrap_or(0),
                healthy: match status {
                    "Healthy" | "0" => Some(true),
                    "" => None,
                    _ => Some(false),
                },
                problem: if status.is_empty() || status == "Healthy" || status == "0" { String::new() } else { format!("Windows reports: {status}") },
                wear_percent: num(f[3]).map(|w| w.min(255) as u8),
                temperature_c: num(f[4]).filter(|t| *t > 0).map(|t| t as f32),
                power_on_hours: num(f[5]),
                bytes_written: None,
            })
        })
        .collect()
}

// ---------- macOS: diskutil ----------

fn read_macos() -> Vec<DiskHealth> {
    // Internal physical disks only.
    let list = run("diskutil", &["list", "internal", "physical"]);
    list.lines()
        .filter_map(|l| l.strip_prefix("/dev/").and_then(|r| r.split_whitespace().next()))
        .filter_map(|disk| parse_diskutil_info(&run("diskutil", &["info", disk])))
        .collect()
}

fn parse_diskutil_info(text: &str) -> Option<DiskHealth> {
    let get = |k: &str| text.lines().find_map(|l| l.trim().strip_prefix(k).map(|v| v.trim_start_matches(':').trim().to_string()));
    let smart = get("SMART Status")?;
    let size = get("Disk Size").and_then(|v| v.split('(').nth(1)?.split_whitespace().next()?.parse().ok()).unwrap_or(0);
    let healthy = match smart.as_str() {
        "Verified" => Some(true),
        "Failing" => Some(false),
        _ => None,
    };
    Some(DiskHealth {
        model: get("Device / Media Name").unwrap_or_else(|| "Internal disk".into()),
        size,
        healthy,
        problem: if healthy == Some(false) { "the drive reports it is failing".into() } else { String::new() },
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DUMP: &str = "/org/freedesktop/UDisks2/block_devices/nvme0n1:
  org.freedesktop.UDisks2.Block:
    Size:                       512110190592
/org/freedesktop/UDisks2/drives/Micron_2210_MTFDHBA512QFD_21212F0F4B7D:
  org.freedesktop.UDisks2.Drive:
    MediaRemovable:             false
    Model:                      Micron_2210_MTFDHBA512QFD
    Removable:                  false
    Size:                       512110190592
  org.freedesktop.UDisks2.NVMe.Controller:
    SmartCriticalWarning:
    SmartPowerOnHours:                  5848
    SmartTemperature:                   316
/org/freedesktop/UDisks2/block_devices/sda:
  org.freedesktop.UDisks2.Block:
    Size:                       42
/org/freedesktop/UDisks2/drives/WDC_WD10EZEX:
  org.freedesktop.UDisks2.Drive:
    Model:                      WDC WD10EZEX
    Removable:                  false
    Size:                       1000204886016
  org.freedesktop.UDisks2.Drive.Ata:
    SmartEnabled:               true
    SmartFailing:               false
    SmartNumBadSectors:         3
    SmartPowerOnSeconds:        36000000
    SmartSupported:             true
    SmartTemperature:           308.15
/org/freedesktop/UDisks2/drives/SanDisk_Cruzer:
  org.freedesktop.UDisks2.Drive:
    Model:                      Cruzer
    Removable:                  true
";

    #[test]
    fn reads_udisks_nvme_and_sata() {
        let drives = parse_udisks_dump(DUMP);
        assert_eq!(drives.len(), 3);
        let attrs = parse_busctl_attrs("a{sv} 3 \"avail_spare\" y 100 \"percent_used\" y 9 \"total_data_written\" t 25373869056000 \"media_errors\" t 0");
        let nvme = drive_from_udisks(&drives[0], Some(&attrs)).unwrap();
        assert_eq!((nvme.model.as_str(), nvme.healthy, nvme.wear_percent, nvme.power_on_hours), ("Micron 2210 MTFDHBA512QFD", Some(true), Some(9), Some(5848)));
        assert!((nvme.temperature_c.unwrap() - 42.85).abs() < 0.01);
        assert_eq!(nvme.bytes_written, Some(25_373_869_056_000));
        let hdd = drive_from_udisks(&drives[1], None).unwrap();
        assert_eq!((hdd.healthy, hdd.power_on_hours, hdd.problem.as_str()), (Some(true), Some(10_000), "3 bad sectors"));
        assert!(drive_from_udisks(&drives[2], None).is_none(), "USB stick skipped");
    }

    #[test]
    fn reads_windows_and_macos_output() {
        let w = parse_windows("Samsung SSD 980 PRO 1TB|1000204886016|Healthy|3|41|1200\r\nST2000DM008|2000398934016|Warning|||\r\n");
        assert_eq!(w.len(), 2);
        assert_eq!((w[0].healthy, w[0].wear_percent, w[0].temperature_c, w[0].power_on_hours), (Some(true), Some(3), Some(41.0), Some(1200)));
        assert_eq!((w[1].healthy, w[1].wear_percent, w[1].problem.as_str()), (Some(false), None, "Windows reports: Warning"));
        let m = parse_diskutil_info("   Device Identifier:         disk0\n   Device / Media Name:       APPLE SSD AP0512Z\n   Disk Size:                 500.3 GB (500277792768 Bytes) (exactly 977105064 512-Byte-Units)\n   SMART Status:              Verified\n").unwrap();
        assert_eq!((m.model.as_str(), m.size, m.healthy), ("APPLE SSD AP0512Z", 500_277_792_768, Some(true)));
        assert!(parse_diskutil_info("   Device Identifier: disk4\n").is_none(), "no SMART line: not a physical disk");
    }
}
