//! Sharing on the Themes screen: Import, drag & drop, and the online
//! gallery (live previews drawn from the theme files, preview on the whole
//! app before installing, verified downloads).

use super::prelude::*;
use crate::theme_share::{self, GalleryEntry};

#[derive(Default)]
pub(crate) struct GalleryState {
    pub open: bool,
    pub loading: bool,
    /// Gallery themes, their downloaded (verified) text, and a preview theme.
    pub entries: Vec<(GalleryEntry, Option<String>, Option<crate::theme::Theme>)>,
    pub error: String,
    pub msg: String,
    /// Previewing gallery theme `entry` on the whole app; `back` is the theme to return to.
    pub preview: Option<(usize, usize)>,
    pub installing: bool,
}

impl App {
    fn load_gallery(&mut self) {
        self.gallery.loading = true;
        self.gallery.error.clear();
        let tx = self.notifier();
        std::thread::spawn(move || {
            let result = theme_share::fetch_index().map(|entries| {
                // Small themes are fetched now for their previews (verified
                // against the index); big ones only when installed.
                entries
                    .into_iter()
                    .map(|e| {
                        let text = (e.size <= 512 * 1024).then(|| theme_share::fetch_theme(&e).ok()).flatten();
                        (e, text)
                    })
                    .collect()
            });
            tx.send(Msg::GalleryLoaded(result));
        });
    }

    pub(crate) fn gallery_loaded(&mut self, result: Result<Vec<(GalleryEntry, Option<String>)>, String>) {
        self.gallery.loading = false;
        match result {
            Ok(list) => {
                let lib = crate::theme::load_library(&crate::theme::user_dir());
                self.gallery.entries = list
                    .into_iter()
                    .map(|(e, text)| {
                        let preview = text.as_deref().and_then(|t| theme_share::preview_theme(t, &lib));
                        (e, text, preview)
                    })
                    .collect();
                if self.gallery.entries.is_empty() {
                    self.gallery.msg = "The gallery is empty for now.".into();
                }
            }
            Err(e) => {
                self.gallery.error = e;
                self.play_sound(crate::sound::Event::Warning);
            }
        }
    }

    /// Imports a shared theme file and switches to it.
    pub(crate) fn import_theme_text(&mut self, text: &str) {
        let lib = crate::theme::load_library(&crate::theme::user_dir());
        self.gallery.msg = match theme_share::import(text, &crate::theme::user_dir(), &lib) {
            Ok((name, _)) => {
                self.end_gallery_preview();
                crate::theme::save_name(&name);
                self.reload_themes(Some(&name));
                self.theme_files = crate::theme_folder_state_pub();
                let ctx = self.ctx.clone();
                self.apply_visuals(&ctx);
                format!("Added \"{name}\" to your themes")
            }
            Err(e) => format!("Couldn't import the theme: {e}"),
        };
        if self.gallery.msg.starts_with("Couldn't") {
            self.play_sound(crate::sound::Event::Warning);
        }
    }

    pub(crate) fn import_theme_file(&mut self, path: Option<String>) {
        let Some(path) = path else { return };
        match std::fs::metadata(&path) {
            Ok(m) if m.len() as usize > theme_share::MAX_SHARED => {
                self.gallery.msg = "Couldn't import the theme: the file is too big".into();
                self.play_sound(crate::sound::Event::Warning);
            }
            _ => match std::fs::read_to_string(&path) {
                Ok(text) => self.import_theme_text(&text),
                Err(e) => {
                    self.gallery.msg = format!("Couldn't import the theme: {e}");
                    self.play_sound(crate::sound::Event::Warning);
                }
            },
        }
    }

    fn start_gallery_preview(&mut self, i: usize) {
        let Some(t) = self.gallery.entries[i].2.clone() else { return };
        self.end_gallery_preview();
        let back = self.theme;
        self.themes.push(t);
        self.theme = self.themes.len() - 1;
        self.gallery.preview = Some((i, back));
        let ctx = self.ctx.clone();
        self.apply_visuals(&ctx);
    }

