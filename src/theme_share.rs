//! Sharing themes: one self-contained file (pictures embedded), a safe
//! import, and the online gallery (a GitHub repository with an index).
//!
//! A shared file is a normal theme file, complete (no base needed), plus a
//! `[files]` table of embedded pictures (`name = "base64..."`) that its
//! picture layers name. Import unpacks the pictures next to the theme in
//! the themes folder and checks everything before writing anything.

use crate::theme::{self, Library};
use base64::Engine;
use std::path::{Path, PathBuf};
use toml::{Table, Value};

/// Biggest shared file accepted, and biggest embedded picture.
pub const MAX_SHARED: usize = 24 * 1024 * 1024;
pub const MAX_PICTURE: usize = 8 * 1024 * 1024;

/// The community gallery: `index.json` and `themes/*.aerotheme` on its main branch.
pub const GALLERY_REPO: &str = "aizenprivai/aero-cleaner-themes";

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

fn picture_layers(t: &mut Table) -> Vec<&mut Table> {
    t.get_mut("scene")
        .and_then(|s| s.get_mut("layers"))
        .and_then(|l| l.as_array_mut())
        .map(|a| a.iter_mut().filter_map(|v| v.as_table_mut()).filter(|l| l.get("type").and_then(|v| v.as_str()) == Some("picture")).collect())
        .unwrap_or_default()
}

/// A theme's complete settings as one shareable file, its pictures inside.
/// `theme_file` is where the theme lives (for pictures named relative to it).
pub fn export(resolved: &Table, theme_file: Option<&Path>) -> Result<String, String> {
    let mut t = resolved.clone();
    let mut files = Table::new();
    for layer in picture_layers(&mut t) {
        let name = layer.get("file").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let path = if let Some(rest) = name.strip_prefix("~/") {
            crate::platform::home().join(rest)
        } else if Path::new(&name).is_absolute() {
            PathBuf::from(&name)
        } else {
            theme_file.and_then(Path::parent).map(|d| d.join(&name)).ok_or_else(|| format!("picture {name}: can't find it"))?
        };
        let bytes = std::fs::read(&path).map_err(|e| format!("picture {}: {e}", path.display()))?;
        if bytes.len() > MAX_PICTURE {
            return Err(format!("picture {} is over 8 MB; use a smaller one to share", path.display()));
        }
        let short = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "picture.png".into());
        let short = safe_file_name(&short).ok_or_else(|| format!("picture {}: rename it with plain letters and numbers", path.display()))?;
        files.insert(short.clone(), Value::String(b64().encode(&bytes)));
        layer.insert("file".into(), Value::String(short));
    }
    if !files.is_empty() {
        t.insert("files".into(), Value::Table(files));
    }
    Ok(format!(
        "# An AeroClean theme, ready to share. Import it in AeroClean (Themes, Import, or drag it\n# onto the window), or run: aero-cleaner theme apply <this file>\n{}",
        toml::to_string(&t).map_err(|e| e.to_string())?
    ))
}

/// A plain picture file name ("sunset.jpg"), or None.
fn safe_file_name(name: &str) -> Option<String> {
    let lower = name.to_lowercase();
    let ok_ext = [".png", ".jpg", ".jpeg", ".webp"].iter().any(|e| lower.ends_with(e));
    let ok_chars = !name.is_empty() && name.len() <= 80 && !name.starts_with('.') && name.chars().all(|c| c.is_ascii_alphanumeric() || "._- ".contains(c));
    (ok_ext && ok_chars).then(|| name.replace(' ', "-"))
}

/// A shared file, checked: the theme (pictures left out, they aren't on
/// disk yet), the pictures to unpack, and the settings without `[files]`.
pub struct Unpacked {
    pub table: Table,
    pub pictures: Vec<(String, Vec<u8>)>,
    pub name: String,
    pub warnings: Vec<String>,
}

