//! The Theme Studio's editing model, kept apart from the UI so it's easy to
//! test: a draft (the theme file's own settings, before its base is added),
//! undo/redo, color harmonies, a randomizer that keeps text readable, and a
//! color-blindness preview.

use crate::theme::{self, Layer, Parsed, SceneLayer, Stops, Theme};
use eframe::egui::Color32;
use std::path::{Path, PathBuf};
use toml::{Table, Value};

/// A theme being edited.
pub struct Draft {
    /// What the file will contain: `meta` (with `base`) and the changes.
    pub table: Table,
    /// Where it's saved; None until the first save.
    pub file: Option<PathBuf>,
    undo: Vec<Table>,
    redo: Vec<Table>,
    pub dirty: bool,
}

fn table_mut<'a>(t: &'a mut Table, path: &[&str]) -> &'a mut Table {
    let mut cur = t;
    for key in path {
        let entry = cur.entry(key.to_string()).or_insert_with(|| Value::Table(Table::new()));
        if !entry.is_table() {
            *entry = Value::Table(Table::new());
        }
        cur = entry.as_table_mut().unwrap();
    }
    cur
}

impl Draft {
    /// A new theme built on `base` (a copy that starts with no changes).
    pub fn customize(base: &Theme, taken: &[String]) -> Draft {
        let mut name = format!("{} (custom)", base.name);
        let mut n = 2;
        while taken.iter().any(|t| t == &name) {
            name = format!("{} (custom {n})", base.name);
            n += 1;
        }
        let mut table = Table::new();
        table.insert("format".into(), Value::Integer(1));
        let meta = table_mut(&mut table, &["meta"]);
        meta.insert("name".into(), Value::String(name));
        meta.insert("base".into(), Value::String(base.name.clone()));
        Draft { table, file: None, undo: vec![], redo: vec![], dirty: true }
    }

    /// One of your theme files, to change it.
    pub fn open(path: &Path) -> Result<Draft, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let mut table: Table = text.parse().map_err(|e: toml::de::Error| e.to_string())?;
        // Files without a base build on the default one.
        let meta = table_mut(&mut table, &["meta"]);
        if !meta.contains_key("base") && path.extension().is_some_and(|e| e == "aerotheme") {
            meta.insert("base".into(), Value::String(theme::DEFAULT_BASE.into()));
        }
        Ok(Draft { table, file: Some(path.to_path_buf()), undo: vec![], redo: vec![], dirty: false })
    }

    pub fn name(&self) -> String {
        self.get(&["meta", "name"]).and_then(|v| v.as_str()).unwrap_or("").to_string()
    }

    pub fn base(&self) -> String {
        self.get(&["meta", "base"]).and_then(|v| v.as_str()).unwrap_or(theme::DEFAULT_BASE).to_string()
    }

    pub fn get(&self, path: &[&str]) -> Option<&Value> {
        let (last, parents) = path.split_last()?;
        let mut cur = &self.table;
        for key in parents {
            cur = cur.get(*key)?.as_table()?;
        }
        cur.get(*last)
    }

    /// Remembers the current state for undo. Call once before a change
    /// (or at the start of a slider drag), not on every frame of it.
    pub fn checkpoint(&mut self) {
        self.undo.push(self.table.clone());
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn set(&mut self, path: &[&str], v: Value) {
        let (last, parents) = path.split_last().unwrap();
        table_mut(&mut self.table, parents).insert(last.to_string(), v);
        self.dirty = true;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.table, prev));
            self.dirty = true;
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.table, next));
            self.dirty = true;
        }
    }

    /// Forgets every change: back to exactly the base theme (name kept).
    pub fn reset_to_base(&mut self) {
        self.checkpoint();
        let (name, base) = (self.name(), self.base());
        self.table = Table::new();
        self.table.insert("format".into(), Value::Integer(1));
        let meta = table_mut(&mut self.table, &["meta"]);
        meta.insert("name".into(), Value::String(name));
        meta.insert("base".into(), Value::String(base));
        self.dirty = true;
    }

    /// The finished theme (with its base), or why it can't be built.
    pub fn build(&self, known: &[(String, Table)]) -> Result<Parsed, String> {
        // Changing an existing theme: it mustn't build on itself.
        let others: Vec<(String, Table)> = known.iter().filter(|(n, _)| *n != self.name() || self.base() == *n).cloned().collect();
        theme::parse(&self.text(), &others, self.file.clone(), false)
    }

    /// The scene layers as editable tables: the draft's own, or else the base's.
    pub fn layers(&self, resolved: &Table) -> Vec<Table> {
        let pick = |t: &Table| t.get("scene").and_then(|s| s.get("layers")).and_then(|l| l.as_array()).map(|a| a.iter().filter_map(|v| v.as_table().cloned()).collect::<Vec<_>>());
        pick(&self.table).or_else(|| pick(resolved)).unwrap_or_default()
    }

    pub fn set_layers(&mut self, layers: Vec<Table>) {
        self.set(&["scene", "layers"], Value::Array(layers.into_iter().map(Value::Table).collect()));
    }

    /// The file's text.
    pub fn text(&self) -> String {
        format!(
            "# An AeroClean theme, made in the Theme Studio. Settings that aren't here come from\n# its base theme. Edit it in any text editor: AeroClean updates when you save.\n{}",
            toml::to_string(&self.table).unwrap_or_default()
        )
    }

    /// Saves into `dir` (a new file named after the theme, never overwriting
    /// another one), or over the file it came from.
    pub fn save(&mut self, dir: &Path) -> Result<PathBuf, String> {
        let path = match &self.file {
            Some(p) => p.clone(),
            None => {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                let stem = theme::file_name(&self.name());
                let stem = stem.trim_end_matches(".aerotheme").to_string();
                let mut p = dir.join(format!("{stem}.aerotheme"));
                let mut n = 2;
                while p.exists() {
                    p = dir.join(format!("{stem}-{n}.aerotheme"));
                    n += 1;
                }
                p
            }
        };
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, self.text()).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
        self.file = Some(path.clone());
        self.dirty = false;
        Ok(path)
    }
}

