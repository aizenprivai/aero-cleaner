//! Themes are files. A `.aerotheme` file (TOML) describes everything about
//! AeroClean's look: colors, material (roundness, gloss, glass, buttons),
//! font, motion, sound, and the background as an ordered stack of scene
//! layers. The 6 built-in themes are such files (in `themes/`), built into
//! the app; your own go in the themes folder of AeroClean's settings.
//!
//! A theme inherits from a base (`meta.base`, Frutiger Aero by default) and
//! only lists what it changes. Themes are data only: nothing in them runs.

use eframe::egui::{Color32, Vec2};
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub type Stops = Vec<(f32, Color32)>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ButtonStyle {
    /// Glossy 2000s pills.
    Aqua,
    /// The same gradient, without the shiny band.
    Soft,
    /// One color and an outline.
    Flat,
}

/// How surfaces are drawn. Every number is a factor: 1.0 is the classic look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// Corner roundness (0 = square, 2 = very round).
    pub corners: f32,
    /// Strength of the white shine on buttons, orbs and panels.
    pub gloss: f32,
    /// Opacity of the frosted glass.
    pub glass: f32,
    pub buttons: ButtonStyle,
    /// Text size.
    pub font_size: f32,
}

impl Material {
    pub const CLASSIC: Material = Material { corners: 1.0, gloss: 1.0, glass: 1.0, buttons: ButtonStyle::Aqua, font_size: 1.0 };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FontChoice {
    /// The system's UI font (Noto, Segoe UI, SF Pro...).
    System,
    /// The font built into AeroClean.
    Builtin,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Transition {
    None,
    Fade,
    Slide,
}

/// One part of the background, drawn in list order.
#[derive(Clone, Debug, PartialEq)]
pub enum Layer {
    Sky {
        stops: Stops,
    },
    Sun {
        pos: Vec2,
        color: Color32,
    },
    Moon {
        pos: Vec2,
        color: Color32,
    },
    /// Slowly turning light rays from `pos`.
    Rays {
        pos: Vec2,
        opacity: u8,
    },
    Stars {
        count: u32,
    },
    Aurora,
    Hills {
        back: Stops,
        front: Stops,
    },
    Bubbles {
        color: Color32,
        count: u32,
    },
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub name: String,
    pub desc: String,
    #[allow(dead_code)] // shown by the Studio (milestone 3)
    pub author: String,
    /// The file it came from; None for built-in themes.
    pub file: Option<PathBuf>,
    /// Dark smoked glass instead of light frosted glass.
    pub dark: bool,
    pub glass: Color32,
    pub text: Color32,
    pub soft: Color32,
    pub accent: Color32,
    pub go: Color32,
    pub user_orb: Color32,
    pub root_orb: Color32,
    pub logo: Color32,
    /// Bubbles (in the scene, and the celebration after a clean).
    pub bubble: Color32,
    pub material: Material,
    pub font: FontChoice,
    /// Animation speed factor.
    pub motion_speed: f32,
    #[allow(dead_code)] // page transitions arrive with the Studio
    pub transition: Transition,
    #[allow(dead_code)] // sounds are milestone 8
    pub sound: (String, f32),
    pub scene: Vec<Layer>,
}

// ---------- The file format ----------

#[derive(Deserialize)]
struct ThemeFile {
    format: u32,
    meta: MetaF,
    colors: ColorsF,
    material: MaterialF,
    font: FontF,
    motion: MotionF,
    sound: SoundF,
    scene: SceneF,
}

#[derive(Deserialize)]
struct MetaF {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    author: String,
}

#[derive(Deserialize)]
struct ColorsF {
    text: String,
    soft: String,
    glass: String,
    accent: String,
    go: String,
    user_orb: String,
    root_orb: String,
    logo: String,
    bubble: String,
}

#[derive(Deserialize)]
struct MaterialF {
    dark_glass: bool,
    corners: f32,
    gloss: f32,
    glass: f32,
    buttons: String,
}

#[derive(Deserialize)]
struct FontF {
    family: String,
    size: f32,
}

#[derive(Deserialize)]
struct MotionF {
    speed: f32,
    transition: String,
}

#[derive(Deserialize)]
struct SoundF {
    pack: String,
    volume: f32,
}

#[derive(Deserialize)]
struct SceneF {
    layers: Vec<LayerF>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum LayerF {
    Sky { stops: Vec<(f32, String)> },
    Sun { x: f32, y: f32, color: String },
    Moon { x: f32, y: f32, color: String },
    Rays { x: Option<f32>, y: Option<f32>, opacity: Option<u8> },
    Stars { count: Option<u32> },
    Aurora {},
    Hills { back: Vec<(f32, String)>, front: Vec<(f32, String)> },
    Bubbles { color: Option<String>, count: Option<u32> },
}

/// Every setting a theme file may contain, for "unknown setting" warnings.
const KNOWN: &[(&str, &[&str])] = &[
    ("meta", &["name", "description", "author", "base"]),
    ("colors", &["text", "soft", "glass", "accent", "go", "user_orb", "root_orb", "logo", "bubble"]),
    ("material", &["dark_glass", "corners", "gloss", "glass", "buttons"]),
    ("font", &["family", "size"]),
    ("motion", &["speed", "transition"]),
    ("sound", &["pack", "volume"]),
    ("scene", &["layers"]),
];
const LAYER_KEYS: &[(&str, &[&str])] = &[
    ("sky", &["stops"]),
    ("sun", &["x", "y", "color"]),
    ("moon", &["x", "y", "color"]),
    ("rays", &["x", "y", "opacity"]),
    ("stars", &["count"]),
    ("aurora", &[]),
    ("hills", &["back", "front"]),
    ("bubbles", &["color", "count"]),
];

/// The root of every theme that names no base.
pub const DEFAULT_BASE: &str = "Frutiger Aero";

const BUILTIN: &[&str] = &[
    include_str!("../themes/frutiger-aero.aerotheme"),
    include_str!("../themes/aqua-abyss.aerotheme"),
    include_str!("../themes/sunset-vista.aerotheme"),
    include_str!("../themes/aurora-night.aerotheme"),
    include_str!("../themes/sakura-spring.aerotheme"),
    include_str!("../themes/aero-basic.aerotheme"),
];

fn color(key: &str, s: &str) -> Result<Color32, String> {
    let hex = s.strip_prefix('#').filter(|h| (h.len() == 6 || h.len() == 8) && h.chars().all(|c| c.is_ascii_hexdigit()));
    let Some(hex) = hex else { return Err(format!("{key}: \"{s}\" isn't a color; write it like \"#1e82e1\"")) };
    let b = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
    Ok(if hex.len() == 6 { Color32::from_rgb(b(0), b(2), b(4)) } else { Color32::from_rgba_unmultiplied(b(0), b(2), b(4), b(6)) })
}

fn range(key: &str, v: f32, lo: f32, hi: f32) -> Result<f32, String> {
    if v.is_finite() && (lo..=hi).contains(&v) { Ok(v) } else { Err(format!("{key} must be between {lo} and {hi} (it's {v})")) }
}

fn stops(key: &str, v: &[(f32, String)]) -> Result<Stops, String> {
    if v.len() < 2 || v.len() > 16 {
        return Err(format!("{key} needs between 2 and 16 color stops"));
    }
    let mut out = Vec::with_capacity(v.len());
    let mut last = -1.0;
    for (i, (p, c)) in v.iter().enumerate() {
        let p = range(&format!("{key}[{i}] position"), *p, 0.0, 1.0)?;
        if p < last {
            return Err(format!("{key}: stop positions must go up (0.0 to 1.0)"));
        }
        last = p;
        out.push((p, color(&format!("{key}[{i}]"), c)?));
    }
    Ok(out)
}

fn convert(f: ThemeFile, file: Option<PathBuf>) -> Result<Theme, String> {
    if f.format != 1 {
        return Err(format!("format {} isn't supported by this version of AeroClean (it reads format 1)", f.format));
    }
    let name = f.meta.name.trim().to_string();
    if name.is_empty() || name.len() > 60 || name.contains(['\n', '\t']) {
        return Err("meta.name: give the theme a name (up to 60 characters)".into());
    }
    let c = &f.colors;
    let buttons = match f.material.buttons.as_str() {
        "aqua" => ButtonStyle::Aqua,
        "soft" => ButtonStyle::Soft,
        "flat" => ButtonStyle::Flat,
        other => return Err(format!("material.buttons: \"{other}\" isn't a style; use \"aqua\", \"soft\" or \"flat\"")),
    };
    let font = match f.font.family.as_str() {
        "system" => FontChoice::System,
        "builtin" => FontChoice::Builtin,
        other => return Err(format!("font.family: \"{other}\" isn't a font; use \"system\" or \"builtin\"")),
    };
    let transition = match f.motion.transition.as_str() {
        "none" => Transition::None,
        "fade" => Transition::Fade,
        "slide" => Transition::Slide,
        other => return Err(format!("motion.transition: \"{other}\" isn't a transition; use \"none\", \"fade\" or \"slide\"")),
    };
    let bubble = color("colors.bubble", &c.bubble)?;
    let pos = |key: &str, x: f32, y: f32| -> Result<Vec2, String> { Ok(Vec2::new(range(&format!("{key}.x"), x, -0.5, 1.5)?, range(&format!("{key}.y"), y, -0.5, 1.5)?)) };
    // Rays without a position shine from the sun or moon.
    let light = f.scene.layers.iter().find_map(|l| match l {
        LayerF::Sun { x, y, .. } | LayerF::Moon { x, y, .. } => Some((*x, *y)),
        _ => None,
    });
    if f.scene.layers.len() > 32 {
        return Err("scene.layers: at most 32 layers".into());
    }
    let mut scene = Vec::new();
    for (i, l) in f.scene.layers.iter().enumerate() {
        let k = format!("scene.layers[{i}]");
        scene.push(match l {
            LayerF::Sky { stops: s } => Layer::Sky { stops: stops(&format!("{k}.stops"), s)? },
            LayerF::Sun { x, y, color: col } => Layer::Sun { pos: pos(&k, *x, *y)?, color: color(&format!("{k}.color"), col)? },
            LayerF::Moon { x, y, color: col } => Layer::Moon { pos: pos(&k, *x, *y)?, color: color(&format!("{k}.color"), col)? },
            LayerF::Rays { x, y, opacity } => {
                let (lx, ly) = light.unwrap_or((0.85, 0.1));
                Layer::Rays { pos: pos(&k, x.unwrap_or(lx), y.unwrap_or(ly))?, opacity: opacity.unwrap_or(45) }
            }
            LayerF::Stars { count } => Layer::Stars { count: range(&format!("{k}.count"), count.unwrap_or(90) as f32, 0.0, 400.0)? as u32 },
            LayerF::Aurora {} => Layer::Aurora,
            LayerF::Hills { back, front } => Layer::Hills { back: stops(&format!("{k}.back"), back)?, front: stops(&format!("{k}.front"), front)? },
            LayerF::Bubbles { color: col, count } => Layer::Bubbles {
                color: col.as_deref().map(|s| color(&format!("{k}.color"), s)).transpose()?.unwrap_or(bubble),
                count: range(&format!("{k}.count"), count.unwrap_or(18) as f32, 0.0, 80.0)? as u32,
            },
        });
    }
    Ok(Theme {
        name,
        desc: f.meta.description.trim().to_string(),
        author: f.meta.author.trim().to_string(),
        file,
        dark: f.material.dark_glass,
        glass: color("colors.glass", &c.glass)?,
        text: color("colors.text", &c.text)?,
        soft: color("colors.soft", &c.soft)?,
        accent: color("colors.accent", &c.accent)?,
        go: color("colors.go", &c.go)?,
        user_orb: color("colors.user_orb", &c.user_orb)?,
        root_orb: color("colors.root_orb", &c.root_orb)?,
        logo: color("colors.logo", &c.logo)?,
        bubble,
        material: Material {
            corners: range("material.corners", f.material.corners, 0.0, 2.0)?,
            gloss: range("material.gloss", f.material.gloss, 0.0, 1.5)?,
            glass: range("material.glass", f.material.glass, 0.3, 1.5)?,
            buttons,
            font_size: range("font.size", f.font.size, 0.8, 1.3)?,
        },
        font,
        motion_speed: range("motion.speed", f.motion.speed, 0.0, 3.0)?,
        transition,
        sound: (f.sound.pack, range("sound.volume", f.sound.volume, 0.0, 1.0)?),
        scene,
    })
}

/// Settings the file has that AeroClean doesn't know (probably typos).
fn unknown_settings(t: &toml::Table) -> Vec<String> {
    let mut out = Vec::new();
    for (k, v) in t {
        if k == "format" {
            continue;
        }
        let Some((_, keys)) = KNOWN.iter().find(|(s, _)| s == k) else {
            out.push(format!("unknown section [{k}] is ignored"));
            continue;
        };
        let Some(sec) = v.as_table() else { continue };
        for key in sec.keys().filter(|key| !keys.contains(&key.as_str())) {
            out.push(format!("unknown setting {k}.{key} is ignored"));
        }
    }
    let layers = t.get("scene").and_then(|s| s.get("layers")).and_then(|l| l.as_array());
    for (i, l) in layers.into_iter().flatten().enumerate() {
        let ty = l.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if let Some((_, keys)) = LAYER_KEYS.iter().find(|(n, _)| *n == ty)
            && let Some(tbl) = l.as_table()
        {
            for key in tbl.keys().filter(|key| key.as_str() != "type" && !keys.contains(&key.as_str())) {
                out.push(format!("unknown setting scene.layers[{i}].{key} ({ty}) is ignored"));
            }
        }
    }
    out
}

fn luminance(c: Color32) -> f32 {
    let ch = |v: u8| {
        let s = v as f32 / 255.0;
        if s <= 0.03928 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * ch(c.r()) + 0.7152 * ch(c.g()) + 0.0722 * ch(c.b())
}

/// WCAG contrast ratio, 1 to 21.
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Readability warnings: text on glass. Light glass is mostly white, so it's
/// compared against a white-ish mix; dark glass against the glass color.
pub fn readability(t: &Theme) -> Vec<String> {
    let behind = if t.dark { t.glass } else { crate::aero::lerp(Color32::WHITE, t.glass, 0.3) };
    let mut out = Vec::new();
    let ratio = contrast(t.text, behind);
    if ratio < 4.5 {
        out.push(format!("colors.text may be hard to read on the glass (contrast {ratio:.1}, aim for 4.5 or more)"));
    }
    let ratio = contrast(t.soft, behind);
    if ratio < 3.0 {
        out.push(format!("colors.soft may be hard to read on the glass (contrast {ratio:.1}, aim for 3 or more)"));
    }
    out
}

fn merge(base: &mut toml::Table, over: toml::Table) {
    for (k, v) in over {
        match (base.get_mut(&k), v) {
            (Some(toml::Value::Table(b)), toml::Value::Table(o)) => merge(b, o),
            // Arrays (like the scene's layers) are replaced, not combined.
            (_, v) => {
                base.insert(k, v);
            }
        }
    }
}

/// A theme read from text, with its full (base included) settings.
pub struct Parsed {
    pub theme: Theme,
    pub resolved: toml::Table,
    pub warnings: Vec<String>,
}

/// Reads a theme file. `known` holds the themes it may name as its base
/// (name, resolved settings). Built-in files name no base and are complete.
pub fn parse(text: &str, known: &[(String, toml::Table)], file: Option<PathBuf>, builtin: bool) -> Result<Parsed, String> {
    if text.len() > 256 * 1024 {
        return Err("the file is too big for a theme (over 256 KB)".into());
    }
    let raw: toml::Table = text.parse().map_err(|e: toml::de::Error| e.to_string().trim().replace('\n', " "))?;
    let mut warnings = unknown_settings(&raw);
    let meta = raw.get("meta").and_then(|m| m.as_table());
    let base_name = meta.and_then(|m| m.get("base")).and_then(|b| b.as_str()).map(str::to_string).or((!builtin).then(|| DEFAULT_BASE.to_string()));
    let mut resolved = match &base_name {
        Some(b) => {
            let (_, t) = known.iter().find(|(n, _)| n == b).ok_or_else(|| format!("meta.base: there's no theme called \"{b}\""))?;
            let mut t = t.clone();
            // The base's name and description belong to the base.
            if let Some(m) = t.get_mut("meta").and_then(|m| m.as_table_mut()) {
                m.remove("name");
                m.remove("description");
                m.remove("author");
            }
            t
        }
        None => toml::Table::new(),
    };
    merge(&mut resolved, raw);
    if let Some(m) = resolved.get_mut("meta").and_then(|m| m.as_table_mut()) {
        m.remove("base");
    }
    let file_data: ThemeFile = resolved.clone().try_into().map_err(|e: toml::de::Error| {
        let msg = e.to_string();
        if msg.contains("missing field `name`") { "meta.name: give the theme a name".to_string() } else { msg.trim().replace('\n', " ") }
    })?;
    let theme = convert(file_data, file)?;
    warnings.extend(readability(&theme));
    Ok(Parsed { theme, resolved, warnings })
}

/// The folder for your own themes.
pub fn user_dir() -> PathBuf {
    crate::platform::config_dir().join("themes")
}

/// The theme library: built-ins, then your own (alphabetically), plus any
/// file that couldn't be read and why.
pub struct Library {
    pub themes: Vec<Theme>,
    pub resolved: Vec<(String, toml::Table)>,
    pub problems: Vec<(PathBuf, String)>,
}

pub fn load_library(dir: &Path) -> Library {
    let mut lib = Library { themes: Vec::new(), resolved: Vec::new(), problems: Vec::new() };
    for text in BUILTIN {
        let p = parse(text, &lib.resolved, None, true).expect("built-in themes are valid");
        lib.resolved.push((p.theme.name.clone(), p.resolved));
        lib.themes.push(p.theme);
    }
    let mut files: Vec<PathBuf> =
        std::fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "aerotheme")).collect()).unwrap_or_default();
    files.sort();
    // A theme may build on another of yours: retry until nothing new loads.
    let mut pending = files;
    loop {
        let before = pending.len();
        let mut still = Vec::new();
        for path in pending {
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    lib.problems.push((path, e.to_string()));
                    continue;
                }
            };
            match parse(&text, &lib.resolved, Some(path.clone()), false) {
                Ok(mut p) => {
                    // Two themes with one name: keep both, number the second.
                    let mut name = p.theme.name.clone();
                    let mut n = 2;
                    while lib.themes.iter().any(|t| t.name == name) {
                        name = format!("{} ({n})", p.theme.name);
                        n += 1;
                    }
                    p.theme.name = name.clone();
                    lib.resolved.push((name, p.resolved));
                    lib.themes.push(p.theme);
                }
                Err(e) if e.starts_with("meta.base: there's no theme") => still.push((path, e)),
                Err(e) => lib.problems.push((path, e)),
            }
        }
        if still.len() == before || still.is_empty() {
            lib.problems.extend(still);
            break;
        }
        pending = still.into_iter().map(|(p, _)| p).collect();
    }
    lib
}

