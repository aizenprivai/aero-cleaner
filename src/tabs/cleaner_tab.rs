//! The cleaner tab.

use super::prelude::*;

impl App {
    pub(crate) fn row(&mut self, ui: &mut egui::Ui, i: usize) {
        let th = &self.themes[self.theme];
        let cat = &CATEGORIES[i];
        let width = ui.available_width() - 14.0;
        let (rect, resp) = ui.allocate_exact_size(vec2(width, 60.0), Sense::click());
        let rect = rect.translate(vec2(4.0, 0.0));
        let mut toggled = false;
        let item = &mut self.items[i];
        let available = item.result.as_ref().is_none_or(|r| r.available);
        let busy = self.phase != Phase::Idle;
        if resp.clicked() && available && !busy {
            item.on = !item.on;
            toggled = true;
        }
        let checked = item.on && available;
        aero::a11y_checkbox(&resp, available && !busy, checked, cat.name);
        let hovered = resp.hovered() && available && !busy;
        let p = ui.painter();

        let tint = checked.then(|| if th.dark { th.go } else { aero::lighten(th.go, 0.75) });
        aero::surface(p, rect, 14.0, th, hovered, tint);

        let cy = rect.center().y;
        aero::bubble_check(p, pos2(rect.min.x + 26.0, cy), 12.0, checked, hovered, th.go, th.accent);
        let grey = rgba(150, 160, 172, 255);
        let hue = if cat.root { th.root_orb } else { th.user_orb };
        aero::orb(p, pos2(rect.min.x + 72.0, cy), 19.0, if available { hue } else { grey }, cat.icon);

        let text_x = rect.min.x + 104.0;
        let name_color = if available { th.text } else { grey };
        let name_rect = p.text(pos2(text_x, cy - 10.0), Align2::LEFT_CENTER, crate::i18n::tr(cat.name), bold(16.0), name_color);
        if cat.root {
            let badge = Rect::from_min_size(pos2(name_rect.max.x + 8.0, cy - 19.0), vec2(56.0, 18.0));
            let b = th.root_orb;
            aero::rrect_grad(p, badge, 9.0, &[(0.0, aero::lighten(b, 0.55)), (1.0, aero::lighten(b, 0.15))]);
            aero::rrect_outline(p, badge, 9.0, Stroke::new(1.0, aero::darken(b, 0.25)));
            p.text(badge.center(), Align2::CENTER_CENTER, crate::i18n::tr("🔒 admin"), regular(11.0), aero::darken(b, 0.65));
        }
        let desc = crate::i18n::tr(cat.desc.replace("{}", &self.settings.journal_days.to_string()));
        aero::fit_text(p, pos2(text_x, cy + 11.0), Align2::LEFT_CENTER, &desc, regular(13.0), th.soft, rect.max.x - 140.0 - text_x);

        let right = pos2(rect.max.x - 18.0, cy);
        match &item.result {
            None => {
                let t = ui.input(|i| i.time) as f32;
                for k in 0..3 {
                    let a = ((t * 4.0 - k as f32 * 0.7).sin() * 0.5 + 0.5) * 255.0;
                    p.circle_filled(right - vec2(k as f32 * 14.0, 0.0), 4.0, with_alpha(th.accent, a as u8));
                }
            }
            Some(r) => {
                let size = r.size.map(format_bytes);
                let (main, sub) = match (size, r.note.as_str()) {
                    (Some(s), "up to") => (format!("up to {s}"), String::new()),
                    (Some(s), note) => (s, note.to_string()),
                    (None, note) => (String::new(), note.to_string()),
                };
                if !main.is_empty() {
                    embossed(p, right - vec2(0.0, if sub.is_empty() { 0.0 } else { 9.0 }), Align2::RIGHT_CENTER, &main, bold(17.0), th.text);
                }
                if !sub.is_empty() {
                    aero::fit_text(p, right + vec2(0.0, if main.is_empty() { 0.0 } else { 11.0 }), Align2::RIGHT_CENTER, &crate::i18n::tr(&sub), regular(12.0), th.soft, 210.0);
                }
            }
        }
        if hovered {
            resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        }
        if toggled {
            self.remember_selection();
        }
    }

