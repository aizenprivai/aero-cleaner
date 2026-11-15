//! `aero-cleaner clean --dry-run`: what a clean would remove, without
//! removing anything, as a table or as JSON for scripts.

use crate::cleaner::{self, CATEGORIES, Kind, ScanResult, format_bytes};
use crate::i18n::tr;
use crate::settings::Settings;

pub struct Row {
    pub kind: Kind,
    pub name: &'static str,
    pub root: bool,
    pub selected: bool,
    pub scan: ScanResult,
    pub preview: Vec<String>,
}

/// Scans every category (read-only). `only` limits it to those kinds.
pub fn collect(settings: &Settings, only: Option<&[Kind]>) -> Vec<Row> {
    let selected: Vec<Kind> = match &settings.selected {
        Some(ids) => CATEGORIES.iter().filter(|c| ids.contains(&format!("{:?}", c.kind))).map(|c| c.kind).collect(),
        None => CATEGORIES.iter().filter(|c| c.default_on).map(|c| c.kind).collect(),
    };
    CATEGORIES
        .iter()
        .filter(|c| only.is_none_or(|o| o.contains(&c.kind)))
        .map(|c| Row {
            kind: c.kind,
            name: c.name,
            root: c.root,
            selected: only.is_some() || selected.contains(&c.kind),
            scan: cleaner::scan(c.kind, &settings.exclusions),
            preview: cleaner::preview(c.kind, &settings.exclusions, settings.journal_days),
        })
        .collect()
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// One JSON object per category, with sizes in bytes (null when unknown).
pub fn to_json(rows: &[Row]) -> String {
    let items: Vec<String> = rows
        .iter()
        .map(|r| {
            let size = r.scan.size.map_or("null".to_string(), |s| s.to_string());
            let preview: Vec<String> = r.preview.iter().map(|l| json_string(l)).collect();
            format!(
                "  {{\"id\": {}, \"name\": {}, \"bytes\": {size}, \"note\": {}, \"available\": {}, \"needs_admin\": {}, \"selected\": {}, \"preview\": [{}]}}",
                json_string(&format!("{:?}", r.kind)),
                json_string(r.name),
                json_string(&r.scan.note),
                r.scan.available,
                r.root,
                r.selected,
                preview.join(", ")
            )
        })
        .collect();
    format!("[\n{}\n]", items.join(",\n"))
}

/// A readable table: ticked categories first, then the total.
pub fn to_text(rows: &[Row]) -> String {
    let mut out = format!("{}\n\n", tr("Dry run: nothing is deleted."));
    let mut total = 0;
    for r in rows {
        let size = match (r.scan.available, r.scan.size) {
            (false, _) => "n/a".to_string(),
            (true, Some(s)) => format_bytes(s),
            (true, None) => "?".to_string(),
        };
        if r.selected && r.scan.available {
            total += r.scan.size.unwrap_or(0);
        }
        let mark = if r.selected { "[x]" } else { "[ ]" };
        let admin = if r.root { " (admin)" } else { "" };
        let note = if r.scan.note.is_empty() { String::new() } else { format!("  {}", tr(&r.scan.note)) };
        let name = tr(r.name);
        let pad = 34usize.saturating_sub(name.chars().count());
        out.push_str(&format!("{mark} {name}{} {size:>10}{note}{admin}\n", " ".repeat(pad)));
        if r.selected {
            for line in r.preview.iter().take(3) {
                out.push_str(&format!("      {}\n", tr(line)));
            }
        }
    }
    out.push_str(&format!("\n{}", tr(format!("Ticked items would free about {}.", format_bytes(total)))));
    out
}

// ---------- Cleaning reports ----------

/// Year, month, day from days since 1970 (proleptic Gregorian calendar).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// "2026-11-15 14:03 UTC".
pub fn date_time(secs: u64) -> String {
    let (y, m, d) = civil((secs / 86_400) as i64);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02} UTC", secs % 86_400 / 3600, secs % 3600 / 60)
}

