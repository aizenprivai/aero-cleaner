//! Day and night: a day theme and a night theme, switched by the system's
//! dark mode, a schedule, or sunrise and sunset. Sunrise and sunset are
//! worked out from the time zone alone (its main city's coordinates, built
//! in), so no location and no internet are needed.

use crate::theme::{Material, SceneLayer, Theme};
use chrono::{Datelike, Local, Offset, Timelike};
use eframe::egui::Color32;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Off,
    /// Follows the system's dark mode.
    System,
    /// Night between two times of day.
    Schedule,
    /// Night between sunset and sunrise.
    Sun,
}

impl Mode {
    pub const ALL: [Mode; 4] = [Mode::Off, Mode::System, Mode::Schedule, Mode::Sun];

    pub fn code(self) -> &'static str {
        match self {
            Mode::Off => "off",
            Mode::System => "system",
            Mode::Schedule => "schedule",
            Mode::Sun => "sun",
        }
    }

    pub fn from_code(s: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|m| m.code() == s.trim())
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Off => "Off",
            Mode::System => "With my system's dark mode",
            Mode::Schedule => "On a schedule",
            Mode::Sun => "At sunrise and sunset",
        }
    }
}

/// The local time: minutes since midnight, day of the year (1 = Jan 1),
/// and the offset from UTC in minutes.
#[derive(Clone, Copy, Debug)]
pub struct Now {
    pub minutes: u32,
    pub day: u32,
    pub offset: i32,
}

pub fn now() -> Now {
    let t = Local::now();
    Now { minutes: t.hour() * 60 + t.minute(), day: t.ordinal(), offset: t.offset().fix().local_minus_utc() / 60 }
}

const ZONES: &str = include_str!("zones.txt");

/// The system's time zone name ("Europe/Paris").
pub fn zone_name() -> Option<String> {
    iana_time_zone::get_timezone().ok()
}

/// Latitude and longitude for a time zone. Unknown zones get a longitude
/// from the UTC offset and a mid-northern latitude.
pub fn coords(zone: Option<&str>, offset_minutes: i32) -> (f64, f64) {
    zone.and_then(|z| {
        ZONES.lines().filter(|l| !l.starts_with('#')).find_map(|l| {
            let mut it = l.split(' ');
            (it.next()? == z).then_some(())?;
            Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
        })
    })
    .unwrap_or((45.0, offset_minutes as f64 / 4.0))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sun {
    /// Sunrise and sunset in local minutes since midnight (may wrap past 0 or 1440).
    RiseSet(f64, f64),
    /// The sun doesn't set (summer near the poles).
    AlwaysUp,
    /// The sun doesn't rise.
    AlwaysDown,
}

/// Sunrise and sunset (NOAA's approximation, within a few minutes).
pub fn sun(day: u32, lat: f64, lon: f64, offset_minutes: i32) -> Sun {
    let g = 2.0 * std::f64::consts::PI / 365.0 * (day as f64 - 1.0);
    let eqtime = 229.18 * (0.000075 + 0.001868 * g.cos() - 0.032077 * g.sin() - 0.014615 * (2.0 * g).cos() - 0.040849 * (2.0 * g).sin());
    let decl =
        0.006918 - 0.399912 * g.cos() + 0.070257 * g.sin() - 0.006758 * (2.0 * g).cos() + 0.000907 * (2.0 * g).sin() - 0.002697 * (3.0 * g).cos() + 0.00148 * (3.0 * g).sin();
    let lat = lat.to_radians();
    let cos_ha = 90.833f64.to_radians().cos() / (lat.cos() * decl.cos()) - lat.tan() * decl.tan();
    if cos_ha > 1.0 {
        return Sun::AlwaysDown;
    }
    if cos_ha < -1.0 {
        return Sun::AlwaysUp;
    }
    let ha = cos_ha.acos().to_degrees();
    let off = offset_minutes as f64;
    Sun::RiseSet(720.0 - 4.0 * (lon + ha) - eqtime + off, 720.0 - 4.0 * (lon - ha) - eqtime + off)
}

/// Whether `minute` is in the night that runs from `night` to `day`
/// (minutes since midnight, wrapping around midnight).
pub fn in_night(minute: f64, day: f64, night: f64) -> bool {
    let (m, d, n) = (minute.rem_euclid(1440.0), day.rem_euclid(1440.0), night.rem_euclid(1440.0));
    if d == n {
        return false;
    }
    if n > d { m >= n || m < d } else { m >= n && m < d }
}

/// Is it night now, for this mode? None when it can't tell (the system
/// doesn't say whether it's in dark mode) or the mode is off.
pub fn is_night(mode: Mode, now: Now, schedule: (u32, u32), system_dark: Option<bool>, coords: (f64, f64)) -> Option<bool> {
    match mode {
        Mode::Off => None,
        Mode::System => system_dark,
        Mode::Schedule => Some(in_night(now.minutes as f64, schedule.0 as f64, schedule.1 as f64)),
        Mode::Sun => Some(match sun(now.day, coords.0, coords.1, now.offset) {
            Sun::RiseSet(rise, set) => in_night(now.minutes as f64, rise, set),
            Sun::AlwaysUp => false,
            Sun::AlwaysDown => true,
        }),
    }
}

/// "07:30".
pub fn clock(minutes: f64) -> String {
    let m = minutes.rem_euclid(1440.0).round() as u32 % 1440;
    format!("{:02}:{:02}", m / 60, m % 60)
}

/// Whether the system is in dark mode, on Linux, from the desktop's
/// settings portal (GNOME, KDE and others). Asked in the background at
/// most every few seconds; None until it answers or if there's no portal.
#[cfg(target_os = "linux")]
pub fn linux_dark() -> Option<bool> {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    static STATE: Mutex<(Option<Instant>, Option<bool>)> = Mutex::new((None, None));
    let mut s = STATE.lock().ok()?;
    if s.0.is_none_or(|t| t.elapsed() > Duration::from_secs(5)) {
        s.0 = Some(Instant::now());
        std::thread::spawn(|| {
            let v = portal_color_scheme();
            if let Ok(mut s) = STATE.lock() {
                s.1 = v;
            }
        });
    }
    s.1
}

#[cfg(target_os = "linux")]
fn portal_color_scheme() -> Option<bool> {
    use zbus::zvariant::{OwnedValue, Value};
    let conn = zbus::blocking::Connection::session().ok()?;
    let proxy = zbus::blocking::Proxy::new(&conn, "org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop", "org.freedesktop.portal.Settings").ok()?;
    let v: OwnedValue = proxy.call("Read", &("org.freedesktop.appearance", "color-scheme")).ok()?;
    // Read returns the value wrapped in one more variant.
    let mut v: Value = v.into();
    while let Value::Value(inner) = v {
        v = *inner;
    }
    match v {
        Value::U32(1) => Some(true),
        Value::U32(_) => Some(false),
        _ => None,
    }
}

fn lerp(a: f32, b: f32, k: f32) -> f32 {
    a + (b - a) * k
}

fn mix(a: Color32, b: Color32, k: f32) -> Color32 {
    let m = |x: u8, y: u8| lerp(x as f32, y as f32, k).round() as u8;
    Color32::from_rgba_premultiplied(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()), m(a.a(), b.a()))
}