pub fn unpack(text: &str, lib: &Library) -> Result<Unpacked, String> {
    if text.len() > MAX_SHARED {
        return Err("the file is too big for a theme (over 24 MB)".into());
    }
    let mut t: Table = text.parse().map_err(|e: toml::de::Error| e.to_string().trim().replace('\n', " "))?;
    let mut pictures = Vec::new();
    if let Some(files) = t.remove("files") {
        let files = files.as_table().ok_or("[files] must list pictures")?.clone();
        for (name, data) in files {
            let safe = safe_file_name(&name).ok_or_else(|| format!("files.{name}: only .png, .jpg or .webp pictures with plain names"))?;
            let data = data.as_str().ok_or_else(|| format!("files.{name}: not picture data"))?;
            let bytes = b64().decode(data.trim()).map_err(|_| format!("files.{name}: the picture data is damaged"))?;
            if bytes.len() > MAX_PICTURE {
                return Err(format!("files.{name}: over 8 MB"));
            }
            // The bytes must really be the kind of picture the name says.
            let fmt = image::guess_format(&bytes).map_err(|_| format!("files.{name}: not a picture"))?;
            let ext_ok = match fmt {
                image::ImageFormat::Png => safe.to_lowercase().ends_with(".png"),
                image::ImageFormat::Jpeg => safe.to_lowercase().ends_with(".jpg") || safe.to_lowercase().ends_with(".jpeg"),
                image::ImageFormat::WebP => safe.to_lowercase().ends_with(".webp"),
                _ => false,
            };
            if !ext_ok {
                return Err(format!("files.{name}: the picture isn't what its name says"));
            }
            pictures.push((safe, bytes));
        }
    }
    // Check the theme itself, with picture layers pointing at a placeholder
    // folder (the pictures land next to the theme on install).
    let mut check = t.clone();
    for layer in picture_layers(&mut check) {
        let file = layer.get("file").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if Path::new(&file).is_absolute() || file.starts_with("~/") {
            return Err(format!("picture {file}: shared themes can only use pictures inside the file"));
        }
        if !pictures.iter().any(|(n, _)| *n == file.replace(' ', "-")) {
            return Err(format!("picture {file} isn't included in the file"));
        }
    }
    let placeholder = std::env::temp_dir().join("aero-import-check.aerotheme");
    let parsed = theme::parse(&toml::to_string(&check).map_err(|e| e.to_string())?, &lib.resolved, Some(placeholder), false)?;
    let warnings = parsed.warnings.into_iter().filter(|w| !w.starts_with("picture ")).collect();
    Ok(Unpacked { table: t, pictures, name: parsed.theme.name, warnings })
}

/// Installs a shared file into `dir`: pictures first (never overwriting),
/// then the theme. Returns the theme's name and its file.
pub fn import(text: &str, dir: &Path, lib: &Library) -> Result<(String, PathBuf), String> {
    let mut u = unpack(text, lib)?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let stem = theme::file_name(&u.name).trim_end_matches(".aerotheme").to_string();
    let unique = |name: String| {
        let mut p = dir.join(&name);
        let mut n = 2;
        while p.exists() {
            let (base, ext) = name.rsplit_once('.').unwrap_or((&name, ""));
            p = dir.join(format!("{base}-{n}.{ext}"));
            n += 1;
        }
        p
    };
    let mut renamed = Vec::new();
    for (name, bytes) in &u.pictures {
        let path = unique(format!("{stem}-{name}"));
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        renamed.push((name.clone(), path.file_name().unwrap().to_string_lossy().to_string()));
    }
    for layer in picture_layers(&mut u.table) {
        let file = layer.get("file").and_then(|v| v.as_str()).unwrap_or("").replace(' ', "-");
        if let Some((_, new)) = renamed.iter().find(|(old, _)| *old == file) {
            layer.insert("file".into(), Value::String(new.clone()));
        }
    }
    let dest = unique(format!("{stem}.aerotheme"));
    let text = format!("# Imported into AeroClean.\n{}", toml::to_string(&u.table).map_err(|e| e.to_string())?);
    // A last check with the real location, then write.
    theme::parse(&text, &lib.resolved, Some(dest.clone()), false)?;
    std::fs::write(&dest, text).map_err(|e| e.to_string())?;
    Ok((u.name, dest))
}

