//! Seasonal and live touches, all optional: a season's particles (snow,
//! petals, summer-night fireflies, falling leaves), a sun that follows the
//! real time of day, and a few small touches on fun dates. They're added to
//! a copy of the theme when it's drawn; theme files never change.

use crate::daynight::{self, Now, Sun};
use crate::theme::{Layer, SceneLayer, Theme};
use eframe::egui::{Color32, Vec2, vec2};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Season {
    Winter,
    Spring,
    Summer,
    Autumn,
}

impl Season {
    pub fn label(self) -> &'static str {
        match self {
            Season::Winter => "winter snow",
            Season::Spring => "spring petals",
            Season::Summer => "summer fireflies (at night)",
            Season::Autumn => "autumn leaves",
        }
    }
}

/// The season for a month; the south of the world has them the other way round.
pub fn season(month: u32, south: bool) -> Season {
    let north = match month {
        12 | 1 | 2 => Season::Winter,
        3..=5 => Season::Spring,
        6..=8 => Season::Summer,
        _ => Season::Autumn,
    };
    if !south {
        return north;
    }
    match north {
        Season::Winter => Season::Summer,
        Season::Spring => Season::Autumn,
        Season::Summer => Season::Winter,
        Season::Autumn => Season::Spring,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FunDate {
    NewYear,
    Valentine,
    Halloween,
    Christmas,
}

impl FunDate {
    pub fn label(self) -> &'static str {
        match self {
            FunDate::NewYear => "🎉 Happy New Year!",
            FunDate::Valentine => "💕 Happy Valentine's Day!",
            FunDate::Halloween => "🎃 Happy Halloween!",
            FunDate::Christmas => "🎄 Merry Christmas!",
        }
    }
}

pub fn fun_date(month: u32, mday: u32) -> Option<FunDate> {
    match (month, mday) {
        (1, 1) => Some(FunDate::NewYear),
        (2, 14) => Some(FunDate::Valentine),
        (10, 31) => Some(FunDate::Halloween),
        (12, 24..=26) => Some(FunDate::Christmas),
        _ => None,
    }
}

/// Where the day is: how far through the daylight (0 at sunrise, 1 at
/// sunset) or through the night, and how close to sunrise or sunset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyTime {
    pub day: Option<f32>,
    pub night: Option<f32>,
    /// 1 at sunrise and sunset, fading to 0 an hour away.
    pub golden: f32,
}

pub fn sky_time(minute: f64, sun: Sun) -> SkyTime {
    match sun {
        Sun::AlwaysUp => SkyTime { day: Some(((minute / 1440.0) as f32).clamp(0.0, 1.0)), night: None, golden: 0.0 },
        Sun::AlwaysDown => SkyTime { day: None, night: Some(((minute / 1440.0) as f32).clamp(0.0, 1.0)), golden: 0.0 },
        Sun::RiseSet(rise, set) => {
            let (rise, set) = (rise.rem_euclid(1440.0), set.rem_euclid(1440.0));
            let m = minute.rem_euclid(1440.0);
            let since = |from: f64| (m - from).rem_euclid(1440.0);
            let day_len = (set - rise).rem_euclid(1440.0);
            let gap = |a: f64| {
                let d = (m - a).abs();
                d.min(1440.0 - d)
            };
            let golden = (1.0 - gap(rise).min(gap(set)) / 60.0).max(0.0) as f32;
            if since(rise) < day_len {
                SkyTime { day: Some((since(rise) / day_len.max(1.0)) as f32), night: None, golden }
            } else {
                SkyTime { day: None, night: Some((since(set) / (1440.0 - day_len).max(1.0)) as f32), golden }
            }
        }
    }
}

/// What's switched on, worked out for now.
#[derive(Clone, Debug, PartialEq)]
pub struct Touches {
    pub season: Option<Season>,
    pub sky: Option<SkyTime>,
    pub date: Option<FunDate>,
    /// Night by the sun (summer fireflies only come out at night).
    pub night: bool,
}

pub fn compute(seasons: bool, live_sky: bool, fun_dates: bool, now: Now, coords: (f64, f64)) -> Option<Touches> {
    if !(seasons || live_sky || fun_dates) {
        return None;
    }
    let sky = sky_time(now.minutes as f64, daynight::sun(now.day, coords.0, coords.1, now.offset));
    Some(Touches {
        season: seasons.then(|| season(now.month, coords.0 < 0.0)),
        sky: live_sky.then_some(sky),
        date: if fun_dates { fun_date(now.month, now.mday) } else { None },
        night: sky.day.is_none(),
    })
}

