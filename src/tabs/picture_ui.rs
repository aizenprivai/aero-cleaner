//! Themes from a picture: pick one, see its colors and three matching
//! themes, save or use them.

use super::prelude::*;
use crate::theme_picture::{self, Analysis, Variant};
use std::path::PathBuf;

#[derive(Default)]
pub(crate) struct PictureState {
    pub open: bool,
    pub loading: bool,
    pub analysis: Option<Analysis>,
    /// The picture itself is the themes' background.
    pub use_picture: bool,
    /// The three themes: variant, file text, theme.
    pub made: Vec<(Variant, String, Theme)>,
    /// Names of the ones saved so far.
    pub saved: Vec<String>,
    pub error: String,
    pub msg: String,
}

impl App {
    /// Asks for a picture (in the background).
    pub(crate) fn pick_picture(&mut self) {
        let tx = self.notifier();
        let title = crate::i18n::tr("Choose a picture (.png, .jpg or .webp)");
        std::thread::spawn(move || tx.send(Msg::PicturePicked(crate::platform::pick_file(&title))));
    }

    /// A picture was chosen: analyze it in the background.
    pub(crate) fn picture_picked(&mut self, path: Option<String>) {
        let Some(path) = path else { return };
        let st = &mut self.picture;
        st.open = true;
        st.loading = true;
        st.error.clear();
        st.msg.clear();
        self.gallery.open = false;
        self.end_gallery_preview();
        let tx = self.notifier();
        std::thread::spawn(move || tx.send(Msg::PictureAnalyzed(theme_picture::analyze(&PathBuf::from(path)))));
    }

    pub(crate) fn picture_analyzed(&mut self, result: Result<Analysis, String>) {
        let st = &mut self.picture;
        st.loading = false;
        st.saved.clear();
        match result {
            Ok(a) => {
                st.analysis = Some(a);
                self.remake_picture_themes();
            }
            Err(e) => {
                st.analysis = None;
                st.made.clear();
                st.error = e;
                self.play_sound(crate::sound::Event::Warning);
            }
        }
    }

    fn remake_picture_themes(&mut self) {
        let taken: Vec<String> = self.themes.iter().map(|t| t.name.clone()).filter(|n| !self.picture.saved.contains(n)).collect();
        let st = &mut self.picture;
        let Some(a) = &st.analysis else { return };
        let file = a.file.to_string_lossy().replace('\\', "/");
        st.made = theme_picture::themes(a, &taken, st.use_picture.then_some(file.as_str()));
    }

    /// Saves one of the three into the themes folder. Returns its name.
    fn save_picture_theme(&mut self, i: usize) -> Result<String, String> {
        let st = &self.picture;
        let (Some(a), Some((v, text, t))) = (&st.analysis, st.made.get(i)) else { return Err("nothing to save".into()) };
        if st.saved.contains(&t.name) {
            return Ok(t.name.clone());
        }
        let dir = theme::user_dir();
        let text = if st.use_picture {
            // The themes get their own copy of the picture, next to them.
            let file = theme_picture::copy_picture(&a.file, &dir, &t.name)?;
            theme_picture::build(a, *v, &t.name, Some(&file))
        } else {
            text.clone()
        };
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let stem = theme::file_name(&t.name).trim_end_matches(".aerotheme").to_string();
        let mut path = dir.join(format!("{stem}.aerotheme"));
        let mut n = 2;
        while path.exists() {
            path = dir.join(format!("{stem}-{n}.aerotheme"));
            n += 1;
        }
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
        let name = t.name.clone();
        self.picture.saved.push(name.clone());
        self.reload_themes(None);
        Ok(name)
    }

