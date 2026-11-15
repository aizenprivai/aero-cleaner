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
    fn text_totals_only_ticked_items() {
        let text = to_text(&[row("Trash", Some(2_000_000), true), row("Other", Some(5_000_000), false)]);
        // Another test may switch the language to French meanwhile.
        assert!(text.contains("[x] ") && text.contains("[ ] Other"));
        assert!(text.contains("2.0 MB.") || text.contains("2,0 Mo."), "{text}");
    }
}
