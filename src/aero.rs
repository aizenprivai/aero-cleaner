//! Frutiger Aero painting toolkit: glossy gradients, glass panels, aqua
//! buttons, bubbles and a sunny sky. Everything is drawn with meshes since
//! egui has no built-in gradients.

use crate::theme::Theme;
use eframe::egui::{Align2, Color32, FontFamily, FontId, Mesh, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, pos2};
use std::f32::consts::TAU;

pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let (a, b) = (a.to_array(), b.to_array());
    let m = |i: usize| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(m(0), m(1), m(2), m(3))
}

pub fn lighten(c: Color32, t: f32) -> Color32 {
    let a = c.a();
    lerp(c, Color32::from_rgba_premultiplied(a, a, a, a), t)
}

pub fn darken(c: Color32, t: f32) -> Color32 {
    lerp(c, Color32::from_rgba_premultiplied(0, 0, 0, c.a()), t)
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}

/// Color at position `t` (0..1) along a list of gradient stops.
fn sample(stops: &[(f32, Color32)], t: f32) -> Color32 {
    if t <= stops[0].0 {
        return stops[0].1;
    }
    for w in stops.windows(2) {
        if t <= w[1].0 {
            return lerp(w[0].1, w[1].1, (t - w[0].0) / (w[1].0 - w[0].0).max(1e-6));
        }
    }
    stops[stops.len() - 1].1
}

