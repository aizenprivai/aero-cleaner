//! The disk history tab: free space over time with a forecast, the folders
//! that grew lately, and the health of your drives.

use super::prelude::*;
use crate::automation::{self, DISK_CHECK};
use crate::diskhistory::{self, Forecast, Sample};
use crate::health::DiskHealth;

#[derive(Default)]
pub(crate) struct HistoryState {
    pub loaded: bool,
    pub samples: Vec<Sample>,
    /// Folders that grew, and how many days apart the two snapshots are.
    pub growth: Option<(Vec<(String, i64)>, u64)>,
    pub health: Option<Vec<DiskHealth>>,
    pub health_loading: bool,
    /// Whether the hourly job runs (it records history while the app is closed).
    pub hourly_on: bool,
}

const DAY: u64 = 86400;

impl App {
    pub(crate) fn load_disk_history(&mut self) {
        self.refresh_disk();
        let h = &mut self.disk_hist;
        h.samples = diskhistory::load();
        h.growth = diskhistory::snapshots_to_compare().map(|(new, old)| (diskhistory::growth(&new, &old, 8), new.day - old.day));
        h.hourly_on = automation::is_enabled(&DISK_CHECK);
        h.loaded = true;
        if !h.health_loading {
            h.health_loading = true;
            let tx = self.notifier();
            std::thread::spawn(move || tx.send(Msg::HealthDone(crate::health::read())));
        }
    }

