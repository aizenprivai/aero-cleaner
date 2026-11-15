//! The Overview: a health score for the PC, one card per area, and
//! "Fix all safe items" (which still goes through the review sheet).

use super::prelude::*;
use crate::Tab;
use crate::overview::{self, Inputs};

impl App {
    fn overview_inputs(&self) -> Inputs {
        let drives = self.disk_hist.health.as_deref().unwrap_or(&[]);
        Inputs {
            disk: self.disk,
            junk: self.total_of(&crate::cleaner::safe_kinds()),
            drives_ok: self.disk_hist.health.as_ref().map(|_| drives.iter().all(|d| d.healthy != Some(false))),
            worst_wear: drives.iter().filter_map(|d| d.wear_percent).max(),
            boot_secs: self.boot.times.as_ref().map(|t| t.total()),
            update_available: self.update.is_some(),
        }
    }

    pub(crate) fn overview_tab(&mut self, ui: &mut egui::Ui, area: Rect) {
        // Gather what the cards need, once.
        if !self.disk_hist.loaded {
            self.load_disk_history();
        }
        if (crate::platform::IS_LINUX || crate::platform::IS_WINDOWS) && !self.boot.loaded && !self.boot.loading {
            self.start_boot_load();
        }
        let (list, _) = self.tab_frame(ui, area, "Overview", "Your PC at a glance: how healthy it is, and what can be improved.", None);
        let inputs = self.overview_inputs();
        let parts = overview::parts(&inputs);
        let score = overview::score(&inputs);
        let th = &self.themes[self.theme];
        let p = ui.painter().clone();
        let inner = list.shrink(14.0);

        // ----- Score card (left) -----
        let card = Rect::from_min_size(inner.min, vec2((inner.width() * 0.38).max(250.0), inner.height()));
        aero::surface(&p, card, 18.0, th, false, None);
        let c = pos2(card.center().x, card.min.y + card.height() * 0.38);
        let r = (card.width() * 0.32).min(card.height() * 0.24).max(60.0);
        let color = match score {
            90.. => th.go,
            75..=89 => rgba(110, 190, 60, 255),
            50..=74 => rgba(240, 160, 30, 255),
            _ => rgba(220, 70, 50, 255),
        };
        // Glossy ring: grey track, colored arc from the top, clockwise.
        let ring = |from: f32, to: f32| -> Vec<egui::Pos2> {
            let steps = 96;
            (0..=steps).map(|k| from + (to - from) * k as f32 / steps as f32).map(|a| c + Vec2::angled(a - std::f32::consts::FRAC_PI_2) * r).collect()
        };
        p.add(egui::Shape::line(ring(0.0, std::f32::consts::TAU), Stroke::new(18.0, with_alpha(th.soft, 50))));
        let end = std::f32::consts::TAU * score as f32 / 100.0;
        if score > 0 {
            p.add(egui::Shape::line(ring(0.0, end), Stroke::new(18.0, color)));
            p.add(egui::Shape::line(ring(0.0, end).iter().map(|q| c + (*q - c) * 1.06).collect(), Stroke::new(3.0, rgba(255, 255, 255, 150))));
        }
        embossed(&p, c - vec2(0.0, 8.0), Align2::CENTER_CENTER, &score.to_string(), bold(52.0), th.text);
        p.text(c + vec2(0.0, 30.0), Align2::CENTER_CENTER, crate::i18n::tr("out of 100"), regular(13.0), th.soft);
        embossed(&p, pos2(c.x, c.y + r + 34.0), Align2::CENTER_CENTER, &crate::i18n::tr(overview::verdict(score)), bold(20.0), color);

        let safe = crate::cleaner::safe_kinds();
        let junk = self.total_of(&safe);
        let fix_r = Rect::from_center_size(pos2(c.x, card.max.y - 70.0), vec2((card.width() - 40.0).min(260.0), 50.0));
        let idle = self.phase == Phase::Idle;
        let fix = aero::aqua_button(ui, fix_r, "✨ Fix all safe items", th.go, idle && junk > 0)
            .on_hover_text(crate::i18n::tr("Cleans caches and thumbnails apps re-create, after showing you the list. Nothing that needs a password, nothing you can't undo."))
            .clicked();
        let note = if !idle {
            crate::i18n::tr("Scanning...")
        } else if junk > 0 {
            crate::i18n::tr(format!("Frees about {}", format_bytes(junk)))
        } else {
            crate::i18n::tr("Nothing to fix right now")
        };
        p.text(pos2(c.x, fix_r.max.y + 16.0), Align2::CENTER_CENTER, note, regular(12.5), th.soft);

        // ----- One card per area (right) -----
        let col = Rect::from_min_max(pos2(card.max.x + 14.0, inner.min.y), inner.max);
        let h = ((col.height() - 4.0 * 10.0) / 5.0).min(96.0);
        let now = crate::diskhistory::now();
        let mut go_to: Option<Tab> = None;
        for (k, part) in parts.iter().enumerate() {
            let row = Rect::from_min_size(pos2(col.min.x, col.min.y + k as f32 * (h + 10.0)), vec2(col.width(), h));
            aero::surface(&p, row, 14.0, th, false, None);
            let cy = row.center().y;
            let (icon, detail, target) = match k {
                0 => {
                    let free = inputs.disk.map(|(t, a)| crate::i18n::tr(format!("{} free of {}", format_bytes(a), format_bytes(t)))).unwrap_or_default();
                    let fc = crate::diskhistory::forecast_sentence(&crate::diskhistory::forecast(&self.disk_hist.samples, now)).map(crate::i18n::tr);
                    ("📈", fc.map(|f| format!("{free}  ·  {f}")).unwrap_or(free), Tab::DiskHistory)
                }
                1 => ("🗑", crate::i18n::tr(format!("{} of caches and thumbnails ready to clean", format_bytes(junk))), Tab::Cleaner),
                2 => {
                    let d = match &self.disk_hist.health {
                        None => crate::i18n::tr("Checking your drives..."),
                        Some(v) if v.is_empty() => crate::i18n::tr("This computer doesn't share drive health information."),
                        Some(_) if inputs.drives_ok == Some(false) => crate::i18n::tr("A drive reports a problem. Back up your files soon."),
                        Some(_) => match inputs.worst_wear {
                            Some(w) => crate::i18n::tr(format!("All drives healthy  ·  {w}% worn")),
                            None => crate::i18n::tr("All drives healthy"),
                        },
                    };
                    ("💽", d, Tab::DiskHistory)
                }
                3 => {
                    let d = match &self.boot.times {
                        Some(t) => crate::i18n::tr(format!("Last boot took {:.1} s", t.total())),
                        None if crate::platform::IS_MACOS => crate::i18n::tr("Not measured on macOS yet"),
                        None => crate::i18n::tr("Boot data isn't available yet"),
                    };
                    ("⏱", d, Tab::Boot)
                }
                _ => {
                    let latest = self.updater.release.as_ref().is_some_and(|r| !r.is_newer());
                    let d = match &self.update {
                        Some(u) => crate::i18n::tr(format!("Version {} is out", u.version)),
                        None if latest => crate::i18n::tr(format!("AeroClean {} is the latest version", crate::update::CURRENT)),
                        None => crate::i18n::tr(format!("AeroClean {}  ·  not checked yet", crate::update::CURRENT)),
                    };
                    ("🔄", d, Tab::Settings)
                }
            };
            let full = part.points >= part.max - 0.5;
            aero::orb(&p, pos2(row.min.x + 30.0, cy), 18.0, if full { th.go } else { color }, icon);
            let text_w = row.width() - 60.0 - 150.0;
            aero::fit_text(&p, pos2(row.min.x + 60.0, cy - 11.0), Align2::LEFT_CENTER, &crate::i18n::tr(part.label), bold(15.0), th.text, text_w);
            aero::fit_text(&p, pos2(row.min.x + 60.0, cy + 11.0), Align2::LEFT_CENTER, &detail, regular(12.5), th.soft, text_w);
            // Points as a small glossy bar.
            let bar = Rect::from_min_size(pos2(row.max.x - 140.0, cy - 20.0), vec2(80.0, 10.0));
            aero::surface(&p, bar, 5.0, th, false, None);
            let fill = Rect::from_min_size(bar.min, vec2((bar.width() * part.points / part.max).max(bar.height()), bar.height()));
            aero::rrect_grad(&p, fill, 5.0, &[(0.0, aero::lighten(if full { th.go } else { color }, 0.4)), (1.0, if full { th.go } else { color })]);
            p.text(pos2(bar.center().x, bar.max.y + 10.0), Align2::CENTER_CENTER, format!("{:.0} / {:.0}", part.points, part.max), regular(11.0), th.soft);
            let open = Rect::from_min_size(pos2(row.max.x - 52.0, cy - 16.0), vec2(40.0, 32.0));
            // Each card gets its own id: the buttons sit side by side with nothing allocated between.
            if ui.push_id(("overview-open", k), |ui| aero::aqua_button(ui, open, "›", th.accent, true).on_hover_text(crate::i18n::tr("Open")).clicked()).inner {
                go_to = Some(target);
            }
        }
        if fix {
            self.open_review_for(safe, vec![]);
        }
        if let Some(t) = go_to {
            self.tab = t;
        }
    }
}