pub fn all() -> Vec<Theme> {
    let lib = load_library(&user_dir());
    for (path, why) in &lib.problems {
        eprintln!("AeroClean: theme {} skipped: {why}", path.display());
    }
    lib.themes
}

fn config_file() -> Option<PathBuf> {
    Some(crate::platform::config_dir().join("theme"))
}

/// Index of the saved theme, or 0 (Frutiger Aero).
pub fn load(themes: &[Theme]) -> usize {
    config_file().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|name| themes.iter().position(|t| t.name == name.trim())).unwrap_or(0)
}

pub fn save(theme: &Theme) {
    save_name(&theme.name);
}

pub fn save_name(name: &str) {
    if let Some(f) = config_file() {
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(f, name);
    }
}

/// A theme's complete settings as a file (base included), for sharing.
pub fn export_text(resolved: &toml::Table) -> String {
    format!("# An AeroClean theme. Put it in AeroClean's themes folder, or run: aero-cleaner theme apply <file>\n{}", toml::to_string(resolved).unwrap_or_default())
}

/// File name for a theme in the themes folder ("Ocean Breeze" -> "ocean-breeze.aerotheme").
fn file_name(name: &str) -> String {
    let slug: String = name.chars().map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    let slug = slug.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    format!("{}.aerotheme", if slug.is_empty() { "theme".into() } else { slug })
}

