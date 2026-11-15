//! Themes from a picture: AeroClean finds a picture's main colors and how
//! its colors run from top to bottom, then builds three matching themes
//! (light, dark and vivid). Text colors are adjusted until they pass the
//! same contrast check as every theme.

use crate::theme::{self, Theme};
use crate::theme_edit::{Draft, from_hsv, hex, stops_value, to_hsv};
use eframe::egui::Color32;
use std::path::{Path, PathBuf};
use toml::{Table, Value};

/// Pictures bigger than this aren't opened.
pub const MAX_FILE: u64 = 64 * 1024 * 1024;

/// What a picture looks like, in a few colors.
#[derive(Clone, Debug)]
pub struct Analysis {
    pub file: PathBuf,
    /// Main colors, most common first, with their share of the picture.
    pub palette: Vec<(Color32, f32)>,
    /// The average color of 4 bands, from the top of the picture down.
    pub bands: [Color32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Variant {
    Light,
    Dark,
    Vivid,
}

impl Variant {
    pub const ALL: [Variant; 3] = [Variant::Light, Variant::Dark, Variant::Vivid];

    pub fn label(self) -> &'static str {
        match self {
            Variant::Light => "Light",
            Variant::Dark => "Dark",
            Variant::Vivid => "Vivid",
        }
    }
}

/// Opens and analyzes a picture (in the background: big photos take a moment).
pub fn analyze(path: &Path) -> Result<Analysis, String> {
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    if !["png", "jpg", "jpeg", "webp"].contains(&ext.as_str()) {
        return Err("pick a .png, .jpg or .webp picture".into());
    }
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_FILE {
        return Err("the picture is too big (over 64 MB)".into());
    }
    let img = image::open(path).map_err(|e| e.to_string())?;
    let small = img.thumbnail(96, 96).to_rgb8();
    let pixels: Vec<[f32; 3]> = small.pixels().map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect();
    if pixels.is_empty() {
        return Err("the picture is empty".into());
    }
    let (w, h) = small.dimensions();
    let bands = std::array::from_fn(|b| {
        let rows = (h as usize * b / 4)..(h as usize * (b + 1) / 4).max(h as usize * b / 4 + 1);
        let band: Vec<[f32; 3]> = rows.flat_map(|y| (0..w as usize).map(move |x| (x, y))).filter_map(|(x, y)| pixels.get(y * w as usize + x).copied()).collect();
        to_color(mean(&band))
    });
    Ok(Analysis { file: path.to_path_buf(), palette: kmeans(&pixels, 6), bands })
}

fn mean(px: &[[f32; 3]]) -> [f32; 3] {
    let n = px.len().max(1) as f32;
    let s = px.iter().fold([0.0; 3], |a, p| [a[0] + p[0], a[1] + p[1], a[2] + p[2]]);
    [s[0] / n, s[1] / n, s[2] / n]
}

fn to_color(c: [f32; 3]) -> Color32 {
    Color32::from_rgb(c[0].round() as u8, c[1].round() as u8, c[2].round() as u8)
}

fn dist(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// The `k` main colors (k-means, started from far-apart pixels so it's
/// the same every time), most common first.
pub fn kmeans(px: &[[f32; 3]], k: usize) -> Vec<(Color32, f32)> {
    let mut centers = vec![mean(px)];
    while centers.len() < k {
        let far = px.iter().max_by(|a, b| {
            let da = centers.iter().map(|c| dist(a, c)).fold(f32::MAX, f32::min);
            let db = centers.iter().map(|c| dist(b, c)).fold(f32::MAX, f32::min);
            da.total_cmp(&db)
        });
        match far {
            Some(p) if !centers.contains(p) => centers.push(*p),
            _ => break,
        }
    }
    let mut owner = vec![0; px.len()];
    for _ in 0..12 {
        for (i, p) in px.iter().enumerate() {
            owner[i] = (0..centers.len()).min_by(|&a, &b| dist(p, &centers[a]).total_cmp(&dist(p, &centers[b]))).unwrap_or(0);
        }
        for (c, center) in centers.iter_mut().enumerate() {
            let mine: Vec<[f32; 3]> = px.iter().zip(&owner).filter(|(_, o)| **o == c).map(|(p, _)| *p).collect();
            if !mine.is_empty() {
                *center = mean(&mine);
            }
        }
    }
    let mut out: Vec<(Color32, f32)> =
        centers.iter().enumerate().map(|(c, center)| (to_color(*center), owner.iter().filter(|o| **o == c).count() as f32 / px.len() as f32)).filter(|(_, w)| *w > 0.0).collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1));
    out
}