    pub(crate) fn history_tab(&mut self, ui: &mut egui::Ui, area: Rect, spin: f32) {
        if !self.disk_hist.loaded {
            self.load_disk_history();
        }
        let (list, refresh) = self.tab_frame(
            ui,
            area,
            "Disk history",
            "How your free space changes over time, when the disk may fill up, what grew lately, and how healthy your drives are.",
            Some(("🔄 Refresh", !self.disk_hist.health_loading)),
        );
        if refresh {
            self.load_disk_history();
        }
        let th = &self.themes[self.theme];
        let now = diskhistory::now();
        let forecast = diskhistory::forecast(&self.disk_hist.samples, now);
        let home = crate::platform::home();
        let mut turn_on_hourly = false;
        let mut open: Option<std::path::PathBuf> = None;
        ui.scope_builder(egui::UiBuilder::new().max_rect(list.shrink(6.0)), |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let row_w = ui.available_width() - 14.0;

                // ----- Free space chart -----
                let (rect, resp) = ui.allocate_exact_size(vec2(row_w, 270.0), Sense::hover());
                let rect = rect.translate(vec2(4.0, 0.0));
                let p = ui.painter();
                aero::surface(p, rect, 16.0, th, false, None);
                if let Some((total, avail)) = self.disk {
                    embossed(p, rect.min + vec2(18.0, 26.0), Align2::LEFT_CENTER, &format!("{} free of {}", format_bytes(avail), format_bytes(total)), bold(24.0), th.text);
                }
                let sentence = match &forecast {
                    Forecast::FullIn { per_day, .. } => format!(
                        "{} {}",
                        crate::i18n::tr(diskhistory::forecast_sentence(&forecast).unwrap_or_default()),
                        crate::i18n::tr(format!("You lose about {} a day.", format_bytes(*per_day)))
                    ),
                    Forecast::NotFilling => crate::i18n::tr("Your free space is steady. 👍"),
                    Forecast::NotEnoughData => crate::i18n::tr("AeroClean needs a few days of history before it can predict anything."),
                };
                let warn = matches!(forecast, Forecast::FullIn { days, .. } if days < 60.0);
                p.text(rect.min + vec2(20.0, 54.0), Align2::LEFT_CENTER, sentence, regular(13.5), if warn { aero::rgba(220, 70, 50, 255) } else { th.soft });
                let chart = Rect::from_min_max(rect.min + vec2(70.0, 78.0), rect.max - vec2(20.0, 30.0));
                let low_pct = self.settings.low_disk_percent as u64;
                draw_chart(ui, chart, &self.disk_hist.samples, now, low_pct, resp.hover_pos(), th);

                // ----- Hourly recording -----
                if !self.disk_hist.hourly_on {
                    let (rect, _) = ui.allocate_exact_size(vec2(row_w, 56.0), Sense::hover());
                    let rect = rect.translate(vec2(4.0, 0.0));
                    let p = ui.painter();
                    aero::surface(p, rect, 14.0, th, false, None);
                    let cy = rect.center().y;
                    aero::orb(p, pos2(rect.min.x + 26.0, cy), 15.0, th.user_orb, "⏰");
                    p.text(pos2(rect.min.x + 52.0, cy - 9.0), Align2::LEFT_CENTER, crate::i18n::tr("Record every hour, even when AeroClean is closed"), bold(14.5), th.text);
                    p.text(
                        pos2(rect.min.x + 52.0, cy + 11.0),
                        Align2::LEFT_CENTER,
                        crate::i18n::tr("Right now history is only saved while the app is open. This also turns on the low disk space alert."),
                        regular(12.0),
                        th.soft,
                    );
                    let btn = Rect::from_min_size(pos2(rect.max.x - 124.0, cy - 15.0), vec2(112.0, 30.0));
                    turn_on_hourly = aero::aqua_button(ui, btn, "Turn on", th.go, true).clicked();
                }

                // ----- What grew -----
                match &self.disk_hist.growth {
                    None => {
                        Self::section(ui, "WHAT GREW LATELY", th);
                        note(ui, row_w, "AeroClean compares daily snapshots of your folders. Come back in a few days to see what grew.", th);
                    }
                    Some((folders, days)) => {
                        Self::section(ui, &format!("WHAT GREW IN THE LAST {days} DAYS"), th);
                        if folders.is_empty() {
                            note(ui, row_w, "No folder grew by more than 50 MB. 👍", th);
                        }
                        for (rel, delta) in folders {
                            let (rect, resp) = ui.allocate_exact_size(vec2(row_w, 50.0), Sense::hover());
                            let rect = rect.translate(vec2(4.0, 0.0));
                            let p = ui.painter();
                            aero::surface(p, rect, 14.0, th, resp.hovered(), None);
                            let cy = rect.center().y;
                            aero::orb(p, pos2(rect.min.x + 26.0, cy), 15.0, th.accent, "📁");
                            let clip = p.with_clip_rect(Rect::from_min_max(rect.min, pos2(rect.max.x - 230.0, rect.max.y)));
                            clip.text(pos2(rect.min.x + 52.0, cy), Align2::LEFT_CENTER, format!("~/{rel}"), bold(14.5), th.text);
                            embossed(p, pos2(rect.max.x - 112.0, cy), Align2::RIGHT_CENTER, &format!("+{}", format_bytes(*delta as u64)), bold(15.0), th.text);
                            let btn = Rect::from_min_size(pos2(rect.max.x - 100.0, cy - 14.0), vec2(88.0, 28.0));
                            if aero::aqua_button(ui, btn, "📂 Open", th.accent, true).clicked() {
                                open = Some(home.join(rel));
                            }
                        }
                    }
                }

                // ----- Health -----
                Self::section(ui, "DISK HEALTH", th);
                match &self.disk_hist.health {
                    None => {
                        let (rect, _) = ui.allocate_exact_size(vec2(row_w, 56.0), Sense::hover());
                        super::spinner(ui.painter(), pos2(rect.min.x + 30.0, rect.center().y), spin, th);
                        ui.painter().text(pos2(rect.min.x + 60.0, rect.center().y), Align2::LEFT_CENTER, crate::i18n::tr("Checking your drives..."), regular(13.0), th.soft);
                    }
                    Some(drives) if drives.is_empty() => note(ui, row_w, "This computer doesn't share drive health information.", th),
                    Some(drives) => {
                        for d in drives {
                            health_row(ui, row_w, d, th);
                        }
                    }
                }
                ui.add_space(6.0);
            });
        });
        if turn_on_hourly {
            match automation::enable(&DISK_CHECK) {
                Ok(()) => self.disk_hist.hourly_on = true,
                Err(e) => self.status = format!("Could not change the timer: {e}"),
            }
            self.auto.checked = None;
        }
        if let Some(dir) = open {
            crate::platform::open_dir(&dir);
        }
    }
}

/// A plain message row; long text wraps onto more lines.
fn note(ui: &mut egui::Ui, w: f32, text: &str, th: &Theme) {
    let g = ui.painter().layout(crate::i18n::tr(text), regular(13.0), th.soft, w - 36.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, (g.size().y + 26.0).max(44.0)), Sense::hover());
    let rect = rect.translate(vec2(4.0, 0.0));
    aero::surface(ui.painter(), rect, 14.0, th, false, None);
    ui.painter().galley(pos2(rect.min.x + 18.0, rect.center().y - g.size().y / 2.0), g, th.soft);
}

