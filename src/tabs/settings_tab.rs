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
    aero::a11y_checkbox(&resp, true, on, title);
    let rect = Rect::from_min_max(rect.min + vec2(4.0 + indent, 0.0), rect.max + vec2(4.0, 0.0));
    let p = ui.painter();
    let tint = on.then(|| if th.dark { th.go } else { aero::lighten(th.go, 0.75) });
    aero::surface(p, rect, 14.0, th, resp.hovered(), tint);
    let cy = rect.center().y;
    aero::bubble_check(p, pos2(rect.min.x + 26.0, cy), 12.0, on, resp.hovered(), th.go, th.accent);
    aero::orb(p, pos2(rect.min.x + 70.0, cy), 17.0, th.user_orb, icon);
    p.text(pos2(rect.min.x + 98.0, cy - 9.0), Align2::LEFT_CENTER, crate::i18n::tr(title), bold(15.0), th.text);
    // Room is kept on the right for a value button or the next run date.
    aero::fit_text(p, pos2(rect.min.x + 98.0, cy + 11.0), Align2::LEFT_CENTER, &crate::i18n::tr(desc), regular(12.0), th.soft, rect.width() - 98.0 - 150.0);
    resp
}

/// A row with a title, a description and a value button on the right.
/// Returns true when the button was clicked.
fn option_row(ui: &mut egui::Ui, w: f32, icon: &str, title: &str, desc: &str, value: &str, tip: &str, th: &Theme) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 60.0), Sense::hover());
    let rect = rect.translate(vec2(4.0, 0.0));
    let p = ui.painter();
    aero::surface(p, rect, 14.0, th, resp.hovered(), None);
    let cy = rect.center().y;
    aero::orb(p, pos2(rect.min.x + 26.0, cy), 15.0, th.user_orb, icon);
    p.text(pos2(rect.min.x + 52.0, cy - 9.0), Align2::LEFT_CENTER, crate::i18n::tr(title), bold(15.0), th.text);
    aero::fit_text(p, pos2(rect.min.x + 52.0, cy + 11.0), Align2::LEFT_CENTER, &crate::i18n::tr(desc), regular(12.0), th.soft, rect.width() - 52.0 - 150.0);
    let btn = Rect::from_min_size(pos2(rect.max.x - 136.0, cy - 15.0), vec2(124.0, 30.0));
    aero::aqua_button(ui, btn, value, th.accent, true).on_hover_text(crate::i18n::tr(tip)).clicked()
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
        let (mut cycle_journal, mut cycle_big, mut reset_sel) = (false, false, false);
        let mut toggle_updates = false;
        let mut cycle_scale = false;
        let mut updater_action: Option<super::UpdaterAction> = None;
        let (mut cycle_undo, mut empty_undo) = (false, false);
        let mut export: Option<bool> = None;
        let mut restore: Option<crate::holding::Held> = None;
        if self.undo_list.is_none() {
            self.load_undo_list();
        }
        let mut set_lang: Option<&'static str> = None;
        let home = crate::platform::home().to_string_lossy().into_owned();
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
                p.text(pos2(rect.min.x + 98.0, cy - 9.0), Align2::LEFT_CENTER, crate::i18n::tr("Reduce motion"), bold(15.0), th.text);
                aero::fit_text(
                    p,
                    pos2(rect.min.x + 98.0, cy + 11.0),
                    Align2::LEFT_CENTER,
                    &crate::i18n::tr("Freeze the animated background to save battery. It already pauses when the window isn't active."),
                    regular(12.0),
                    th.soft,
                    rect.width() - 98.0 - 16.0,
                );

                cycle_scale = option_row(
                    ui,
                    row_w,
                    "🔍",
                    "Text and controls size",
                    "Makes everything bigger. The window gets bigger too, so it may not fit small screens above 150%.",
                    &format!("{}%", self.settings.ui_scale),
                    "Change the size",
                    th,
                );

                // ----- Automation -----
                Self::section(ui, "AUTOMATION", th);
                let resp = toggle_row(ui, row_w, 0.0, self.auto.clean_on, "📅", "Weekly automatic clean", "Thumbnails and app caches, once a week. Never asks for a password.", th);
                let resp = resp.on_hover_text(crate::i18n::tr("Browser caches are skipped if a browser is open at that moment. Folders to keep are respected."));
                toggle_clean = resp.clicked();
                if let Some(next) = &self.auto.next_clean {
                    let r = resp.rect;
                    ui.painter().text(
                        pos2(r.max.x - 14.0, r.center().y),
                        Align2::RIGHT_CENTER,
                        // Just the date ("Mon 2026-11-16 00:42 CET" -> "2026-11-16"): no untranslated weekday.
                        crate::i18n::tr(format!("next: {}", next.split_whitespace().find(|w| w.contains('-')).unwrap_or(next))),
                        regular(12.0),
                        th.soft,
                    );
                }
                toggle_trash = toggle_row(
                    ui,
                    row_w - 40.0,
                    40.0,
                    self.settings.auto_trash,
                    "🗑",
                    "Also empty the Trash",
                    "Files you deleted more than a week ago can't be restored after this.",
                    th,
                )
                .clicked();
                let pct = self.settings.low_disk_percent;
                let resp = toggle_row(
                    ui,
                    row_w,
                    0.0,
                    self.auto.disk_on,
                    "🔔",
                    "Low disk space alert",
                    &format!("Checks every hour and notifies you when less than {pct}% of the disk is free (at most once a day)."),
                    th,
                );
                let pr = Rect::from_min_size(pos2(resp.rect.max.x - 82.0, resp.rect.center().y - 15.0), vec2(70.0, 30.0));
                cycle_pct = aero::aqua_button(ui, pr, &format!("{pct}%"), th.accent, true).on_hover_text(crate::i18n::tr("Change the threshold")).clicked();
                toggle_disk = resp.clicked() && !cycle_pct;
                let (rect, _) = ui.allocate_exact_size(vec2(row_w, 44.0), Sense::hover());
                let rect = rect.translate(vec2(4.0, 0.0));
                let run_r = Rect::from_min_size(rect.min + vec2(0.0, 4.0), vec2(250.0, 36.0));
                let label = if self.auto.running { "Cleaning..." } else { "▶ Run automatic clean now" };
                run_now = aero::aqua_button(ui, run_r, label, th.go, !self.auto.running).clicked();
                let tip = if crate::platform::IS_WINDOWS {
                    "Runs through Windows Task Scheduler, under the AeroClean folder."
                } else if crate::platform::IS_MACOS {
                    "Runs in the background through launchd. Keep AeroClean in Applications."
                } else {
                    "Tip: right-click AeroClean in the menu for Quick clean and Disk map."
                };
                aero::fit_text(
                    ui.painter(),
                    pos2(run_r.max.x + 14.0, run_r.center().y),
                    Align2::LEFT_CENTER,
                    &crate::i18n::tr(tip),
                    regular(12.0),
                    th.soft,
                    rect.max.x - run_r.max.x - 24.0,
                );

                // ----- Cleaning -----
                Self::section(ui, "CLEANING", th);
                let days = format!("{} days", self.settings.journal_days);
                // The systemd journal only exists on Linux.
                if crate::platform::IS_LINUX {
                    cycle_journal = option_row(
                        ui,
                        row_w,
                        "📜",
                        "System journal retention",
                        "How many days of system logs to keep when cleaning the journal.",
                        &days,
                        "Change the threshold",
                        th,
                    );
                }
                let mb = self.settings.big_min_mb;
                let unit = if crate::i18n::is_french() { ("Mo", "Go") } else { ("MB", "GB") };
                let big = if mb >= 1000 { format!("{} {}", mb / 1000, unit.1) } else { format!("{mb} {}", unit.0) };
                cycle_big = option_row(ui, row_w, "🐘", "Big file size", "Files bigger than this are listed in Big files.", &big, "Change the threshold", th);
                reset_sel = option_row(
                    ui,
                    row_w,
                    "☑",
                    "Remember my selection",
                    "The items you tick in the Cleaner stay ticked next time.",
                    "Reset",
                    "Back to the recommended selection",
                    th,
                );

                // ----- Language -----
                Self::section(ui, "LANGUAGE", th);
                let (rect, _) = ui.allocate_exact_size(vec2(row_w, 46.0), Sense::hover());
                let rect = rect.translate(vec2(4.0, 0.0));
                for (k, (code, label)) in [("auto", "Automatic"), ("en", "English"), ("fr", "Français")].into_iter().enumerate() {
                    let r = Rect::from_min_size(pos2(rect.min.x + k as f32 * 150.0, rect.min.y + 4.0), vec2(140.0, 38.0));
                    let active = self.settings.language == code;
                    let text = if active { format!("✔ {}", crate::i18n::tr(label)) } else { crate::i18n::tr(label) };
                    let resp = aero::aqua_button(ui, r, &text, if active { th.go } else { th.accent }, true);
                    let resp = if code == "auto" { resp.on_hover_text(crate::i18n::tr("Follows your system language")) } else { resp };
                    if resp.clicked() && !active {
                        set_lang = Some(code);
                    }
                }

                // ----- Exclusions -----
                Self::section(ui, "FOLDERS TO KEEP  (the cleaner never touches these)", th);
                let mut remove = None;
                if self.settings.exclusions.is_empty() {
                    let (rect, _) = ui.allocate_exact_size(vec2(row_w, 40.0), Sense::hover());
                    let rect = rect.translate(vec2(4.0, 0.0));
                    aero::surface(ui.painter(), rect, 12.0, th, false, None);
                    ui.painter().text(rect.left_center() + vec2(16.0, 0.0), Align2::LEFT_CENTER, crate::i18n::tr("Nothing excluded yet."), regular(13.0), th.soft);
                }
                for (i, x) in self.settings.exclusions.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(vec2(row_w, 44.0), Sense::hover());
                    let rect = rect.translate(vec2(4.0, 0.0));
                    aero::surface(ui.painter(), rect, 12.0, th, resp.hovered(), None);
                    let cy = rect.center().y;
                    aero::orb(ui.painter(), pos2(rect.min.x + 22.0, cy), 12.0, th.go, "🛡");
                    let missing = if x.exists() { "" } else { "   (folder not found)" };
                    ui.painter().text(pos2(rect.min.x + 44.0, cy), Align2::LEFT_CENTER, crate::i18n::tr(format!("{}{missing}", pretty(x))), bold(14.0), th.text);
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
                        let picked = crate::platform::pick_folder(&crate::i18n::tr("Choose a folder to keep"));
                        tx.send(Msg::Picked(picked));
                    });
                }

                let (rect, _) = ui.allocate_exact_size(vec2(row_w, 40.0), Sense::hover());
                let rect = rect.translate(vec2(4.0, 0.0));
                let keep_r = Rect::from_min_size(rect.min + vec2(0.0, 2.0), vec2(230.0, 36.0));
                // Browser caches aren't part of the Windows cleaner, so the shortcut is Linux-only.
                if !crate::platform::IS_WINDOWS
                    && aero::aqua_button(ui, keep_r, "🌐 Keep browser caches", th.go, true)
                        .on_hover_text(crate::i18n::tr("Websites stay fast to load, but those caches won't be cleaned"))
                        .clicked()
                {
                    let dirs = settings::browser_cache_dirs();
                    let added = dirs.iter().filter(|d| self.settings.add_exclusion(&d.to_string_lossy()).is_ok()).count();
                    self.settings_msg = if dirs.is_empty() { "No browser caches found.".into() } else { format!("Added {added} browser cache folder(s).") };
                    rescan |= added > 0;
                }
                if !self.settings_msg.is_empty() {
                    ui.painter().text(pos2(keep_r.max.x + 14.0, keep_r.center().y), Align2::LEFT_CENTER, crate::i18n::tr(&self.settings_msg), regular(13.0), th.soft);
                }

                // ----- Updates -----
                Self::section(ui, "UPDATES", th);
                updater_action = Self::updater_rows(ui, row_w, th, &self.updater);
                toggle_updates = toggle_row(
                    ui,
                    row_w,
                    0.0,
                    self.settings.check_updates,
                    "🔔",
                    "Tell me when a new version is out",
                    "Once a day, asks GitHub for the latest version number. Nothing about you or your computer is sent.",
                    th,
                )
                .clicked();

                // ----- Undo -----
                Self::section(ui, "UNDO", th);
                let keep = match self.settings.undo_hours {
                    0 => "Off".to_string(),
                    24 => "24 hours".to_string(),
                    h if h % 24 == 0 && h / 24 == 7 => "1 week".to_string(),
                    h if h % 24 == 0 => format!("{} days", h / 24),
                    h => format!("{h} hours"),
                };
                cycle_undo = option_row(
                    ui,
                    row_w,
                    "↩",
                    "Keep cleaned files for",
                    "So a clean can be undone. They're deleted for real afterwards, or right away if the disk is almost full.",
                    &keep,
                    "Change how long",
                    th,
                );
                let held = self.undo_list.clone().unwrap_or_default();
                for h in &held {
                    let (rect, resp) = ui.allocate_exact_size(vec2(row_w, 44.0), Sense::hover());
                    let rect = rect.translate(vec2(4.0, 0.0));
                    let p = ui.painter();
                    aero::surface(p, rect, 12.0, th, resp.hovered(), None);
                    let cy = rect.center().y;
                    let when = tools::age(Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(h.when)));
                    p.text(pos2(rect.min.x + 16.0, cy), Align2::LEFT_CENTER, &when, bold(13.0), th.text);
                    // Stored in English: "Firefox: Cache, Browsing history" or category names.
                    let what = h
                        .label
                        .split(", ")
                        .map(|part| match part.split_once(": ") {
                            Some((app, first)) => format!("{app}: {}", crate::i18n::tr(first)),
                            None => crate::i18n::tr(part),
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    aero::fit_text(p, pos2(rect.min.x + 120.0, cy), Align2::LEFT_CENTER, &what, regular(12.0), th.soft, rect.width() - 120.0 - 230.0);
                    embossed(p, pos2(rect.max.x - 128.0, cy), Align2::RIGHT_CENTER, &format_bytes(h.size), bold(14.0), th.text);
                    let btn = Rect::from_min_size(pos2(rect.max.x - 116.0, cy - 14.0), vec2(104.0, 28.0));
                    if aero::aqua_button(ui, btn, "↩ Restore", th.accent, !self.undo_busy).on_hover_text(crate::i18n::tr("Put these files back where they were")).clicked() {
                        restore = Some(h.clone());
                    }
                }
                if !held.is_empty() {
                    let total: u64 = held.iter().map(|h| h.size).sum();
                    let (rect, _) = ui.allocate_exact_size(vec2(row_w, 40.0), Sense::hover());
                    let r = Rect::from_min_size(rect.min + vec2(4.0, 2.0), vec2(230.0, 36.0));
                    empty_undo = aero::aqua_button(ui, r, &format!("🗑 Delete now ({})", format_bytes(total)), th.root_orb, !self.undo_busy)
                        .on_hover_text(crate::i18n::tr("Frees the space now; these cleans can't be undone anymore"))
                        .clicked();
                }

                // ----- History -----
                Self::section(ui, "CLEANING HISTORY", th);
                let total: u64 = self.history.iter().map(|h| h.freed).sum();
                let (rect, _) = ui.allocate_exact_size(vec2(row_w, 56.0), Sense::hover());
                let rect = rect.translate(vec2(4.0, 0.0));
                aero::surface(ui.painter(), rect, 14.0, th, false, Some(aero::lighten(th.go, 0.75)));
                aero::orb(ui.painter(), pos2(rect.min.x + 28.0, rect.center().y), 17.0, th.go, "✨");
                let amount_color = if th.dark { aero::lighten(th.go, 0.35) } else { aero::darken(th.go, 0.1) };
                embossed(
                    ui.painter(),
                    pos2(rect.min.x + 56.0, rect.center().y - 8.0),
                    Align2::LEFT_CENTER,
                    &format!("{} freed in total", format_bytes(total)),
                    bold(18.0),
                    amount_color,
                );
                let n = self.history.len();
                ui.painter().text(
                    pos2(rect.min.x + 56.0, rect.center().y + 13.0),
                    Align2::LEFT_CENTER,
                    crate::i18n::tr(format!("over {n} clean{}", if n == 1 { "" } else { "s" })),
                    regular(12.0),
                    th.soft,
                );
                if n > 0 {
                    let md_r = Rect::from_min_size(pos2(rect.max.x - 132.0, rect.center().y - 15.0), vec2(120.0, 30.0));
                    let csv_r = Rect::from_min_size(pos2(md_r.min.x - 88.0, md_r.min.y), vec2(80.0, 30.0));
                    let tip = crate::i18n::tr("Save the cleaning history as a file in your Documents folder");
                    if aero::aqua_button(ui, csv_r, "📄 CSV", th.accent, true).on_hover_text(&tip).clicked() {
                        export = Some(false);
                    }
                    if aero::aqua_button(ui, md_r, "📝 Markdown", th.accent, true).on_hover_text(&tip).clicked() {
                        export = Some(true);
                    }
                }
                for h in self.history.iter().take(20) {
                    let (rect, resp) = ui.allocate_exact_size(vec2(row_w, 40.0), Sense::hover());
                    let rect = rect.translate(vec2(4.0, 0.0));
                    let p = ui.painter();
                    aero::surface(p, rect, 12.0, th, false, None);
                    let cy = rect.center().y;
                    p.text(pos2(rect.min.x + 16.0, cy), Align2::LEFT_CENTER, crate::i18n::tr(tools::age(Some(h.when))), bold(13.0), th.text);
                    let clip = p.with_clip_rect(Rect::from_min_max(pos2(rect.min.x + 120.0, rect.min.y), pos2(rect.max.x - 110.0, rect.max.y)));
                    // Stored in English; translate each category name for display.
                    let what = h.what.split(", ").map(crate::i18n::tr).collect::<Vec<_>>().join(", ");
                    clip.text(pos2(rect.min.x + 120.0, cy), Align2::LEFT_CENTER, &what, regular(12.0), th.soft);
                    embossed(p, pos2(rect.max.x - 16.0, cy), Align2::RIGHT_CENTER, &format_bytes(h.freed), bold(14.0), th.text);
                    resp.on_hover_text(what);
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
        if let Some(markdown) = export {
            self.settings_msg = match crate::report::export(&self.history, markdown) {
                Ok(path) => {
                    tools::open_folder(&path);
                    format!("Saved {}", path.display())
                }
                Err(e) => format!("Could not save the report: {e}"),
            };
        }
        if cycle_undo {
            self.settings.undo_hours = match self.settings.undo_hours {
                24 => 72,
                72 => 168,
                168 => 0,
                _ => 24,
            };
            self.settings.save();
        }
        if empty_undo {
            self.undo_busy = true;
            let tx = self.notifier();
            std::thread::spawn(move || {
                let freed = crate::holding::purge(0);
                tx.send(Msg::Restored(format!("Freed {} kept for undo.", format_bytes(freed))));
            });
        }
        if let Some(h) = restore {
            self.restore_held(h);
        }
        if let Some(a) = updater_action {
            self.run_updater_action(a);
        }
        if cycle_scale {
            self.settings.ui_scale = match self.settings.ui_scale {
                100 => 125,
                125 => 150,
                150 => 175,
                175 => 200,
                _ => 100,
            };
            self.settings.save();
            self.apply_scale(ui.ctx());
        }
        if toggle_updates {
            self.settings.check_updates = !self.settings.check_updates;
            self.settings.save();
            if self.settings.check_updates {
                self.start_update_check(true);
            } else {
                self.update = None;
            }
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
        if cycle_journal {
            self.settings.journal_days = match self.settings.journal_days {
                3 => 7,
                7 => 14,
                14 => 30,
                _ => 3,
            };
            self.settings.save();
        }
        if cycle_big {
            self.settings.big_min_mb = match self.settings.big_min_mb {
                50 => 100,
                100 => 250,
                250 => 500,
                500 => 1000,
                _ => 50,
            };
            self.settings.save();
            // The Big files list was built with the old threshold.
            self.big.clear();
            self.big_done = false;
        }
        if reset_sel {
            self.settings.selected = None;
            self.settings.save();
            let fresh = self.initial_selection();
            for (it, f) in self.items.iter_mut().zip(fresh) {
                it.on = f.on;
            }
        }
        if let Some(code) = set_lang {
            self.settings.language = code.to_string();
            self.settings.save();
            crate::i18n::set_language(code);
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
