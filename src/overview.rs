//! The Overview's health score: one number from 0 to 100 built from free
//! space, junk waiting to be cleaned, drive health, boot time and whether
//! AeroClean is up to date. Missing information counts as "fine", so the
//! score never punishes what can't be measured.

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Inputs {
    /// (total, available) bytes on the main disk.
    pub disk: Option<(u64, u64)>,
    /// Bytes the Cleaner would free with the safe items.
    pub junk: u64,
    /// Some(false) if any drive reports a problem; worst SSD wear in %.
    pub drives_ok: Option<bool>,
    pub worst_wear: Option<u8>,
    /// Seconds of the last boot.
    pub boot_secs: Option<f32>,
    pub update_available: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub label: &'static str,
    pub points: f32,
    pub max: f32,
}

fn lerp(x: f32, from: (f32, f32), to: (f32, f32)) -> f32 {
    let t = ((x - from.0) / (from.1 - from.0)).clamp(0.0, 1.0);
    to.0 + (to.1 - to.0) * t
}

/// Each part's points, in display order.
pub fn parts(i: &Inputs) -> Vec<Part> {
    let free = match i.disk {
        // 5% free or less: 0 points; 25% or more: all 40.
        Some((total, avail)) if total > 0 => lerp(avail as f32 * 100.0 / total as f32, (5.0, 25.0), (0.0, 40.0)),
        _ => 40.0,
    };
    // Up to 200 MB of junk is normal; 5 GB or more costs most of the points.
    let junk = lerp(i.junk as f32 / 1e9, (0.2, 5.0), (20.0, 4.0));
    let drives = match (i.drives_ok, i.worst_wear) {
        (Some(false), _) => 0.0,
        (_, Some(w)) if w >= 90 => 8.0,
        (_, Some(w)) if w >= 70 => 15.0,
        _ => 20.0,
    };
    let boot = i.boot_secs.map_or(10.0, |s| lerp(s, (25.0, 90.0), (10.0, 2.0)));
    let update = if i.update_available { 5.0 } else { 10.0 };
    vec![
        Part { label: "Free space", points: free, max: 40.0 },
        Part { label: "Junk to clean", points: junk, max: 20.0 },
        Part { label: "Drive health", points: drives, max: 20.0 },
        Part { label: "Boot time", points: boot, max: 10.0 },
        Part { label: "Up to date", points: update, max: 10.0 },
    ]
}

pub fn score(i: &Inputs) -> u32 {
    parts(i).iter().map(|p| p.points).sum::<f32>().round() as u32
}

/// A word for the score (English; translated by the caller).
pub fn verdict(score: u32) -> &'static str {
    match score {
        90.. => "Excellent",
        75..=89 => "Good",
        50..=74 => "Could be better",
        _ => "Needs attention",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthy_machine_scores_high_and_problems_cost_points() {
        let healthy = Inputs { disk: Some((500, 200)), junk: 50_000_000, drives_ok: Some(true), worst_wear: Some(9), boot_secs: Some(20.0), update_available: false };
        assert_eq!(score(&healthy), 100);
        assert_eq!(verdict(score(&healthy)), "Excellent");

        let full = Inputs { disk: Some((500, 10)), ..healthy.clone() }; // 2% free
        assert_eq!(score(&full), 60);
        let failing = Inputs { drives_ok: Some(false), ..healthy.clone() };
        assert_eq!(score(&failing), 80);
        let junky = Inputs { junk: 10_000_000_000, ..healthy.clone() };
        assert_eq!(score(&junky), 84);
        let slow = Inputs { boot_secs: Some(120.0), update_available: true, ..healthy.clone() };
        assert_eq!(score(&slow), 87);
        assert_eq!(verdict(score(&full)), "Could be better");
        assert_eq!(verdict(score(&Inputs { disk: Some((500, 10)), drives_ok: Some(false), ..healthy })), "Needs attention", "40 points");
    }

    #[test]
    fn unknowns_count_as_fine() {
        assert_eq!(score(&Inputs::default()), 100);
        assert_eq!(parts(&Inputs::default()).iter().map(|p| p.max).sum::<f32>(), 100.0);
    }
}
