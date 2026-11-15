//! Custom cleaning rules: "files matching *pattern* in *folder* older than
//! *N days*". Stored as a small text file anyone can read, edit and share:
//!
//! ```text
//! [Old ISO files]
//! folder = ~/Downloads
//! pattern = *.iso;*.img
//! older_than_days = 30
//! subfolders = no
//! enabled = yes
//! ```
//!
//! Safety: rules only delete files (never folders), only inside the home
//! folder (never the home folder itself), never follow symbolic links,
//! respect "Folders to keep", and go through the undo window like the rest.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub name: String,
    /// As typed, usually starting with "~".
    pub folder: String,
    /// Wildcards `*` and `?`; several patterns separated by ";".
    pub pattern: String,
    pub older_than_days: u32,
    pub subfolders: bool,
    /// Ticked in the Cleaner.
    pub enabled: bool,
    /// Where it came from ("" for the user's own, "BleachBit" for imports).
    pub source: String,
}

impl Default for Rule {
    fn default() -> Self {
        Rule { name: String::new(), folder: "~/Downloads".into(), pattern: "*.iso".into(), older_than_days: 30, subfolders: false, enabled: true, source: String::new() }
    }
}

fn rules_file() -> PathBuf {
    crate::platform::config_dir().join("rules.txt")
}

pub fn parse(text: &str) -> Vec<Rule> {
    let mut rules: Vec<Rule> = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            rules.push(Rule { name: name.trim().to_string(), folder: String::new(), pattern: String::new(), ..Default::default() });
            continue;
        }
        let (Some(r), Some((k, v))) = (rules.last_mut(), line.split_once('=')) else { continue };
        let v = v.trim();
        let yes = matches!(v.to_ascii_lowercase().as_str(), "yes" | "true" | "1" | "on");
        match k.trim() {
            "folder" => r.folder = v.to_string(),
            "pattern" => r.pattern = v.to_string(),
            "older_than_days" => r.older_than_days = v.parse().unwrap_or(0),
            "subfolders" => r.subfolders = yes,
            "enabled" => r.enabled = yes,
            "source" => r.source = v.to_string(),
            _ => {}
        }
    }
    rules.retain(|r| !r.folder.is_empty() && !r.pattern.is_empty());
    rules
}

pub fn to_text(rules: &[Rule]) -> String {
    let mut out =
        String::from("# AeroClean cleaning rules. Each one deletes matching files (never folders)\n# inside your home folder. Share this file; imported rules start unticked.\n");
    for r in rules {
        let yn = |b: bool| if b { "yes" } else { "no" };
        out.push_str(&format!(
            "\n[{}]\nfolder = {}\npattern = {}\nolder_than_days = {}\nsubfolders = {}\nenabled = {}\n",
            r.name.replace(['[', ']', '\n'], ""),
            r.folder,
            r.pattern,
            r.older_than_days,
            yn(r.subfolders),
            yn(r.enabled)
        ));
        if !r.source.is_empty() {
            out.push_str(&format!("source = {}\n", r.source));
        }
    }
    out
}

pub fn load() -> Vec<Rule> {
    parse(&fs::read_to_string(rules_file()).unwrap_or_default())
}

pub fn save(rules: &[Rule]) {
    let _ = fs::create_dir_all(crate::platform::config_dir());
    let _ = fs::write(rules_file(), to_text(rules));
}

/// Adds rules from a shared file, unticked, skipping ones already there.
pub fn merge_imported(existing: &mut Vec<Rule>, imported: Vec<Rule>) -> usize {
    let mut added = 0;
    for mut r in imported {
        if existing.iter().any(|e| e.folder == r.folder && e.pattern == r.pattern && e.older_than_days == r.older_than_days) {
            continue;
        }
        r.enabled = false;
        existing.push(r);
        added += 1;
    }
    added
}

/// Saves the rules as a file in Documents (or home) to share.
pub fn export(rules: &[Rule]) -> Result<PathBuf, String> {
    let home = crate::platform::home();
    let dir = [home.join("Documents"), home.clone()].into_iter().find(|d| d.is_dir()).unwrap_or(home);
    let path = dir.join("AeroClean-rules.txt");
    fs::write(&path, to_text(rules)).map_err(|e| e.to_string())?;
    Ok(path)
}

/// "~/Downloads" -> /home/me/Downloads.
pub fn expand(folder: &str, home: &Path) -> PathBuf {
    match folder.strip_prefix('~') {
        Some(rest) => home.join(rest.trim_start_matches(['/', '\\'])),
        None => PathBuf::from(folder),
    }
}