fn hue_gap(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// The picture's key hues: the most striking color (common and colorful),
/// and a second one that's clearly different (or its complement).
pub fn key_hues(a: &Analysis) -> (f32, f32, f32) {
    let score = |(c, w): &(Color32, f32)| {
        let (_, s, v) = to_hsv(*c);
        w.sqrt() * (s * v + 0.03)
    };
    let mut ranked = a.palette.clone();
    ranked.sort_by(|x, y| score(y).total_cmp(&score(x)));
    let (main, _) = ranked.first().copied().unwrap_or((Color32::from_rgb(40, 140, 220), 1.0));
    let (h1, s1, _) = to_hsv(main);
    // Gray pictures: a calm blue instead of a hue that's just noise.
    let h1 = if s1 < 0.08 { 205.0 } else { h1 };
    let h2 = ranked.iter().map(|(c, _)| to_hsv(*c)).find(|(h, s, _)| *s > 0.15 && hue_gap(*h, h1) > 40.0).map(|(h, _, _)| h).unwrap_or(h1 + 150.0);
    (h1, h2, s1)
}

/// Moves `text` toward black (light glass) or white (dark glass) until it
/// reaches `min` contrast with what's behind it.
pub fn readable(text: Color32, behind: Color32, dark: bool, min: f32) -> Color32 {
    let (h, mut s, mut v) = to_hsv(text);
    let mut c = text;
    for _ in 0..60 {
        if theme::contrast(c, behind) >= min {
            return c;
        }
        if dark {
            v = (v + 0.03).min(1.0);
            s *= 0.9;
        } else {
            v = (v - 0.03).max(0.0);
            s = (s + 0.02).min(1.0);
        }
        c = from_hsv(h, s, v);
    }
    if dark { Color32::WHITE } else { Color32::from_rgb(10, 14, 24) }
}

/// What text sits on: the same as the contrast check in `theme::readability`.
fn behind(glass: Color32, dark: bool) -> Color32 {
    if dark { glass } else { crate::aero::lerp(Color32::WHITE, glass, 0.3) }
}

/// A theme's file text. `picture` is the picture layer's `file` setting,
/// when the picture itself is the background.
pub fn build(a: &Analysis, variant: Variant, name: &str, picture: Option<&str>) -> String {
    let (h1, h2, _) = key_hues(a);
    let dark = variant == Variant::Dark;
    let vivid = variant == Variant::Vivid;
    let (sat, val) = if vivid { (0.88, 0.9) } else { (0.68, 0.8) };
    let accent = from_hsv(h1, sat, val);
    let go = from_hsv(h2, sat, if dark { 0.75 } else { 0.72 });
    let glass = match variant {
        Variant::Light => from_hsv(h1, 0.08, 1.0),
        Variant::Vivid => from_hsv(h1, 0.18, 1.0),
        Variant::Dark => from_hsv(h1, 0.45, 0.22),
    };
    let back = behind(glass, dark);
    let (text, soft) = if dark { (from_hsv(h1, 0.06, 0.97), from_hsv(h1, 0.22, 0.84)) } else { (from_hsv(h1, 0.8, 0.3), from_hsv(h1, 0.5, 0.46)) };
    // A little over the minimum, so rounding never tips it under.
    let text = readable(text, back, dark, 4.8);
    let soft = readable(soft, back, dark, 3.3);

    let builtin = theme::load_library(Path::new("/nonexistent"));
    let mut d = Draft::customize(&builtin.themes[0], &[]);
    d.set(&["meta", "name"], Value::String(name.into()));
    d.set(&["meta", "description"], Value::String(format!("Made from {}", a.file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())));
    for (k, c) in [
        ("text", text),
        ("soft", soft),
        ("glass", glass),
        ("accent", accent),
        ("go", go),
        ("user_orb", from_hsv(h1 + 12.0, sat * 0.9, (val + 0.08).min(1.0))),
        ("root_orb", from_hsv(h2 + 20.0, sat, val * 0.95)),
        ("logo", from_hsv(h2, sat, val)),
        ("bubble", from_hsv(h1, 0.12, 1.0)),
    ] {
        d.set(&["colors", k], Value::String(hex(c)));
    }
    d.set(&["material", "dark_glass"], Value::Boolean(dark));

    // The sky follows the picture's colors from top to bottom.
    let sky: Vec<(f32, Color32)> = a
        .bands
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let (h, s, v) = to_hsv(*c);
            let h = if s < 0.06 { h1 } else { h };
            let c = match variant {
                Variant::Light => from_hsv(h, (s * 0.6).clamp(0.08, 0.42), v.clamp(0.82, 1.0)),
                Variant::Vivid => from_hsv(h, (s * 1.4).clamp(0.65, 1.0), v.clamp(0.8, 1.0)),
                Variant::Dark => from_hsv(h, s.clamp(0.3, 0.8), v.clamp(0.06, 0.38)),
            };
            (i as f32 / 3.0, c)
        })
        .collect();
    let (bh, bs, _) = to_hsv(a.bands[3]);
    let hh = if bs < 0.1 { h2 } else { bh };
    let hv = if dark { 0.4 } else { 0.85 };
    let hills = |v: f32| vec![(0.0, from_hsv(hh, 0.45, v)), (0.12, from_hsv(hh, 0.6, v * 0.82)), (0.3, from_hsv(hh, 0.7, v * 0.62)), (1.0, from_hsv(hh, 0.75, v * 0.45))];
    let layer = |kind: &str, items: Vec<(&str, Value)>| {
        let mut t = Table::new();
        t.insert("type".into(), Value::String(kind.into()));
        for (k, v) in items {
            t.insert(k.into(), v);
        }
        t
    };
    let light = from_hsv(h1 + 30.0, 0.1, 1.0);
    let mut layers = vec![layer("sky", vec![("stops", stops_value(&sky))])];
    match picture {
        // By night, the picture is dimmed by the dark sky under it.
        Some(file) => {
            layers.push(layer("picture", vec![("file", Value::String(file.into())), ("frosted", Value::Boolean(true)), ("opacity", Value::Float(if dark { 0.35 } else { 1.0 }))]))
        }
        None => {
            if dark {
                layers.push(layer("stars", vec![("count", Value::Integer(110))]));
                layers.push(layer("moon", vec![("x", Value::Float(0.84)), ("y", Value::Float(0.14)), ("color", Value::String(hex(light)))]));
            } else {
                layers.push(layer("sun", vec![("x", Value::Float(0.84)), ("y", Value::Float(0.1)), ("color", Value::String(hex(light)))]));
                layers.push(layer("rays", vec![("strength", Value::Integer(if vivid { 55 } else { 40 }))]));
            }
            layers.push(layer("hills", vec![("back", stops_value(&hills(hv * 0.9))), ("front", stops_value(&hills(hv)))]));
        }
    }
    layers.push(layer("bubbles", vec![("count", Value::Integer(if dark { 12 } else { 18 }))]));
    d.set_layers(layers);
    d.text().replacen("made in the Theme Studio", "made from a picture", 1)
}

