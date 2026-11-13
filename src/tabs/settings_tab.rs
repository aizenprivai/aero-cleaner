//! The settings tab.

use super::prelude::*;
use crate::automation::{self, AUTO_CLEAN, DISK_CHECK};

/// Cached state of the systemd timers (asking systemctl every frame would be slow).
#[derive(Default)]
pub(crate) struct AutoState {
    pub clean_on: bool,
    pub disk_on: bool,
    pub next_clean: Option<String>,
    pub checked: Option<f64>,
    pub running: bool,
}

/// Draws a toggle row (bubble checkbox, orb, title, description). Returns the response.
fn toggle_row(ui: &mut egui::Ui, w: f32, indent: f32, on: bool, icon: &str, title: &str, desc: &str, th: &Theme) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 60.0), Sense::click());
    let rect = Rect::from_min_max(rect.min + vec2(4.0 + indent, 0.0), rect.max + vec2(4.0, 0.0));
    let p = ui.painter();
    let tint = on.then(|| if th.dark { th.go } else { aero::lighten(th.go, 0.75) });
    aero::surface(p, rect, 14.0, th, resp.hovered(), tint);
    let cy = rect.center().y;
    aero::bubble_check(p, pos2(rect.min.x + 26.0, cy), 12.0, on, resp.hovered(), th.go, th.accent);
    aero::orb(p, pos2(rect.min.x + 70.0, cy), 17.0, th.user_orb, icon);
    p.text(pos2(rect.min.x + 98.0, cy - 9.0), Align2::LEFT_CENTER, title, bold(15.0), th.text);
    let clip = p.with_clip_rect(Rect::from_min_max(rect.min, pos2(rect.max.x - 120.0, rect.max.y)));
    clip.text(pos2(rect.min.x + 98.0, cy + 11.0), Align2::LEFT_CENTER, desc, regular(12.0), th.soft);
    resp
}

