//! The browsers tab: cache, cookies and history per browser.

use super::prelude::*;
use crate::browsers::{self, Browser, Part};

#[derive(Default)]
pub(crate) struct BrowserState {
    pub list: Vec<Browser>,
    /// Which parts are ticked, per browser (same order as `Part::ALL`).
    pub chosen: Vec<[bool; 3]>,
    pub scanning: bool,
    pub loaded: bool,
    pub msg: String,
    /// Browser index and deadline of a pending "click again to confirm".
    pub confirm: Option<(usize, f64)>,
    /// Whether each browser is running, refreshed every couple of seconds.
    pub running: Vec<bool>,
    pub running_checked: f64,
}

impl App {
    pub(crate) fn start_browser_scan(&mut self) {
        self.browsers.scanning = true;
        let tx = self.notifier();
        std::thread::spawn(move || tx.send(Msg::BrowsersDone(browsers::detect())));
    }

    pub(crate) fn browsers_tab(&mut self, ui: &mut egui::Ui, area: Rect, spin: f32, time: f64) {
        if !self.browsers.loaded && !self.browsers.scanning {
            self.start_browser_scan();
        }
        let (list, refresh) = self.tab_frame(
            ui,
            area,
            "Browsers",
            "Clean each browser on its own terms: cache, cookies or history. The browser has to be closed while cleaning.",
            Some(("🔄 Refresh", !self.browsers.scanning)),
        );
        if refresh {
            self.browsers.msg.clear();
            self.start_browser_scan();
            return;
        }
        // Checking /proc every frame would be wasteful, so refresh every 2 s.
        if time - self.browsers.running_checked > 2.0 || self.browsers.running.len() != self.browsers.list.len() {
            self.browsers.running = self.browsers.list.iter().map(|b| browsers::is_running(b.process)).collect();
            self.browsers.running_checked = time;
        }
        let th = &self.themes[self.theme];
        let p = ui.painter().clone();
        if self.browsers.scanning {
            super::spinner(&p, list.center() - vec2(0.0, 20.0), spin, th);
            embossed(&p, list.center() + vec2(0.0, 18.0), Align2::CENTER_CENTER, "Looking for browsers...", regular(15.0), th.text);
            return;
        }
        if self.browsers.list.is_empty() {
            embossed(&p, list.center(), Align2::CENTER_CENTER, "No supported browser found (Brave, Chrome, Chromium, Vivaldi, Edge, Firefox).", regular(15.0), th.soft);
            return;
        }

        let mut toggle = None;
        let mut clean = None;
        ui.scope_builder(egui::UiBuilder::new().max_rect(list.shrink(6.0)), |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let row_w = ui.available_width() - 14.0;
                if !self.browsers.msg.is_empty() {
                    Self::section(ui, &self.browsers.msg.to_uppercase(), th);
                }
                for (bi, b) in self.browsers.list.iter().enumerate() {
                    let running = self.browsers.running.get(bi).copied().unwrap_or(false);
                    let profiles = crate::i18n::tr(format!("{} profile{}", b.profiles, if b.profiles == 1 { "" } else { "s" }));
                    let title = if running {
                        format!("{}  ·  {profiles}  ·  ⚠ OPEN, CLOSE IT TO CLEAN", b.name.to_uppercase())
                    } else {
                        format!("{}  ·  {profiles}", b.name.to_uppercase())
                    };
                    Self::section(ui, &title, th);
                    for (pi, part) in Part::ALL.iter().enumerate() {
                        let data = b.parts.iter().find(|(p, _)| p == part).and_then(|(_, d)| d.as_ref());
                        let (rect, resp) = ui.allocate_exact_size(vec2(row_w, 56.0), Sense::click());
                        let rect = rect.translate(vec2(4.0, 0.0));
                        let usable = data.is_some();
                        if resp.clicked() && usable {
                            toggle = Some((bi, pi));
                        }
                        let on = usable && self.browsers.chosen[bi][pi];
                        let hovered = resp.hovered() && usable;
                        let p = ui.painter();
                        let tint = on.then(|| if th.dark { th.go } else { aero::lighten(th.go, 0.75) });
                        aero::surface(p, rect, 14.0, th, hovered, tint);
                        let cy = rect.center().y;
                        aero::bubble_check(p, pos2(rect.min.x + 26.0, cy), 12.0, on, hovered, th.go, th.accent);
                        let (icon, hue) = match part {
                            Part::Cache => ("🗄", th.user_orb),
                            Part::Cookies => ("🍪", th.root_orb),
                            Part::History => ("🕘", th.accent),
                        };
                        let grey = rgba(150, 160, 172, 255);
                        aero::orb(p, pos2(rect.min.x + 70.0, cy), 17.0, if usable { hue } else { grey }, icon);
                        p.text(pos2(rect.min.x + 98.0, cy - 9.0), Align2::LEFT_CENTER, crate::i18n::tr(part.label()), bold(15.0), if usable { th.text } else { grey });
                        let desc = if usable { part.desc() } else { "Not supported for this browser." };
                        aero::fit_text(p, pos2(rect.min.x + 98.0, cy + 11.0), Align2::LEFT_CENTER, &crate::i18n::tr(desc), regular(12.0), th.soft, rect.width() - 98.0 - 120.0);
                        if let Some((_, size)) = data {
                            embossed(p, pos2(rect.max.x - 18.0, cy), Align2::RIGHT_CENTER, &format_bytes(*size), bold(16.0), th.text);
                        }
                    }
                    let picked: Vec<Part> = Part::ALL.iter().zip(self.browsers.chosen[bi]).filter(|(_, on)| *on).map(|(p, _)| *p).collect();
                    let risky = picked.iter().any(|p| *p != Part::Cache);
                    let confirming = self.browsers.confirm.is_some_and(|(i, t)| i == bi && time < t);
                    let (rect, _) = ui.allocate_exact_size(vec2(row_w, 50.0), Sense::hover());
                    let btn = Rect::from_min_size(pos2(rect.max.x - 236.0, rect.min.y + 4.0), vec2(240.0, 42.0));
                    let warn = if picked.contains(&Part::Cookies) { "Click again: signs you out" } else { "Click again: erases history" };
                    let label = if confirming { warn.to_string() } else { format!("✨ Clean {}", b.name) };
                    if aero::aqua_button(ui, btn, &label, th.go, !running && !picked.is_empty()).clicked() {
                        clean = Some((bi, picked, risky, confirming));
                    }
                    ui.add_space(4.0);
                }
                ui.add_space(6.0);
            });
        });

        if let Some((bi, pi)) = toggle {
            self.browsers.chosen[bi][pi] = !self.browsers.chosen[bi][pi];
            self.browsers.confirm = None;
        }
        if let Some((bi, parts, risky, confirming)) = clean {
            // Cookies and history need a second click, since they sign you out / erase history.
            if risky && !confirming {
                self.browsers.confirm = Some((bi, time + 4.0));
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
                return;
            }
            self.browsers.confirm = None;
            let b = &self.browsers.list[bi];
            let names: Vec<&str> = parts.iter().map(|p| crate::i18n::t(p.label())).collect();
            self.browsers.msg = match browsers::clean(b, &parts) {
                Ok(0) => format!("{} cleaned: {}", b.name, names.join(", ")),
                Ok(n) => format!("{} cleaned, {n} item(s) skipped", b.name),
                Err(e) => e,
            };
            self.start_browser_scan();
            if self.phase == Phase::Idle {
                self.start_scan();
            }
        }
        if self.browsers.confirm.is_some_and(|(_, t)| time < t) {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
        }
    }
}