    pub(crate) fn cleaner_tab(&mut self, ui: &mut egui::Ui, area: Rect, t: f32, time: f64) {
        let th = &self.themes[self.theme];
        let p = ui.painter().clone();
        let footer_h = 80.0;
        let list = Rect::from_min_max(area.min, pos2(area.max.x, area.max.y - footer_h));
        aero::rrect_grad(&p, list, 16.0, &[(0.0, with_alpha(th.glass, 90)), (1.0, with_alpha(th.glass, 50))]);
        aero::rrect_outline(&p, list, 16.0, Stroke::new(1.0, with_alpha(th.accent, 90)));
        ui.scope_builder(egui::UiBuilder::new().max_rect(list.shrink(6.0)), |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                Self::section(ui, "YOUR FILES", &self.themes[self.theme]);
                for i in (0..CATEGORIES.len()).filter(|&i| !CATEGORIES[i].root) {
                    self.row(ui, i);
                }
                self.rules_section(ui);
                let title = if crate::platform::IS_WINDOWS { "SYSTEM  (asks for administrator permission once)" } else { "SYSTEM  (asks for your password once)" };
                Self::section(ui, title, &self.themes[self.theme]);
                for i in (0..CATEGORIES.len()).filter(|&i| CATEGORIES[i].root) {
                    self.row(ui, i);
                }
                ui.add_space(6.0);
            });
        });

        // Footer: status box and the two aqua buttons.
        let th = &self.themes[self.theme];
        let fy = area.max.y - footer_h + 12.0;
        let busy = self.phase != Phase::Idle;
        let clean_btn = Rect::from_min_size(pos2(area.max.x - 200.0, fy + 8.0), vec2(200.0, 52.0));
        let scan_btn = Rect::from_min_size(pos2(clean_btn.min.x - 160.0, fy + 8.0), vec2(145.0, 52.0));
        let status_box = Rect::from_min_max(pos2(area.min.x, fy + 4.0), pos2(scan_btn.min.x - 18.0, fy + 64.0));

        aero::surface(&p, status_box, 14.0, th, false, None);
        let sc = status_box.left_center() + vec2(26.0, 0.0);
        if busy {
            for k in 0..8 {
                let a = t * 5.0 + k as f32 * std::f32::consts::TAU / 8.0;
                p.circle_filled(sc + Vec2::angled(a) * 10.0, 1.5 + k as f32 * 0.35, with_alpha(th.accent, (60 + k * 24) as u8));
            }
        } else {
            aero::orb(&p, sc, 12.0, th.go, "✔");
        }
        // "Undo last clean" while something is kept from the last clean.
        if self.undo_list.is_none() {
            self.load_undo_list();
        }
        let th = &self.themes[self.theme];
        let latest = self.undo_list.as_ref().and_then(|l| l.first()).filter(|h| crate::diskhistory::now().saturating_sub(h.when) < self.settings.undo_hours as u64 * 3600).cloned();
        let undo_w = if latest.is_some() { 112.0 } else { 0.0 };
        // At most two lines, so the box never overflows.
        let mut job = egui::text::LayoutJob::simple(crate::i18n::tr(self.status.clone()), regular(14.0), th.text, status_box.width() - 64.0 - undo_w);
        job.wrap.max_rows = 2;
        let status = p.layout_job(job);
        p.galley(pos2(sc.x + 22.0, status_box.center().y - status.size().y / 2.0), status, th.text);
        if let Some(held) = latest {
            let r = Rect::from_min_size(pos2(status_box.max.x - undo_w - 6.0, status_box.center().y - 17.0), vec2(undo_w, 34.0));
            let tip = crate::i18n::tr(format!("Puts back what the last clean removed ({}). Kept until it's {} hours old.", format_bytes(held.size), self.settings.undo_hours));
            if aero::aqua_button(ui, r, "↩ Undo", th.accent, !busy && !self.undo_busy).on_hover_text(tip).clicked() {
                self.restore_held(held);
            }
        }
        let th = &self.themes[self.theme];

        let (accent, go, bubble_tint) = (th.accent, th.go, th.bubble);
        let can_clean = !busy && !self.selected().is_empty();
        if aero::aqua_button(ui, scan_btn, "🔍 Scan", accent, !busy).clicked() {
            self.start_scan();
        }
        if aero::aqua_button(ui, clean_btn, "✨ Clean now", go, can_clean).clicked() {
            self.open_review();
        }

        // Celebration: a burst of bubbles rising from the Clean button.
        if let Some(start) = self.celebrate.filter(|_| !self.settings.reduce_motion) {
            let age = (time - start) as f32;
            if age > 3.5 {
                self.celebrate = None;
            }
            for i in 0..30 {
                let a = age - aero::hash(i, 7) * 0.8;
                if a <= 0.0 {
                    continue;
                }
                let r = 4.0 + aero::hash(i, 8) * 12.0;
                let pos = clean_btn.center() + vec2((aero::hash(i, 9) - 0.5) * 260.0 * a.min(1.2) + (a * 3.0 + i as f32).sin() * 10.0, -a * (140.0 + aero::hash(i, 10) * 200.0));
                aero::bubble(&p, pos, r, (1.0 - a / 2.7).clamp(0.0, 1.0), bubble_tint);
            }
        }
    }
}