/// Why a rule can't run, if it can't.
pub fn check(rule: &Rule, home: &Path) -> Result<PathBuf, String> {
    if rule.name.trim().is_empty() {
        return Err("Give the rule a name.".into());
    }
    let patterns: Vec<&str> = rule.pattern.split(';').map(str::trim).filter(|p| !p.is_empty()).collect();
    if patterns.is_empty() {
        return Err("Type a pattern, like *.iso".into());
    }
    if patterns.iter().any(|p| p.contains(['/', '\\'])) {
        return Err("The pattern is for file names only; put the folder in Folder.".into());
    }
    let dir = expand(rule.folder.trim(), home);
    if !dir.is_absolute() {
        return Err("The folder must start with ~ or /.".into());
    }
    // No ".." tricks: the folder must really be inside home.
    if dir.components().any(|c| matches!(c, std::path::Component::ParentDir)) || !dir.starts_with(home) {
        return Err("Rules only work inside your home folder.".into());
    }
    if dir == home {
        return Err("Pick a folder inside your home folder, not the whole home folder.".into());
    }
    Ok(dir)
}

/// Wildcard match (`*`, `?`), ignoring case.
pub fn wildcard(name: &str, pattern: &str) -> bool {
    let n: Vec<char> = name.to_lowercase().chars().collect();
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let (mut i, mut j, mut star, mut mark) = (0, 0, None, 0);
    while i < n.len() {
        if j < p.len() && (p[j] == '?' || p[j] == n[i]) {
            i += 1;
            j += 1;
        } else if j < p.len() && p[j] == '*' {
            star = Some(j);
            mark = i;
            j += 1;
        } else if let Some(s) = star {
            j = s + 1;
            mark += 1;
            i = mark;
        } else {
            return false;
        }
    }
    p[j..].iter().all(|c| *c == '*')
}

fn matches_any(name: &str, pattern: &str) -> bool {
    pattern.split(';').map(str::trim).filter(|p| !p.is_empty()).any(|p| wildcard(name, p))
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuleScan {
    pub files: Vec<(PathBuf, u64)>,
    pub size: u64,
    /// Set when the rule can't run.
    pub error: String,
}

/// Files the rule would delete, biggest first.
pub fn scan(rule: &Rule, exclusions: &[PathBuf]) -> RuleScan {
    scan_in(rule, exclusions, &crate::platform::home(), SystemTime::now())
}

fn scan_in(rule: &Rule, exclusions: &[PathBuf], home: &Path, now: SystemTime) -> RuleScan {
    let dir = match check(rule, home) {
        Ok(d) => d,
        Err(e) => return RuleScan { error: e, ..Default::default() },
    };
    let cutoff = now.checked_sub(Duration::from_secs(rule.older_than_days as u64 * 86_400)).unwrap_or(SystemTime::UNIX_EPOCH);
    let mut files = Vec::new();
    let mut stack = vec![dir];
    while let Some(d) = stack.pop() {
        if exclusions.iter().any(|x| d.starts_with(x)) {
            continue;
        }
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            let path = e.path();
            if ft.is_dir() {
                if rule.subfolders {
                    stack.push(path);
                }
            } else if ft.is_file()
                && matches_any(&e.file_name().to_string_lossy(), &rule.pattern)
                && !exclusions.iter().any(|x| path.starts_with(x))
                && let Ok(md) = e.metadata()
                && md.modified().is_ok_and(|m| rule.older_than_days == 0 || m <= cutoff)
            {
                files.push((path, md.len()));
            }
        }
    }
    files.sort_by_key(|f| std::cmp::Reverse(f.1));
    let size = files.iter().map(|f| f.1).sum();
    RuleScan { files, size, error: String::new() }
}

/// "*.iso in ~/Downloads, older than 30 days", in the app's language.
pub fn describe(rule: &Rule) -> String {
    use crate::i18n::tr;
    let age = match rule.older_than_days {
        0 => String::new(),
        1 => format!(", {}", tr("older than 1 day")),
        n => format!(", {}", tr(format!("older than {n} days"))),
    };
    let sub = if rule.subfolders { format!(" {}", tr("and its subfolders")) } else { String::new() };
    format!("{} {} {}{sub}{age}", rule.pattern, tr("in"), rule.folder)
}