// ---------- The online gallery ----------

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct GalleryEntry {
    /// Path in the repository, like "themes/ocean-breeze.aerotheme".
    pub file: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct GalleryIndex {
    format: u32,
    themes: Vec<GalleryEntry>,
}

fn raw_url(path: &str) -> String {
    format!("https://raw.githubusercontent.com/{GALLERY_REPO}/main/{path}")
}

/// Only "themes/<plain-name>.aerotheme" paths are fetched.
fn entry_ok(e: &GalleryEntry) -> bool {
    e.file.strip_prefix("themes/").and_then(|f| f.strip_suffix(".aerotheme")).is_some_and(|stem| !stem.is_empty() && stem.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        && e.size as usize <= MAX_SHARED
        && e.sha256.len() == 64
        && e.sha256.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn parse_index(json: &str) -> Result<Vec<GalleryEntry>, String> {
    let idx: GalleryIndex = serde_json::from_str(json).map_err(|_| "the gallery sent something unexpected".to_string())?;
    if idx.format != 1 {
        return Err("this gallery needs a newer AeroClean".into());
    }
    Ok(idx.themes.into_iter().filter(entry_ok).take(500).collect())
}

pub fn fetch_index() -> Result<Vec<GalleryEntry>, String> {
    let bytes = crate::update::get(&raw_url("index.json"), None)?;
    parse_index(&String::from_utf8_lossy(&bytes))
}

/// Downloads one gallery theme, checking its size and SHA-256 against the index.
pub fn fetch_theme(e: &GalleryEntry) -> Result<String, String> {
    if !entry_ok(e) {
        return Err("not a gallery theme".into());
    }
    let bytes = crate::update::get(&raw_url(&e.file), None)?;
    verify(e, &bytes)?;
    String::from_utf8(bytes).map_err(|_| "the theme isn't text".into())
}

fn verify(e: &GalleryEntry, bytes: &[u8]) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    if bytes.len() as u64 != e.size {
        return Err(format!("{}: the download doesn't match the gallery (size)", e.name));
    }
    let hash: String = Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect();
    if hash != e.sha256.to_lowercase() {
        return Err(format!("{}: the download doesn't match the gallery (checksum)", e.name));
    }
    Ok(())
}

/// The gallery's `index.json` for the theme files in `dir` (run by the
/// gallery repository's CI). Every file must pass the import checks.
pub fn build_index(dir: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let lib = theme::load_library(Path::new("/nonexistent"));
    let mut files: Vec<PathBuf> =
        std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "aerotheme")).collect();
    files.sort();
    let mut themes = Vec::new();
    for path in files {
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let text = String::from_utf8(bytes.clone()).map_err(|_| format!("{}: not text", path.display()))?;
        let u = unpack(&text, &lib).map_err(|e| format!("{}: {e}", path.display()))?;
        let meta = u.table.get("meta").and_then(|m| m.as_table());
        let get = |k: &str| meta.and_then(|m| m.get(k)).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let entry = GalleryEntry {
            file: format!("themes/{}", path.file_name().unwrap().to_string_lossy()),
            name: u.name,
            author: get("author"),
            description: get("description"),
            sha256: Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect(),
            size: bytes.len() as u64,
        };
        if !entry_ok(&entry) {
            return Err(format!("{}: name the file with lowercase letters, numbers and dashes", path.display()));
        }
        themes.push(entry);
    }
    serde_json::to_string_pretty(&GalleryIndex { format: 1, themes }).map_err(|e| e.to_string())
}

