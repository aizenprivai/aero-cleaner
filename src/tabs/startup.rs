//! The startup tab.

use super::prelude::*;

impl App {
    pub(crate) fn startup_tab(&mut self, ui: &mut egui::Ui, area: Rect) {
        let (list, clicked) = self.tab_frame(
            ui,
            area,
            "Startup apps",
            "Apps that start when you log in. Turning off the ones you don't need makes login faster. Changes apply at your next login.",
            Some(("🔄 Refresh", true)),
        );
        if clicked {
            self.startup = tools::startup_entries();
            self.startup_msg.clear();
        }
        let th = &self.themes[self.theme];
        let mut toggle = None;
        ui.scope_builder(egui::UiBuilder::new().max_rect(list.shrink(6.0)), |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let on = self.startup.iter().filter(|e| e.enabled).count();
                let msg = if self.startup_msg.is_empty() { format!("{on} OF {} APPS START AT LOGIN", self.startup.len()) } else { self.startup_msg.clone() };
                Self::section(ui, &msg, th);
                for (i, e) in self.startup.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width() - 14.0, 54.0), Sense::click());
                    let rect = rect.translate(vec2(4.0, 0.0));
                    let hovered = resp.hovered() && !e.critical;
                    if resp.clicked() && !e.critical {
                        toggle = Some(i);
                    }
                    let p = ui.painter();
                    let tint = e.enabled.then(|| if th.dark { th.go } else { aero::lighten(th.go, 0.75) });
                    aero::surface(p, rect, 14.0, th, hovered, tint);
                    let cy = rect.center().y;
                    if e.critical {
                        aero::orb(p, pos2(rect.min.x + 26.0, cy), 12.0, rgba(150, 160, 172, 255), "🔒");
                    } else {
                        aero::bubble_check(p, pos2(rect.min.x + 26.0, cy), 12.0, e.enabled, hovered, th.go, th.accent);
                    }
                    aero::orb(p, pos2(rect.min.x + 70.0, cy), 17.0, if e.system { th.root_orb } else { th.user_orb }, "🚀");
                    let clip = p.with_clip_rect(Rect::from_min_max(rect.min, pos2(rect.max.x - 130.0, rect.max.y)));
                    clip.text(pos2(rect.min.x + 98.0, cy - 9.0), Align2::LEFT_CENTER, &e.name, bold(15.0), th.text);
                    clip.text(pos2(rect.min.x + 98.0, cy + 11.0), Align2::LEFT_CENTER, &e.comment, regular(12.0), th.soft);
                    let (badge, color) = if e.critical {
                        ("needed by desktop", rgba(150, 160, 172, 255))
                    } else if e.enabled {
                        ("starts at login", th.go)
                    } else {
                        ("off", rgba(150, 160, 172, 255))
                    };
                    let tag = p.layout_no_wrap(badge.to_string(), regular(11.0), aero::darken(color, 0.6));
                    let br = Rect::from_min_size(pos2(rect.max.x - 18.0 - tag.size().x - 16.0, cy - 10.0), vec2(tag.size().x + 16.0, 20.0));
                    aero::rrect_grad(p, br, 10.0, &[(0.0, aero::lighten(color, 0.6)), (1.0, aero::lighten(color, 0.2))]);
                    aero::rrect_outline(p, br, 10.0, Stroke::new(1.0, aero::darken(color, 0.25)));
                    p.galley(br.center() - tag.size() / 2.0, tag, aero::darken(color, 0.6));
                    if e.critical {
                        resp.on_hover_text("Part of the desktop. Turning it off could break things, so it's locked.");
                    } else if hovered {
                        resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                    }
                }
                ui.add_space(6.0);
            });
        });
        if let Some(i) = toggle {
            let e = &self.startup[i];
            let (file, name, want) = (e.file.clone(), e.name.clone(), !e.enabled);
            self.startup_msg = match tools::set_startup(&file, want) {
                Ok(()) => format!("{name} will {}start at login", if want { "" } else { "no longer " }).to_uppercase(),
                Err(err) => format!("COULD NOT CHANGE {name}: {err}"),
            };
            self.startup = tools::startup_entries();
        }
    }
}
