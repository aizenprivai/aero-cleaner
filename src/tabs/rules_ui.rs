//! "My rules" in the Cleaner, and the sheet that creates or edits a rule.

use super::prelude::*;
use crate::rules::{self, Rule};

/// The rule being edited (or created, when `index` is None).
pub(crate) struct RuleEditor {
    pub index: Option<usize>,
    pub name: String,
    pub folder: String,
    pub pattern: String,
    pub days: String,
    pub subfolders: bool,
    pub source: String,
}

impl RuleEditor {
    fn from_rule(index: Option<usize>, r: &Rule) -> Self {
        RuleEditor {
            index,
            name: r.name.clone(),
            folder: r.folder.clone(),
            pattern: r.pattern.clone(),
            days: r.older_than_days.to_string(),
            subfolders: r.subfolders,
            source: r.source.clone(),
        }
    }

    fn to_rule(&self, enabled: bool) -> Result<Rule, String> {
        let days = self.days.trim().parse::<u32>().map_err(|_| "Older than: type a number of days (0 for any age).".to_string())?;
        let rule = Rule {
            name: self.name.trim().to_string(),
            folder: self.folder.trim().to_string(),
            pattern: self.pattern.trim().to_string(),
            older_than_days: days,
            subfolders: self.subfolders,
            enabled,
            source: self.source.clone(),
        };
        rules::check(&rule, &crate::platform::home())?;
        Ok(rule)
    }
}

/// "/home/me/Downloads" -> "~/Downloads".
pub(crate) fn tilde(path: &str) -> String {
    let home = crate::platform::home().to_string_lossy().into_owned();
    match path.strip_prefix(&home) {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => format!("~{rest}"),
        _ => path.to_string(),
    }
}

impl App {
    fn save_rules(&mut self) {
        rules::save(&self.rules);
        if self.phase == Phase::Idle {
            self.start_scan();
        }
    }

    /// The "MY RULES" part of the Cleaner list.
    pub(crate) fn rules_section(&mut self, ui: &mut egui::Ui) {
        let th = &self.themes[self.theme];
        let busy = self.phase != Phase::Idle;
        let w = ui.available_width() - 14.0;
        let (head, _) = ui.allocate_exact_size(vec2(w, 36.0), Sense::hover());
        embossed(ui.painter(), pos2(head.min.x + 8.0, head.center().y + 2.0), Align2::LEFT_CENTER, "MY RULES", bold(14.0), th.soft);
        let new_r = Rect::from_min_size(pos2(head.max.x - 118.0, head.min.y + 4.0), vec2(118.0, 28.0));
        let import_r = Rect::from_min_size(pos2(new_r.min.x - 96.0, new_r.min.y), vec2(90.0, 28.0));
        let export_r = Rect::from_min_size(pos2(import_r.min.x - 96.0, new_r.min.y), vec2(90.0, 28.0));
        if aero::aqua_button(ui, new_r, "+ New rule", th.go, !busy).clicked() {
            self.rule_editor = Some(RuleEditor::from_rule(None, &Rule { name: crate::i18n::tr("Old ISO files"), ..Default::default() }));
        }
        if aero::aqua_button(ui, import_r, "📥 Import", th.accent, !busy)
            .on_hover_text(crate::i18n::tr("Add rules from a shared AeroClean rules file (they start unticked)"))
            .clicked()
        {
            let tx = self.notifier();
            let title = crate::i18n::tr("Choose a rules file");
            std::thread::spawn(move || tx.send(Msg::RulesFile(crate::platform::pick_file(&title))));
        }
        if !self.rules.is_empty() && aero::aqua_button(ui, export_r, "📤 Export", th.accent, true).on_hover_text(crate::i18n::tr("Save your rules as a file to share")).clicked()
        {
            self.status = match rules::export(&self.rules) {
                Ok(path) => {
                    tools::open_folder(&path);
                    format!("Saved {}", path.display())
                }
                Err(e) => format!("Could not save the rules: {e}"),
            };
        }

        if self.rules.is_empty() {
            let (rect, _) = ui.allocate_exact_size(vec2(w, 50.0), Sense::hover());
            let rect = rect.translate(vec2(4.0, 0.0));
            aero::surface(ui.painter(), rect, 14.0, th, false, None);
            aero::fit_text(
                ui.painter(),
                pos2(rect.min.x + 18.0, rect.center().y),
                Align2::LEFT_CENTER,
                &crate::i18n::tr("Clean files your way, like ISO images in Downloads older than 30 days."),
                regular(13.0),
                th.soft,
                rect.width() - 36.0,
            );
            return;
        }

        let mut toggle = None;
        let mut edit = None;
        for (i, r) in self.rules.iter().enumerate() {
            let th = &self.themes[self.theme];
            let (rect, resp) = ui.allocate_exact_size(vec2(w, 60.0), Sense::click());
            let rect = rect.translate(vec2(4.0, 0.0));
            let scan = self.rule_scans.get(i).and_then(|s| s.as_ref());
            let broken = scan.is_some_and(|s| !s.error.is_empty());
            if resp.clicked() && !busy {
                toggle = Some(i);
            }
            let p = ui.painter();
            let on = r.enabled && !broken;
            let tint = on.then(|| if th.dark { th.go } else { aero::lighten(th.go, 0.75) });
            aero::surface(p, rect, 14.0, th, resp.hovered() && !busy, tint);
            let cy = rect.center().y;
            aero::bubble_check(p, pos2(rect.min.x + 26.0, cy), 12.0, on, resp.hovered(), th.go, th.accent);
            aero::orb(p, pos2(rect.min.x + 72.0, cy), 19.0, if broken { rgba(150, 160, 172, 255) } else { th.user_orb }, "📐");
            let text_x = rect.min.x + 104.0;
            let room = rect.max.x - 190.0 - text_x;
            let title = if r.source.is_empty() { r.name.clone() } else { format!("{}  ·  {}", r.name, r.source) };
            aero::fit_text(p, pos2(text_x, cy - 10.0), Align2::LEFT_CENTER, &title, bold(16.0), th.text, room);
            let desc = match scan {
                Some(s) if broken => crate::i18n::tr(&s.error),
                _ => crate::i18n::tr(rules::describe(r)),
            };
            aero::fit_text(p, pos2(text_x, cy + 11.0), Align2::LEFT_CENTER, &desc, regular(13.0), if broken { rgba(220, 70, 50, 255) } else { th.soft }, room);
            let right = pos2(rect.max.x - 64.0, cy);
            match scan {
                None => {
                    let t = ui.input(|i| i.time) as f32;
                    for k in 0..3 {
                        let a = ((t * 4.0 - k as f32 * 0.7).sin() * 0.5 + 0.5) * 255.0;
                        p.circle_filled(right - vec2(k as f32 * 14.0, 0.0), 4.0, with_alpha(th.accent, a as u8));
                    }
                }
                Some(s) if s.error.is_empty() => {
                    embossed(p, right - vec2(0.0, 9.0), Align2::RIGHT_CENTER, &format_bytes(s.size), bold(17.0), th.text);
                    p.text(right + vec2(0.0, 11.0), Align2::RIGHT_CENTER, crate::i18n::tr(format!("{} file(s)", s.files.len())), regular(12.0), th.soft);
                }
                Some(_) => {}
            }
            let edit_r = Rect::from_min_size(pos2(rect.max.x - 52.0, cy - 16.0), vec2(40.0, 32.0));
            if aero::aqua_button(ui, edit_r, "✏", th.accent, !busy).on_hover_text(crate::i18n::tr("Edit this rule")).clicked() {
                edit = Some(i);
            }
        }
        if let Some(i) = toggle {
            self.rules[i].enabled = !self.rules[i].enabled;
            rules::save(&self.rules);
        }
        if let Some(i) = edit {
            self.rule_editor = Some(RuleEditor::from_rule(Some(i), &self.rules[i]));
        }
    }