fn health_row(ui: &mut egui::Ui, w: f32, d: &DiskHealth, th: &Theme) {
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 60.0), Sense::hover());
    let rect = rect.translate(vec2(4.0, 0.0));
    let p = ui.painter();
    let bad = d.healthy == Some(false);
    let tint = bad.then(|| aero::rgba(255, 190, 180, 255));
    aero::surface(p, rect, 14.0, th, resp.hovered(), tint);
    let cy = rect.center().y;
    let (icon, color) = match d.healthy {
        Some(true) => ("✔", th.go),
        Some(false) => ("⚠", aero::rgba(220, 70, 50, 255)),
        None => ("?", aero::rgba(150, 160, 172, 255)),
    };
    aero::orb(p, pos2(rect.min.x + 28.0, cy), 17.0, color, icon);
    let title = if d.size > 0 { format!("{}  ·  {}", d.model, format_bytes(d.size)) } else { d.model.clone() };
    p.text(pos2(rect.min.x + 56.0, cy - 10.0), Align2::LEFT_CENTER, &title, bold(15.0), th.text);
    // Translate the reason first, then the sentence around it.
    let problem = crate::i18n::tr(&d.problem);
    let status = match (d.healthy, d.problem.is_empty()) {
        (Some(true), true) => crate::i18n::tr("Healthy"),
        (Some(true), false) => crate::i18n::tr(format!("Healthy, but {problem}")),
        (Some(false), _) => crate::i18n::tr(format!("Problem: {problem}. Back up your files soon.")),
        (None, _) => crate::i18n::tr("Health unknown"),
    };
    let mut details = vec![status];
    // With a gauge on the right, the wear % is written on the gauge instead.
    if let Some(t) = d.temperature_c {
        details.push(format!("{t:.0} °C"));
    }
    if let Some(h) = d.power_on_hours {
        details.push(crate::i18n::tr(format!("{h} hours on")));
    }
    if let Some(b) = d.bytes_written {
        details.push(crate::i18n::tr(format!("{} written", format_bytes(b))));
    }
    let text_end = if d.wear_percent.is_some() { rect.max.x - 166.0 } else { rect.max.x - 12.0 };
    let clip = p.with_clip_rect(Rect::from_min_max(rect.min, pos2(text_end, rect.max.y)));
    clip.text(pos2(rect.min.x + 56.0, cy + 11.0), Align2::LEFT_CENTER, details.join("  ·  "), regular(12.0), if bad { aero::rgba(190, 40, 30, 255) } else { th.soft });
    if let Some(wear) = d.wear_percent {
        // A small glossy wear gauge on the right.
        let bar = Rect::from_min_size(pos2(rect.max.x - 150.0, cy + 2.0), vec2(130.0, 12.0));
        aero::surface(p, bar, 7.0, th, false, None);
        let c = if wear >= 90 {
            aero::rgba(220, 70, 50, 255)
        } else if wear >= 70 {
            aero::rgba(240, 160, 30, 255)
        } else {
            th.go
        };
        let fill = Rect::from_min_size(bar.min, vec2((bar.width() * (wear.min(100) as f32 / 100.0)).max(bar.height()), bar.height()));
        aero::rrect_grad(p, fill, 7.0, &[(0.0, aero::lighten(c, 0.4)), (1.0, c)]);
        p.text(pos2(bar.center().x, bar.min.y - 9.0), Align2::CENTER_CENTER, crate::i18n::tr(format!("{wear}% worn")), bold(11.5), th.text);
        resp.on_hover_text(crate::i18n::tr(
            "Wear: how much of the SSD's rated writes are used. 100% means it reached its rating; drives usually keep working past it, but keep backups.",
        ));
    }
}