/// A theme on its way from `a` to `b` (`k` from 0 to 1): colors and
/// material blend, and `b`'s scene fades in over `a`'s.
pub fn blend(a: &Theme, b: &Theme, k: f32) -> Theme {
    let k = k.clamp(0.0, 1.0);
    let mut t = b.clone();
    t.dark = if k < 0.5 { a.dark } else { b.dark };
    for (out, x, y) in [
        (&mut t.glass, a.glass, b.glass),
        (&mut t.text, a.text, b.text),
        (&mut t.soft, a.soft, b.soft),
        (&mut t.accent, a.accent, b.accent),
        (&mut t.go, a.go, b.go),
        (&mut t.user_orb, a.user_orb, b.user_orb),
        (&mut t.root_orb, a.root_orb, b.root_orb),
        (&mut t.logo, a.logo, b.logo),
        (&mut t.bubble, a.bubble, b.bubble),
    ] {
        *out = mix(x, y, k);
    }
    let (ma, mb) = (a.material, b.material);
    t.material = Material {
        corners: lerp(ma.corners, mb.corners, k),
        gloss: lerp(ma.gloss, mb.gloss, k),
        glass: lerp(ma.glass, mb.glass, k),
        buttons: if k < 0.5 { ma.buttons } else { mb.buttons },
        font_size: lerp(ma.font_size, mb.font_size, k),
    };
    if k < 0.5 {
        t.font = a.font;
    }
    t.motion_speed = lerp(a.motion_speed, b.motion_speed, k);
    t.scene = a.scene.iter().cloned().chain(b.scene.iter().map(|l| SceneLayer { opacity: l.opacity * k, ..l.clone() })).collect();
    t
}

/// How long a crossfade takes, in seconds.
pub const FADE_SECONDS: f64 = 1.8;