fn secs(e: &crate::settings::HistoryEntry) -> u64 {
    e.when.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

/// One line per clean, oldest first; sizes in bytes and readable.
pub fn history_csv(entries: &[crate::settings::HistoryEntry]) -> String {
    let mut out = String::from("date,freed_bytes,freed,what\n");
    for e in entries.iter().rev() {
        out.push_str(&format!("{},{},{},{}\n", date_time(secs(e)), e.freed, csv_field(&format_bytes(e.freed)), csv_field(&e.what)));
    }
    out
}

pub fn history_markdown(entries: &[crate::settings::HistoryEntry]) -> String {
    let total: u64 = entries.iter().map(|e| e.freed).sum();
    let mut out = format!(
        "# AeroClean cleaning report\n\nGenerated {} by AeroClean {}.\n\n**{} freed** over {} clean{}.\n\n| Date | Freed | What |\n|---|---:|---|\n",
        date_time(crate::diskhistory::now()),
        crate::update::CURRENT,
        format_bytes(total),
        entries.len(),
        if entries.len() == 1 { "" } else { "s" }
    );
    for e in entries.iter().rev() {
        out.push_str(&format!("| {} | {} | {} |\n", date_time(secs(e)), format_bytes(e.freed), e.what.replace('|', "\\|")));
    }
    out
}

/// Saves a report in Documents (or the home folder) and returns its path.
pub fn export(entries: &[crate::settings::HistoryEntry], markdown: bool) -> Result<std::path::PathBuf, String> {
    let home = crate::platform::home();
    let dir = [home.join("Documents"), home.clone()].into_iter().find(|d| d.is_dir()).unwrap_or(home);
    let day = &date_time(crate::diskhistory::now())[..10];
    let path = dir.join(format!("AeroClean-report-{day}.{}", if markdown { "md" } else { "csv" }));
    let text = if markdown { history_markdown(entries) } else { history_csv(entries) };
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &'static str, size: Option<u64>, selected: bool) -> Row {
        Row { kind: Kind::Trash, name, root: false, selected, scan: ScanResult { size, note: String::new(), available: true }, preview: vec!["Deletes \"x\"\tnow".into()] }
    }

    #[test]
    fn json_is_escaped_and_complete() {
        let json = to_json(&[row("Trash", Some(1200), true), row("Other", None, false)]);
        assert!(json.contains("\"bytes\": 1200") && json.contains("\"bytes\": null"));
        assert!(json.contains(r#""preview": ["Deletes \"x\"\tnow"]"#), "{json}");
        assert_eq!(json_string("a\u{1}b"), "\"a\\u0001b\"");
        assert!(json.starts_with('[') && json.ends_with(']'));
    }

    #[test]
    fn reports_have_dates_and_quoted_fields() {
        use crate::settings::HistoryEntry;
        use std::time::{Duration, UNIX_EPOCH};
        assert_eq!(date_time(0), "1970-01-01 00:00 UTC");
        assert_eq!(date_time(1_794_707_873), "2026-11-15 01:57 UTC");
        assert_eq!(date_time(951_782_400), "2000-02-29 00:00 UTC", "leap day");
        let entries = vec![
            HistoryEntry { when: UNIX_EPOCH + Duration::from_secs(1_794_707_873), freed: 2_000_000, what: "Trash, App & browser cache".into() },
            HistoryEntry { when: UNIX_EPOCH + Duration::from_secs(1_794_000_000), freed: 5, what: "Automatic: Thumbnail cache".into() },
        ];
        let csv = history_csv(&entries);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines[0], "date,freed_bytes,freed,what");
        assert!(lines[1].starts_with("2026-11-06") && lines[2].starts_with("2026-11-15"), "oldest first: {csv}");
        assert!(lines[2].ends_with(",\"Trash, App & browser cache\""), "commas quoted: {}", lines[2]);
        let md = history_markdown(&entries);
        assert!(md.contains("over 2 cleans") && md.contains("| Date | Freed | What |"));
    }

    #[test]
    fn text_totals_only_ticked_items() {
        let text = to_text(&[row("Trash", Some(2_000_000), true), row("Other", Some(5_000_000), false)]);
        // Another test may switch the language to French meanwhile.
        assert!(text.contains("[x] ") && text.contains("[ ] Other"));
        assert!(text.contains("2.0 MB.") || text.contains("2,0 Mo."), "{text}");
    }
}
