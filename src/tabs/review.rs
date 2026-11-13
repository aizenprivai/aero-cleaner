//! The review tab.

use super::prelude::*;

impl App {
    pub(crate) fn review_sheet(&mut self, ctx: &egui::Context, screen: Rect, spin: f32) {
        let th = &self.themes[self.theme];
        let Some(review) = self.review.as_ref() else { return };
        let total = self.total_of(&review.kinds);
        let mut action = None;
        egui::Area::new(egui::Id::unique("review-sheet")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
            // Swallow clicks so nothing underneath reacts while the sheet is open.
            ui.allocate_exact_size(screen.size(), Sense::click());
            let p = ui.painter().clone();
            p.rect_filled(screen, 0.0, rgba(0, 20, 50, 120));
            let sheet = Rect::from_center_size(screen.center(), vec2((screen.width() - 80.0).min(660.0), (screen.height() - 80.0).min(580.0)));
            // Drawn twice for a more opaque sheet, so text stays readable.
            aero::glass_panel(&p, sheet, 20.0, th);
            aero::glass_panel(&p, sheet, 20.0, th);
            let inner = sheet.shrink(24.0);
            embossed(&p, pos2(inner.min.x, inner.min.y + 14.0), Align2::LEFT_CENTER, "Ready to clean?", bold(24.0), th.text);
            let summary = format!("{} items selected · about {} will be freed", review.kinds.len(), format_bytes(total));
            p.text(pos2(inner.min.x, inner.min.y + 44.0), Align2::LEFT_CENTER, summary, regular(14.0), th.soft);

            let mut top = inner.min.y + 66.0;
            if !review.browsers.is_empty() {
                let warn = Rect::from_min_size(pos2(inner.min.x, top), vec2(inner.width(), 48.0));
                let c = rgba(240, 160, 30, 255);
                aero::rrect_grad(&p, warn, 12.0, &[(0.0, aero::lighten(c, 0.75)), (1.0, aero::lighten(c, 0.45))]);
                aero::rrect_outline(&p, warn, 12.0, Stroke::new(1.0, aero::darken(c, 0.2)));
                let msg = format!(
                    "⚠ {} is open. Close it first for a full clean: files it's using will be skipped, and it may need to reload pages.",
                    review.browsers.join(" and ")
                );
                let g = p.layout(msg, regular(13.0), aero::darken(c, 0.7), warn.width() - 24.0);
                p.galley(pos2(warn.min.x + 12.0, warn.center().y - g.size().y / 2.0), g, aero::darken(c, 0.7));
                top = warn.max.y + 10.0;
            }

            let list = Rect::from_min_max(pos2(inner.min.x, top), pos2(inner.max.x, inner.max.y - 62.0));
            aero::surface(&p, list, 14.0, th, false, None);
            match &review.lines {
                None => {
                    for k in 0..8 {
                        let a = spin * 5.0 + k as f32 * std::f32::consts::TAU / 8.0;
                        p.circle_filled(list.center() - vec2(0.0, 14.0) + Vec2::angled(a) * 14.0, 2.0 + k as f32 * 0.4, with_alpha(th.accent, (60 + k * 24) as u8));
                    }
                    p.text(list.center() + vec2(0.0, 18.0), Align2::CENTER_CENTER, "Preparing the list...", regular(14.0), th.soft);
                }
                Some(lines) => {
                    ui.scope_builder(egui::UiBuilder::new().max_rect(list.shrink(12.0)), |ui| {
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                            for (kind, ls) in lines {
                                let cat = CATEGORIES.iter().find(|c| c.kind == *kind).unwrap();
                                let size = self.items.iter().zip(CATEGORIES).find(|(_, c)| c.kind == *kind).and_then(|(it, _)| it.result.as_ref()?.size);
                                let title = match size {
                                    Some(sz) => format!("{} {}  ·  {}", cat.icon, cat.name, format_bytes(sz)),
                                    None => format!("{} {}", cat.icon, cat.name),
                                };
                                ui.label(egui::RichText::new(title).font(bold(15.0)).color(th.text));
                                for l in ls {
                                    ui.label(egui::RichText::new(format!("    {l}")).font(regular(12.5)).color(th.soft));
                                }
                                ui.add_space(6.0);
                            }
                        });
                    });
                }
            }

            let clean_r = Rect::from_min_size(pos2(inner.max.x - 200.0, inner.max.y - 48.0), vec2(200.0, 48.0));
            let cancel_r = Rect::from_min_size(pos2(clean_r.min.x - 150.0, clean_r.min.y), vec2(140.0, 48.0));
            if aero::aqua_button(ui, cancel_r, "Cancel", rgba(125, 145, 170, 255), true).clicked() {
                action = Some(false);
            }
            if aero::aqua_button(ui, clean_r, "✨ Clean now", th.go, true).clicked() {
                action = Some(true);
            }
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            action = Some(false);
        }
        match action {
            Some(true) => {
                let kinds = self.review.take().map(|r| r.kinds).unwrap_or_default();
                self.start_clean(kinds);
            }
            Some(false) => self.review = None,
            None => {}
        }
    }
}