/// Glossy area chart of free space over the last 90 days.
fn draw_chart(ui: &egui::Ui, chart: Rect, samples: &[Sample], now: u64, low_pct: u64, hover: Option<egui::Pos2>, th: &Theme) {
    let p = ui.painter();
    let pts: Vec<&Sample> = samples.iter().filter(|s| now.saturating_sub(s.when) <= 90 * DAY).collect();
    if pts.len() < 2 {
        p.text(chart.center(), Align2::CENTER_CENTER, crate::i18n::tr("Your disk history starts today. The chart fills in as days go by."), regular(13.0), th.soft);
        return;
    }
    let start = pts[0].when;
    let span = (now.max(pts[pts.len() - 1].when) - start).max(3600) as f32;
    let total = pts.iter().map(|s| s.total).max().unwrap_or(1).max(1);
    let max_avail = pts.iter().map(|s| s.avail).max().unwrap_or(0);
    // Leave headroom above the highest point, but never above the disk size.
    let top = ((max_avail as f64 * 1.25) as u64).clamp(1, total);
    let x = |t: u64| chart.min.x + chart.width() * (t - start) as f32 / span;
    let y = |v: u64| chart.max.y - chart.height() * (v.min(top) as f32 / top as f32);

    // Grid and labels.
    for k in 0..=3u64 {
        let v = top * k / 3;
        let gy = y(v);
        p.line_segment([pos2(chart.min.x, gy), pos2(chart.max.x, gy)], Stroke::new(1.0, with_alpha(th.soft, 40)));
        p.text(pos2(chart.min.x - 8.0, gy), Align2::RIGHT_CENTER, format_bytes(v), regular(11.0), th.soft);
    }
    let line: Vec<egui::Pos2> = pts.iter().map(|s| pos2(x(s.when), y(s.avail))).collect();

    // Area under the line, as a gradient mesh.
    let mut mesh = egui::epaint::Mesh::default();
    let top_c = with_alpha(th.go, 150);
    let bottom_c = with_alpha(th.go, 25);
    for (i, pt) in line.iter().enumerate() {
        mesh.colored_vertex(*pt, top_c);
        mesh.colored_vertex(pos2(pt.x, chart.max.y), bottom_c);
        if i > 0 {
            let b = (i as u32) * 2;
            mesh.add_triangle(b - 2, b - 1, b);
            mesh.add_triangle(b - 1, b + 1, b);
        }
    }
    p.add(egui::Shape::mesh(mesh));
    p.add(egui::Shape::line(line.clone(), Stroke::new(2.5, aero::darken(th.go, 0.15))));
    // A white gloss line just above, for the Aero shine.
    p.add(egui::Shape::line(line.iter().map(|pt| *pt - vec2(0.0, 1.5)).collect(), Stroke::new(1.0, rgba(255, 255, 255, 150))));

    // Low disk alert level.
    let low = total * low_pct / 100;
    if low <= top {
        let ly = y(low);
        p.add(egui::Shape::dashed_line(&[pos2(chart.min.x, ly), pos2(chart.max.x, ly)], Stroke::new(1.5, aero::rgba(220, 70, 50, 200)), 6.0, 4.0));
        p.text(pos2(chart.min.x + 6.0, ly - 9.0), Align2::LEFT_CENTER, crate::i18n::tr("alert level"), bold(11.0), aero::rgba(220, 70, 50, 255));
    }
    let left = crate::tools::age(Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(start)));
    p.text(pos2(chart.min.x, chart.max.y + 12.0), Align2::LEFT_CENTER, left, regular(11.0), th.soft);
    p.text(pos2(chart.max.x, chart.max.y + 12.0), Align2::RIGHT_CENTER, crate::i18n::tr("now"), regular(11.0), th.soft);

    // Hover: the closest sample.
    if let Some(h) = hover.filter(|h| chart.expand(4.0).contains(*h)) {
        let (i, pt) = line.iter().enumerate().min_by(|a, b| (a.1.x - h.x).abs().total_cmp(&(b.1.x - h.x).abs())).unwrap();
        p.line_segment([pos2(pt.x, chart.min.y), pos2(pt.x, chart.max.y)], Stroke::new(1.0, with_alpha(th.text, 90)));
        p.circle(*pt, 5.0, rgba(255, 255, 255, 255), Stroke::new(2.0, aero::darken(th.go, 0.15)));
        let when = crate::tools::age(Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(pts[i].when)));
        let label = crate::i18n::tr(format!("{when}: {} free", format_bytes(pts[i].avail)));
        let g = p.layout_no_wrap(label, bold(12.0), th.text);
        let bx = Rect::from_min_size(pos2((pt.x + 10.0).min(chart.max.x - g.size().x - 16.0), chart.min.y), g.size() + vec2(16.0, 10.0));
        aero::surface(p, bx, 8.0, th, true, None);
        p.galley(bx.min + vec2(8.0, 5.0), g, th.text);
    }
}
