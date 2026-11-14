//! The boot time tab: last boot breakdown, boot history and slow services.

use super::prelude::*;
use crate::boot::{self, BootTimes, Service};

#[derive(Default)]
pub(crate) struct BootState {
    pub times: Option<BootTimes>,
    pub history: Vec<BootTimes>,
    pub services: Vec<Service>,
    pub loading: bool,
    pub loaded: bool,
    pub msg: String,
    /// Service being enabled/disabled right now (waiting for the password).
    pub busy_unit: Option<String>,
}

impl App {
    pub(crate) fn start_boot_load(&mut self) {
        self.boot.loading = true;
        let tx = self.notifier();
        std::thread::spawn(move || {
            let times = boot::last_boot();
            if let Some(t) = &times {
                boot::record(t);
            }
            tx.send(Msg::BootDone(times, boot::history(), boot::slowest(20)));
        });
    }

    pub(crate) fn boot_tab(&mut self, ui: &mut egui::Ui, area: Rect, spin: f32) {
        if !self.boot.loaded && !self.boot.loading {
            self.start_boot_load();
        }
        let (list, refresh) = self.tab_frame(
            ui,
            area,
            "Boot time",
            "How long your PC takes to start, and which services slow it down. Changes apply at the next boot.",
            Some(("🔄 Refresh", !self.boot.loading)),
        );
        if refresh {
            self.boot.msg.clear();
            self.start_boot_load();
        }
        let th = &self.themes[self.theme];
        let p = ui.painter().clone();
        if self.boot.loading && !self.boot.loaded {
            super::spinner(&p, list.center() - vec2(0.0, 20.0), spin, th);
            embossed(&p, list.center() + vec2(0.0, 18.0), Align2::CENTER_CENTER, "Reading boot data...", regular(15.0), th.text);
            return;
        }
        let Some(times) = self.boot.times.clone() else {
            embossed(&p, list.center(), Align2::CENTER_CENTER, "Boot data isn't available yet (the system may still be starting up).", regular(15.0), th.soft);
            return;
        };

        let stages = [
            ("Firmware", times.firmware, rgba(120, 140, 170, 255), "Your PC's BIOS/UEFI. Change it in the firmware setup (F2/Del at power-on)."),
            ("Boot menu", times.loader, rgba(150, 100, 210, 255), "GRUB's menu wait plus loading Linux."),
            ("Kernel", times.kernel, rgba(240, 150, 40, 255), "Linux starting up and finding your hardware."),
            ("Services", times.userspace, th.go, "Everything systemd starts until the desktop is ready."),
        ];
        let mut toggle: Option<(String, bool)> = None;
        ui.scope_builder(egui::UiBuilder::new().max_rect(list.shrink(6.0)), |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let row_w = ui.available_width() - 14.0;

                // ----- Last boot -----
                let (rect, _) = ui.allocate_exact_size(vec2(row_w, 150.0), Sense::hover());
                let rect = rect.translate(vec2(4.0, 0.0));
                let p = ui.painter();
                aero::surface(p, rect, 16.0, th, false, None);
                embossed(p, rect.min + vec2(18.0, 26.0), Align2::LEFT_CENTER, &format!("{:.1} s", times.total()), bold(30.0), th.text);
                p.text(rect.min + vec2(20.0, 52.0), Align2::LEFT_CENTER, crate::i18n::tr("last boot, from power button to desktop"), regular(12.5), th.soft);
                let bar = Rect::from_min_size(rect.min + vec2(18.0, 70.0), vec2(rect.width() - 36.0, 26.0));
                aero::rrect_grad(p, bar, 13.0, &[(0.0, rgba(0, 0, 0, 25)), (1.0, rgba(0, 0, 0, 10))]);
                let total = times.total().max(0.001);
                let mut x = bar.min.x;
                for (i, (name, secs, color, tip)) in stages.iter().enumerate() {
                    let w = bar.width() * secs / total;
                    if w < 1.0 {
                        continue;
                    }
                    let seg = Rect::from_min_size(pos2(x, bar.min.y), vec2(w, bar.height()));
                    let r = if i == 0 || i == stages.len() - 1 { 13.0f32.min(w / 2.0) } else { 3.0 };
                    aero::rrect_grad(p, seg.shrink2(vec2(1.0, 0.0)), r, &[(0.0, aero::lighten(*color, 0.4)), (0.5, *color), (1.0, aero::lighten(*color, 0.25))]);
                    let gloss = Rect::from_min_max(seg.min + vec2(3.0, 1.5), pos2(seg.max.x - 3.0, seg.center().y));
                    aero::rrect_grad(p, gloss, 4.0, &[(0.0, rgba(255, 255, 255, 170)), (1.0, rgba(255, 255, 255, 20))]);
                    if w > 46.0 {
                        aero::shadowed(&p.with_clip_rect(seg), seg.center(), Align2::CENTER_CENTER, &format!("{secs:.1}s"), bold(12.0), egui::Color32::WHITE);
                    }
                    ui.interact(seg, egui::Id::unique(("stage", i)), Sense::hover()).on_hover_text(format!("{}: {secs:.1} s\n{}", crate::i18n::t(name), crate::i18n::t(tip)));
                    x += w;
                }
                // Legend.
                let mut lx = bar.min.x;
                for (name, secs, color, _) in &stages {
                    let c = pos2(lx + 6.0, bar.max.y + 22.0);
                    p.circle_filled(c, 5.0, *color);
                    let r = p.text(c + vec2(10.0, 0.0), Align2::LEFT_CENTER, format!("{} {secs:.1}s", crate::i18n::t(name)), regular(12.5), th.text);
                    lx = r.max.x + 18.0;
                }

                // ----- History -----
                let hist: Vec<&BootTimes> = self.boot.history.iter().rev().take(12).collect::<Vec<_>>().into_iter().rev().collect();
                Self::section(ui, "BOOT HISTORY", th);
                let (rect, _) = ui.allocate_exact_size(vec2(row_w, 110.0), Sense::hover());
                let rect = rect.translate(vec2(4.0, 0.0));
                let p = ui.painter();
                aero::surface(p, rect, 16.0, th, false, None);
                if hist.len() < 2 {
                    p.text(rect.center(), Align2::CENTER_CENTER, crate::i18n::tr("Your boot history builds up each time you restart. Come back after your next reboot!"), regular(13.0), th.soft);
                } else {
                    let max = hist.iter().map(|b| b.total()).fold(1.0f32, f32::max);
                    let slot = (rect.width() - 30.0) / hist.len() as f32;
                    for (i, b) in hist.iter().enumerate() {
                        let h = (rect.height() - 44.0) * b.total() / max;
                        let col = Rect::from_min_max(pos2(rect.min.x + 15.0 + i as f32 * slot + slot * 0.2, rect.max.y - 22.0 - h), pos2(rect.min.x + 15.0 + (i as f32 + 0.8) * slot, rect.max.y - 22.0));
                        let c = if i == hist.len() - 1 { th.go } else { th.accent };
                        aero::rrect_grad(p, col, 5.0f32.min(col.width() / 2.0), &[(0.0, aero::lighten(c, 0.4)), (0.5, c), (1.0, aero::darken(c, 0.1))]);
                        p.text(pos2(col.center().x, col.min.y - 8.0), Align2::CENTER_CENTER, crate::i18n::tr(format!("{:.0}s", b.total())), bold(11.0), th.text);
                    }
                    p.text(pos2(rect.min.x + 15.0, rect.max.y - 10.0), Align2::LEFT_CENTER, crate::i18n::tr("older"), regular(11.0), th.soft);
                    p.text(pos2(rect.max.x - 15.0, rect.max.y - 10.0), Align2::RIGHT_CENTER, crate::i18n::tr("latest"), regular(11.0), th.soft);
                }

                // ----- Slow services -----
                let header = if self.boot.msg.is_empty() { "SLOWEST SERVICES AT LAST BOOT".to_string() } else { self.boot.msg.to_uppercase() };
                Self::section(ui, &header, th);
                for svc in &self.boot.services {
                    let (rect, resp) = ui.allocate_exact_size(vec2(row_w, 52.0), Sense::hover());
                    let rect = rect.translate(vec2(4.0, 0.0));
                    let p = ui.painter();
                    aero::surface(p, rect, 14.0, th, resp.hovered(), None);
                    let cy = rect.center().y;
                    let off = svc.state == "disabled";
                    aero::orb(p, pos2(rect.min.x + 24.0, cy), 15.0, if off { rgba(150, 160, 172, 255) } else { th.user_orb }, "⚙");
                    let clip = p.with_clip_rect(Rect::from_min_max(rect.min, pos2(rect.max.x - 210.0, rect.max.y)));
                    clip.text(pos2(rect.min.x + 48.0, cy - 9.0), Align2::LEFT_CENTER, crate::i18n::tr(svc.unit.trim_end_matches(".service")), bold(14.5), th.text);
                    clip.text(pos2(rect.min.x + 48.0, cy + 10.0), Align2::LEFT_CENTER, crate::i18n::tr(&svc.description), regular(12.0), th.soft);
                    embossed(p, pos2(rect.max.x - 140.0, cy), Align2::RIGHT_CENTER, &format!("{:.2}s", svc.seconds), bold(15.0), th.text);
                    let btn = Rect::from_min_size(pos2(rect.max.x - 124.0, cy - 14.0), vec2(112.0, 28.0));
                    let busy = self.boot.busy_unit.as_deref() == Some(svc.unit.as_str());
                    if svc.toggleable() {
                        let (label, color) = if busy { ("Waiting...", th.accent) } else if off { ("Turn on", th.accent) } else { ("Turn off", th.root_orb) };
                        let tip = if off { "Start this service at boot again" } else { "Stop starting this service at boot (asks for your password)" };
                        if aero::aqua_button(ui, btn, label, color, self.boot.busy_unit.is_none()).on_hover_text(crate::i18n::tr(tip)).clicked() {
                            toggle = Some((svc.unit.clone(), off));
                        }
                    } else {
                        let (text, tip) = if svc.critical {
                            ("needed", "Part of the system. Turning it off could break booting or the desktop.")
                        } else {
                            ("on demand", "Started by a timer, socket or another service, not directly at boot.")
                        };
                        let g = p.layout_no_wrap(crate::i18n::tr(text.to_string()), regular(11.5), th.soft);
                        let br = Rect::from_center_size(btn.center(), vec2(g.size().x + 18.0, 22.0));
                        aero::surface(p, br, 11.0, th, false, None);
                        p.galley(br.center() - g.size() / 2.0, g, th.soft);
                        ui.interact(br, egui::Id::unique(("svc-badge", &svc.unit)), Sense::hover()).on_hover_text(crate::i18n::tr(tip));
                    }
                }
                ui.add_space(6.0);
            });
        });

        if let Some((unit, enable)) = toggle {
            self.boot.busy_unit = Some(unit.clone());
            let tx = self.notifier();
            std::thread::spawn(move || {
                let res = boot::set_enabled(&unit, enable)
                    .map(|_| format!("{} will {}start at boot", unit.trim_end_matches(".service"), if enable { "" } else { "no longer " }));
                tx.send(Msg::BootToggled(res));
            });
        }
    }
}