/// The sun's (or moon's) path across the sky, from 0 (rising) to 1
/// (setting). It stays near the top, where it shows around the window.
pub fn arc(f: f32) -> Vec2 {
    let f = f.clamp(0.0, 1.0);
    vec2(0.06 + 0.88 * f, 0.2 - 0.15 * (std::f32::consts::PI * f).sin())
}

fn particles(kind: &str, color: Color32, count: u32) -> SceneLayer {
    let kind = match kind {
        "snow" => Layer::Snow { color, count },
        "petals" => Layer::Petals { color, count },
        "leaves" => Layer::Leaves { color, count },
        "fireflies" => Layer::Fireflies { color, count },
        _ => Layer::Bubbles { color, count },
    };
    SceneLayer { opacity: 0.85, ..SceneLayer::new(kind) }
}

/// The theme with today's touches.
pub fn apply(theme: &Theme, t: &Touches) -> Theme {
    let mut out = theme.clone();
    if let Some(sky) = t.sky {
        let warm = Color32::from_rgb(255, 160, 80);
        let mut sun_at = None;
        for l in &mut out.scene {
            match &mut l.kind {
                Layer::Sun { pos, color } => {
                    match sky.day {
                        Some(f) => *pos = arc(f),
                        // Set: behind the hills until morning.
                        None => pos.y = 1.2,
                    }
                    *color = crate::aero::lerp(*color, warm, 0.45 * sky.golden);
                    sun_at = Some(*pos);
                }
                Layer::Moon { pos, .. } => {
                    if let Some(f) = sky.night {
                        *pos = arc(f);
                    }
                }
                Layer::Sky { stops } => {
                    // A glow along the horizon around sunrise and sunset (the top stays blue:
                    // mixing blue and orange all the way up just looks gray).
                    for (p, c) in stops.iter_mut() {
                        *c = crate::aero::lerp(*c, warm, 0.7 * sky.golden * p.powf(1.3));
                    }
                }
                _ => {}
            }
        }
        // The light follows the sun, and is gone when it's set.
        for l in &mut out.scene {
            if let (Layer::Rays { pos, .. }, Some(at)) = (&mut l.kind, sun_at) {
                *pos = at;
                l.opacity *= match sky.day {
                    Some(f) => 0.35 + 0.65 * (std::f32::consts::PI * f).sin(),
                    None => 0.0,
                };
            }
        }
    }
    let has = |out: &Theme, name: &str| out.scene.iter().any(|l| l.enabled && l.kind.type_name() == name);
    let mut extra: Vec<SceneLayer> = vec![];
    match t.date {
        Some(FunDate::NewYear) => extra.push(particles("bubbles", Color32::from_rgb(255, 215, 106), 26)),
        Some(FunDate::Valentine) => extra.push(particles("petals", Color32::from_rgb(255, 111, 145), 40)),
        Some(FunDate::Halloween) => {
            extra.push(particles("leaves", Color32::from_rgb(255, 122, 26), 30));
            extra.push(particles("fireflies", Color32::from_rgb(255, 179, 71), 20));
        }
        Some(FunDate::Christmas) => extra.push(particles("snow", Color32::from_rgb(250, 252, 255), 90)),
        None => {}
    }
    let season = match t.season {
        Some(Season::Winter) => Some(("snow", Color32::from_rgb(240, 248, 255), 60)),
        Some(Season::Spring) => Some(("petals", Color32::from_rgb(255, 192, 216), 32)),
        Some(Season::Summer) if t.night => Some(("fireflies", Color32::from_rgb(255, 245, 154), 32)),
        Some(Season::Summer) => None,
        Some(Season::Autumn) => Some(("leaves", Color32::from_rgb(232, 137, 43), 26)),
        None => None,
    };
    // A season's particles, unless the theme or the date already has that kind.
    if let Some((kind, color, count)) = season
        && !has(&out, kind)
        && !extra.iter().any(|l| l.kind.type_name() == kind)
    {
        extra.push(particles(kind, color, count));
    }
    out.scene.extend(extra);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme(name: &str) -> Theme {
        crate::theme::load_library(std::path::Path::new("/nonexistent")).themes.into_iter().find(|t| t.name == name).unwrap()
    }

    #[test]
    fn seasons_and_hemispheres() {
        assert_eq!(season(1, false), Season::Winter);
        assert_eq!(season(4, false), Season::Spring);
        assert_eq!(season(7, false), Season::Summer);
        assert_eq!(season(10, false), Season::Autumn);
        assert_eq!(season(12, true), Season::Summer, "December in Sydney");
        assert_eq!(season(4, true), Season::Autumn);
        assert_eq!(fun_date(10, 31), Some(FunDate::Halloween));
        assert_eq!(fun_date(12, 25), Some(FunDate::Christmas));
        assert_eq!(fun_date(7, 14), None);
    }

    #[test]
    fn the_sun_crosses_the_sky() {
        let sun = Sun::RiseSet(420.0, 1200.0); // 07:00 to 20:00
        let noon = sky_time(810.0, sun);
        assert!((noon.day.unwrap() - 0.5).abs() < 0.01 && noon.golden == 0.0);
        let p = arc(noon.day.unwrap());
        assert!((p.x - 0.5).abs() < 0.01 && p.y < 0.11, "high at midday: {p:?}");
        let dawn = sky_time(425.0, sun);
        assert!(dawn.day.unwrap() < 0.01 && dawn.golden > 0.9);
        assert!(arc(0.0).y > 0.15 && arc(1.0).x > 0.9);
        let midnight = sky_time(0.0, sun);
        assert!(midnight.day.is_none() && midnight.night.is_some() && midnight.golden == 0.0);
        // A day that crosses midnight in local time still works.
        let late = sky_time(30.0, Sun::RiseSet(600.0, 1500.0));
        assert!(late.day.is_some());
    }

    #[test]
    fn touches_change_a_copy() {
        let aero = theme("Frutiger Aero");
        let off = compute(false, false, false, daynight::now(), (48.9, 2.3));
        assert!(off.is_none(), "all off: nothing to do");
        let winter_night = Touches { season: Some(Season::Winter), sky: Some(sky_time(0.0, Sun::RiseSet(480.0, 1020.0))), date: None, night: true };
        let t = apply(&aero, &winter_night);
        assert_eq!(t.scene.len(), aero.scene.len() + 1);
        assert!(matches!(t.scene.last().unwrap().kind, Layer::Snow { .. }));
        let sun = t.scene.iter().find_map(|l| if let Layer::Sun { pos, .. } = l.kind { Some(pos) } else { None }).unwrap();
        assert!(sun.y > 1.0, "the sun has set");
        assert!(t.scene.iter().any(|l| matches!(l.kind, Layer::Rays { .. }) && l.opacity == 0.0), "and its light with it");
        // Winter Glass already has snow: no second layer.
        let glass = theme("Winter Glass");
        assert_eq!(apply(&glass, &winter_night).scene.len(), glass.scene.len());
        // Summer fireflies come out only at night.
        let summer_day = Touches { season: Some(Season::Summer), sky: None, date: None, night: false };
        assert_eq!(apply(&aero, &summer_day).scene.len(), aero.scene.len());
        let summer_night = Touches { night: true, ..summer_day };
        assert_eq!(apply(&aero, &summer_night).scene.len(), aero.scene.len() + 1);
        // On Christmas, the date's snow wins over the season's.
        let xmas = Touches { date: Some(FunDate::Christmas), ..winter_night };
        let x = apply(&aero, &xmas);
        assert_eq!(x.scene.iter().filter(|l| matches!(l.kind, Layer::Snow { .. })).count(), 1);
        assert!(matches!(x.scene.last().unwrap().kind, Layer::Snow { count: 90, .. }));
        // Golden hour warms the sky.
        let dusk = Touches { season: None, sky: Some(sky_time(1020.0, Sun::RiseSet(480.0, 1020.0))), date: None, night: false };
        let d = apply(&aero, &dusk);
        let red = |t: &Theme| t.scene.iter().find_map(|l| if let Layer::Sky { stops } = &l.kind { Some(stops.last().unwrap().1.r()) } else { None }).unwrap();
        let blue = |t: &Theme| t.scene.iter().find_map(|l| if let Layer::Sky { stops } = &l.kind { Some(stops.last().unwrap().1.b()) } else { None }).unwrap();
        assert!(red(&d) >= red(&aero) && blue(&d) < blue(&aero));
    }
}
