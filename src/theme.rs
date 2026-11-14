//! The five built-in themes, plus loading and saving the user's choice.

use eframe::egui::{Color32, Vec2};
use std::path::PathBuf;

pub type Stops = [(f32, Color32); 4];

pub struct Theme {
    pub name: &'static str,
    pub desc: &'static str,
    pub sky: Stops,
    /// Sun (or moon) position as a fraction of the window size.
    pub sun: Vec2,
    pub sun_color: Color32,
    /// Opacity of the light rays coming from the sun (0 = none).
    pub rays: u8,
    pub hill_back: Stops,
    pub hill_front: Stops,
    /// Night sky: stars, a moon instead of a sun, and an aurora.
    pub night: bool,
    pub bubble: Color32,
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
}

const fn c(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub fn all() -> Vec<Theme> {
    vec![
        Theme {
            name: "Frutiger Aero",
            desc: "Blue skies, green hills, bubbles",
            sky: [(0.0, c(10, 88, 180)), (0.4, c(64, 170, 238)), (0.72, c(190, 236, 255)), (1.0, c(235, 250, 255))],
            sun: Vec2::new(0.86, 0.1),
            sun_color: c(255, 255, 240),
            rays: 45,
            hill_back: [(0.0, c(150, 225, 110)), (0.12, c(90, 185, 60)), (0.3, c(40, 130, 30)), (1.0, c(30, 100, 20))],
            hill_front: [(0.0, c(185, 245, 120)), (0.1, c(110, 205, 60)), (0.3, c(50, 150, 30)), (1.0, c(30, 110, 20))],
            night: false,
            bubble: c(220, 245, 255),
            dark: false,
            glass: c(225, 245, 255),
            text: c(12, 60, 110),
            soft: c(50, 95, 140),
            accent: c(30, 130, 225),
            go: c(50, 175, 30),
            user_orb: c(30, 140, 230),
            root_orb: c(255, 150, 30),
            logo: c(40, 175, 90),
        },
        Theme {
            name: "Aqua Abyss",
            desc: "Underwater light and sandy dunes",
            sky: [(0.0, c(130, 235, 245)), (0.35, c(30, 170, 200)), (0.75, c(10, 95, 140)), (1.0, c(5, 50, 95))],
            sun: Vec2::new(0.5, -0.2),
            sun_color: c(220, 255, 255),
            rays: 60,
            hill_back: [(0.0, c(230, 215, 165)), (0.12, c(200, 180, 125)), (0.3, c(165, 140, 90)), (1.0, c(130, 105, 65))],
            hill_front: [(0.0, c(250, 235, 185)), (0.1, c(225, 200, 140)), (0.3, c(185, 155, 100)), (1.0, c(150, 120, 70))],
            night: false,
            bubble: c(200, 255, 255),
            dark: false,
            glass: c(215, 250, 250),
            text: c(5, 60, 80),
            soft: c(30, 100, 120),
            accent: c(20, 160, 190),
            go: c(0, 185, 145),
            user_orb: c(20, 170, 210),
            root_orb: c(255, 120, 90),
            logo: c(0, 180, 200),
        },
        Theme {
            name: "Sunset Vista",
            desc: "Warm sky over purple mountains",
            sky: [(0.0, c(60, 40, 120)), (0.35, c(200, 80, 130)), (0.7, c(255, 160, 80)), (1.0, c(255, 225, 150))],
            sun: Vec2::new(0.75, 0.6),
            sun_color: c(255, 240, 190),
            rays: 30,
            hill_back: [(0.0, c(175, 105, 150)), (0.12, c(140, 80, 130)), (0.3, c(105, 55, 105)), (1.0, c(80, 40, 85))],
            hill_front: [(0.0, c(130, 70, 120)), (0.1, c(100, 50, 100)), (0.3, c(75, 35, 80)), (1.0, c(55, 25, 60))],
            night: false,
            bubble: c(255, 225, 205),
            dark: false,
            glass: c(255, 235, 225),
            text: c(90, 30, 60),
            soft: c(140, 70, 95),
            accent: c(235, 105, 55),
            go: c(220, 60, 125),
            user_orb: c(240, 125, 60),
            root_orb: c(170, 75, 200),
            logo: c(240, 90, 90),
        },
        Theme {
            name: "Aurora Night",
            desc: "Smoked glass and northern lights",
            sky: [(0.0, c(4, 8, 28)), (0.4, c(10, 28, 68)), (0.75, c(20, 58, 108)), (1.0, c(40, 90, 140))],
            sun: Vec2::new(0.82, 0.13),
            sun_color: c(225, 238, 255),
            rays: 0,
            hill_back: [(0.0, c(40, 95, 105)), (0.12, c(25, 70, 80)), (0.3, c(15, 45, 55)), (1.0, c(8, 28, 36))],
            hill_front: [(0.0, c(30, 75, 80)), (0.1, c(18, 52, 60)), (0.3, c(10, 32, 40)), (1.0, c(5, 20, 26))],
            night: true,
            bubble: c(150, 220, 255),
            dark: true,
            glass: c(20, 40, 80),
            text: c(232, 245, 255),
            soft: c(160, 195, 228),
            accent: c(40, 120, 230),
            go: c(30, 185, 130),
            user_orb: c(50, 130, 240),
            root_orb: c(230, 120, 40),
            logo: c(60, 200, 200),
        },
        Theme {
            name: "Sakura Spring",
            desc: "Pastel skies and cherry blossoms",
            sky: [(0.0, c(150, 170, 240)), (0.4, c(210, 190, 245)), (0.75, c(255, 215, 235)), (1.0, c(255, 240, 246))],
            sun: Vec2::new(0.14, 0.12),
            sun_color: c(255, 250, 240),
            rays: 35,
            hill_back: [(0.0, c(195, 238, 155)), (0.12, c(155, 212, 115)), (0.3, c(115, 182, 85)), (1.0, c(85, 150, 60))],
            hill_front: [(0.0, c(255, 205, 225)), (0.1, c(240, 170, 200)), (0.3, c(215, 135, 175)), (1.0, c(185, 105, 150))],
            night: false,
            bubble: c(255, 205, 228),
            dark: false,
            glass: c(255, 236, 246),
            text: c(100, 30, 70),
            soft: c(150, 80, 115),
            accent: c(230, 110, 170),
            go: c(95, 185, 75),
            user_orb: c(240, 120, 180),
            root_orb: c(160, 110, 230),
            logo: c(240, 130, 180),
        },
    ]
}

fn config_file() -> Option<PathBuf> {
    Some(crate::platform::config_dir().join("theme"))
}

/// Index of the saved theme, or 0 (Frutiger Aero).
pub fn load(themes: &[Theme]) -> usize {
    config_file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|name| themes.iter().position(|t| t.name == name.trim()))
        .unwrap_or(0)
}

pub fn save(theme: &Theme) {
    if let Some(f) = config_file() {
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(f, theme.name);
    }
}
