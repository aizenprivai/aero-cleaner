//! Translations. Each language is a plain text file in `lang/` (English
//! line, then "= translation"), built into the app, so anyone can add or
//! fix a language without touching code.
//!
//! Text is translated where it is drawn (`tr`), so most code just writes
//! English. Formatted text is matched against templates: "Freed 1,2 Go"
//! matches "Freed {}" and becomes "1,2 Go libérés". Uppercase section
//! headers match their template case-insensitively.
//!
//! Run with `AERO_I18N_MISSING=1` to print strings that have no translation.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lang {
    En,
    Fr,
    Es,
    De,
    Pt,
}

impl Lang {
    pub const ALL: [Lang; 5] = [Lang::En, Lang::Fr, Lang::Es, Lang::De, Lang::Pt];

    /// The code used in settings and system locales ("fr").
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Fr => "fr",
            Lang::Es => "es",
            Lang::De => "de",
            Lang::Pt => "pt",
        }
    }

    /// The language's own name, for the language buttons.
    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Fr => "Français",
            Lang::Es => "Español",
            Lang::De => "Deutsch",
            Lang::Pt => "Português",
        }
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        let c = code.trim().to_ascii_lowercase();
        Lang::ALL.into_iter().find(|l| c.starts_with(l.code()))
    }

    fn file(self) -> &'static str {
        match self {
            Lang::En => "",
            Lang::Fr => include_str!("../lang/fr.txt"),
            Lang::Es => include_str!("../lang/es.txt"),
            Lang::De => include_str!("../lang/de.txt"),
            Lang::Pt => include_str!("../lang/pt.txt"),
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Tests that switch the (global) language hold this, so they don't race.
#[cfg(test)]
pub static LANGUAGE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// "auto" follows the system language; a code ("fr", "es"...) forces one.
pub fn set_language(setting: &str) {
    let lang = Lang::from_code(setting).unwrap_or_else(system_language);
    set_lang(lang);
}

pub fn set_lang(lang: Lang) {
    CURRENT.store(Lang::ALL.iter().position(|l| *l == lang).unwrap_or(0) as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    Lang::ALL[CURRENT.load(Ordering::Relaxed) as usize % Lang::ALL.len()]
}

pub fn is_french() -> bool {
    lang() == Lang::Fr
}

/// Languages that write 1,5 instead of 1.5.
pub fn decimal_comma() -> bool {
    lang() != Lang::En
}

/// The system's language, if it's one AeroClean speaks (English otherwise).
pub fn system_language() -> Lang {
    let env = ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"].iter().filter_map(|k| std::env::var(k).ok()).find(|v| !v.is_empty() && v != "C" && v != "POSIX");
    if let Some(v) = env {
        return Lang::from_code(&v).unwrap_or(Lang::En);
    }
    // Windows has no LANG: ask for the display language ("fr-FR").
    if crate::platform::IS_WINDOWS {
        let out = crate::platform::command("powershell").args(["-NoProfile", "-Command", "(Get-UICulture).Name"]).output();
        if let Ok(o) = out {
            return Lang::from_code(String::from_utf8_lossy(&o.stdout).trim()).unwrap_or(Lang::En);
        }
    }
    Lang::En
}

/// "English\n= Traduction" pairs from a language file.
pub fn parse_file(text: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let mut english: Option<&str> = None;
    for line in text.lines() {
        if let Some(t) = line.strip_prefix("= ").or_else(|| (line == "=").then_some("")) {
            if let Some(en) = english.take()
                && !t.trim().is_empty()
            {
                out.push((en, t.trim_end()));
            }
        } else if !line.trim().is_empty() && !line.starts_with('#') {
            english = Some(line.trim());
        }
    }
    out
}

struct Table {
    exact: HashMap<String, &'static str>,
    upper: HashMap<String, String>,
    /// (literal parts of the English template, translated template), most specific first.
    templates: Vec<(Vec<String>, &'static str)>,
}

fn table(lang: Lang) -> &'static Table {
    static T: OnceLock<Mutex<HashMap<Lang, &'static Table>>> = OnceLock::new();
    let mut tables = T.get_or_init(Default::default).lock().unwrap();
    tables.entry(lang).or_insert_with(|| {
        let mut exact = HashMap::new();
        let mut upper = HashMap::new();
        let mut templates = Vec::new();
        for (en, tr) in parse_file(lang.file()) {
            if en.contains("{}") {
                templates.push((en.split("{}").map(str::to_string).collect::<Vec<_>>(), tr));
            } else {
                exact.insert(en.to_string(), tr);
                upper.insert(en.to_uppercase(), tr.to_uppercase());
            }
        }
        templates.sort_by_key(|(parts, _)| std::cmp::Reverse(parts.iter().map(|p| p.len()).sum::<usize>()));
        Box::leak(Box::new(Table { exact, upper, templates }))
    })
}

/// Extracts the holes if `s` matches the template `parts` (case-insensitive when `upper`).
fn match_template(s: &str, parts: &[String], upper: bool) -> Option<Vec<String>> {
    let norm = |x: &str| if upper { x.to_uppercase() } else { x.to_string() };
    let first = norm(&parts[0]);
    let mut rest = s.strip_prefix(first.as_str())?;
    let mut holes = Vec::new();
    for (i, lit) in parts.iter().enumerate().skip(1) {
        let lit = norm(lit);
        let last = i == parts.len() - 1;
        let pos = if last {
            if !rest.ends_with(lit.as_str()) {
                return None;
            }
            rest.len() - lit.len()
        } else if lit.is_empty() {
            return None;
        } else {
            rest.find(lit.as_str())?
        };
        if pos == 0 {
            return None; // holes are never empty
        }
        holes.push(rest[..pos].to_string());
        rest = &rest[pos + lit.len()..];
    }
    Some(holes)
}

fn fill(template: &str, holes: &[String]) -> String {
    let mut out = template.to_string();
    if template.contains("{0}") {
        for (i, h) in holes.iter().enumerate() {
            out = out.replace(&format!("{{{i}}}"), h);
        }
    } else {
        for h in holes {
            out = out.replacen("{}", h, 1);
        }
    }
    out
}

fn translate(s: &str) -> Option<String> {
    let t = table(lang());
    if let Some(tr) = t.exact.get(s) {
        return Some(tr.to_string());
    }
    let is_upper = s.chars().any(|c| c.is_alphabetic()) && !s.chars().any(|c| c.is_lowercase());
    if is_upper && let Some(tr) = t.upper.get(s) {
        return Some(tr.clone());
    }
    for (parts, tr) in &t.templates {
        if let Some(holes) = match_template(s, parts, false) {
            // Holes like "Cache" or "3 days ago" get translated too.
            let holes: Vec<String> = holes.iter().map(|h| t.exact.get(h.as_str()).map(|f| f.to_string()).unwrap_or_else(|| h.clone())).collect();
            return Some(fill(tr, &holes));
        }
        if is_upper && let Some(holes) = match_template(s, parts, true) {
            return Some(fill(tr, &holes).to_uppercase());
        }
    }
    None
}

/// Translates text about to be drawn. Leading/trailing spaces are kept.
pub fn tr(s: impl AsRef<str>) -> String {
    let s = s.as_ref();
    let lang = lang();
    if lang == Lang::En {
        return s.to_string();
    }
    static CACHE: OnceLock<Mutex<HashMap<(Lang, String), String>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().unwrap().get(&(lang, s.to_string())) {
        return hit.clone();
    }
    let core = s.trim();
    let out = match translate(core) {
        Some(t) => {
            let start = &s[..s.len() - s.trim_start().len()];
            let end = &s[s.trim_end().len()..];
            format!("{start}{t}{end}")
        }
        None => {
            report_missing(core);
            s.to_string()
        }
    };
    let mut c = cache.lock().unwrap();
    if c.len() > 4000 {
        c.clear();
    }
    c.insert((lang, s.to_string()), out.clone());
    out
}

/// Translates a fixed English string (for text built before drawing).
pub fn t(s: &'static str) -> &'static str {
    match lang() {
        Lang::En => s,
        l => table(l).exact.get(s).copied().unwrap_or(s),
    }
}

fn report_missing(s: &str) {
    if std::env::var_os("AERO_I18N_MISSING").is_none() || s.chars().filter(|c| c.is_alphabetic()).count() < 2 {
        return;
    }
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    if SEEN.get_or_init(Default::default).lock().unwrap().insert(s.to_string()) {
        eprintln!("[i18n missing] {s}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_and_case() {
        let _lang = LANGUAGE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_lang(Lang::Fr);
        assert_eq!(tr("Trash"), "Corbeille");
        assert_eq!(tr("    Removes: vim, nano"), "    Supprime : vim, nano", "spaces kept");
        assert_eq!(tr("Freed 1,2 Go"), "1,2 Go libérés");
        assert_eq!(tr("Freed 1,2 Go, 2 failed: oops"), "1,2 Go libérés, 2 échec(s) : oops", "reordered");
        assert_eq!(tr("YOUR FILES"), "VOS FICHIERS");
        assert_eq!(tr("12 FILES  ·  3,0 Go IN TOTAL"), "12 FICHIERS  ·  3,0 Go AU TOTAL");
        assert_eq!(tr("BRAVE WILL NO LONGER START AT LOGIN"), "BRAVE NE SE LANCERA PLUS À LA CONNEXION");
        assert_eq!(tr("Deleting this is safe: it's rebuilt by cargo build."), "Vous pouvez le supprimer sans risque : il est recréé par cargo build.", "hole translated");
        assert_eq!(tr("my-photo.jpg"), "my-photo.jpg", "unknown text unchanged");
        set_lang(Lang::En);
        assert_eq!(tr("Trash"), "Trash");
    }

    #[test]
    fn every_language_file_is_well_formed() {
        for lang in Lang::ALL.into_iter().skip(1) {
            let pairs = parse_file(lang.file());
            assert!(pairs.len() > 100, "{lang:?} has only {} entries", pairs.len());
            let mut seen = HashSet::new();
            for (en, tr) in &pairs {
                assert!(seen.insert(*en), "{lang:?}: {en:?} is listed twice");
                let n = en.matches("{}").count();
                let m = if tr.contains("{0}") { (0..n).filter(|i| tr.contains(&format!("{{{i}}}"))).count() } else { tr.matches("{}").count() };
                assert_eq!(n, m, "{lang:?}: placeholder count differs: {en:?} -> {tr:?}");
            }
        }
    }

    #[test]
    fn language_codes() {
        assert_eq!(Lang::from_code("fr_FR.UTF-8"), Some(Lang::Fr));
        assert_eq!(Lang::from_code("pt-BR"), Some(Lang::Pt));
        assert_eq!(Lang::from_code("de"), Some(Lang::De));
        assert_eq!(Lang::from_code("auto"), None);
        assert_eq!(parse_file("# comment\nTrash\n= Corbeille\n\nNo translation\n=\nSave\n= Enregistrer\n"), vec![("Trash", "Corbeille"), ("Save", "Enregistrer")]);
    }
}
