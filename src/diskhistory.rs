//! Free space over time: one sample per hour at most (recorded by the app
//! and by the hourly low-disk check), a forecast of when the disk fills up,
//! and daily folder-size snapshots to find what grew.
//!
//! Kept small on purpose: hourly samples for 30 days, then one per day up to
//! 400 days (about 1,100 lines), and folder snapshots for 60 days.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
const HOUR: u64 = 3600;
const DAY: u64 = 86400;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    /// Unix time, seconds.
    pub when: u64,
    pub total: u64,
    pub avail: u64,
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn history_file() -> PathBuf {
    crate::platform::data_dir().join("disk-history.tsv")
}

fn parse(text: &str) -> Vec<Sample> {
    text.lines()
        .filter_map(|l| {
            let mut f = l.split('\t').map(|v| v.parse::<u64>());
            Some(Sample { when: f.next()?.ok()?, total: f.next()?.ok()?, avail: f.next()?.ok()? })
        })
        .collect()
}

pub fn load() -> Vec<Sample> {
    let mut v = parse(&fs::read_to_string(history_file()).unwrap_or_default());
    v.sort_by_key(|s| s.when);
    v
}

/// Hourly samples for the last 30 days, then the last sample of each day.
fn compact(samples: &[Sample], now: u64) -> Vec<Sample> {
    let mut out: Vec<Sample> = Vec::with_capacity(samples.len());
    for s in samples.iter().filter(|s| now.saturating_sub(s.when) < 400 * DAY) {
        let old = now.saturating_sub(s.when) >= 30 * DAY;
        match out.last_mut() {
            Some(last) if old && last.when / DAY == s.when / DAY => *last = *s,
            _ => out.push(*s),
        }
    }
    out
}

/// Records free space, at most once every 50 minutes.
pub fn record(total: u64, avail: u64) {
    record_at(&history_file(), Sample { when: now(), total, avail });
}

fn record_at(file: &Path, sample: Sample) {
    let mut samples = parse(&fs::read_to_string(file).unwrap_or_default());
    samples.sort_by_key(|s| s.when);
    if samples.last().is_some_and(|l| sample.when.saturating_sub(l.when) < 50 * 60) {
        return;
    }
    samples.push(sample);
    let samples = compact(&samples, sample.when);
    let text: String = samples.iter().map(|s| format!("{}\t{}\t{}\n", s.when, s.total, s.avail)).collect();
    let _ = fs::create_dir_all(file.parent().unwrap_or(Path::new(".")));
    // Write then rename, so a crash never leaves a half-written history.
    let tmp = file.with_extension("tmp");
    if fs::write(&tmp, text).is_ok() {
        let _ = fs::rename(&tmp, file);
    }
}

// ---------- Forecast ----------

#[derive(Debug, PartialEq)]
pub enum Forecast {
    /// Less than three days of history.
    NotEnoughData,
    /// Free space isn't going down.
    NotFilling,
    /// At the current pace the disk is full in `days`; it loses `per_day` bytes a day.
    FullIn { days: f64, per_day: u64 },
}

/// Fits a straight line through the last 30 days of free space.
pub fn forecast(samples: &[Sample], now: u64) -> Forecast {
    let recent: Vec<&Sample> = samples.iter().filter(|s| now.saturating_sub(s.when) <= 30 * DAY).collect();
    let (Some(first), Some(last)) = (recent.first(), recent.last()) else { return Forecast::NotEnoughData };
    if recent.len() < 6 || last.when - first.when < 3 * DAY {
        return Forecast::NotEnoughData;
    }
    let n = recent.len() as f64;
    let xs: Vec<f64> = recent.iter().map(|s| (s.when - first.when) as f64 / DAY as f64).collect();
    let ys: Vec<f64> = recent.iter().map(|s| s.avail as f64).collect();
    let (mx, my) = (xs.iter().sum::<f64>() / n, ys.iter().sum::<f64>() / n);
    let var: f64 = xs.iter().map(|x| (x - mx).powi(2)).sum();
    if var <= 0.0 {
        return Forecast::NotEnoughData;
    }
    let slope = xs.iter().zip(&ys).map(|(x, y)| (x - mx) * (y - my)).sum::<f64>() / var; // bytes per day
    // Less than 50 MB a day lost is noise (caches come and go).
    if slope > -50_000_000.0 {
        return Forecast::NotFilling;
    }
    Forecast::FullIn { days: last.avail as f64 / -slope, per_day: (-slope) as u64 }
}

