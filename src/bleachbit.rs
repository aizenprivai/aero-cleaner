//! Imports BleachBit's CleanerML files (one XML file per app) as AeroClean
//! rules. Only what fits AeroClean's safe rule model is taken:
//!
//! * `command="delete"` actions with `search` = `file`, `glob` (wildcards in
//!   the file name only) or `walk.files` / `walk.all` / `walk.top` (every
//!   file under a folder);
//! * inside the home folder, for apps installed here (the folder exists),
//!   and not already covered by the Cleaner's own categories.
//!
//! Everything else (registry, ini/json edits, regex filters, SQLite
//! vacuums, wildcards in folder names, system paths) is skipped.

use crate::rules::Rule;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One XML element: name, attributes and its own text.
#[derive(Debug, Default)]
struct Element {
    name: String,
    attrs: HashMap<String, String>,
    text: String,
    children: Vec<Element>,
}

impl Element {
    fn attr(&self, k: &str) -> &str {
        self.attrs.get(k).map(String::as_str).unwrap_or("")
    }
    fn child_text(&self, name: &str) -> String {
        self.children.iter().find(|c| c.name == name).map(|c| c.text.trim().to_string()).unwrap_or_default()
    }
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// A small XML reader: enough for CleanerML (tags, attributes, text,
/// comments, the `<?xml ?>` header). Returns the root element.
fn parse_xml(xml: &str) -> Option<Element> {
    let mut stack: Vec<Element> = vec![Element::default()];
    let mut rest = xml;
    while let Some(lt) = rest.find('<') {
        let text = &rest[..lt];
        if let Some(top) = stack.last_mut() {
            top.text.push_str(&unescape(text));
        }
        rest = &rest[lt..];
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = &after[after.find("-->")? + 3..];
            continue;
        }
        let gt = rest.find('>')?;
        let tag = &rest[1..gt];
        rest = &rest[gt + 1..];
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        if let Some(name) = tag.strip_prefix('/') {
            let done = stack.pop()?;
            if done.name != name.trim() {
                return None;
            }
            stack.last_mut()?.children.push(done);
            continue;
        }
        let self_closing = tag.ends_with('/');
        let tag = tag.trim_end_matches('/');
        let (name, mut attr_src) = tag.split_once(char::is_whitespace).unwrap_or((tag, ""));
        let mut el = Element { name: name.to_string(), ..Default::default() };
        // key="value" or key='value'
        while let Some(eq) = attr_src.find('=') {
            let key = attr_src[..eq].trim().to_string();
            let after = attr_src[eq + 1..].trim_start();
            let quote = after.chars().next()?;
            if quote != '"' && quote != '\'' {
                return None;
            }
            let end = after[1..].find(quote)? + 1;
            el.attrs.insert(key, unescape(&after[1..end]));
            attr_src = &after[end + 1..];
        }
        if self_closing {
            stack.last_mut()?.children.push(el);
        } else {
            stack.push(el);
        }
    }
    let mut root = stack.pop()?;
    root.children.pop()
}

/// This system's name in CleanerML `os` attributes.
fn our_os() -> &'static str {
    if crate::platform::IS_WINDOWS {
        "windows"
    } else if crate::platform::IS_MACOS {
        "darwin"
    } else {
        "linux"
    }
}

fn os_ok(el: &Element, os: &str) -> bool {
    match el.attr("os") {
        "" => true,
        v => v == os || (os != "windows" && (v == "unix" || v == "posix")),
    }
}