    pub(crate) fn end_gallery_preview(&mut self) {
        if let Some((_, back)) = self.gallery.preview.take() {
            self.themes.pop();
            self.theme = back.min(self.themes.len() - 1);
            let ctx = self.ctx.clone();
            self.apply_visuals(&ctx);
        }
    }

    fn install_gallery_theme(&mut self, i: usize) {
        let (entry, text, _) = self.gallery.entries[i].clone();
        self.gallery.installing = true;
        let tx = self.notifier();
        std::thread::spawn(move || {
            let text = match text {
                Some(t) => Ok(t),
                None => theme_share::fetch_theme(&entry),
            };
            tx.send(Msg::GalleryInstall(text));
        });
    }

    pub(crate) fn gallery_install(&mut self, text: Result<String, String>) {
        self.gallery.installing = false;
        match text {
            Ok(t) => self.import_theme_text(&t),
            Err(e) => {
                self.gallery.msg = format!("Couldn't install the theme: {e}");
                self.play_sound(crate::sound::Event::Warning);
            }
        }
    }

    /// The Themes screen's top bar. Returns the area left for the cards.
    pub(crate) fn themes_top_bar(&mut self, ui: &mut egui::Ui, area: Rect) -> Rect {
        let th = &self.themes[self.theme];
        let p = ui.painter().clone();
        let bar = Rect::from_min_size(area.min, vec2(area.width(), 40.0));
        let editing = self.studio.is_some();
        let sub = self.gallery.open || self.picture.open;
        let title = if self.gallery.open {
            "Theme gallery"
        } else if self.picture.open {
            "Themes from a picture"
        } else {
            "Your themes"
        };
        embossed(&p, pos2(bar.min.x + 6.0, bar.center().y), Align2::LEFT_CENTER, title, bold(17.0), th.text);
        let import_r = Rect::from_min_size(pos2(bar.max.x - 132.0, bar.min.y + 4.0), vec2(132.0, 32.0));
        let gallery_r = Rect::from_min_size(pos2(import_r.min.x - 150.0, bar.min.y + 4.0), vec2(142.0, 32.0));
        let (glabel, gtip) = if sub { ("⬅ Your themes", "Back to your themes") } else { ("🌐 Gallery", "Themes shared by the community (downloads only when you open it)") };
        let toggle = aero::aqua_button(ui, gallery_r, glabel, th.accent, !editing).on_hover_text(crate::i18n::tr(gtip)).clicked();
        let daynight_r = Rect::from_min_size(pos2(gallery_r.min.x - 166.0, bar.min.y + 4.0), vec2(158.0, 32.0));
        let mut daynight = false;
        if !sub {
            // Lit up while its settings are showing.
            let color = if self.daynight_open { th.go } else { th.accent };
            daynight = aero::aqua_button(ui, daynight_r, "🌗 Day & night", color, true).on_hover_text(crate::i18n::tr("Use one theme by day and another at night")).clicked();
        }
        // Import: a shared theme file, or a new theme from a picture.
        let import_resp =
            aero::aqua_button(ui, import_r, "📥 Import", th.accent, !editing).on_hover_text(crate::i18n::tr("Add a shared theme file, or make themes from a picture"));
        let (mut import, mut from_picture) = (false, false);
        egui::Popup::menu(&import_resp).show(|ui| {
            ui.set_min_width(230.0);
            import = ui.button(crate::i18n::tr("📄 A theme file someone shared...")).clicked();
            from_picture = ui.button(crate::i18n::tr("🖼 Themes from a picture...")).clicked();
        });
        let msg = if !self.gallery.error.is_empty() && self.gallery.open {
            crate::i18n::tr(format!("Couldn't open the gallery: {}", self.gallery.error))
        } else {
            crate::i18n::tr(&self.gallery.msg)
        };
        let title_w = p.layout_no_wrap(crate::i18n::tr(title), bold(17.0), th.text).size().x;
        let room = if sub { gallery_r.min.x } else { daynight_r.min.x } - bar.min.x - title_w - 30.0;
        if !msg.is_empty() && room > 40.0 {
            aero::fit_text(&p, pos2(bar.min.x + title_w + 22.0, bar.center().y), Align2::LEFT_CENTER, &msg, regular(12.5), th.soft, room);
        }
        if toggle && self.picture.open {
            self.picture.open = false;
        } else if toggle {
            self.gallery.open = !self.gallery.open;
            self.gallery.msg.clear();
            if !self.gallery.open {
                self.end_gallery_preview();
            } else if self.gallery.entries.is_empty() && !self.gallery.loading {
                self.load_gallery();
            }
        }
        if daynight {
            self.daynight_open = !self.daynight_open;
        }
        if from_picture {
            self.pick_picture();
        }
        if import {
            let tx = self.notifier();
            let title = crate::i18n::tr("Choose a theme file (.aerotheme)");
            std::thread::spawn(move || tx.send(Msg::ImportTheme(crate::platform::pick_file(&title))));
        }
        Rect::from_min_max(pos2(area.min.x, area.min.y + 48.0), area.max)
    }