    /// The sheet for creating or editing a rule.
    pub(crate) fn rule_editor_sheet(&mut self, ctx: &egui::Context, screen: Rect) {
        let th = &self.themes[self.theme];
        let Some(ed) = self.rule_editor.as_mut() else { return };
        let mut action: Option<&str> = None;
        let mut browse = false;
        let check = ed.to_rule(true);
        egui::Area::new(egui::Id::unique("rule-editor")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
            ui.allocate_exact_size(screen.size(), Sense::click());
            let p = ui.painter().clone();
            p.rect_filled(screen, 0.0, rgba(0, 20, 50, 120));
            let sheet = Rect::from_center_size(screen.center(), vec2((screen.width() - 80.0).min(620.0), 470.0f32.min(screen.height() - 60.0)));
            aero::glass_panel(&p, sheet, 20.0, th);
            aero::glass_panel(&p, sheet, 20.0, th);
            let inner = sheet.shrink(24.0);
            let title = if ed.index.is_some() { "Edit rule" } else { "New rule" };
            embossed(&p, pos2(inner.min.x, inner.min.y + 14.0), Align2::LEFT_CENTER, title, bold(24.0), th.text);
            aero::fit_text(
                &p,
                pos2(inner.min.x, inner.min.y + 42.0),
                Align2::LEFT_CENTER,
                &crate::i18n::tr("Matching files are deleted when you clean (and kept for undo). Folders are never deleted."),
                regular(13.0),
                th.soft,
                inner.width(),
            );
            let label_w = 130.0;
            let mut y = inner.min.y + 72.0;
            let field = |ui: &mut egui::Ui, y: &mut f32, label: &str, value: &mut String, hint: &str, right_pad: f32| {
                p.text(pos2(inner.min.x, *y + 17.0), Align2::LEFT_CENTER, crate::i18n::tr(label), bold(14.0), th.text);
                let r = Rect::from_min_max(pos2(inner.min.x + label_w, *y + 2.0), pos2(inner.max.x - right_pad, *y + 32.0));
                ui.put(r, egui::TextEdit::singleline(value).hint_text(hint).font(regular(14.0)));
                *y += 46.0;
            };
            field(ui, &mut y, "Name", &mut ed.name, "Old ISO files", 0.0);
            let browse_r = Rect::from_min_size(pos2(inner.max.x - 110.0, y + 1.0), vec2(110.0, 32.0));
            field(ui, &mut y, "Folder", &mut ed.folder, "~/Downloads", 120.0);
            browse = aero::aqua_button(ui, browse_r, "📁 Browse", th.accent, true).clicked();
            field(ui, &mut y, "Files", &mut ed.pattern, "*.iso;*.img", 0.0);
            p.text(pos2(inner.min.x + label_w, y - 8.0), Align2::LEFT_CENTER, crate::i18n::tr("* means anything; separate several patterns with ;"), regular(11.5), th.soft);
            y += 8.0;
            p.text(pos2(inner.min.x, y + 17.0), Align2::LEFT_CENTER, crate::i18n::tr("Older than"), bold(14.0), th.text);
            let days_r = Rect::from_min_size(pos2(inner.min.x + label_w, y + 2.0), vec2(70.0, 30.0));
            ui.put(days_r, egui::TextEdit::singleline(&mut ed.days).font(regular(14.0)));
            p.text(pos2(days_r.max.x + 10.0, y + 17.0), Align2::LEFT_CENTER, crate::i18n::tr("days (0 = any age)"), regular(13.0), th.soft);
            y += 46.0;
            let sub_r = Rect::from_min_size(pos2(inner.min.x + label_w, y), vec2(inner.width() - label_w, 32.0));
            let sub = ui.interact(sub_r, egui::Id::unique("rule-subfolders"), Sense::click());
            if sub.clicked() {
                ed.subfolders = !ed.subfolders;
            }
            aero::bubble_check(&p, pos2(sub_r.min.x + 12.0, sub_r.center().y), 11.0, ed.subfolders, sub.hovered(), th.go, th.accent);
            p.text(pos2(sub_r.min.x + 32.0, sub_r.center().y), Align2::LEFT_CENTER, crate::i18n::tr("Also look inside subfolders"), regular(14.0), th.text);
            y += 44.0;
            let (msg, color) = match &check {
                Ok(r) => (crate::i18n::tr(format!("Deletes {}.", rules::describe(r))), th.soft),
                Err(e) => (crate::i18n::tr(e), rgba(220, 70, 50, 255)),
            };
            let g = p.layout(msg, regular(13.0), color, inner.width());
            p.galley(pos2(inner.min.x, y), g, color);

            let save_r = Rect::from_min_size(pos2(inner.max.x - 150.0, inner.max.y - 44.0), vec2(150.0, 44.0));
            let cancel_r = Rect::from_min_size(pos2(save_r.min.x - 130.0, save_r.min.y), vec2(120.0, 44.0));
            if aero::aqua_button(ui, save_r, "✔ Save", th.go, check.is_ok()).clicked() {
                action = Some("save");
            }
            if aero::aqua_button(ui, cancel_r, "Cancel", rgba(125, 145, 170, 255), true).clicked() {
                action = Some("cancel");
            }
            if ed.index.is_some() {
                let del_r = Rect::from_min_size(pos2(inner.min.x, save_r.min.y), vec2(130.0, 44.0));
                if aero::aqua_button(ui, del_r, "🗑 Delete", th.root_orb, true).clicked() {
                    action = Some("delete");
                }
            }
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            action = Some("cancel");
        }
        if browse {
            self.pick_for_rule = true;
            let tx = self.notifier();
            let title = crate::i18n::tr("Choose the folder this rule cleans");
            std::thread::spawn(move || tx.send(Msg::Picked(crate::platform::pick_folder(&title))));
        }
        match action {
            Some("save") => {
                let ed = self.rule_editor.take().unwrap();
                let enabled = ed.index.map(|i| self.rules[i].enabled).unwrap_or(true);
                if let Ok(rule) = ed.to_rule(enabled) {
                    match ed.index {
                        Some(i) => self.rules[i] = rule,
                        None => self.rules.push(rule),
                    }
                    self.save_rules();
                }
            }
            Some("delete") => {
                if let Some(i) = self.rule_editor.take().and_then(|e| e.index) {
                    self.rules.remove(i);
                    self.save_rules();
                }
            }
            Some(_) => self.rule_editor = None,
            None => {}
        }
    }

    /// A rules file was picked for import.
    pub(crate) fn import_rules_file(&mut self, path: Option<String>) {
        let Some(path) = path else { return };
        self.status = match std::fs::read_to_string(&path) {
            Ok(text) => {
                let found = rules::parse(&text);
                let added = rules::merge_imported(&mut self.rules, found);
                self.save_rules();
                format!("Imported {added} rule(s). Tick the ones you want to use.")
            }
            Err(e) => format!("Could not read the file: {e}"),
        };
    }
}