/// Resolves `$$var$$`, `~`, and the environment variables BleachBit uses.
fn expand(path: &str, vars: &HashMap<String, String>, home: &Path) -> Option<PathBuf> {
    let mut p = path.to_string();
    for (k, v) in vars {
        p = p.replace(&format!("$${k}$$"), v);
    }
    if p.contains("$$") {
        return None;
    }
    let h = home.to_string_lossy().to_string();
    let env = |k: &str, fallback: &str| std::env::var(k).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| format!("{h}/{fallback}"));
    let known = [
        ("$XDG_CACHE_HOME", env("XDG_CACHE_HOME", ".cache")),
        ("$XDG_CONFIG_HOME", env("XDG_CONFIG_HOME", ".config")),
        ("$XDG_DATA_HOME", env("XDG_DATA_HOME", ".local/share")),
        ("$LOCALAPPDATA", env("LOCALAPPDATA", "AppData/Local")),
        ("%LocalAppData%", env("LOCALAPPDATA", "AppData/Local")),
        ("$APPDATA", env("APPDATA", "AppData/Roaming")),
        ("%AppData%", env("APPDATA", "AppData/Roaming")),
        ("$USERPROFILE", h.clone()),
        ("%UserProfile%", h.clone()),
        ("$HOME", h.clone()),
    ];
    for (k, v) in known {
        p = p.replace(k, &v);
    }
    if p.starts_with('~') {
        p = format!("{h}{}", &p[1..]);
    }
    // Any variable left is one we don't know: skip rather than guess.
    if p.contains('$') || p.contains('%') {
        return None;
    }
    Some(PathBuf::from(p))
}

fn has_wildcard(s: &str) -> bool {
    s.contains(['*', '?', '['])
}

/// Turns one action into a rule (folder, pattern, subfolders), if it fits.
fn action_rule(a: &Element, vars: &HashMap<String, String>, home: &Path) -> Option<(PathBuf, String, bool)> {
    if a.attr("command") != "delete" || ["regex", "nregex", "wholeregex", "nwholeregex", "type"].iter().any(|k| a.attrs.contains_key(*k)) {
        return None;
    }
    let path = expand(a.attr("path"), vars, home)?;
    match a.attr("search") {
        "file" | "glob" => {
            let name = path.file_name()?.to_string_lossy().to_string();
            let folder = path.parent()?.to_path_buf();
            (!has_wildcard(&folder.to_string_lossy())).then_some((folder, name, false))
        }
        "walk.files" | "walk.all" | "walk.top" => (!has_wildcard(&path.to_string_lossy())).then_some((path, "*".to_string(), true)),
        _ => None,
    }
}

/// Rules from one CleanerML file, for apps installed here.
fn rules_from(xml: &str, home: &Path, covered: &[PathBuf]) -> Vec<Rule> {
    let os = our_os();
    let Some(cleaner) = parse_xml(xml).filter(|c| c.name == "cleaner" && os_ok(c, os)) else { return vec![] };
    let app = cleaner.child_text("label");
    let mut vars = HashMap::new();
    for v in cleaner.children.iter().filter(|c| c.name == "var") {
        if let Some(value) = v.children.iter().find(|c| c.name == "value" && os_ok(c, os)) {
            vars.insert(v.attr("name").to_string(), value.text.trim().to_string());
        }
    }
    let mut out: Vec<Rule> = Vec::new();
    for opt in cleaner.children.iter().filter(|c| c.name == "option" && os_ok(c, os)) {
        let label = opt.child_text("label");
        for a in opt.children.iter().filter(|c| c.name == "action" && os_ok(c, os)) {
            let Some((folder, pattern, subfolders)) = action_rule(a, &vars, home) else { continue };
            // Installed here, and not something the Cleaner already does.
            if !folder.is_dir() || covered.iter().any(|c| folder.starts_with(c)) {
                continue;
            }
            let rel = folder.strip_prefix(home).ok().map(|r| format!("~/{}", r.to_string_lossy().replace('\\', "/")));
            let Some(rel) = rel else { continue };
            let base = format!("{app}: {label}");
            let n = out.iter().filter(|r| r.name == base || r.name.starts_with(&format!("{base} ("))).count();
            let name = if n == 0 { base } else { format!("{base} ({})", n + 1) };
            let rule = Rule { name, folder: rel, pattern, older_than_days: 0, subfolders, enabled: false, source: "BleachBit".into() };
            if crate::rules::check(&rule, home).is_ok() && !out.iter().any(|r| r.folder == rule.folder && r.pattern == rule.pattern) {
                out.push(rule);
            }
        }
    }
    out
}