/// Review lines: the biggest matches.
pub fn preview(scan: &RuleScan, home: &Path) -> Vec<String> {
    if !scan.error.is_empty() {
        return vec![scan.error.clone()];
    }
    let pretty = |p: &Path| p.to_string_lossy().replacen(&*home.to_string_lossy(), "~", 1);
    let mut lines: Vec<String> = scan.files.iter().take(6).map(|(p, s)| format!("{}  ({})", pretty(p), crate::cleaner::format_bytes(*s))).collect();
    if scan.files.len() > 6 {
        lines.push(format!("…and {} more", scan.files.len() - 6));
    }
    if lines.is_empty() {
        lines.push("No matching files right now".into());
    }
    lines
}

/// Deletes (or holds for undo) what `scan` found, re-checking each file.
pub fn clean(rule: &Rule, exclusions: &[PathBuf], batch: Option<&crate::holding::Batch>) -> usize {
    let found = scan(rule, exclusions);
    found.files.iter().filter(|(p, _)| crate::holding::dispose(p, false, batch).is_err()).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards() {
        assert!(wildcard("Ubuntu-24.04.ISO", "*.iso"));
        assert!(wildcard("photo1.jpg", "photo?.jpg") && !wildcard("photo10.jpg", "photo?.jpg"));
        assert!(wildcard("a.tar.gz", "*.tar.*") && !wildcard("a.zip", "*.tar.*"));
        assert!(wildcard("anything", "*") && wildcard("", "*") && !wildcard("x", ""));
        assert!(matches_any("disk.img", "*.iso; *.img"));
    }

    #[test]
    fn file_format_round_trips_and_imports_unticked() {
        let r = vec![Rule { name: "Old ISO files".into(), pattern: "*.iso;*.img".into(), subfolders: true, ..Default::default() }];
        let parsed = parse(&to_text(&r));
        assert_eq!(parsed, r);
        let mut mine = parsed.clone();
        let shared = parse(
            "[Logs]\nfolder = ~/logs\npattern = *.log\nolder_than_days = 7\nenabled = yes\n[Old ISO files]\nfolder = ~/Downloads\npattern = *.iso;*.img\nolder_than_days = 30\n[Broken]\npattern = *\n",
        );
        assert_eq!(shared.len(), 2, "rule without a folder dropped");
        assert_eq!(merge_imported(&mut mine, shared), 1, "duplicate skipped");
        assert!(!mine[1].enabled, "imported rules start unticked");
    }

    #[test]
    fn refuses_unsafe_folders() {
        let home = Path::new("/home/me");
        let rule = |folder: &str| Rule { name: "x".into(), folder: folder.into(), ..Default::default() };
        assert!(check(&rule("~/Downloads"), home).is_ok());
        assert!(check(&rule("~"), home).is_err(), "whole home");
        assert!(check(&rule("~/../other"), home).is_err(), "escapes home");
        assert!(check(&rule("/etc"), home).is_err(), "outside home");
        assert!(check(&rule("Downloads"), home).is_err(), "relative");
        assert!(check(&Rule { pattern: "a/*.iso".into(), ..rule("~/x") }, home).is_err());
    }

    #[test]
    fn scans_only_old_matching_files_and_never_folders() {
        let home = std::env::temp_dir().join(format!("aero-rules-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        let dl = home.join("Downloads");
        fs::create_dir_all(dl.join("sub")).unwrap();
        fs::create_dir_all(dl.join("folder.iso")).unwrap(); // a folder named like the pattern
        fs::create_dir_all(dl.join("keep")).unwrap();
        for f in ["big.iso", "sub/deep.iso", "notes.txt", "keep/mine.iso"] {
            fs::write(dl.join(f), vec![0u8; 100]).unwrap();
        }
        let rule = Rule { name: "ISO".into(), folder: "~/Downloads".into(), pattern: "*.iso".into(), older_than_days: 30, ..Default::default() };
        let later = SystemTime::now() + Duration::from_secs(40 * 86_400);
        let keep = vec![dl.join("keep")];
        let names = |s: &RuleScan| {
            let mut v: Vec<String> = s.files.iter().map(|(p, _)| p.strip_prefix(&dl).unwrap().to_string_lossy().replace('\\', "/")).collect();
            v.sort();
            v
        };
        assert!(scan_in(&rule, &keep, &home, SystemTime::now()).files.is_empty(), "files are new: too young");
        assert_eq!(names(&scan_in(&rule, &keep, &home, later)), vec!["big.iso"], "no subfolders, no folders, kept folder respected");
        let deep = Rule { subfolders: true, ..rule.clone() };
        assert_eq!(names(&scan_in(&deep, &keep, &home, later)), vec!["big.iso", "sub/deep.iso"]);
        fs::remove_dir_all(&home).unwrap();
    }
}