/// The three themes for a picture, with names that aren't taken yet.
/// `picture` is the background picture's file setting, if it's used.
pub fn themes(a: &Analysis, taken: &[String], picture: Option<&str>) -> Vec<(Variant, String, Theme)> {
    let stem = a.file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let mut stem: String = stem.replace(['_', '-', '.'], " ").split_whitespace().collect::<Vec<_>>().join(" ");
    // Wallpapers often end with a credit ("Fuji san by amaral"): the name is enough.
    if let Some(i) = stem.to_lowercase().find(" by ")
        && i > 0
    {
        stem.truncate(i);
    }
    // At most about 24 characters, cut between words.
    while stem.chars().count() > 24 && stem.contains(' ') {
        stem.truncate(stem.rfind(' ').unwrap_or(stem.len()));
    }
    stem = stem.chars().take(24).collect();
    if let Some(first) = stem.chars().next() {
        stem = first.to_uppercase().chain(stem.chars().skip(1)).collect();
    }
    if stem.is_empty() {
        stem = "Picture".into();
    }
    let known = theme::load_library(Path::new("/nonexistent")).resolved;
    Variant::ALL
        .into_iter()
        .filter_map(|v| {
            let base = format!("{stem} {}", v.label());
            let mut name = base.clone();
            let mut n = 2;
            while taken.contains(&name) {
                name = format!("{base} {n}");
                n += 1;
            }
            let text = build(a, v, &name, picture);
            let parsed = theme::parse(&text, &known, None, false).ok()?;
            Some((v, text, parsed.theme))
        })
        .collect()
}