// ---------- Colors ----------

pub fn hex(c: Color32) -> String {
    if c.a() == 255 { format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b()) } else { format!("#{:02x}{:02x}{:02x}{:02x}", c.r(), c.g(), c.b(), c.a()) }
}

/// Hue (0..360), saturation and value (0..1).
pub fn to_hsv(c: Color32) -> (f32, f32, f32) {
    let (r, g, b) = (c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0);
    let max = r.max(g).max(b);
    let d = max - r.min(g).min(b);
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, if max == 0.0 { 0.0 } else { d / max }, max)
}

pub fn from_hsv(h: f32, s: f32, v: f32) -> Color32 {
    let h = h.rem_euclid(360.0) / 60.0;
    let (s, v) = (s.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let ch = |v: f32| ((v + m) * 255.0).round() as u8;
    Color32::from_rgb(ch(r), ch(g), ch(b))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Harmony {
    Complementary,
    Analogous,
    Triad,
}

/// The color keys a harmony fills in, from the accent color.
pub fn harmony(accent: Color32, kind: Harmony) -> Vec<(&'static str, Color32)> {
    let (h, s, v) = to_hsv(accent);
    let (s, v) = (s.clamp(0.5, 0.85), v.clamp(0.7, 0.92));
    let at = |dh: f32, dv: f32| from_hsv(h + dh, s, (v + dv).clamp(0.0, 1.0));
    let (go, root, user, logo) = match kind {
        Harmony::Complementary => (at(180.0, 0.0), at(160.0, 0.0), at(0.0, 0.05), at(200.0, 0.05)),
        Harmony::Analogous => (at(35.0, 0.0), at(-35.0, 0.0), at(15.0, 0.05), at(-15.0, 0.05)),
        Harmony::Triad => (at(120.0, 0.0), at(240.0, 0.0), at(0.0, 0.05), at(120.0, 0.05)),
    };
    vec![("accent", at(0.0, 0.0)), ("go", go), ("root_orb", root), ("user_orb", user), ("logo", logo)]
}

/// A small deterministic random generator (no extra dependency).
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

pub fn stops_value(stops: &[(f32, Color32)]) -> Value {
    Value::Array(stops.iter().map(|(p, c)| Value::Array(vec![Value::Float((*p as f64 * 100.0).round() / 100.0), Value::String(hex(*c))])).collect())
}

/// A new harmonious look: colors, sky, hills, sun and bubbles, with text
/// that stays readable. Keeps light or dark glass as it is.
pub fn randomize(draft: &mut Draft, resolved: &Table, dark: bool, seed: u64) {
    let mut rng = Rng::new(seed);
    let h = rng.next() * 360.0;
    let kind = [Harmony::Complementary, Harmony::Analogous, Harmony::Triad][(rng.next() * 3.0) as usize % 3];
    let accent = from_hsv(h, 0.55 + rng.next() * 0.3, 0.75 + rng.next() * 0.15);
    for (k, c) in harmony(accent, kind) {
        draft.set(&["colors", k], Value::String(hex(c)));
    }
    let (glass, text, soft, sky): (Color32, Color32, Color32, Stops) = if dark {
        (
            from_hsv(h, 0.55, 0.3),
            from_hsv(h, 0.08, 0.98),
            from_hsv(h, 0.25, 0.85),
            vec![(0.0, from_hsv(h + 20.0, 0.75, 0.1)), (0.4, from_hsv(h + 10.0, 0.7, 0.25)), (0.75, from_hsv(h, 0.6, 0.42)), (1.0, from_hsv(h - 10.0, 0.5, 0.55))],
        )
    } else {
        (
            from_hsv(h, 0.1, 1.0),
            from_hsv(h, 0.85, 0.32),
            from_hsv(h, 0.55, 0.48),
            vec![(0.0, from_hsv(h, 0.85, 0.7)), (0.4, from_hsv(h, 0.6, 0.92)), (0.72, from_hsv(h, 0.25, 1.0)), (1.0, from_hsv(h, 0.08, 1.0))],
        )
    };
    draft.set(&["colors", "glass"], Value::String(hex(glass)));
    draft.set(&["colors", "text"], Value::String(hex(text)));
    draft.set(&["colors", "soft"], Value::String(hex(soft)));
    let bubble = from_hsv(h, 0.12, 1.0);
    draft.set(&["colors", "bubble"], Value::String(hex(bubble)));
    // Hills in a neighboring hue, darker toward the bottom.
    let hh = h + if rng.next() > 0.5 { 120.0 } else { -60.0 };
    let hills = |v: f32| vec![(0.0, from_hsv(hh, 0.45, v)), (0.12, from_hsv(hh, 0.6, v * 0.82)), (0.3, from_hsv(hh, 0.7, v * 0.62)), (1.0, from_hsv(hh, 0.75, v * 0.45))];
    let mut layers = draft.layers(resolved);
    for l in &mut layers {
        match l.get("type").and_then(|v| v.as_str()) {
            Some("sky") => {
                l.insert("stops".into(), stops_value(&sky));
            }
            Some("hills") => {
                let v = if dark { 0.45 } else { 0.9 };
                l.insert("back".into(), stops_value(&hills(v * 0.9)));
                l.insert("front".into(), stops_value(&hills(v)));
            }
            Some("sun") | Some("moon") => {
                l.insert("color".into(), Value::String(hex(from_hsv(h + 30.0, 0.1, 1.0))));
            }
            Some("bubbles") => {
                l.remove("color"); // follows colors.bubble
            }
            _ => {}
        }
    }
    draft.set_layers(layers);
}

/// Settings for a layer just added in the Studio.
pub fn new_layer(kind: &str) -> Option<Table> {
    let mut t = Table::new();
    t.insert("type".into(), Value::String(kind.into()));
    let mut put = |k: &str, v: Value| {
        t.insert(k.into(), v);
    };
    match kind {
        "sky" => put("stops", stops_value(&[(0.0, Color32::from_rgb(10, 88, 180)), (1.0, Color32::from_rgb(235, 250, 255))])),
        "sun" | "moon" => {
            put("x", Value::Float(0.85));
            put("y", Value::Float(0.12));
            put("color", Value::String("#fffff0".into()));
        }
        "rays" => put("strength", Value::Integer(45)),
        "hills" => {
            put("back", stops_value(&[(0.0, Color32::from_rgb(150, 225, 110)), (1.0, Color32::from_rgb(30, 100, 20))]));
            put("front", stops_value(&[(0.0, Color32::from_rgb(185, 245, 120)), (1.0, Color32::from_rgb(30, 110, 20))]));
        }
        "stars" | "aurora" | "bubbles" | "clouds" | "fish" | "fireflies" | "petals" | "snow" | "leaves" => {}
        _ => return None,
    }
    Some(t)
}

/// Layer types the Studio can add (pictures go through a file picker).
pub const ADDABLE: &[&str] = &["sky", "sun", "moon", "rays", "stars", "aurora", "hills", "bubbles", "clouds", "fish", "fireflies", "petals", "snow", "leaves"];

// ---------- Color-blindness preview ----------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Vision {
    Normal,
    Protanopia,
    Deuteranopia,
    Tritanopia,
}

fn to_linear(v: u8) -> f32 {
    let s = v as f32 / 255.0;
    if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
}

fn to_srgb(l: f32) -> u8 {
    let l = l.clamp(0.0, 1.0);
    let s = if l <= 0.0031308 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0).round() as u8
}