/// A gallery theme as a Theme for its preview card (pictures left out).
pub fn preview_theme(text: &str, lib: &Library) -> Option<theme::Theme> {
    let u = unpack(text, lib).ok()?;
    let mut t = u.table;
    if let Some(layers) = t.get_mut("scene").and_then(|s| s.get_mut("layers")).and_then(|l| l.as_array_mut()) {
        layers.retain(|l| l.get("type").and_then(|v| v.as_str()) != Some("picture"));
    }
    theme::parse(&toml::to_string(&t).ok()?, &lib.resolved, None, false).ok().map(|p| p.theme)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib() -> Library {
        theme::load_library(Path::new("/nonexistent"))
    }

    fn png() -> Vec<u8> {
        let img = image::RgbImage::from_pixel(4, 3, image::Rgb([10, 120, 200]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img).write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn export_then_import_carries_pictures() {
        let base = std::env::temp_dir().join(format!("aero-share-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mine = base.join("mine");
        std::fs::create_dir_all(&mine).unwrap();
        std::fs::write(mine.join("my sky.png"), png()).unwrap();
        let theme_file = mine.join("dreamy.aerotheme");
        std::fs::write(&theme_file, "format = 1\n[meta]\nname = \"Dreamy\"\n[[scene.layers]]\ntype = \"picture\"\nfile = \"my sky.png\"\n[[scene.layers]]\ntype = \"snow\"\n")
            .unwrap();
        let lib = theme::load_library(&mine);
        let (_, resolved) = lib.resolved.iter().find(|(n, _)| n == "Dreamy").unwrap();
        let shared = export(resolved, Some(&theme_file)).unwrap();
        assert!(shared.contains("[files]") && shared.contains("my-sky.png"), "picture embedded");

        let theirs = base.join("theirs");
        let (name, path) = import(&shared, &theirs, &lib).unwrap();
        assert_eq!(name, "Dreamy");
        assert_eq!(path.file_name().unwrap(), "dreamy.aerotheme");
        assert_eq!(std::fs::read(theirs.join("dreamy-my-sky.png")).unwrap(), png(), "picture unpacked next to the theme");
        let installed = theme::load_library(&theirs);
        let t = installed.themes.iter().find(|t| t.name == "Dreamy").unwrap();
        assert!(matches!(&t.scene[0].kind, theme::Layer::Picture { path, .. } if path.is_file()));
        // Importing again keeps both, never overwriting.
        let (_, again) = import(&shared, &theirs, &lib).unwrap();
        assert_eq!(again.file_name().unwrap(), "dreamy-2.aerotheme");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn refuses_unsafe_shared_files() {
        let lib = lib();
        let head = "format = 1\n[meta]\nname = \"X\"\n[[scene.layers]]\ntype = \"picture\"\n";
        let b = |bytes: &[u8]| b64().encode(bytes);
        let cases = [
            (format!("{head}file = \"../../evil.png\"\n[files]\n\"../../evil.png\" = \"{}\"\n", b(&png())), "plain names"),
            (format!("{head}file = \"a.png\"\n[files]\n\"a.png\" = \"{}\"\n", b(b"#!/bin/sh\nrm -rf ~")), "not a picture"),
            (format!("{head}file = \"a.jpg\"\n[files]\n\"a.jpg\" = \"{}\"\n", b(&png())), "isn't what its name says"),
            (format!("{head}file = \"/etc/hosts.png\"\n"), "only use pictures inside the file"),
            (format!("{head}file = \"missing.png\"\n"), "isn't included"),
            (format!("{head}file = \"a.png\"\n[files]\n\"a.png\" = \"%%%\"\n"), "damaged"),
        ];
        for (text, why) in cases {
            let err = unpack(&text, &lib).err().unwrap_or_default();
            assert!(err.contains(why), "expected '{why}', got '{err}'");
        }
    }

    /// The real gallery on GitHub (needs the internet):
    /// `cargo test live_gallery -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_gallery() {
        let entries = fetch_index().unwrap();
        assert!(!entries.is_empty());
        for e in &entries {
            let text = fetch_theme(e).unwrap();
            let t = preview_theme(&text, &lib()).unwrap();
            println!("{} by {}: verified, {} layers", t.name, e.author, t.scene.len());
        }
    }

    #[test]
    fn gallery_index_round_trip_and_verification() {
        let dir = std::env::temp_dir().join(format!("aero-gallery-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let text = "format = 1\n[meta]\nname = \"Ocean Breeze\"\nauthor = \"sam\"\ndescription = \"Sea and sky\"\n[colors]\naccent = \"#1188cc\"\n";
        std::fs::write(dir.join("ocean-breeze.aerotheme"), text).unwrap();
        let json = build_index(&dir).unwrap();
        let entries = parse_index(&json).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!((e.file.as_str(), e.name.as_str(), e.author.as_str(), e.size), ("themes/ocean-breeze.aerotheme", "Ocean Breeze", "sam", text.len() as u64));
        assert!(verify(e, text.as_bytes()).is_ok());
        assert!(verify(e, b"tampered").unwrap_err().contains("size"));
        let mut other = text.as_bytes().to_vec();
        other[20] ^= 1;
        assert!(verify(e, &other).unwrap_err().contains("checksum"));
        assert!(preview_theme(text, &lib()).is_some_and(|t| t.name == "Ocean Breeze"));
        // Paths outside themes/ are dropped from an index.
        let bad = json.replace("themes/ocean-breeze.aerotheme", "../../.bashrc");
        assert!(parse_index(&bad).unwrap().is_empty());
        std::fs::write(dir.join("Bad Name.aerotheme"), text).unwrap();
        assert!(build_index(&dir).unwrap_err().contains("lowercase"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