/// `aero-cleaner theme ...`: list, check, apply, export.
pub fn cli(args: &[&str]) -> Result<String, String> {
    cli_in(args, &user_dir())
}

fn cli_in(args: &[&str], dir: &Path) -> Result<String, String> {
    let lib = load_library(dir);
    let find = |name: &str| lib.themes.iter().position(|t| t.name.eq_ignore_ascii_case(name));
    match args {
        ["list"] | [] => {
            let mut out: Vec<String> =
                lib.themes.iter().map(|t| format!("{:<24} {}", t.name, t.file.as_ref().map(|f| f.display().to_string()).unwrap_or_else(|| "built-in".into()))).collect();
            out.extend(lib.problems.iter().map(|(p, e)| format!("(skipped) {}: {e}", p.display())));
            Ok(out.join("\n"))
        }
        ["check", file] => {
            let text = std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
            let p = parse(&text, &lib.resolved, Some(PathBuf::from(file)), false).map_err(|e| format!("{file}: {e}"))?;
            let mut out = vec![format!("{file}: OK, \"{}\" with {} scene layers", p.theme.name, p.theme.scene.len())];
            out.extend(p.warnings.iter().map(|w| format!("  warning: {w}")));
            Ok(out.join("\n"))
        }
        ["apply", what] => {
            let path = Path::new(what);
            let name = if path.is_file() {
                let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
                let p = parse(&text, &lib.resolved, None, false).map_err(|e| format!("{what}: {e}"))?;
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                let dest = dir.join(file_name(&p.theme.name));
                if dest != path {
                    std::fs::write(&dest, &text).map_err(|e| e.to_string())?;
                }
                p.theme.name
            } else {
                let i = find(what).ok_or_else(|| format!("there's no theme called \"{what}\" (see: aero-cleaner theme list)"))?;
                lib.themes[i].name.clone()
            };
            save_name(&name);
            Ok(format!("Using \"{name}\". Open AeroClean to see it."))
        }
        ["export", name] | ["export", name, _] => {
            let i = find(name).ok_or_else(|| format!("there's no theme called \"{name}\""))?;
            let (tname, resolved) = &lib.resolved[i];
            let out = args.get(2).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(file_name(tname)));
            std::fs::write(&out, export_text(resolved)).map_err(|e| format!("{}: {e}", out.display()))?;
            Ok(format!("Saved {}", out.display()))
        }
        _ => Err("usage: aero-cleaner theme list | check <file> | apply <name or file> | export <name> [file]".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_themes_load_cleanly() {
        let lib = load_library(Path::new("/nonexistent"));
        let names: Vec<&str> = lib.themes.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Frutiger Aero", "Aqua Abyss", "Sunset Vista", "Aurora Night", "Sakura Spring", "Aero Basic"]);
        for (text, t) in BUILTIN.iter().zip(&lib.themes) {
            let p = parse(text, &[], None, true).unwrap();
            assert!(p.warnings.is_empty(), "{}: {:?}", t.name, p.warnings);
            assert_eq!(t.material, Material::CLASSIC);
        }
        let aurora = &lib.themes[3];
        assert!(aurora.dark && matches!(aurora.scene[1], Layer::Stars { count: 90 }) && aurora.scene[2] == Layer::Aurora);
        assert_eq!(aurora.text, Color32::from_rgb(232, 245, 255));
    }

    #[test]
    fn a_small_file_inherits_the_rest() {
        let lib = load_library(Path::new("/nonexistent"));
        let p = parse("format = 1\n[meta]\nname = \"Mint\"\n[colors]\naccent = \"#3cb371\"\n[material]\ncorners = 0.4\n", &lib.resolved, None, false).unwrap();
        assert_eq!(p.theme.name, "Mint");
        assert_eq!(p.theme.accent, Color32::from_rgb(0x3c, 0xb3, 0x71));
        assert_eq!(p.theme.text, lib.themes[0].text, "inherited from Frutiger Aero");
        assert_eq!(p.theme.material.corners, 0.4);
        assert_eq!(p.theme.scene, lib.themes[0].scene);
        assert!(p.theme.desc.is_empty(), "the base's description isn't inherited");

        let night = parse("format = 1\n[meta]\nname = \"Deep\"\nbase = \"Aurora Night\"\n[[scene.layers]]\ntype = \"sky\"\nstops = [[0.0, \"#000000\"], [1.0, \"#102040\"]]\n[[scene.layers]]\ntype = \"bubbles\"\ncount = 40\n", &lib.resolved, None, false).unwrap();
        assert!(night.theme.dark);
        assert_eq!(night.theme.scene.len(), 2, "layers replace the base's");
        assert_eq!(night.theme.scene[1], Layer::Bubbles { color: lib.themes[3].bubble, count: 40 });
    }

    #[test]
    fn explains_mistakes() {
        let lib = load_library(Path::new("/nonexistent"));
        let err = |t: &str| parse(t, &lib.resolved, None, false).err().unwrap_or_default();
        assert!(err("format = 1\n[meta]\nname = \"X\"\n[colors]\ntext = \"blue\"\n").contains("colors.text: \"blue\" isn't a color"));
        assert!(err("format = 1\n[meta]\nname = \"X\"\n[material]\ngloss = 9\n").contains("material.gloss must be between 0 and 1.5"));
        assert!(err("format = 1\n[meta]\nname = \"X\"\nbase = \"Nope\"\n").contains("no theme called \"Nope\""));
        assert!(err("format = 2\n[meta]\nname = \"X\"\n").contains("format 2 isn't supported"));
        assert!(err("format = 1\n[colors]\ntext = \"#000000\"\n").contains("meta.name"));
        assert!(err("format = 1\n[meta]\nname = \"X\"\n[[scene.layers]]\ntype = \"volcano\"\n").contains("volcano"));
        assert!(!err("format = 1\n[meta\nname = 1").is_empty(), "TOML errors come through");
        let p = parse("format = 1\n[meta]\nname = \"X\"\n[colors]\ntext = \"#e0e0e0\"\naccnet = \"#ff0000\"\n[sparkles]\nx = 1\n", &lib.resolved, None, false).unwrap();
        assert!(p.warnings.iter().any(|w| w.contains("colors.accnet")));
        assert!(p.warnings.iter().any(|w| w.contains("[sparkles]")));
        assert!(p.warnings.iter().any(|w| w.contains("colors.text may be hard to read")), "{:?}", p.warnings);
    }

    #[test]
    fn library_loads_user_themes_in_any_order() {
        let dir = std::env::temp_dir().join(format!("aero-themes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // "a" builds on "b", which comes later alphabetically.
        std::fs::write(dir.join("a.aerotheme"), "format = 1\n[meta]\nname = \"Child\"\nbase = \"Parent\"\n").unwrap();
        std::fs::write(dir.join("b.aerotheme"), "format = 1\n[meta]\nname = \"Parent\"\n[colors]\ngo = \"#ff00ff\"\n").unwrap();
        std::fs::write(dir.join("c.aerotheme"), "format = 1\n[meta]\nname = \"Frutiger Aero\"\n").unwrap();
        std::fs::write(dir.join("d.aerotheme"), "not a theme").unwrap();
        std::fs::write(dir.join("notes.txt"), "ignored").unwrap();
        let lib = load_library(&dir);
        let names: Vec<&str> = lib.themes.iter().skip(6).map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Parent", "Frutiger Aero (2)", "Child"]);
        assert_eq!(lib.themes.iter().find(|t| t.name == "Child").unwrap().go, Color32::from_rgb(255, 0, 255));
        assert_eq!(lib.problems.len(), 1, "the broken file is reported, not fatal");
        // Export round-trips.
        let (_, resolved) = lib.resolved.iter().find(|(n, _)| n == "Child").unwrap();
        let again = parse(&export_text(resolved), &[], None, true).unwrap();
        assert_eq!(again.theme.go, Color32::from_rgb(255, 0, 255));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn command_line() {
        let dir = std::env::temp_dir().join(format!("aero-theme-cli-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("incoming.txt");
        std::fs::write(&file, "format = 1\n[meta]\nname = \"Ocean Breeze!\"\n[colors]\naccent = \"#0080c0\"\n").unwrap();
        let f = file.to_str().unwrap();
        assert!(cli_in(&["check", f], &dir).unwrap().contains("OK, \"Ocean Breeze!\""));
        let themes = dir.join("themes");
        // apply copies a file into the themes folder (it doesn't touch the real settings here
        // only through save_name, which writes the chosen name: checked by list below).
        let list_before = cli_in(&["list"], &themes).unwrap();
        assert!(list_before.contains("Aero Basic") && !list_before.contains("Ocean"));
        std::fs::create_dir_all(&themes).unwrap();
        std::fs::copy(&file, themes.join(file_name("Ocean Breeze!"))).unwrap();
        assert_eq!(file_name("Ocean Breeze!"), "ocean-breeze.aerotheme");
        assert!(cli_in(&["list"], &themes).unwrap().contains("Ocean Breeze!"));
        let out = dir.join("out.aerotheme");
        assert!(cli_in(&["export", "ocean breeze!", out.to_str().unwrap()], &themes).unwrap().starts_with("Saved"));
        let again = std::fs::read_to_string(&out).unwrap();
        assert!(parse(&again, &[], None, true).unwrap().theme.accent == Color32::from_rgb(0, 0x80, 0xc0), "export is complete on its own");
        assert!(cli_in(&["check", "/nonexistent/x"], &dir).is_err());
        assert!(cli_in(&["export", "Nope"], &dir).unwrap_err().contains("no theme called"));
        assert!(cli_in(&["frobnicate"], &dir).unwrap_err().starts_with("usage"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
