//! The themes tab.

use super::prelude::*;

impl App {
    pub(crate) fn themes_tab(&mut self, ui: &mut egui::Ui, area: Rect, t: f32) {
        let gap = 18.0;
        let cols = 3;
        let w = (area.width() - gap * (cols as f32 - 1.0)) / cols as f32;
        let h = (area.height() - gap) / 2.0;
        let mut picked = None;

        for i in 0..self.themes.len() {
            let (row, col) = (i / cols, i % cols);
            // Center the second row (2 cards under 3).
            let offset = if row == 1 { (w + gap) / 2.0 } else { 0.0 };
            let card = Rect::from_min_size(area.min + vec2(offset + col as f32 * (w + gap), row as f32 * (h + gap)), vec2(w, h));
            let resp = ui.interact(card, egui::Id::unique(("theme-card", i)), Sense::click());
            let cur = &self.themes[self.theme];
            let th = &self.themes[i];
            let selected = i == self.theme;
            let p = ui.painter().clone();

            aero::surface(&p, card, 16.0, cur, resp.hovered() && !selected, None);
            if selected {
                for k in 1..=3 {
                    aero::rrect_outline(&p, card.expand(k as f32 * 1.5), 16.0 + k as f32 * 1.5, Stroke::new(2.0, with_alpha(cur.accent, 200 / k)));
                }
            }

            // Live mini preview of the theme.
            let preview = Rect::from_min_max(card.min + vec2(8.0, 8.0), pos2(card.max.x - 8.0, card.max.y - 52.0));
            let pp = p.with_clip_rect(preview);
            aero::background(&pp, preview, t, th);
            let mini = preview.shrink2(vec2(preview.width() * 0.16, preview.height() * 0.18));
            aero::glass_panel(&pp, mini, 8.0, th);
            for (k, c) in [th.user_orb, th.root_orb].into_iter().enumerate() {
                let rowr = Rect::from_min_size(mini.min + vec2(8.0, 10.0 + k as f32 * 20.0), vec2(mini.width() - 16.0, 15.0));
                aero::surface(&pp, rowr, 6.0, th, false, None);
                aero::orb(&pp, rowr.left_center() + vec2(9.0, 0.0), 5.5, c, "");
                pp.rect_filled(Rect::from_min_size(rowr.left_center() + vec2(20.0, -2.0), vec2(rowr.width() * 0.4, 4.0)), 2.0, with_alpha(th.soft, 160));
            }
            for (k, c) in [th.accent, th.go].into_iter().enumerate() {
                let pill = Rect::from_min_size(pos2(mini.max.x - 8.0 - (2 - k) as f32 * 40.0, mini.max.y - 22.0), vec2(36.0, 14.0));
                aero::rrect_grad(&pp, pill, 7.0, &[(0.0, aero::lighten(c, 0.4)), (0.5, c), (1.0, aero::lighten(c, 0.4))]);
                let gl = Rect::from_min_max(pill.min + vec2(4.0, 1.0), pos2(pill.max.x - 4.0, pill.center().y));
                aero::rrect_grad(&pp, gl, 4.0, &[(0.0, rgba(255, 255, 255, 200)), (1.0, rgba(255, 255, 255, 30))]);
            }
            aero::rrect_outline(&p, preview, 4.0, Stroke::new(1.0, rgba(255, 255, 255, 180)));

            embossed(&p, pos2(card.min.x + 14.0, card.max.y - 33.0), Align2::LEFT_CENTER, th.name, bold(16.0), cur.text);
            let room = card.width() - 28.0 - if selected { 30.0 } else { 0.0 };
            aero::fit_text(&p, pos2(card.min.x + 14.0, card.max.y - 14.0), Align2::LEFT_CENTER, &crate::i18n::tr(th.desc), regular(12.0), cur.soft, room);
            if selected {
                aero::orb(&p, pos2(card.max.x - 22.0, card.max.y - 26.0), 12.0, cur.go, "✔");
            }
            if resp.clicked() {
                picked = Some(i);
            }
            if !selected {
                resp.on_hover_cursor(egui::CursorIcon::PointingHand);
            }
        }

        if let Some(i) = picked {
            self.theme = i;
            theme::save(&self.themes[i]);
            self.apply_visuals(ui.ctx());
        }
    }
}
