//! The developer cleanup tab.

use super::prelude::*;
use crate::devclean::{self, DevItem};

#[derive(Default)]
pub(crate) struct DevState {
    pub items: Vec<DevItem>,
    pub selected: Vec<bool>,
    /// Folders looked at so far while scanning.
    pub scanning: Option<usize>,
    pub deleting: bool,
    pub done: bool,
    pub msg: String,
    pub waydroid: Option<u64>,
    /// "Click again to confirm" deadline for the delete button.
    pub confirm_until: Option<f64>,
}

impl App {
    pub(crate) fn start_dev_scan(&mut self) {
        self.dev.scanning = Some(0);
        let tx = self.notifier();
        let home = std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default());
        std::thread::spawn(move || {
            let ptx = tx.clone();
            let items = devclean::scan(&home, &move |n| ptx.send(Msg::DevProgress(n)));
            tx.send(Msg::DevDone(items, devclean::waydroid_images()));
        });
    }

    fn start_dev_delete(&mut self) {
        let chosen: Vec<DevItem> = self
            .dev
            .items
            .drain(..)
            .zip(self.dev.selected.drain(..))
            .filter_map(|(item, on)| on.then_some(item))
            .collect();
        self.dev.deleting = true;
        let tx = self.notifier();
        std::thread::spawn(move || {
            let (mut freed, mut errors) = (0u64, Vec::new());
            for item in &chosen {
                match devclean::delete(item) {
                    Ok(()) => freed += item.size,
                    Err(e) => errors.push(format!("{}: {e}", item.path.display())),
                }
            }
            tx.send(Msg::DevDeleted(freed, errors));
        });
    }

    pub(crate) fn dev_tab(&mut self, ui: &mut egui::Ui, area: Rect, spin: f32, time: f64) {
        if !self.dev.done && self.dev.scanning.is_none() {
            self.start_dev_scan();
        }
        let busy = self.dev.scanning.is_some() || self.dev.deleting;
        let sel_size: u64 = self.dev.items.iter().zip(&self.dev.selected).filter(|(_, on)| **on).map(|(i, _)| i.size).sum();
        let confirming = self.dev.confirm_until.is_some_and(|t| time < t);
        let label = if confirming { "Click again to delete".to_string() } else { format!("🗑 Delete {}", format_bytes(sel_size)) };
        let (list, clicked) = self.tab_frame(
            ui,
            area,
            "Developer cleanup",
            "Build folders and packages inside your projects. Your code is never touched, and these come back on the next build. Projects untouched for 30+ days are pre-selected.",
            Some((&label, !busy && sel_size > 0)),
        );
        if clicked {
            if confirming {
                self.dev.confirm_until = None;
                self.start_dev_delete();
            } else {
                self.dev.confirm_until = Some(time + 4.0);
            }
        }
        if confirming {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
        }
        let th = &self.themes[self.theme];
        let p = ui.painter().clone();
        if busy {
            let text = if self.dev.deleting { "Deleting...".to_string() } else { format!("Looked at {} folders...", self.dev.scanning.unwrap_or(0)) };
            super::spinner(&p, list.center() - vec2(0.0, 20.0), spin, th);
            embossed(&p, list.center() + vec2(0.0, 18.0), Align2::CENTER_CENTER, &text, regular(15.0), th.text);
            return;
        }

        let home = std::env::var("HOME").unwrap_or_default();
        let pretty = |p: &std::path::Path| p.to_string_lossy().replacen(&home, "~", 1);
        let mut toggle = None;
        let mut rescan = false;
        ui.scope_builder(egui::UiBuilder::new().max_rect(list.shrink(6.0)), |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let row_w = ui.available_width() - 14.0;
                let total: u64 = self.dev.items.iter().map(|i| i.size).sum();
                let header = if self.dev.msg.is_empty() {
                    format!("{} FOLDERS  ·  {} IN TOTAL", self.dev.items.len(), format_bytes(total))
                } else {
                    self.dev.msg.to_uppercase()
                };
                let (hr, _) = ui.allocate_exact_size(vec2(row_w, 34.0), Sense::hover());
                embossed(ui.painter(), pos2(hr.min.x + 8.0, hr.center().y + 2.0), Align2::LEFT_CENTER, &header, bold(14.0), th.soft);
                let rr = Rect::from_min_size(pos2(hr.max.x - 96.0, hr.min.y + 3.0), vec2(96.0, 28.0));
                rescan = aero::aqua_button(ui, rr, "🔄 Rescan", th.accent, true).clicked();

                if self.dev.items.is_empty() {
                    Self::section(ui, "NO BUILD FOLDERS FOUND IN YOUR PROJECTS.", th);
                }
                for (i, item) in self.dev.items.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(vec2(row_w, 56.0), Sense::click());
                    let rect = rect.translate(vec2(4.0, 0.0));
                    if resp.clicked() {
                        toggle = Some(i);
                    }
                    let on = self.dev.selected[i];
                    let p = ui.painter();
                    let tint = on.then(|| if th.dark { th.go } else { aero::lighten(th.go, 0.75) });
                    aero::surface(p, rect, 14.0, th, resp.hovered(), tint);
                    let cy = rect.center().y;
                    aero::bubble_check(p, pos2(rect.min.x + 26.0, cy), 12.0, on, resp.hovered(), th.go, th.accent);
                    aero::orb(p, pos2(rect.min.x + 70.0, cy), 17.0, th.user_orb, item.kind.icon());
                    let project = item.path.parent().and_then(|d| d.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    let clip = p.with_clip_rect(Rect::from_min_max(rect.min, pos2(rect.max.x - 110.0, rect.max.y)));
                    let name_r = clip.text(pos2(rect.min.x + 98.0, cy - 9.0), Align2::LEFT_CENTER, &project, bold(15.0), th.text);
                    clip.text(pos2(name_r.max.x + 8.0, cy - 9.0), Align2::LEFT_CENTER, item.kind.label(), regular(12.0), th.soft);
                    let touched = match tools::age(item.project_touched).as_str() {
                        "" => String::new(),
                        a => format!("  ·  project changed {a}"),
                    };
                    clip.text(pos2(rect.min.x + 98.0, cy + 11.0), Align2::LEFT_CENTER, format!("{}{touched}", pretty(&item.path)), regular(12.0), th.soft);
                    embossed(p, pos2(rect.max.x - 18.0, cy), Align2::RIGHT_CENTER, &format_bytes(item.size), bold(16.0), th.text);
                    resp.on_hover_text(format!("Deleting this is safe: it's {}.", item.kind.rebuild_hint()));
                }

                if let Some(size) = self.dev.waydroid {
                    Self::section(ui, "NOT CLEANED HERE", th);
                    let (rect, _) = ui.allocate_exact_size(vec2(row_w, 56.0), Sense::hover());
                    let rect = rect.translate(vec2(4.0, 0.0));
                    let p = ui.painter();
                    aero::surface(p, rect, 14.0, th, false, None);
                    let cy = rect.center().y;
                    aero::orb(p, pos2(rect.min.x + 26.0, cy), 15.0, th.root_orb, "🤖");
                    p.text(pos2(rect.min.x + 52.0, cy - 9.0), Align2::LEFT_CENTER, "Waydroid Android image", bold(15.0), th.text);
                    p.text(
                        pos2(rect.min.x + 52.0, cy + 11.0),
                        Align2::LEFT_CENTER,
                        "Needed to run Android apps. If you don't use them: sudo apt purge waydroid && sudo rm -rf /var/lib/waydroid",
                        regular(12.0),
                        th.soft,
                    );
                    embossed(p, pos2(rect.max.x - 18.0, cy), Align2::RIGHT_CENTER, &format_bytes(size), bold(16.0), th.text);
                }
                ui.add_space(6.0);
            });
        });
        if let Some(i) = toggle {
            self.dev.selected[i] = !self.dev.selected[i];
            self.dev.confirm_until = None;
        }
        if rescan {
            self.dev.msg.clear();
            self.start_dev_scan();
        }
    }
}
