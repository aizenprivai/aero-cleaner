//! The leftovers tab: folders left behind by apps that are gone.

use super::prelude::*;
use crate::leftovers::{self, Leftover};

#[derive(Default)]
pub(crate) struct LeftoverState {
    pub items: Vec<Leftover>,
    pub scanning: bool,
    pub done: bool,
    pub msg: String,
}

impl App {
    fn start_leftover_scan(&mut self) {
        self.leftovers.scanning = true;
        let tx = self.notifier();
        std::thread::spawn(move || tx.send(Msg::LeftoversDone(leftovers::find())));
    }

    pub(crate) fn leftovers_tab(&mut self, ui: &mut egui::Ui, area: Rect, spin: f32) {
        if !self.leftovers.done && !self.leftovers.scanning {
            self.start_leftover_scan();
        }
        let (list, again) = self.tab_frame(
            ui,
            area,
            "Leftovers",
            "Folders left behind by apps that don't seem installed anymore, untouched for 3 months or more. It's a guess: check each one. The Trash can bring it back.",
            Some(("🔄 Search again", !self.leftovers.scanning)),
        );
        if again {
            self.leftovers.msg.clear();
            self.start_leftover_scan();
        }
        let th = &self.themes[self.theme];
        let p = ui.painter().clone();
        if self.leftovers.scanning {
            super::spinner(&p, list.center() - vec2(0.0, 20.0), spin, th);
            embossed(&p, list.center() + vec2(0.0, 18.0), Align2::CENTER_CENTER, "Looking for leftovers...", regular(15.0), th.text);
            return;
        }
        let home = crate::platform::home().to_string_lossy().into_owned();
        let pretty = |p: &std::path::Path| p.to_string_lossy().replacen(&home, "~", 1);
        let (mut trash, mut hide, mut open) = (None, None, None);
        ui.scope_builder(egui::UiBuilder::new().max_rect(list.shrink(6.0)), |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let row_w = ui.available_width() - 14.0;
                let total: u64 = self.leftovers.items.iter().map(|l| l.size).sum();
                let header = if self.leftovers.msg.is_empty() {
                    format!("{} FOLDERS  ·  {} IN TOTAL", self.leftovers.items.len(), format_bytes(total))
                } else {
                    self.leftovers.msg.to_uppercase()
                };
                Self::section(ui, &header, th);
                if self.leftovers.items.is_empty() {
                    Self::section(ui, "NO LEFTOVERS FOUND. TIDY!", th);
                }
                for (i, l) in self.leftovers.items.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(vec2(row_w, 58.0), Sense::hover());
                    let rect = rect.translate(vec2(4.0, 0.0));
                    let p = ui.painter();
                    aero::surface(p, rect, 14.0, th, resp.hovered(), None);
                    let cy = rect.center().y;
                    aero::orb(p, pos2(rect.min.x + 26.0, cy), 16.0, th.user_orb, "📁");
                    let name = l.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    let room = rect.width() - 52.0 - 400.0;
                    aero::fit_text(p, pos2(rect.min.x + 52.0, cy - 10.0), Align2::LEFT_CENTER, &name, bold(15.0), th.text, room);
                    let used = tools::age(l.last_used);
                    let sub = if used.is_empty() { pretty(&l.path) } else { format!("{}  ·  {}", pretty(&l.path), crate::i18n::tr(format!("last used {used}"))) };
                    aero::fit_text(p, pos2(rect.min.x + 52.0, cy + 11.0), Align2::LEFT_CENTER, &sub, regular(12.0), th.soft, room);
                    embossed(p, pos2(rect.max.x - 320.0, cy), Align2::RIGHT_CENTER, &format_bytes(l.size), bold(16.0), th.text);
                    let open_r = Rect::from_min_size(pos2(rect.max.x - 306.0, cy - 14.0), vec2(80.0, 28.0));
                    let keep_r = Rect::from_min_size(pos2(rect.max.x - 220.0, cy - 14.0), vec2(116.0, 28.0));
                    let trash_r = Rect::from_min_size(pos2(rect.max.x - 98.0, cy - 14.0), vec2(86.0, 28.0));
                    if aero::aqua_button(ui, open_r, "📂 Open", th.accent, true).clicked() {
                        open = Some(i);
                    }
                    if aero::aqua_button(ui, keep_r, "Not leftover", rgba(125, 145, 170, 255), true)
                        .on_hover_text(crate::i18n::tr("Still in use: never show this folder again"))
                        .clicked()
                    {
                        hide = Some(i);
                    }
                    if aero::aqua_button(ui, trash_r, "🗑 Trash", th.root_orb, true).on_hover_text(crate::i18n::tr("Move this folder to the Trash (recoverable)")).clicked() {
                        trash = Some(i);
                    }
                }
                ui.add_space(6.0);
            });
        });
        if let Some(i) = open {
            crate::platform::open_dir(&self.leftovers.items[i].path);
        }
        if let Some(i) = hide {
            let l = self.leftovers.items.remove(i);
            leftovers::hide(&l.path);
            self.leftovers.msg = format!("{} won't be shown again", l.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
        }
        if let Some(i) = trash {
            let name = self.leftovers.items[i].path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            self.leftovers.msg = match tools::trash(&self.leftovers.items[i].path) {
                Ok(()) => {
                    let l = self.leftovers.items.remove(i);
                    format!("Moved {name} to the Trash. Empty the Trash in the Cleaner tab to free {}.", format_bytes(l.size))
                }
                Err(e) => format!("Could not move {name} to the Trash: {e}"),
            };
        }
    }
}