    pub(crate) fn picture_view(&mut self, ui: &mut egui::Ui, area: Rect, t: f32, spin: f32) {
        let th = self.themes[self.theme].clone();
        let p = ui.painter().clone();
        if self.picture.loading {
            super::spinner(&p, area.center() - vec2(0.0, 20.0), spin, &th);
            embossed(&p, area.center() + vec2(0.0, 18.0), Align2::CENTER_CENTER, "Looking at your picture...", regular(15.0), th.text);
            return;
        }
        let Some(a) = self.picture.analysis.clone() else {
            let why = crate::i18n::tr(format!("Couldn't use this picture: {}", self.picture.error));
            aero::fit_text(&p, area.center() - vec2(0.0, 10.0), Align2::CENTER_CENTER, &why, regular(15.0), th.text, area.width() - 40.0);
            let r = Rect::from_center_size(area.center() + vec2(0.0, 34.0), vec2(190.0, 34.0));
            if aero::aqua_button(ui, r, "🖼 Another picture", th.accent, true).clicked() {
                self.pick_picture();
            }
            return;
        };

        // Top: the picture, its colors and the options.
        let top = Rect::from_min_size(area.min, vec2(area.width(), 168.0));
        aero::surface(&p, top, 14.0, &th, false, None);
        let pic = Rect::from_min_size(top.min + vec2(10.0, 10.0), vec2(((top.height() - 20.0) * 1.6).min(top.width() * 0.4), top.height() - 20.0));
        let mut shown = th.clone();
        shown.scene = vec![theme::SceneLayer::new(theme::Layer::Picture { path: a.file.clone(), frosted: false })];
        aero::background(&p.with_clip_rect(pic), pic, t, &shown);
        aero::rrect_outline(&p, pic, 6.0, Stroke::new(1.0, rgba(255, 255, 255, 200)));
        let x = pic.max.x + 18.0;
        let name = a.file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        aero::fit_text(&p, pos2(x, top.min.y + 22.0), Align2::LEFT_CENTER, &name, bold(15.0), th.text, top.max.x - x - 10.0);
        p.text(pos2(x, top.min.y + 46.0), Align2::LEFT_CENTER, crate::i18n::tr("Its main colors"), regular(12.5), th.soft);
        for (k, (c, share)) in a.palette.iter().enumerate() {
            let center = pos2(x + 16.0 + k as f32 * 40.0, top.min.y + 74.0);
            aero::orb(&p, center, 15.0, *c, "");
            let r = Rect::from_center_size(center, vec2(32.0, 32.0));
            ui.interact(r, egui::Id::unique(("picture-swatch", k)), Sense::hover()).on_hover_text(format!(
                "{} · {}",
                crate::theme_edit::hex(*c),
                crate::i18n::tr(format!("{}% of the picture", (share * 100.0).round()))
            ));
        }
        let mut changed = false;
        let opts = Rect::from_min_max(pos2(x, top.min.y + 100.0), pos2(top.max.x - 10.0, top.max.y - 6.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(opts), |ui| {
            ui.style_mut().visuals.override_text_color = Some(th.text);
            ui.horizontal_wrapped(|ui| {
                changed = ui.checkbox(&mut self.picture.use_picture, crate::i18n::tr("Use the picture as the background (frosted)")).changed();
            });
        });
        let another = Rect::from_min_size(pos2(top.max.x - 186.0, top.max.y - 42.0), vec2(176.0, 32.0));
        if aero::aqua_button(ui, another, "🖼 Another picture", th.accent, true).clicked() {
            self.pick_picture();
        }
        if changed {
            self.remake_picture_themes();
        }

        // Below: the three themes.
        let gap = 16.0;
        let row = Rect::from_min_max(pos2(area.min.x, top.max.y + 14.0), pos2(area.max.x, area.max.y - 22.0));
        let w = (row.width() - gap * 2.0) / 3.0;
        let made = self.picture.made.clone();
        let (mut save, mut use_it) = (None, None);
        for (i, (v, _, pt)) in made.iter().enumerate() {
            let card = Rect::from_min_size(row.min + vec2(i as f32 * (w + gap), 0.0), vec2(w, row.height()));
            aero::surface(&p, card, 16.0, &th, false, None);
            let pv = Rect::from_min_max(card.min + vec2(8.0, 8.0), pos2(card.max.x - 8.0, card.max.y - 84.0));
            let pp = p.with_clip_rect(pv);
            aero::background(&pp, pv, t, pt);
            let mini = pv.shrink2(vec2(pv.width() * 0.14, pv.height() * 0.2));
            aero::glass_panel(&pp, mini, 8.0, pt);
            pp.text(mini.min + vec2(12.0, 16.0), Align2::LEFT_CENTER, "AeroClean", bold(13.0), pt.text);
            pp.text(mini.min + vec2(12.0, 34.0), Align2::LEFT_CENTER, crate::i18n::tr("Ready to free"), regular(11.0), pt.soft);
            for (k, c) in [pt.accent, pt.go].into_iter().enumerate() {
                let pill = Rect::from_min_size(pos2(mini.max.x - 10.0 - (2 - k) as f32 * 46.0, mini.max.y - 24.0), vec2(42.0, 16.0));
                aero::rrect_grad(&pp, pill, 8.0, &[(0.0, aero::lighten(c, 0.4)), (0.5, c), (1.0, aero::lighten(c, 0.4))]);
            }
            aero::orb(&pp, pos2(mini.min.x + 18.0, mini.max.y - 16.0), 8.0, pt.user_orb, "");
            aero::rrect_outline(&p, pv, 6.0, Stroke::new(1.0, rgba(255, 255, 255, 180)));
            aero::fit_text(&p, pos2(card.min.x + 12.0, card.max.y - 66.0), Align2::LEFT_CENTER, &pt.name, bold(15.0), th.text, w - 24.0);
            aero::fit_text(&p, pos2(card.min.x + 12.0, card.max.y - 47.0), Align2::LEFT_CENTER, &crate::i18n::tr(variant_desc(*v)), regular(12.0), th.soft, w - 24.0);
            let saved = self.picture.saved.contains(&pt.name);
            let half = (w - 30.0) / 2.0;
            let sr = Rect::from_min_size(pos2(card.min.x + 10.0, card.max.y - 36.0), vec2(half, 28.0));
            let ur = Rect::from_min_size(pos2(sr.max.x + 10.0, sr.min.y), vec2(half, 28.0));
            let slabel = if saved { "✔ Saved" } else { "💾 Save" };
            if ui.push_id(("picture-save", i), |ui| aero::aqua_button(ui, sr, slabel, th.accent, !saved).on_hover_text(crate::i18n::tr("Add it to your themes")).clicked()).inner {
                save = Some(i);
            }
            if ui.push_id(("picture-use", i), |ui| aero::aqua_button(ui, ur, "✔ Use it", th.go, true).on_hover_text(crate::i18n::tr("Save it and switch to it")).clicked()).inner
            {
                use_it = Some(i);
            }
        }
        let note = if self.picture.msg.is_empty() { crate::i18n::tr("Text colors are adjusted so they stay easy to read.") } else { self.picture.msg.clone() };
        aero::fit_text(&p, pos2(area.min.x + 6.0, area.max.y - 9.0), Align2::LEFT_CENTER, &note, regular(12.0), th.soft, area.width() - 12.0);

        if let Some(i) = save.or(use_it) {
            match self.save_picture_theme(i) {
                Ok(name) => {
                    self.picture.msg = crate::i18n::tr(format!("Added \"{name}\" to your themes"));
                    if use_it.is_some()
                        && let Some(idx) = self.themes.iter().position(|t| t.name == name)
                    {
                        self.switch_theme(idx, ui.ctx());
                        theme::save(&self.themes[idx]);
                    }
                }
                Err(e) => {
                    self.picture.msg = crate::i18n::tr(format!("Couldn't save the theme: {e}"));
                    self.play_sound(crate::sound::Event::Warning);
                }
            }
        }
    }
}

fn variant_desc(v: Variant) -> &'static str {
    match v {
        Variant::Light => "Light frosted glass, the picture's colors",
        Variant::Dark => "Smoked glass, the picture by night",
        Variant::Vivid => "The picture's colors, turned up",
    }
}