    /// The gallery: a card per theme with its live preview.
    pub(crate) fn gallery_view(&mut self, ui: &mut egui::Ui, area: Rect, t: f32, spin: f32) {
        let th = self.themes[self.theme].clone();
        let p = ui.painter().clone();
        if self.gallery.loading {
            super::spinner(&p, area.center() - vec2(0.0, 20.0), spin, &th);
            embossed(&p, area.center() + vec2(0.0, 18.0), Align2::CENTER_CENTER, "Opening the gallery...", regular(15.0), th.text);
            return;
        }
        if !self.gallery.error.is_empty() {
            embossed(&p, area.center(), Align2::CENTER_CENTER, "The gallery couldn't be reached. Are you online?", regular(15.0), th.text);
            let r = Rect::from_center_size(area.center() + vec2(0.0, 40.0), vec2(150.0, 34.0));
            if aero::aqua_button(ui, r, "🔄 Try again", th.accent, true).clicked() {
                self.load_gallery();
            }
            return;
        }
        let installed: Vec<String> = self.themes.iter().map(|t| t.name.clone()).collect();
        let (mut preview, mut install, mut stop) = (None, None, false);
        // While previewing: a bar to install or go back.
        let mut cards_area = area;
        if let Some((i, _)) = self.gallery.preview {
            let bar = Rect::from_min_size(area.min, vec2(area.width(), 46.0));
            aero::surface(&p, bar, 14.0, &th, false, Some(aero::lighten(th.accent, 0.7)));
            let name = &self.gallery.entries[i].0.name;
            aero::fit_text(
                &p,
                pos2(bar.min.x + 16.0, bar.center().y),
                Align2::LEFT_CENTER,
                &crate::i18n::tr(format!("Previewing \"{name}\" on the whole app")),
                bold(14.0),
                th.text,
                bar.width() - 300.0,
            );
            let inst = Rect::from_min_size(pos2(bar.max.x - 132.0, bar.min.y + 7.0), vec2(124.0, 32.0));
            let back = Rect::from_min_size(pos2(inst.min.x - 112.0, bar.min.y + 7.0), vec2(104.0, 32.0));
            if aero::aqua_button(ui, inst, "⬇ Install", th.go, !self.gallery.installing).clicked() {
                install = Some(i);
            }
            if aero::aqua_button(ui, back, "Back", rgba(125, 145, 170, 255), true).clicked() {
                stop = true;
            }
            cards_area = Rect::from_min_max(pos2(area.min.x, bar.max.y + 10.0), area.max);
        }
        let entries = self.gallery.entries.clone();
        ui.scope_builder(egui::UiBuilder::new().max_rect(cards_area), |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let cols = 3;
                let gap = 16.0;
                let w = (ui.available_width() - 10.0 - gap * (cols as f32 - 1.0)) / cols as f32;
                let h = 210.0;
                for (row, chunk) in entries.chunks(cols).enumerate() {
                    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h + gap), Sense::hover());
                    for (col, (e, _, prev)) in chunk.iter().enumerate() {
                        let i = row * cols + col;
                        let card = Rect::from_min_size(rect.min + vec2(col as f32 * (w + gap), 0.0), vec2(w, h));
                        let p = ui.painter();
                        aero::surface(p, card, 16.0, &th, false, None);
                        let pv = Rect::from_min_max(card.min + vec2(8.0, 8.0), pos2(card.max.x - 8.0, card.max.y - 84.0));
                        match prev {
                            Some(pt) => {
                                let pp = p.with_clip_rect(pv);
                                aero::background(&pp, pv, t, pt);
                                let mini = pv.shrink2(vec2(pv.width() * 0.2, pv.height() * 0.2));
                                aero::glass_panel(&pp, mini, 8.0, pt);
                                aero::orb(&pp, mini.left_center() + vec2(14.0, 0.0), 7.0, pt.user_orb, "");
                                aero::orb(&pp, mini.right_center() - vec2(14.0, 0.0), 7.0, pt.go, "");
                            }
                            None => {
                                aero::rrect_grad(p, pv, 6.0, &[(0.0, with_alpha(th.glass, 120)), (1.0, with_alpha(th.glass, 60))]);
                                p.text(pv.center(), Align2::CENTER_CENTER, crate::i18n::tr("Preview after download"), regular(12.0), th.soft);
                            }
                        }
                        aero::rrect_outline(p, pv, 6.0, Stroke::new(1.0, rgba(255, 255, 255, 180)));
                        aero::fit_text(p, pos2(card.min.x + 12.0, card.max.y - 66.0), Align2::LEFT_CENTER, &e.name, bold(15.0), th.text, w - 24.0);
                        let by = if e.author.is_empty() { e.description.clone() } else { format!("{} · {}", crate::i18n::tr(format!("by {}", e.author)), e.description) };
                        aero::fit_text(p, pos2(card.min.x + 12.0, card.max.y - 47.0), Align2::LEFT_CENTER, &by, regular(12.0), th.soft, w - 24.0);
                        let have = installed.contains(&e.name) && self.gallery.preview.map(|(pi, _)| pi) != Some(i);
                        let half = (w - 30.0) / 2.0;
                        let pr = Rect::from_min_size(pos2(card.min.x + 10.0, card.max.y - 36.0), vec2(half, 28.0));
                        let ir = Rect::from_min_size(pos2(pr.max.x + 10.0, pr.min.y), vec2(half, 28.0));
                        if ui.push_id(("gallery-preview", i), |ui| aero::aqua_button(ui, pr, "👁 Preview", th.accent, prev.is_some()).clicked()).inner {
                            preview = Some(i);
                        }
                        let ilabel = if have { "✔ Installed" } else { "⬇ Install" };
                        if ui.push_id(("gallery-install", i), |ui| aero::aqua_button(ui, ir, ilabel, th.go, !have && !self.gallery.installing).clicked()).inner {
                            install = Some(i);
                        }
                    }
                }
                ui.add_space(4.0);
                ui.label(egui::RichText::new(crate::i18n::tr(format!("Share yours: send a pull request to github.com/{}", theme_share::GALLERY_REPO))).size(12.0).color(th.soft));
            });
        });
        if stop {
            self.end_gallery_preview();
        }
        if let Some(i) = preview {
            self.start_gallery_preview(i);
        }
        if let Some(i) = install {
            self.install_gallery_theme(i);
        }
    }
}