/// Where BleachBit keeps its cleaner files, if it's installed.
pub fn default_dirs() -> Vec<PathBuf> {
    let home = crate::platform::home();
    let mut dirs = vec![PathBuf::from("/usr/share/bleachbit/cleaners"), PathBuf::from("/usr/local/share/bleachbit/cleaners"), home.join(".config/bleachbit/cleaners")];
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(pf) = std::env::var_os(var) {
            dirs.push(PathBuf::from(pf).join("BleachBit").join("share").join("cleaners"));
        }
    }
    dirs.push(crate::platform::roaming_app_data().join("BleachBit").join("cleaners"));
    dirs.into_iter().filter(|d| d.is_dir()).collect()
}

/// Reads every `.xml` file in `dirs`. Returns the rules and how many
/// cleaner files were read.
pub fn import(dirs: &[PathBuf]) -> (Vec<Rule>, usize) {
    let home = crate::platform::home();
    let covered = crate::cleaner::covered_paths();
    let mut rules = Vec::new();
    let mut files = 0;
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(dir) else { continue };
        let mut paths: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "xml")).collect();
        paths.sort();
        for p in paths {
            if let Ok(xml) = std::fs::read_to_string(&p) {
                files += 1;
                rules.extend(rules_from(&xml, &home, &covered));
            }
        }
    }
    (rules, files)
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!-- A cleaner in BleachBit's format -->
<cleaner id="fakeapp" os="linux">
  <label>Fake App</label>
  <description>An app &amp; its junk</description>
  <var name="profile">
    <value os="windows">$APPDATA\FakeApp</value>
    <value os="linux">~/.fakeapp</value>
  </var>
  <option id="logs">
    <label>Logs</label>
    <description>Delete the logs</description>
    <action command="delete" search="glob" path="$$profile$$/logs/*.log"/>
    <action command="delete" search="walk.files" path="$$profile$$/crash/"/>
    <action command="delete" search="glob" path="$$profile$$/*/old.log"/>
  </option>
  <option id="other">
    <label>Other</label>
    <description>Things AeroClean can't express</description>
    <action command="delete" search="walk.all" path="~/.fakeapp/thumbs" regex="\.png$"/>
    <action command="ini" search="file" path="$$profile$$/app.ini" section="Recent"/>
    <action command="delete" search="file" path="/var/log/fakeapp.log"/>
    <action command="delete" search="walk.files" path="~/.cache/fakeapp"/>
    <action command="delete" search="file" path="~/.notinstalled/data.db"/>
    <action command="delete" search="file" path="$UNKNOWN_VAR/x"/>
  </option>
</cleaner>"#;

    #[test]
    fn reads_cleanerml() {
        let root = parse_xml(XML).unwrap();
        assert_eq!(root.name, "cleaner");
        assert_eq!(root.child_text("description"), "An app & its junk");
        assert_eq!(root.children.iter().filter(|c| c.name == "option").count(), 2);
        assert!(parse_xml("<a><b></a>").is_none(), "mismatched tags rejected");
    }

    #[test]
    fn converts_only_safe_actions_for_installed_apps() {
        if our_os() != "linux" {
            return; // the sample cleaner is Linux-only
        }
        let home = std::env::temp_dir().join(format!("aero-bb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        for d in [".fakeapp/logs", ".fakeapp/crash", ".fakeapp/thumbs", ".cache/fakeapp"] {
            std::fs::create_dir_all(home.join(d)).unwrap();
        }
        let covered = vec![home.join(".cache")];
        let rules = rules_from(XML, &home, &covered);
        let got: Vec<(&str, &str, &str, bool)> = rules.iter().map(|r| (r.name.as_str(), r.folder.as_str(), r.pattern.as_str(), r.subfolders)).collect();
        assert_eq!(got, vec![("Fake App: Logs", "~/.fakeapp/logs", "*.log", false), ("Fake App: Logs (2)", "~/.fakeapp/crash", "*", true)]);
        assert!(rules.iter().all(|r| !r.enabled && r.source == "BleachBit" && r.older_than_days == 0));
        std::fs::remove_dir_all(&home).unwrap();
    }
}
