//! The "UPDATES" part of Settings: check now, what's new, and Update now.

use super::prelude::*;
use crate::update::{self, Release};

/// What the user clicked in the updater rows.
pub(crate) enum UpdaterAction {
    Check,
    Start(Release),
    OpenPage(String),
}

#[derive(Default)]
pub(crate) struct UpdaterState {
    pub checking: bool,
    pub release: Option<Release>,
    pub error: String,
    /// Bytes received and expected while downloading.
    pub progress: Option<(u64, u64)>,
    pub installing: bool,
    pub done: String,
}

impl App {
    pub(crate) fn check_for_update_now(&mut self) {
        if self.updater.checking {
            return;
        }
        self.updater.checking = true;
        self.updater.error.clear();
        self.updater.done.clear();
        let tx = self.notifier();
        std::thread::spawn(move || tx.send(Msg::Release(update::latest_release())));
    }

    pub(crate) fn run_updater_action(&mut self, action: UpdaterAction) {
        match action {
            UpdaterAction::Check => self.check_for_update_now(),
            UpdaterAction::Start(r) => self.start_update(r),
            UpdaterAction::OpenPage(url) => update::open(&url),
        }
    }

    fn start_update(&mut self, release: Release) {
        let Some(kind) = update::installer() else { return };
        let Some(asset) = update::pick_asset(&release, kind).cloned() else { return };
        self.updater.progress = Some((0, asset.size));
        self.updater.error.clear();
        let tx = self.notifier();
        std::thread::spawn(move || {
            let ptx = tx.clone();
            let total = asset.size;
            match update::download(&release, &asset, &move |n| ptx.send(Msg::UpdateProgress(n, total))) {
                Ok(file) => {
                    tx.send(Msg::UpdateProgress(total, total));
                    tx.send(Msg::UpdateInstalled(update::install(&file, kind).map(|m| (m, kind))));
                }
                Err(e) => tx.send(Msg::UpdateInstalled(Err(e))),
            }
        });
    }

    /// Draws the updater rows inside the Settings list; returns what was clicked.
    pub(crate) fn updater_rows(ui: &mut egui::Ui, row_w: f32, th: &Theme, u: &UpdaterState) -> Option<UpdaterAction> {
        let (rect, _) = ui.allocate_exact_size(vec2(row_w, 60.0), Sense::hover());
        let rect = rect.translate(vec2(4.0, 0.0));
        let p = ui.painter();
        aero::surface(p, rect, 14.0, th, false, None);
        let cy = rect.center().y;
        aero::orb(p, pos2(rect.min.x + 26.0, cy), 15.0, th.user_orb, "🔄");
        p.text(pos2(rect.min.x + 52.0, cy - 9.0), Align2::LEFT_CENTER, format!("AeroClean {}", update::CURRENT), bold(15.0), th.text);
        let newer = u.release.as_ref().filter(|r| r.is_newer());
        let (status, color) = if !u.error.is_empty() {
            (crate::i18n::tr(&u.error), rgba(220, 70, 50, 255))
        } else if !u.done.is_empty() {
            (crate::i18n::tr(&u.done), th.soft)
        } else if u.checking {
            (crate::i18n::tr("Checking GitHub..."), th.soft)
        } else if let Some((got, total)) = u.progress {
            (crate::i18n::tr(format!("Downloading... {} of {}", format_bytes(got), format_bytes(total))), th.soft)
        } else if let Some(r) = newer {
            (crate::i18n::tr(format!("Version {} is out", r.version)), th.text)
        } else if u.release.is_some() {
            (crate::i18n::tr("You have the latest version. 👍"), th.soft)
        } else {
            (crate::i18n::tr("Press Check now to look for a new version."), th.soft)
        };
        aero::fit_text(p, pos2(rect.min.x + 52.0, cy + 11.0), Align2::LEFT_CENTER, &status, regular(12.0), color, rect.width() - 52.0 - 200.0);
        let btn = Rect::from_min_size(pos2(rect.max.x - 182.0, cy - 16.0), vec2(170.0, 32.0));
        let busy = u.checking || u.progress.is_some() || u.installing;
        let mut check = false;
        let mut start: Option<Release> = None;
        let mut open_page: Option<String> = None;
        match newer.cloned() {
            Some(r) => {
                let kind = update::installer();
                let can = kind.is_some_and(|k| update::pick_asset(&r, k).is_some());
                if can {
                    let tip = crate::i18n::tr("Downloads the new version, checks it against the release's SHA256SUMS, then installs it");
                    if aero::aqua_button(ui, btn, "⬇ Update now", th.go, !busy).on_hover_text(tip).clicked() {
                        start = Some(r);
                    }
                } else {
                    let tip = crate::i18n::tr("This copy wasn't installed from a package, so it can't update itself");
                    if aero::aqua_button(ui, btn, "Open download page", th.accent, true).on_hover_text(tip).clicked() {
                        open_page = Some(r.page.clone());
                    }
                }
            }
            None => check = aero::aqua_button(ui, btn, "🔍 Check now", th.accent, !busy).clicked(),
        }
        if let Some((got, total)) = u.progress {
            let (bar, _) = ui.allocate_exact_size(vec2(row_w, 18.0), Sense::hover());
            let bar = Rect::from_min_size(bar.min + vec2(4.0, 2.0), vec2(row_w, 14.0));
            aero::surface(ui.painter(), bar, 7.0, th, false, None);
            let f = if total > 0 { (got as f32 / total as f32).min(1.0) } else { 0.0 };
            let fill = Rect::from_min_size(bar.min, vec2((bar.width() * f).max(bar.height()), bar.height()));
            aero::rrect_grad(ui.painter(), fill, 7.0, &[(0.0, aero::lighten(th.go, 0.4)), (1.0, th.go)]);
        }
        if let Some(r) = newer.filter(|r| !r.notes_lines(1).is_empty()) {
            // What's new: the first lines of the release notes.
            let notes = r.notes_lines(8).join("\n");
            let text = format!("{}\n{}", crate::i18n::tr(format!("What's new in {}", r.version)), notes);
            let g = ui.painter().layout(text, regular(12.5), th.text, row_w - 36.0);
            let (rect, _) = ui.allocate_exact_size(vec2(row_w, g.size().y + 24.0), Sense::hover());
            let rect = rect.translate(vec2(4.0, 0.0));
            aero::surface(ui.painter(), rect, 14.0, th, false, None);
            ui.painter().galley(rect.min + vec2(18.0, 12.0), g, th.text);
        }
        if check {
            return Some(UpdaterAction::Check);
        }
        start.map(UpdaterAction::Start).or(open_page.map(UpdaterAction::OpenPage))
    }
}