/// "about 3 weeks", "about 4 months" (English; translated by the caller).
pub fn humanize_days(days: f64) -> String {
    match days {
        d if d < 1.5 => "about a day".into(),
        d if d < 14.0 => format!("about {} days", d.round() as u64),
        d if d < 60.0 => format!("about {} weeks", (d / 7.0).round() as u64),
        d if d < 730.0 => format!("about {} months", (d / 30.4).round() as u64),
        _ => "more than 2 years".into(),
    }
}

/// One line for the low-disk alert and the Disk screen.
pub fn forecast_sentence(f: &Forecast) -> Option<String> {
    match f {
        // The duration is translated on its own, so the sentence's template still matches.
        Forecast::FullIn { days, .. } => Some(format!("At this rate your disk is full in {}.", crate::i18n::tr(humanize_days(*days)))),
        _ => None,
    }
}

// ---------- Folder snapshots (growth finder) ----------

/// Folder sizes relative to home, from one day.
#[derive(Debug, Default, PartialEq)]
pub struct Snapshot {
    /// Days since 1970.
    pub day: u64,
    pub folders: Vec<(String, u64)>,
}

fn snapshot_dir() -> PathBuf {
    crate::platform::data_dir().join("snapshots")
}

/// Folders worth remembering: every top-level folder, and deeper ones
/// (down to 4 levels) of at least 20 MB.
pub fn snapshot_from_tree(tree: &crate::diskmap::Node, day: u64) -> Snapshot {
    fn walk(node: &crate::diskmap::Node, prefix: &str, depth: usize, out: &mut Vec<(String, u64)>) {
        for c in node.children.iter().filter(|c| c.is_dir) {
            let rel = if prefix.is_empty() { c.name.clone() } else { format!("{prefix}/{}", c.name) };
            if depth == 0 || c.size >= 20_000_000 {
                out.push((rel.clone(), c.size));
                if depth < 3 {
                    walk(c, &rel, depth + 1, out);
                }
            }
        }
    }
    let mut folders = Vec::new();
    walk(tree, "", 0, &mut folders);
    Snapshot { day, folders }
}

pub fn save_snapshot(s: &Snapshot) {
    save_snapshot_in(&snapshot_dir(), s);
}

fn save_snapshot_in(dir: &Path, s: &Snapshot) {
    let _ = fs::create_dir_all(dir);
    let text: String = s.folders.iter().filter(|(p, _)| !p.contains(['\t', '\n'])).map(|(p, size)| format!("{size}\t{p}\n")).collect();
    let _ = fs::write(dir.join(format!("day-{}.tsv", s.day)), text);
    // Keep 60 days.
    for (day, path) in list_snapshots(dir) {
        if day + 60 < s.day {
            let _ = fs::remove_file(path);
        }
    }
}

fn list_snapshots(dir: &Path) -> Vec<(u64, PathBuf)> {
    let mut v: Vec<(u64, PathBuf)> = fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    let day = name.strip_prefix("day-")?.strip_suffix(".tsv")?.parse().ok()?;
                    Some((day, e.path()))
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn read_snapshot(day: u64, path: &Path) -> Snapshot {
    let folders = fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let (size, p) = l.split_once('\t')?;
            Some((p.to_string(), size.parse().ok()?))
        })
        .collect();
    Snapshot { day, folders }
}

/// Is there a snapshot for `day` already?
pub fn has_snapshot(day: u64) -> bool {
    list_snapshots(&snapshot_dir()).iter().any(|(d, _)| *d == day)
}

/// The newest snapshot and the one closest to a week before it (at least two days older).
pub fn snapshots_to_compare() -> Option<(Snapshot, Snapshot)> {
    pick_pair(&snapshot_dir())
}

fn pick_pair(dir: &Path) -> Option<(Snapshot, Snapshot)> {
    let list = list_snapshots(dir);
    let (new_day, new_path) = list.last()?;
    let (old_day, old_path) = list.iter().filter(|(d, _)| d + 2 <= *new_day).min_by_key(|(d, _)| (*new_day - d).abs_diff(7))?;
    Some((read_snapshot(*new_day, new_path), read_snapshot(*old_day, old_path)))
}