/// Fills a vertically convex shape (described by its horizontal extent at each
/// y) with a top-to-bottom gradient, by stacking thin colored quads.
fn fill_vgrad(painter: &Painter, y0: f32, y1: f32, extent: impl Fn(f32) -> (f32, f32), stops: &[(f32, Color32)]) {
    let h = (y1 - y0).max(1.0);
    let mut ys: Vec<f32> = (0..=40).map(|i| y0 + h * i as f32 / 40.0).collect();
    ys.extend(stops.iter().map(|s| y0 + h * s.0));
    ys.retain(|y| *y >= y0 && *y <= y1);
    ys.sort_by(f32::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.01);

    let mut mesh = Mesh::default();
    for (i, &y) in ys.iter().enumerate() {
        let (l, r) = extent(y);
        let c = sample(stops, (y - y0) / h);
        mesh.colored_vertex(pos2(l, y), c);
        mesh.colored_vertex(pos2(r, y), c);
        if i > 0 {
            let k = (i * 2) as u32;
            mesh.add_triangle(k - 2, k - 1, k);
            mesh.add_triangle(k - 1, k, k + 1);
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// Horizontal extent of a rounded rectangle at height `y`.
fn rrect_extent(rect: Rect, r: f32) -> impl Fn(f32) -> (f32, f32) {
    let r = r.min(rect.width() / 2.0).min(rect.height() / 2.0);
    move |y| {
        let dy = if y < rect.min.y + r {
            rect.min.y + r - y
        } else if y > rect.max.y - r {
            y - (rect.max.y - r)
        } else {
            0.0
        };
        let inset = r - (r * r - dy.min(r) * dy.min(r)).max(0.0).sqrt();
        (rect.min.x + inset, rect.max.x - inset)
    }
}

pub fn rrect_grad(painter: &Painter, rect: Rect, r: f32, stops: &[(f32, Color32)]) {
    fill_vgrad(painter, rect.min.y, rect.max.y, rrect_extent(rect, r), stops);
}

pub fn circle_grad(painter: &Painter, c: Pos2, r: f32, stops: &[(f32, Color32)]) {
    fill_vgrad(
        painter,
        c.y - r,
        c.y + r,
        |y| {
            let dx = (r * r - (y - c.y) * (y - c.y)).max(0.0).sqrt();
            (c.x - dx, c.x + dx)
        },
        stops,
    );
}

/// Radial gradient disc from `inner` (center) to `outer` (edge).
pub fn radial(painter: &Painter, c: Pos2, r: f32, inner: Color32, outer: Color32) {
    const RINGS: usize = 10;
    const SEGS: usize = 48;
    let mut mesh = Mesh::default();
    mesh.colored_vertex(c, inner);
    for ring in 1..=RINGS {
        let t = ring as f32 / RINGS as f32;
        let col = lerp(inner, outer, t);
        for s in 0..SEGS {
            let a = s as f32 / SEGS as f32 * TAU;
            mesh.colored_vertex(c + Vec2::angled(a) * r * t, col);
        }
    }
    let idx = |ring: usize, s: usize| (1 + (ring - 1) * SEGS + s % SEGS) as u32;
    for s in 0..SEGS {
        mesh.add_triangle(0, idx(1, s), idx(1, s + 1));
    }
    for ring in 2..=RINGS {
        for s in 0..SEGS {
            mesh.add_triangle(idx(ring - 1, s), idx(ring, s), idx(ring, s + 1));
            mesh.add_triangle(idx(ring - 1, s), idx(ring, s + 1), idx(ring - 1, s + 1));
        }
    }
    painter.add(Shape::mesh(mesh));
}

pub fn rrect_outline(painter: &Painter, rect: Rect, r: f32, stroke: Stroke) {
    let r = r.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let mut pts = Vec::with_capacity(48);
    let corners = [
        (pos2(rect.max.x - r, rect.min.y + r), -0.25),
        (pos2(rect.max.x - r, rect.max.y - r), 0.0),
        (pos2(rect.min.x + r, rect.max.y - r), 0.25),
        (pos2(rect.min.x + r, rect.min.y + r), 0.5),
    ];
    for (c, start) in corners {
        for i in 0..=10 {
            let a = (start + 0.25 * i as f32 / 10.0) * TAU;
            pts.push(c + Vec2::angled(a) * r);
        }
    }
    painter.add(Shape::closed_line(pts, stroke));
}

/// Text with a soft light "emboss" underneath, typical of the era.
pub fn embossed(painter: &Painter, pos: Pos2, align: Align2, text: &str, font: FontId, color: Color32) {
    let text = &crate::i18n::tr(text);
    // Dark text gets a light highlight below; light text gets a dark shadow.
    let light_text = color.r() as u32 + color.g() as u32 + color.b() as u32 > 450;
    let under = if light_text { rgba(0, 10, 30, 150) } else { rgba(255, 255, 255, 170) };
    painter.text(pos + Vec2::new(0.0, 1.0), align, text, font.clone(), under);
    painter.text(pos, align, text, font, color);
}

pub fn with_alpha(c: Color32, a: u8) -> Color32 {
    rgba(c.r(), c.g(), c.b(), a)
}

/// Text with a dark drop shadow, for light text on colored buttons.
pub fn shadowed(painter: &Painter, pos: Pos2, align: Align2, text: &str, font: FontId, color: Color32) {
    let text = &crate::i18n::tr(text);
    painter.text(pos + Vec2::new(0.0, 1.5), align, text, font.clone(), rgba(0, 30, 60, 110));
    painter.text(pos, align, text, font, color);
}

/// Translucent frosted-glass panel with a glossy top highlight.
pub fn glass_panel(painter: &Painter, rect: Rect, r: f32, theme: &Theme) {
    for i in 1..=6 {
        let g = i as f32 * 2.5;
        rrect_grad(painter, rect.expand(g).translate(Vec2::new(0.0, 4.0)), r + g, &[(0.0, rgba(0, 20, 50, 7)), (1.0, rgba(0, 20, 50, 7))]);
    }
    let gl = theme.glass;
    let (fill, gloss_a, border) = if theme.dark {
        ([(0.0, with_alpha(lighten(gl, 0.15), 200)), (0.5, with_alpha(gl, 170)), (1.0, with_alpha(darken(gl, 0.3), 190))], 55, rgba(255, 255, 255, 90))
    } else {
        ([(0.0, rgba(255, 255, 255, 165)), (0.5, with_alpha(lighten(gl, 0.5), 115)), (1.0, with_alpha(gl, 140))], 150, rgba(255, 255, 255, 230))
    };
    rrect_grad(painter, rect, r, &fill);
    let gloss = Rect::from_min_max(rect.min + Vec2::splat(2.0), pos2(rect.max.x - 2.0, rect.min.y + (rect.height() * 0.45).min(70.0)));
    rrect_grad(painter, gloss, r - 2.0, &[(0.0, rgba(255, 255, 255, gloss_a)), (1.0, rgba(255, 255, 255, 0))]);
    rrect_outline(painter, rect, r, Stroke::new(1.5, border));
    rrect_outline(painter, rect.expand(1.0), r + 1.0, Stroke::new(1.0, with_alpha(darken(theme.accent, 0.3), 100)));
}

/// Translucent inner surface (list rows, boxes) that suits light or dark glass.
pub fn surface(painter: &Painter, rect: Rect, r: f32, theme: &Theme, hovered: bool, tint: Option<Color32>) {
    let (top, bottom, border) = if theme.dark {
        let a = if hovered { 70 } else { 38 };
        (rgba(255, 255, 255, a), with_alpha(tint.unwrap_or(Color32::WHITE), a / 2), rgba(255, 255, 255, 70))
    } else {
        let a = if hovered { 215 } else { 155 };
        (rgba(255, 255, 255, a), with_alpha(tint.unwrap_or(lighten(theme.glass, 0.4)), a * 3 / 5), rgba(255, 255, 255, 240))
    };
    rrect_grad(painter, rect, r, &[(0.0, top), (1.0, bottom)]);
    rrect_outline(painter, rect, r, Stroke::new(1.0, border));
    if hovered {
        rrect_outline(painter, rect.expand(1.0), r + 1.0, Stroke::new(1.0, with_alpha(theme.accent, 170)));
    }
}

/// A glossy "aqua" pill button. Returns the click response.
pub fn aqua_button(ui: &mut Ui, rect: Rect, label: &str, base: Color32, enabled: bool) -> Response {
    // An id that stays the same while the list scrolls, so keyboard focus sticks.
    let id = ui.auto_id_with(("aqua", label, rect.min.x as i32));
    let resp = ui.interact(rect, id, if enabled { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    let mut c = if enabled { base } else { rgba(150, 165, 180, 255) };
    if enabled && resp.is_pointer_button_down_on() {
        c = darken(c, 0.15);
    } else if enabled && resp.hovered() {
        c = lighten(c, 0.15);
    }
    let r = rect.height() / 2.0;

    // Soft shadow + outer glow when hovered.
    rrect_grad(p, rect.translate(Vec2::new(0.0, 3.0)).expand(1.0), r, &[(0.0, rgba(0, 30, 70, 0)), (1.0, rgba(0, 30, 70, 70))]);
    if enabled && resp.hovered() {
        for i in 1..=4 {
            rrect_outline(p, rect.expand(i as f32 * 1.5), r + i as f32 * 1.5, Stroke::new(1.5, lighten(c, 0.5).gamma_multiply(0.25 / i as f32)));
        }
    }
    rrect_grad(p, rect, r, &[(0.0, lighten(c, 0.35)), (0.5, c), (0.85, darken(c, 0.05)), (1.0, lighten(c, 0.45))]);
    let gloss = Rect::from_min_max(rect.min + Vec2::new(r * 0.5, 2.0), pos2(rect.max.x - r * 0.5, rect.center().y));
    rrect_grad(p, gloss, r * 0.8, &[(0.0, rgba(255, 255, 255, 215)), (1.0, rgba(255, 255, 255, 35))]);
    rrect_outline(p, rect, r, Stroke::new(1.2, darken(c, 0.4)));
    shadowed(p, rect.center() + Vec2::new(0.0, 1.0), Align2::CENTER_CENTER, label, bold((rect.height() * 0.33).clamp(12.0, 17.0)), Color32::WHITE);
    if enabled { resp.on_hover_cursor(eframe::egui::CursorIcon::PointingHand) } else { resp }
}

/// A glossy colored sphere ("orb") with a glyph in the middle.
pub fn orb(painter: &Painter, c: Pos2, r: f32, base: Color32, glyph: &str) {
    radial(painter, c + Vec2::new(0.0, r * 0.15), r * 1.25, rgba(0, 40, 80, 40), rgba(0, 40, 80, 0));
    circle_grad(painter, c, r, &[(0.0, lighten(base, 0.3)), (0.55, base), (1.0, lighten(base, 0.55))]);
    painter.circle_stroke(c, r, Stroke::new(1.0, darken(base, 0.35)));
    painter.text(c + Vec2::new(0.0, r * 0.08), Align2::CENTER_CENTER, glyph, regular(r * 1.0), Color32::WHITE);
    let hl = Rect::from_center_size(c - Vec2::new(0.0, r * 0.48), Vec2::new(r * 1.3, r * 0.85));
    rrect_grad(painter, hl, r * 0.42, &[(0.0, rgba(255, 255, 255, 220)), (1.0, rgba(255, 255, 255, 10))]);
}

/// Glossy bubble checkbox, green when checked.
pub fn bubble_check(painter: &Painter, c: Pos2, r: f32, checked: bool, hovered: bool, on: Color32, accent: Color32) {
    if checked {
        orb(painter, c, r, on, "✔");
    } else {
        circle_grad(painter, c, r, &[(0.0, rgba(255, 255, 255, 240)), (1.0, rgba(205, 230, 245, 240))]);
        let stroke = if hovered { accent } else { rgba(110, 150, 185, 255) };
        painter.circle_stroke(c, r, Stroke::new(1.5, stroke));
        let hl = Rect::from_center_size(c - Vec2::new(0.0, r * 0.45), Vec2::new(r * 1.2, r * 0.8));
        rrect_grad(painter, hl, r * 0.4, &[(0.0, rgba(255, 255, 255, 255)), (1.0, rgba(255, 255, 255, 0))]);
    }
}

/// A transparent soap bubble.
pub fn bubble(painter: &Painter, c: Pos2, r: f32, alpha: f32, tint: Color32) {
    let a = |v: f32| (v * alpha) as u8;
    radial(painter, c, r, rgba(255, 255, 255, a(8.0)), with_alpha(tint, a(80.0)));
    painter.circle_stroke(c, r, Stroke::new(1.2, rgba(255, 255, 255, a(150.0))));
    let hl = Rect::from_center_size(c + Vec2::new(-r * 0.3, -r * 0.42), Vec2::new(r * 0.75, r * 0.42));
    rrect_grad(painter, hl, r * 0.21, &[(0.0, rgba(255, 255, 255, a(230.0))), (1.0, rgba(255, 255, 255, a(30.0)))]);
    painter.circle_filled(c + Vec2::new(r * 0.45, r * 0.45), r * 0.12, rgba(255, 255, 255, a(110.0)));
}

/// Deterministic pseudo-random number in 0..1.
pub fn hash(i: u32, salt: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65535.0
}

/// Sky, sun (or moon and aurora), rolling hills and rising bubbles.
/// Scales with `rect`, so it also draws the small previews in the theme tab.
pub fn background(painter: &Painter, rect: Rect, time: f32, theme: &Theme) {
    let (w, h) = (rect.width(), rect.height());
    let scale = (h / 740.0).clamp(0.15, 1.5);
    rrect_grad(painter, rect, 0.0, &theme.sky);

    let sun = rect.min + Vec2::new(w * theme.sun.x, h * theme.sun.y);
    if theme.night {
        // Twinkling stars.
        for i in 0..90 {
            let p = rect.min + Vec2::new(hash(i, 21) * w, hash(i, 22) * h * 0.75);
            let tw = ((time * (1.0 + hash(i, 23) * 2.0) + i as f32).sin() * 0.5 + 0.5) * 200.0 + 40.0;
            painter.circle_filled(p, (0.6 + hash(i, 24) * 1.2) * scale.max(0.5), rgba(255, 255, 255, tw as u8));
        }
        // Aurora: soft wavy curtains of light.
        for (k, col) in [(0, rgba(80, 255, 170, 70)), (1, rgba(120, 180, 255, 55)), (2, rgba(190, 120, 255, 40))] {
            let base = rect.min.y + h * (0.2 + k as f32 * 0.08);
            let height = h * 0.16;
            let mut mesh = Mesh::default();
            let steps = 60;
            for s in 0..=steps {
                let fx = s as f32 / steps as f32;
                let x = rect.min.x + fx * w;
                let y = base + ((fx * 7.0 + time * 0.35 + k as f32 * 2.0).sin() * 0.6 + (fx * 13.0 - time * 0.2).sin() * 0.4) * h * 0.05;
                let fade = (fx * std::f32::consts::PI).sin();
                let c = with_alpha(col, (col.a() as f32 * fade) as u8);
                mesh.colored_vertex(pos2(x, y - height * 0.25), with_alpha(col, 0));
                mesh.colored_vertex(pos2(x, y), c);
                mesh.colored_vertex(pos2(x, y + height), with_alpha(col, 0));
                if s > 0 {
                    let i = (s * 3) as u32;
                    for j in 0..2 {
                        mesh.add_triangle(i - 3 + j, i + j, i - 2 + j);
                        mesh.add_triangle(i - 2 + j, i + j, i + 1 + j);
                    }
                }
            }
            painter.add(Shape::mesh(mesh));
        }
        // Moon.
        radial(painter, sun, 120.0 * scale, with_alpha(theme.sun_color, 70), with_alpha(theme.sun_color, 0));
        circle_grad(painter, sun, 26.0 * scale, &[(0.0, Color32::WHITE), (1.0, theme.sun_color)]);
    } else {
        // Sun and slowly turning light rays.
        if theme.rays > 0 {
            for i in 0..9 {
                let a0 = time * 0.03 + i as f32 / 9.0 * TAU;
                let mut mesh = Mesh::default();
                mesh.colored_vertex(sun, rgba(255, 255, 255, theme.rays));
                mesh.colored_vertex(sun + Vec2::angled(a0) * w * 1.6, rgba(255, 255, 255, 0));
                mesh.colored_vertex(sun + Vec2::angled(a0 + 0.13) * w * 1.6, rgba(255, 255, 255, 0));
                mesh.add_triangle(0, 1, 2);
                painter.add(Shape::mesh(mesh));
            }
        }
        radial(painter, sun, w * 0.35, with_alpha(theme.sun_color, 170), with_alpha(theme.sun_color, 0));
        radial(painter, sun, 34.0 * scale, Color32::WHITE, with_alpha(theme.sun_color, 0));
    }

    // Two layers of hills.
    circle_grad(painter, rect.min + Vec2::new(w * 0.78, h * 1.55), h * 0.78, &theme.hill_back);
    circle_grad(painter, rect.min + Vec2::new(w * 0.15, h * 1.75), h * 0.92, &theme.hill_front);

    // Rising bubbles.
    for i in 0..18 {
        let r = (8.0 + hash(i, 1) * 28.0) * scale;
        let speed = (12.0 + hash(i, 2) * 22.0) * scale;
        let span = h + 2.0 * r;
        let y = rect.max.y + r - ((time * speed + hash(i, 3) * span) % span);
        let x = rect.min.x + hash(i, 4) * w + (time * 0.6 + i as f32).sin() * 14.0 * scale;
        bubble(painter, pos2(x, y), r, 0.55 + hash(i, 5) * 0.45, theme.bubble);
    }
}

/// A sidebar entry: an aqua bar when active, a glass highlight on hover.
pub fn nav_item(ui: &mut Ui, rect: Rect, icon: &str, label: &str, active: bool, theme: &Theme) -> Response {
    let resp = ui.interact(rect, eframe::egui::Id::unique(("nav", label)), Sense::click());
    let p = ui.painter();
    let r = 12.0;
    let text_color = if active {
        let c = theme.accent;
        rrect_grad(p, rect, r, &[(0.0, lighten(c, 0.35)), (0.5, c), (0.85, darken(c, 0.05)), (1.0, lighten(c, 0.4))]);
        let gloss = Rect::from_min_max(rect.min + Vec2::new(6.0, 2.0), pos2(rect.max.x - 6.0, rect.center().y));
        rrect_grad(p, gloss, r * 0.7, &[(0.0, rgba(255, 255, 255, 190)), (1.0, rgba(255, 255, 255, 25))]);
        rrect_outline(p, rect, r, Stroke::new(1.2, darken(c, 0.4)));
        Color32::WHITE
    } else {
        if resp.hovered() {
            surface(p, rect, r, theme, true, None);
        }
        theme.text
    };
    let cy = rect.center().y;
    let draw = |pos: Pos2, text: &str, font: FontId| {
        if active {
            shadowed(p, pos, Align2::LEFT_CENTER, text, font, text_color);
        } else {
            p.text(pos, Align2::LEFT_CENTER, crate::i18n::tr(text), font, text_color);
        }
    };
    draw(pos2(rect.min.x + 12.0, cy), icon, regular(16.0));
    draw(pos2(rect.min.x + 40.0, cy), label, bold(14.5));
    if active { resp } else { resp.on_hover_cursor(eframe::egui::CursorIcon::PointingHand) }
}