/// A copy of the picture for the themes folder: at most 1920×1200, as a JPEG
/// (so it stays small enough to share). Returns its file name.
pub fn copy_picture(src: &Path, dir: &Path, name: &str) -> Result<String, String> {
    let img = image::open(src).map_err(|e| e.to_string())?;
    let img = if img.width() > 1920 || img.height() > 1200 { img.resize(1920, 1200, image::imageops::FilterType::Triangle) } else { img };
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let stem = theme::file_name(name).trim_end_matches(".aerotheme").to_string();
    let mut file = format!("{stem}.jpg");
    let mut n = 2;
    while dir.join(&file).exists() {
        file = format!("{stem}-{n}.jpg");
        n += 1;
    }
    let out = std::fs::File::create(dir.join(&file)).map_err(|e| e.to_string())?;
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(std::io::BufWriter::new(out), 88);
    enc.encode_image(&img.to_rgb8()).map_err(|e| e.to_string())?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme_edit::Rng;

    fn picture(dir: &Path, name: &str, f: impl Fn(u32, u32) -> [u8; 3]) -> PathBuf {
        let img = image::RgbImage::from_fn(160, 120, |x, y| image::Rgb(f(x, y)));
        let p = dir.join(name);
        img.save(&p).unwrap();
        p
    }

    #[test]
    fn finds_the_main_colors() {
        let dir = std::env::temp_dir().join(format!("aero-picture-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // A sunset: orange sky over a purple sea, a third of it sea.
        let p = picture(&dir, "sunset_beach.png", |_, y| if y < 80 { [250, 140, 40] } else { [70, 30, 110] });
        let a = analyze(&p).unwrap();
        let (c, w) = a.palette[0];
        assert!(c.r() > 230 && (0.6..0.72).contains(&w), "{:?}", a.palette);
        assert!(a.palette.iter().any(|(c, _)| c.b() > 100 && c.r() < 90));
        assert!(a.bands[0].r() > 230 && a.bands[3].b() > 100, "bands go from the sky to the sea");
        let (h1, h2, _) = key_hues(&a);
        assert!(hue_gap(h1, 30.0) < 10.0 && hue_gap(h2, 270.0) < 15.0, "{h1} {h2}");
        // Names come from the file.
        let a2 = Analysis { file: dir.join("Monument_valley_by_orbitelambda.jpg"), ..a.clone() };
        assert_eq!(themes(&a2, &[], None)[0].2.name, "Monument valley Light", "credits are left out");
        let a3 = Analysis { file: dir.join("a-very-long-name-for-a-holiday-photo-from-2024.jpg"), ..a.clone() };
        assert_eq!(themes(&a3, &[], None)[0].2.name, "A very long name for a Light");
        let made = themes(&a, &["Sunset beach Light".into()], None);
        let names: Vec<&str> = made.iter().map(|(_, _, t)| t.name.as_str()).collect();
        assert_eq!(names, ["Sunset beach Light 2", "Sunset beach Dark", "Sunset beach Vivid"]);
        assert!(made[1].2.dark && !made[0].2.dark);
        // Not a picture.
        std::fs::write(dir.join("notes.txt"), "hi").unwrap();
        assert!(analyze(&dir.join("notes.txt")).is_err());
        std::fs::write(dir.join("broken.png"), "not really").unwrap();
        assert!(analyze(&dir.join("broken.png")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Whatever the picture, all three themes are valid and readable.
    #[test]
    fn every_picture_makes_readable_themes() {
        let dir = std::env::temp_dir().join(format!("aero-picture-many-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut rng = Rng::new(7);
        let mut pictures = vec![
            picture(&dir, "black.png", |_, _| [0, 0, 0]),
            picture(&dir, "white.png", |_, _| [255, 255, 255]),
            picture(&dir, "gray.png", |_, _| [128, 128, 128]),
            picture(&dir, "yellow.png", |_, _| [255, 255, 0]),
        ];
        for i in 0..30 {
            let cols: Vec<[u8; 3]> = (0..4).map(|_| [(rng.next() * 255.0) as u8, (rng.next() * 255.0) as u8, (rng.next() * 255.0) as u8]).collect();
            pictures.push(picture(&dir, &format!("random-{i}.png"), move |x, y| cols[((x / 40 + y / 30) % 4) as usize]));
        }
        for p in &pictures {
            let a = analyze(p).unwrap();
            let made = themes(&a, &[], None);
            assert_eq!(made.len(), 3, "{}", p.display());
            for (v, text, t) in &made {
                assert!(theme::readability(t).is_empty(), "{} {v:?}: {:?}\n{text}", p.display(), theme::readability(t));
            }
            // With the picture as the background.
            let with = themes(&a, &[], Some(&p.to_string_lossy()));
            assert!(with.iter().all(|(_, _, t)| t.scene.iter().any(|l| matches!(l.kind, theme::Layer::Picture { .. }))));
            let dimmed = |t: &Theme| t.scene.iter().find(|l| matches!(l.kind, theme::Layer::Picture { .. })).map(|l| l.opacity);
            assert_eq!(with.iter().map(|(_, _, t)| dimmed(t)).collect::<Vec<_>>(), [Some(1.0), Some(0.35), Some(1.0)], "dimmed by night");
        }
        // The copy for the themes folder is a smaller JPEG.
        let big = image::RgbImage::from_fn(3000, 2000, |x, _| image::Rgb([(x % 255) as u8, 90, 160]));
        big.save(dir.join("big.png")).unwrap();
        let file = copy_picture(&dir.join("big.png"), &dir.join("themes"), "Big Light").unwrap();
        assert_eq!(file, "big-light.jpg");
        let copy = image::open(dir.join("themes").join(&file)).unwrap();
        assert!(copy.width() <= 1920 && copy.height() <= 1200);
        assert_eq!(copy_picture(&dir.join("big.png"), &dir.join("themes"), "Big Light").unwrap(), "big-light-2.jpg", "never overwrites");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