/// How a color looks with a color vision deficiency (Machado et al. 2009, full severity).
pub fn simulate(c: Color32, vision: Vision) -> Color32 {
    let m: [[f32; 3]; 3] = match vision {
        Vision::Normal => return c,
        Vision::Protanopia => [[0.152286, 1.052583, -0.204868], [0.114503, 0.786281, 0.099216], [-0.003882, -0.048116, 1.051998]],
        Vision::Deuteranopia => [[0.367322, 0.860646, -0.227968], [0.280085, 0.672501, 0.047413], [-0.011820, 0.042940, 0.968881]],
        Vision::Tritanopia => [[1.255528, -0.076749, -0.178779], [-0.078411, 0.930809, 0.147602], [0.004733, 0.691367, 0.303900]],
    };
    let v = [to_linear(c.r()), to_linear(c.g()), to_linear(c.b())];
    let o = |row: [f32; 3]| to_srgb(row[0] * v[0] + row[1] * v[1] + row[2] * v[2]);
    Color32::from_rgba_unmultiplied(o(m[0]), o(m[1]), o(m[2]), c.a())
}

/// The whole theme as someone with that vision would see it (preview only).
pub fn simulate_theme(t: &Theme, vision: Vision) -> Theme {
    if vision == Vision::Normal {
        return t.clone();
    }
    let f = |c: Color32| simulate(c, vision);
    let fs = |s: &Stops| s.iter().map(|(p, c)| (*p, f(*c))).collect::<Stops>();
    let mut out = t.clone();
    for c in [&mut out.glass, &mut out.text, &mut out.soft, &mut out.accent, &mut out.go, &mut out.user_orb, &mut out.root_orb, &mut out.logo, &mut out.bubble] {
        *c = f(*c);
    }
    out.scene = t
        .scene
        .iter()
        .map(|l| {
            let kind = match &l.kind {
                Layer::Sky { stops } => Layer::Sky { stops: fs(stops) },
                Layer::Sun { pos, color } => Layer::Sun { pos: *pos, color: f(*color) },
                Layer::Moon { pos, color } => Layer::Moon { pos: *pos, color: f(*color) },
                Layer::Hills { back, front } => Layer::Hills { back: fs(back), front: fs(front) },
                Layer::Bubbles { color, count } => Layer::Bubbles { color: f(*color), count: *count },
                Layer::Clouds { color, count, y, size } => Layer::Clouds { color: f(*color), count: *count, y: *y, size: *size },
                Layer::Fish { color, count, y } => Layer::Fish { color: f(*color), count: *count, y: *y },
                Layer::Fireflies { color, count } => Layer::Fireflies { color: f(*color), count: *count },
                Layer::Petals { color, count } => Layer::Petals { color: f(*color), count: *count },
                Layer::Snow { color, count } => Layer::Snow { color: f(*color), count: *count },
                Layer::Leaves { color, count } => Layer::Leaves { color: f(*color), count: *count },
                other => other.clone(),
            };
            SceneLayer { kind, ..l.clone() }
        })
        .collect();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib() -> theme::Library {
        theme::load_library(Path::new("/nonexistent"))
    }

    #[test]
    fn draft_edits_undo_and_saves_only_changes() {
        let lib = lib();
        let mut d = Draft::customize(&lib.themes[1], &["Aqua Abyss (custom)".into()]);
        assert_eq!(d.name(), "Aqua Abyss (custom 2)", "a free name");
        assert_eq!(d.build(&lib.resolved).unwrap().theme.accent, lib.themes[1].accent, "starts as its base");
        d.checkpoint();
        d.set(&["colors", "accent"], Value::String("#ff0000".into()));
        d.checkpoint();
        d.set(&["material", "corners"], Value::Float(0.5));
        let built = d.build(&lib.resolved).unwrap().theme;
        assert_eq!((built.accent, built.material.corners), (Color32::from_rgb(255, 0, 0), 0.5));
        d.undo();
        assert!(d.get(&["material", "corners"]).is_none());
        d.undo();
        assert!(d.get(&["colors", "accent"]).is_none() && !d.can_undo());
        d.redo();
        assert_eq!(d.get(&["colors", "accent"]).and_then(|v| v.as_str()), Some("#ff0000"));

        let dir = std::env::temp_dir().join(format!("aero-draft-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = d.save(&dir).unwrap();
        assert_eq!(path.file_name().unwrap(), "aqua-abyss-custom-2.aerotheme");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("base = \"Aqua Abyss\"") && text.contains("#ff0000") && !text.contains("hills"), "only the changes:\n{text}");
        let mut again = Draft::customize(&lib.themes[1], &[]);
        again.table = d.table.clone();
        let p2 = again.save(&dir).unwrap();
        assert_ne!(p2, path, "never overwrites another theme's file");
        // Reopen and change the saved file.
        let mut reopened = Draft::open(&path).unwrap();
        assert!(!reopened.dirty);
        reopened.reset_to_base();
        assert_eq!(reopened.build(&lib.resolved).unwrap().theme.accent, lib.themes[1].accent);
        assert_eq!(reopened.name(), "Aqua Abyss (custom 2)");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn editing_layers_copies_them_from_the_base() {
        let lib = lib();
        let mut d = Draft::customize(&lib.themes[0], &[]);
        let base_resolved = &lib.resolved[0].1;
        let mut layers = d.layers(base_resolved);
        assert_eq!(layers.len(), 5, "Frutiger Aero's layers to start from");
        layers.push(new_layer("fish").unwrap());
        layers.swap(0, 1);
        layers.swap(0, 1);
        d.set_layers(layers);
        let t = d.build(&lib.resolved).unwrap().theme;
        assert_eq!(t.scene.len(), 6);
        assert!(matches!(t.scene[5].kind, Layer::Fish { .. }));
        for kind in ADDABLE {
            let mut d = Draft::customize(&lib.themes[0], &[]);
            d.set_layers(vec![new_layer(kind).unwrap()]);
            assert!(d.build(&lib.resolved).is_ok(), "a new {kind} layer is valid");
        }
    }

    #[test]
    fn colors_harmonies_and_randomize_stay_readable() {
        for c in [Color32::from_rgb(30, 130, 225), Color32::from_rgb(200, 40, 90), Color32::from_rgb(12, 12, 12), Color32::WHITE] {
            let (h, s, v) = to_hsv(c);
            let back = from_hsv(h, s, v);
            assert!((back.r() as i32 - c.r() as i32).abs() <= 1 && (back.g() as i32 - c.g() as i32).abs() <= 1, "{c:?} -> {back:?}");
        }
        let h = harmony(Color32::from_rgb(30, 130, 225), Harmony::Complementary);
        let (ah, ..) = to_hsv(h[0].1);
        let (gh, ..) = to_hsv(h[1].1);
        assert!(((gh - ah).rem_euclid(360.0) - 180.0).abs() < 3.0, "complementary is opposite");
        let lib = lib();
        for seed in 0..40 {
            for dark in [false, true] {
                let mut d = Draft::customize(&lib.themes[if dark { 3 } else { 0 }], &[]);
                let resolved = lib.resolved[if dark { 3 } else { 0 }].1.clone();
                randomize(&mut d, &resolved, dark, seed);
                let p = d.build(&lib.resolved).unwrap();
                assert!(p.warnings.is_empty(), "seed {seed} dark {dark}: {:?}", p.warnings);
            }
        }
        let mut r = Rng::new(7);
        assert!((0..1000).map(|_| r.next()).all(|v| (0.0..1.0).contains(&v)));
    }

    #[test]
    fn color_blindness_preview() {
        let red = Color32::from_rgb(220, 40, 40);
        let green = Color32::from_rgb(40, 180, 60);
        let (r, g) = (simulate(red, Vision::Deuteranopia), simulate(green, Vision::Deuteranopia));
        let dist = |a: Color32, b: Color32| (a.r() as i32 - b.r() as i32).abs() + (a.g() as i32 - b.g() as i32).abs() + (a.b() as i32 - b.b() as i32).abs();
        assert!(dist(r, g) < dist(red, green) / 2, "red and green get much closer: {r:?} {g:?}");
        assert_eq!(simulate(Color32::WHITE, Vision::Protanopia), Color32::WHITE, "white stays white");
        let lib = lib();
        let sim = simulate_theme(&lib.themes[0], Vision::Tritanopia);
        assert_ne!(sim.accent, lib.themes[0].accent);
        assert_eq!(sim.scene.len(), lib.themes[0].scene.len());
    }
}