/// Eased progress of a crossfade that started `elapsed` seconds ago.
pub fn fade_progress(elapsed: f64) -> f32 {
    let x = (elapsed / FADE_SECONDS).clamp(0.0, 1.0) as f32;
    x * x * (3.0 - 2.0 * x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn night_wraps_around_midnight() {
        // Night from 20:00 to 07:00.
        let (day, night) = (7.0 * 60.0, 20.0 * 60.0);
        assert!(in_night(23.0 * 60.0, day, night));
        assert!(in_night(3.0 * 60.0, day, night));
        assert!(!in_night(12.0 * 60.0, day, night));
        assert!(in_night(20.0 * 60.0, day, night), "night starts on the minute");
        assert!(!in_night(7.0 * 60.0, day, night), "day starts on the minute");
        // An odd schedule: "night" in the afternoon.
        assert!(in_night(14.0 * 60.0, 16.0 * 60.0, 13.0 * 60.0));
        assert!(!in_night(10.0 * 60.0, 16.0 * 60.0, 13.0 * 60.0));
        assert!(!in_night(10.0, 600.0, 600.0), "the same time for both means always day");
    }

    #[test]
    fn sunrise_and_sunset_are_about_right() {
        let (lat, lon) = coords(Some("Europe/Paris"), 60);
        assert!((lat - 48.9).abs() < 0.2 && (lon - 2.3).abs() < 0.2);
        // Paris, June 21 (UTC+2): about 05:47 and 21:58.
        let Sun::RiseSet(rise, set) = sun(172, lat, lon, 120) else { panic!() };
        assert!((rise - (5.0 * 60.0 + 47.0)).abs() < 10.0, "{}", clock(rise));
        assert!((set - (21.0 * 60.0 + 58.0)).abs() < 10.0, "{}", clock(set));
        // Paris, December 21 (UTC+1): about 08:42 and 16:56.
        let Sun::RiseSet(rise, set) = sun(355, lat, lon, 60) else { panic!() };
        assert!((rise - (8.0 * 60.0 + 42.0)).abs() < 10.0, "{}", clock(rise));
        assert!((set - (16.0 * 60.0 + 56.0)).abs() < 10.0, "{}", clock(set));
        // Sydney, December 21 (UTC+11): about 05:41 and 20:05.
        let (lat, lon) = coords(Some("Australia/Sydney"), 660);
        let Sun::RiseSet(rise, set) = sun(355, lat, lon, 660) else { panic!() };
        assert!((rise - (5.0 * 60.0 + 41.0)).abs() < 10.0, "{}", clock(rise));
        assert!((set - (20.0 * 60.0 + 5.0)).abs() < 10.0, "{}", clock(set));
        // Near the poles.
        assert_eq!(sun(172, 78.2, 15.6, 120), Sun::AlwaysUp);
        assert_eq!(sun(355, 78.2, 15.6, 60), Sun::AlwaysDown);
    }

    #[test]
    fn unknown_zones_still_get_a_sky() {
        let (lat, lon) = coords(Some("Nowhere/Atlantis"), -300);
        assert_eq!((lat, lon), (45.0, -75.0));
        assert!(matches!(sun(80, lat, lon, -300), Sun::RiseSet(..)));
        // Every zone in the table parses.
        for l in ZONES.lines().filter(|l| !l.starts_with('#')) {
            let p: Vec<&str> = l.split(' ').collect();
            assert_eq!(p.len(), 3, "{l}");
            assert!(p[1].parse::<f64>().is_ok() && p[2].parse::<f64>().is_ok(), "{l}");
        }
    }

    #[test]
    fn modes() {
        let now = Now { minutes: 22 * 60, day: 172, offset: 120 };
        let paris = (48.9, 2.3);
        assert_eq!(is_night(Mode::Off, now, (420, 1200), Some(true), paris), None);
        assert_eq!(is_night(Mode::System, now, (420, 1200), None, paris), None);
        assert_eq!(is_night(Mode::System, now, (420, 1200), Some(false), paris), Some(false));
        assert_eq!(is_night(Mode::Schedule, now, (420, 1200), None, paris), Some(true));
        assert_eq!(is_night(Mode::Sun, now, (420, 1200), None, paris), Some(true), "22:00 in June is after sunset");
        let noon = Now { minutes: 12 * 60, ..now };
        assert_eq!(is_night(Mode::Sun, noon, (420, 1200), None, paris), Some(false));
        for m in Mode::ALL {
            assert_eq!(Mode::from_code(m.code()), Some(m));
        }
    }

    #[test]
    fn blending_goes_from_one_theme_to_the_other() {
        let themes = crate::theme::all();
        let a = &themes[0];
        let b = themes.iter().find(|t| t.dark).unwrap();
        let start = blend(a, b, 0.0);
        assert_eq!((start.accent, start.text, start.dark), (a.accent, a.text, a.dark));
        let end = blend(a, b, 1.0);
        assert_eq!((end.accent, end.text, end.dark, end.name.as_str()), (b.accent, b.text, b.dark, b.name.as_str()));
        assert_eq!(end.scene.len(), a.scene.len() + b.scene.len());
        let mid = blend(a, b, 0.5);
        assert!(mid.scene[a.scene.len()..].iter().zip(&b.scene).all(|(m, l)| (m.opacity - l.opacity * 0.5).abs() < 1e-6));
        assert_eq!(fade_progress(0.0), 0.0);
        assert_eq!(fade_progress(FADE_SECONDS * 2.0), 1.0);
        assert!((fade_progress(FADE_SECONDS / 2.0) - 0.5).abs() < 1e-6);
    }
}