impl App {
    pub(crate) fn settings_tab(&mut self, ui: &mut egui::Ui, area: Rect) {
        let (list, _) = self.tab_frame(ui, area, "Settings", "Motion, automation, folders to keep, and your cleaning history.", None);
        let notifier = self.notifier();
        let time = ui.input(|i| i.time);
        if self.auto.checked.is_none_or(|t| time - t > 5.0) {
            self.auto.clean_on = automation::is_enabled(&AUTO_CLEAN);
            self.auto.disk_on = automation::is_enabled(&DISK_CHECK);
            self.auto.next_clean = automation::next_run(&AUTO_CLEAN);
            self.auto.checked = Some(time);
        }
        let (mut toggle_clean, mut toggle_trash, mut toggle_disk, mut cycle_pct, mut run_now) = (false, false, false, false, false);
        let home = std::env::var("HOME").unwrap_or_default();
        let pretty = |p: &std::path::Path| p.to_string_lossy().replacen(&home, "~", 1);
        let th = &self.themes[self.theme];
        let mut rescan = false;
        ui.scope_builder(egui::UiBuilder::new().max_rect(list.shrink(6.0)), |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let row_w = ui.available_width() - 14.0;

                // ----- Motion -----
                Self::section(ui, "MOTION", th);
                let (rect, resp) = ui.allocate_exact_size(vec2(row_w, 60.0), Sense::click());
                let rect = rect.translate(vec2(4.0, 0.0));
                if resp.clicked() {
                    self.settings.reduce_motion = !self.settings.reduce_motion;
                    self.settings.save();
                }
                let p = ui.painter();
                aero::surface(p, rect, 14.0, th, resp.hovered(), None);
                let cy = rect.center().y;
                aero::bubble_check(p, pos2(rect.min.x + 26.0, cy), 12.0, self.settings.reduce_motion, resp.hovered(), th.go, th.accent);
                aero::orb(p, pos2(rect.min.x + 70.0, cy), 17.0, th.user_orb, "⏸");
                p.text(pos2(rect.min.x + 98.0, cy - 9.0), Align2::LEFT_CENTER, "Reduce motion", bold(15.0), th.text);
                p.text(pos2(rect.min.x + 98.0, cy + 11.0), Align2::LEFT_CENTER, "Freeze the animated background to save battery. It already pauses when the window isn't active.", regular(12.0), th.soft);

                // ----- Automation -----
                Self::section(ui, "AUTOMATION", th);
                let resp = toggle_row(ui, row_w, 0.0, self.auto.clean_on, "🗓", "Weekly automatic clean",
                    "Thumbnails and app caches, once a week. Never asks for a password.", th);
                let resp = resp.on_hover_text("Browser caches are skipped if a browser is open at that moment. Folders to keep are respected.");
                toggle_clean = resp.clicked();
                if let Some(next) = &self.auto.next_clean {
                    let r = resp.rect;
                    ui.painter().text(pos2(r.max.x - 14.0, r.center().y), Align2::RIGHT_CENTER, format!("next: {}", next.split_whitespace().take(2).collect::<Vec<_>>().join(" ")), regular(12.0), th.soft);
                }
                toggle_trash = toggle_row(ui, row_w - 40.0, 40.0, self.settings.auto_trash, "🗑", "Also empty the Trash",
                    "Files you deleted more than a week ago can't be restored after this.", th).clicked();
                let pct = self.settings.low_disk_percent;
                let resp = toggle_row(ui, row_w, 0.0, self.auto.disk_on, "🔔", "Low disk space alert",
                    &format!("Checks every hour and notifies you when less than {pct}% of the disk is free (at most once a day)."), th);
                let pr = Rect::from_min_size(pos2(resp.rect.max.x - 82.0, resp.rect.center().y - 15.0), vec2(70.0, 30.0));
                cycle_pct = aero::aqua_button(ui, pr, &format!("{pct}%"), th.accent, true).on_hover_text("Change the threshold").clicked();
                toggle_disk = resp.clicked() && !cycle_pct;
                let (rect, _) = ui.allocate_exact_size(vec2(row_w, 44.0), Sense::hover());
                let rect = rect.translate(vec2(4.0, 0.0));
                let run_r = Rect::from_min_size(rect.min + vec2(0.0, 4.0), vec2(250.0, 36.0));
                let label = if self.auto.running { "Cleaning..." } else { "▶ Run automatic clean now" };
                run_now = aero::aqua_button(ui, run_r, label, th.go, !self.auto.running).clicked();
                ui.painter().text(pos2(run_r.max.x + 14.0, run_r.center().y), Align2::LEFT_CENTER,
                    "Tip: right-click AeroClean in the menu for Quick clean and Disk map.", regular(12.0), th.soft);

                // ----- Exclusions -----
                Self::section(ui, "FOLDERS TO KEEP  (the cleaner never touches these)", th);
                let mut remove = None;
                if self.settings.exclusions.is_empty() {
                    let (rect, _) = ui.allocate_exact_size(vec2(row_w, 40.0), Sense::hover());
                    let rect = rect.translate(vec2(4.0, 0.0));
                    aero::surface(ui.painter(), rect, 12.0, th, false, None);
                    ui.painter().text(rect.left_center() + vec2(16.0, 0.0), Align2::LEFT_CENTER, "Nothing excluded yet.", regular(13.0), th.soft);
                }
                for (i, x) in self.settings.exclusions.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(vec2(row_w, 44.0), Sense::hover());
                    let rect = rect.translate(vec2(4.0, 0.0));
                    aero::surface(ui.painter(), rect, 12.0, th, resp.hovered(), None);
                    let cy = rect.center().y;
                    aero::orb(ui.painter(), pos2(rect.min.x + 22.0, cy), 12.0, th.go, "🛡");
                    let missing = if x.exists() { "" } else { "   (folder not found)" };
                    ui.painter().text(pos2(rect.min.x + 44.0, cy), Align2::LEFT_CENTER, format!("{}{missing}", pretty(x)), bold(14.0), th.text);
                    let rm = Rect::from_min_size(pos2(rect.max.x - 104.0, cy - 14.0), vec2(92.0, 28.0));
                    if aero::aqua_button(ui, rm, "Remove", th.root_orb, true).clicked() {
                        remove = Some(i);
                    }
                }
                if let Some(i) = remove {
                    self.settings.exclusions.remove(i);
                    self.settings.save();
                    rescan = true;
                }

                let (rect, _) = ui.allocate_exact_size(vec2(row_w, 40.0), Sense::hover());
                let rect = rect.translate(vec2(4.0, 0.0));
                let browse_r = Rect::from_min_size(pos2(rect.max.x - 110.0, rect.min.y + 2.0), vec2(110.0, 36.0));
                let add_r = Rect::from_min_size(pos2(browse_r.min.x - 90.0, rect.min.y + 2.0), vec2(82.0, 36.0));
                let field = Rect::from_min_max(pos2(rect.min.x, rect.min.y + 4.0), pos2(add_r.min.x - 10.0, rect.max.y - 4.0));
                let edit = ui.put(field, egui::TextEdit::singleline(&mut self.excl_input).hint_text("~/path/to/folder").font(regular(14.0)));
                let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if aero::aqua_button(ui, add_r, "Add", th.accent, true).clicked() || enter {
                    match self.settings.add_exclusion(&self.excl_input) {
                        Ok(()) => {
                            self.settings_msg = format!("Keeping {}", self.excl_input.trim());
                            self.excl_input.clear();
                            rescan = true;
                        }
                        Err(e) => self.settings_msg = e,
                    }
                }
                if aero::aqua_button(ui, browse_r, "📁 Browse", th.accent, true).clicked() {
                    let tx = notifier.clone();
                    std::thread::spawn(move || {
                        let out = std::process::Command::new("zenity")
                            .args(["--file-selection", "--directory", "--title=Choose a folder to keep"])
                            .output();
                        let picked = out.ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
                        tx.send(Msg::Picked(picked));
                    });
                }

                let (rect, _) = ui.allocate_exact_size(vec2(row_w, 40.0), Sense::hover());
                let rect = rect.translate(vec2(4.0, 0.0));
                let keep_r = Rect::from_min_size(rect.min + vec2(0.0, 2.0), vec2(230.0, 36.0));
                if aero::aqua_button(ui, keep_r, "🌐 Keep browser caches", th.go, true).on_hover_text("Websites stay fast to load, but those caches won't be cleaned").clicked() {
                    let dirs = settings::browser_cache_dirs();
                    let added = dirs.iter().filter(|d| self.settings.add_exclusion(&d.to_string_lossy()).is_ok()).count();
                    self.settings_msg = if dirs.is_empty() { "No browser caches found.".into() } else { format!("Added {added} browser cache folder(s).") };
                    rescan |= added > 0;
                }
                if !self.settings_msg.is_empty() {
                    ui.painter().text(pos2(keep_r.max.x + 14.0, keep_r.center().y), Align2::LEFT_CENTER, &self.settings_msg, regular(13.0), th.soft);
                }

                // ----- History -----
                Self::section(ui, "CLEANING HISTORY", th);
                let total: u64 = self.history.iter().map(|h| h.freed).sum();
                let (rect, _) = ui.allocate_exact_size(vec2(row_w, 56.0), Sense::hover());
                let rect = rect.translate(vec2(4.0, 0.0));
                aero::surface(ui.painter(), rect, 14.0, th, false, Some(aero::lighten(th.go, 0.75)));
                aero::orb(ui.painter(), pos2(rect.min.x + 28.0, rect.center().y), 17.0, th.go, "✨");
                let amount_color = if th.dark { aero::lighten(th.go, 0.35) } else { aero::darken(th.go, 0.1) };
                embossed(ui.painter(), pos2(rect.min.x + 56.0, rect.center().y - 8.0), Align2::LEFT_CENTER, &format!("{} freed in total", format_bytes(total)), bold(18.0), amount_color);
                let n = self.history.len();
                ui.painter().text(pos2(rect.min.x + 56.0, rect.center().y + 13.0), Align2::LEFT_CENTER, format!("over {n} clean{}", if n == 1 { "" } else { "s" }), regular(12.0), th.soft);
                for h in self.history.iter().take(20) {
                    let (rect, resp) = ui.allocate_exact_size(vec2(row_w, 40.0), Sense::hover());
                    let rect = rect.translate(vec2(4.0, 0.0));
                    let p = ui.painter();
                    aero::surface(p, rect, 12.0, th, false, None);
                    let cy = rect.center().y;
                    p.text(pos2(rect.min.x + 16.0, cy), Align2::LEFT_CENTER, tools::age(Some(h.when)), bold(13.0), th.text);
                    let clip = p.with_clip_rect(Rect::from_min_max(pos2(rect.min.x + 120.0, rect.min.y), pos2(rect.max.x - 110.0, rect.max.y)));
                    clip.text(pos2(rect.min.x + 120.0, cy), Align2::LEFT_CENTER, &h.what, regular(12.0), th.soft);
                    embossed(p, pos2(rect.max.x - 16.0, cy), Align2::RIGHT_CENTER, &format_bytes(h.freed), bold(14.0), th.text);
                    resp.on_hover_text(&h.what);
                }
                ui.add_space(6.0);
            });
        });
        let result = |r: Result<(), String>| r.err().map(|e| format!("Could not change the timer: {e}"));
        if toggle_clean {
            let r = if self.auto.clean_on { automation::disable(&AUTO_CLEAN) } else { automation::enable(&AUTO_CLEAN) };
            if let Some(e) = result(r) {
                self.settings_msg = e;
            }
            self.auto.checked = None;
        }
        if toggle_disk {
            let r = if self.auto.disk_on { automation::disable(&DISK_CHECK) } else { automation::enable(&DISK_CHECK) };
            if let Some(e) = result(r) {
                self.settings_msg = e;
            }
            self.auto.checked = None;
        }
        if toggle_trash {
            self.settings.auto_trash = !self.settings.auto_trash;
            self.settings.save();
        }
        if cycle_pct {
            self.settings.low_disk_percent = match self.settings.low_disk_percent {
                5 => 10,
                10 => 15,
                15 => 20,
                _ => 5,
            };
            self.settings.save();
        }
        if run_now {
            self.auto.running = true;
            std::thread::spawn(move || {
                let summary = automation::auto_clean(&crate::settings::Settings::load());
                notifier.send(Msg::AutoDone(summary));
            });
        }
        if rescan && self.phase == Phase::Idle {
            self.start_scan();
        }
    }
}
