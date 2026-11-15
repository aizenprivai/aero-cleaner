//! One module per tab, plus widgets they share.

mod big_files;
mod boot_tab;
mod browsers_tab;
mod cleaner_tab;
mod dev_tab;
mod dupes_tab;
mod history_tab;
mod leftovers_tab;
mod map_tab;
mod prelude;
mod review;
mod rules_ui;
mod settings_tab;
mod startup;
mod themes_tab;

pub(crate) use boot_tab::BootState;
pub(crate) use browsers_tab::BrowserState;
pub(crate) use dev_tab::DevState;
pub(crate) use dupes_tab::DupeState;
pub(crate) use history_tab::HistoryState;
pub(crate) use leftovers_tab::LeftoverState;
pub(crate) use map_tab::MapState;
pub(crate) use rules_ui::{RuleEditor, tilde};
pub(crate) use settings_tab::AutoState;

use prelude::*;

/// Rotating dots shown while something works in the background.
pub(crate) fn spinner(p: &egui::Painter, c: egui::Pos2, spin: f32, th: &Theme) {
    for k in 0..8 {
        let a = spin * 5.0 + k as f32 * std::f32::consts::TAU / 8.0;
        p.circle_filled(c + Vec2::angled(a) * 16.0, 2.0 + k as f32 * 0.5, with_alpha(th.accent, (60 + k * 24) as u8));
    }
}

impl App {
    pub(crate) fn section(ui: &mut egui::Ui, title: &str, th: &Theme) {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
        embossed(ui.painter(), pos2(rect.min.x + 8.0, rect.center().y + 3.0), Align2::LEFT_CENTER, title, bold(14.0), th.soft);
    }

    /// Glass frame for a tab's list, with a header line and an optional button.
    /// Returns the list rect and whether the button was clicked.
    pub(crate) fn tab_frame(&self, ui: &mut egui::Ui, area: Rect, title: &str, info: &str, button: Option<(&str, bool)>) -> (Rect, bool) {
        let th = &self.themes[self.theme];
        let p = ui.painter().clone();
        embossed(&p, pos2(area.min.x + 6.0, area.min.y + 12.0), Align2::LEFT_CENTER, title, bold(17.0), th.text);
        // At most two lines above the list: shrink a little, then cut with "…".
        let info = crate::i18n::tr(info);
        let wrap = area.width() - 230.0;
        let mut size = 13.0;
        let info_g = loop {
            let mut job = egui::text::LayoutJob::simple(info.clone(), regular(size), th.soft, wrap);
            job.wrap.max_rows = 2;
            let g = p.layout_job(job);
            if !g.elided || size <= 11.5 {
                break g;
            }
            size -= 0.5;
        };
        p.galley(pos2(area.min.x + 6.0, area.min.y + 26.0), info_g, th.soft);
        let mut clicked = false;
        if let Some((label, enabled)) = button {
            let r = Rect::from_min_size(pos2(area.max.x - 200.0, area.min.y + 2.0), vec2(200.0, 44.0));
            clicked = aero::aqua_button(ui, r, label, th.accent, enabled).clicked();
        }
        let list = Rect::from_min_max(pos2(area.min.x, area.min.y + 60.0), area.max);
        aero::rrect_grad(&p, list, 16.0, &[(0.0, with_alpha(th.glass, 90)), (1.0, with_alpha(th.glass, 50))]);
        aero::rrect_outline(&p, list, 16.0, Stroke::new(1.0, with_alpha(th.accent, 90)));
        (list, clicked)
    }
}

/// Glossy bar showing how full the main disk is.
pub(crate) fn disk_meter(p: &egui::Painter, bar: Rect, size: u64, avail: u64, th: &Theme) {
    let used = 1.0 - avail as f32 / size.max(1) as f32;
    aero::surface(p, bar, 10.0, th, false, None);
    let color = if used > 0.9 {
        aero::rgba(230, 70, 50, 255)
    } else if used > 0.75 {
        aero::rgba(240, 160, 30, 255)
    } else {
        th.go
    };
    let fill = Rect::from_min_size(bar.min, vec2((bar.width() * used).max(bar.height()), bar.height()));
    aero::rrect_grad(p, fill, 10.0, &[(0.0, aero::lighten(color, 0.4)), (0.5, color), (1.0, aero::lighten(color, 0.3))]);
    let gloss = Rect::from_min_max(fill.min + vec2(4.0, 1.0), pos2(fill.max.x - 4.0, fill.center().y));
    aero::rrect_grad(p, gloss, 5.0, &[(0.0, rgba(255, 255, 255, 190)), (1.0, rgba(255, 255, 255, 20))]);
    let text = format!("💽 {} free of {}", format_bytes(avail), format_bytes(size));
    embossed(p, bar.center(), Align2::CENTER_CENTER, &text, bold(12.0), th.text);
}