/// Folders that grew the most between two snapshots, most specific first:
/// a parent is dropped when one of its listed children explains most of
/// its growth.
pub fn growth(new: &Snapshot, old: &Snapshot, limit: usize) -> Vec<(String, i64)> {
    let before: std::collections::HashMap<&str, u64> = old.folders.iter().map(|(p, s)| (p.as_str(), *s)).collect();
    let mut grew: Vec<(String, i64)> =
        new.folders.iter().map(|(p, s)| (p.clone(), *s as i64 - before.get(p.as_str()).copied().unwrap_or(0) as i64)).filter(|(_, d)| *d >= 50_000_000).collect();
    grew.sort_by_key(|(_, d)| std::cmp::Reverse(*d));
    let snapshot = grew.clone();
    grew.retain(|(p, d)| {
        let prefix = format!("{p}/");
        !snapshot.iter().any(|(c, cd)| c.starts_with(&prefix) && *cd as f64 >= *d as f64 * 0.8)
    });
    grew.truncate(limit);
    grew
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(when: u64, avail: u64) -> Sample {
        Sample { when, total: 500_000_000_000, avail }
    }

    #[test]
    fn records_at_most_hourly_and_compacts_old_days() {
        let dir = std::env::temp_dir().join(format!("aero-hist-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let file = dir.join("disk-history.tsv");
        let start = 1_000 * DAY;
        record_at(&file, s(start, 100));
        record_at(&file, s(start + 10 * 60, 99)); // 10 minutes later: ignored
        record_at(&file, s(start + HOUR, 98));
        assert_eq!(parse(&fs::read_to_string(&file).unwrap()).len(), 2);

        // 40 days of hourly samples: the first ~10 days shrink to one per day.
        let samples: Vec<Sample> = (0..40 * 24).map(|h| s(start + h * HOUR, 1_000 - h)).collect();
        let now = start + 40 * DAY;
        let c = compact(&samples, now);
        let old = c.iter().filter(|x| now - x.when >= 30 * DAY).count();
        assert!((10..=11).contains(&old), "one per day for old days, got {old}");
        // Samples exactly 30 days old count as old, hence 30 * 24 - 1.
        assert_eq!(c.iter().filter(|x| now - x.when < 30 * DAY).count(), 30 * 24 - 1, "recent stays hourly");
        assert!(compact(&[s(now - 500 * DAY, 5)], now).is_empty(), "older than 400 days dropped");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn forecasts_from_the_trend() {
        let start = 2_000 * DAY;
        // Losing 1 GB a day, 100 GB left: full in ~100 days.
        let filling: Vec<Sample> = (0..10 * 24).step_by(6).map(|h| s(start + h * HOUR, 110_000_000_000 - h * 1_000_000_000 / 24)).collect();
        let now = filling.last().unwrap().when;
        match forecast(&filling, now) {
            Forecast::FullIn { days, per_day } => {
                assert!((days - 100.0).abs() < 2.0, "{days}");
                assert!((per_day as f64 - 1e9).abs() < 2e7);
            }
            f => panic!("{f:?}"),
        }
        assert_eq!(humanize_days(100.0), "about 3 months");
        let flat: Vec<Sample> = (0..10).map(|d| s(start + d * DAY, 50_000_000_000 + (d % 2) * 10_000_000)).collect();
        assert_eq!(forecast(&flat, start + 9 * DAY), Forecast::NotFilling);
        assert_eq!(forecast(&filling[..3], now), Forecast::NotEnoughData);
    }

    #[test]
    fn growth_points_at_the_folder_that_grew() {
        let snap = |day, f: &[(&str, u64)]| Snapshot { day, folders: f.iter().map(|(p, s)| (p.to_string(), *s)).collect() };
        let old = snap(10, &[("Videos", 1_000_000_000), ("Videos/Obs", 100_000_000), ("Downloads", 500_000_000), (".cache", 900_000_000)]);
        let new = snap(17, &[("Videos", 4_000_000_000), ("Videos/Obs", 3_050_000_000), ("Downloads", 520_000_000), (".cache", 1_300_000_000), ("Games", 200_000_000)]);
        let g = growth(&new, &old, 10);
        assert_eq!(
            g,
            vec![("Videos/Obs".to_string(), 2_950_000_000), (".cache".to_string(), 400_000_000), ("Games".to_string(), 200_000_000)],
            "Videos is explained by Videos/Obs; Downloads grew too little"
        );

        let dir = std::env::temp_dir().join(format!("aero-snap-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        for day in [100, 103, 106, 109] {
            save_snapshot_in(&dir, &snap(day, &[("A", day * 1000)]));
        }
        let (n, o) = pick_pair(&dir).unwrap();
        assert_eq!((n.day, o.day), (109, 103), "closest to a week before");
        save_snapshot_in(&dir, &snap(200, &[]));
        assert_eq!(list_snapshots(&dir).len(), 1, "older than 60 days removed");
        fs::remove_dir_all(&dir).unwrap();
    }
}
