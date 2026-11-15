//! The Theme Studio: a glass panel on the right that edits a theme while
//! the rest of the app shows it live. Tabs: Colors, Scene, Material,
//! Motion & Sound. Saves a small file with only what changed.

use super::prelude::*;
use crate::theme::{Layer, Theme};
use crate::theme_edit::{self, Draft, Harmony, Vision};
use eframe::egui::Color32;
use toml::Value;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum StudioTab {
    Colors,
    Scene,
    Material,
    Motion,
}

pub(crate) struct Studio {
    pub draft: Draft,
    pub tab: StudioTab,
    vision: Vision,
    error: String,
    warnings: Vec<String>,
    pub msg: String,
    /// Where the live preview sits in `App::themes` (a temporary entry).
    pub preview_index: usize,
    /// The theme to go back to when closing without saving.
    pub return_index: usize,
    known: Vec<(String, toml::Table)>,
    built: Option<Theme>,
    pub expanded: Option<usize>,
    /// The color waiting for an eyedropper click, and whether a pick is on its way.
    pub eyedropper: Option<&'static str>,
    pub picking: Option<&'static str>,
    last_edit: (String, f64),
    confirm_close: bool,
    pub changed: bool,
    name_buf: String,
}

/// The color settings, with their labels.
const COLOR_KEYS: &[(&str, &str)] = &[
    ("text", "Text"),
    ("soft", "Soft text"),
    ("glass", "Glass"),
    ("accent", "Accent"),
    ("go", "Main buttons"),
    ("user_orb", "Your items"),
    ("root_orb", "System items"),
    ("logo", "Logo"),
    ("bubble", "Bubbles"),
];

fn color_of(t: &Theme, key: &str) -> Color32 {
    match key {
        "text" => t.text,
        "soft" => t.soft,
        "glass" => t.glass,
        "accent" => t.accent,
        "go" => t.go,
        "user_orb" => t.user_orb,
        "root_orb" => t.root_orb,
        "logo" => t.logo,
        _ => t.bubble,
    }
}

fn num(t: &toml::Table, key: &str, default: f32) -> f32 {
    t.get(key).and_then(|v| v.as_float().or(v.as_integer().map(|n| n as f64))).map(|v| v as f32).unwrap_or(default)
}

impl Studio {
    /// Applies a change, remembering the state before it for undo (one
    /// undo step per control, not per frame of a drag).
    fn edit(&mut self, key: &str, now: f64, f: impl FnOnce(&mut Draft)) {
        if self.last_edit.0 != key || now - self.last_edit.1 > 0.8 {
            self.draft.checkpoint();
        }
        self.last_edit = (key.to_string(), now);
        f(&mut self.draft);
        self.changed = true;
        self.confirm_close = false;
    }

    fn base_resolved(&self) -> toml::Table {
        let base = self.draft.base();
        self.known.iter().find(|(n, _)| *n == base).map(|(_, t)| t.clone()).unwrap_or_default()
    }

    fn layers(&self) -> Vec<toml::Table> {
        self.draft.layers(&self.base_resolved())
    }
}

impl App {
    /// Opens the Studio on the current theme: your own file is edited in
    /// place; a built-in theme becomes a new theme built on it.
    pub(crate) fn open_studio(&mut self, ctx: &egui::Context) {
        let lib = crate::theme::load_library(&crate::theme::user_dir());
        let current = self.themes[self.theme].clone();
        let draft = match &current.file {
            Some(f) => Draft::open(f).unwrap_or_else(|_| Draft::customize(&current, &[])),
            None => Draft::customize(&current, &lib.themes.iter().map(|t| t.name.clone()).collect::<Vec<_>>()),
        };
        let name_buf = draft.name();
        self.themes.push(current);
        let st = Studio {
            draft,
            tab: StudioTab::Colors,
            vision: Vision::Normal,
            error: String::new(),
            warnings: vec![],
            msg: String::new(),
            preview_index: self.themes.len() - 1,
            return_index: self.theme,
            known: lib.resolved,
            built: None,
            expanded: None,
            eyedropper: None,
            picking: None,
            last_edit: (String::new(), 0.0),
            confirm_close: false,
            changed: true,
            name_buf,
        };
        self.theme = st.preview_index;
        self.studio = Some(st);
        self.studio_rebuild(ctx);
    }

    /// Rebuilds the preview after a change.
    pub(crate) fn studio_rebuild(&mut self, ctx: &egui::Context) {
        let Some(st) = self.studio.as_mut() else { return };
        if !st.changed {
            return;
        }
        st.changed = false;
        match st.draft.build(&st.known) {
            Ok(p) => {
                st.error.clear();
                st.warnings = p.warnings;
                let taken = st.known.iter().any(|(n, _)| *n == p.theme.name && st.draft.file.is_none());
                if taken {
                    st.warnings.insert(0, format!("A theme called \"{}\" already exists; saving keeps both (the new one gets a number)", p.theme.name));
                }
                self.themes[st.preview_index] = theme_edit::simulate_theme(&p.theme, st.vision);
                st.built = Some(p.theme);
            }
            Err(e) => st.error = e,
        }
        self.apply_visuals(ctx);
    }

    /// Leaves the Studio: back to the theme you had, or to the one just saved.
    fn close_studio(&mut self, ctx: &egui::Context, select: Option<String>) {
        let Some(st) = self.studio.take() else { return };
        self.themes.truncate(st.preview_index);
        self.theme = st.return_index.min(self.themes.len() - 1);
        if let Some(name) = select {
            self.reload_themes(Some(&name));
        }
        self.apply_visuals(ctx);
    }

    /// Reloads the theme library (after saving, or when a file changed on
    /// disk), keeping the chosen theme and an open Studio's preview.
    pub(crate) fn reload_themes(&mut self, select: Option<&str>) {
        let keep = select.map(str::to_string).unwrap_or_else(|| {
            let idx = self.studio.as_ref().map(|s| s.return_index).unwrap_or(self.theme);
            self.themes.get(idx).map(|t| t.name.clone()).unwrap_or_default()
        });
        let preview = self.studio.as_ref().map(|s| self.themes[s.preview_index].clone());
        let lib = crate::theme::load_library(&crate::theme::user_dir());
        self.theme_problems = lib.problems.clone();
        self.themes = lib.themes;
        let idx = self.themes.iter().position(|t| t.name == keep).unwrap_or(0);
        if let (Some(st), Some(p)) = (self.studio.as_mut(), preview) {
            st.known = lib.resolved;
            self.themes.push(p);
            st.preview_index = self.themes.len() - 1;
            st.return_index = idx;
            self.theme = st.preview_index;
        } else {
            self.theme = idx;
        }
    }

    pub(crate) fn studio_picked_color(&mut self, ctx: &egui::Context, c: Color32) {
        let now = ctx.input(|i| i.time);
        if let Some(st) = self.studio.as_mut()
            && let Some(key) = st.picking.take()
        {
            st.edit(key, now, |d| d.set(&["colors", key], Value::String(theme_edit::hex(c))));
            st.msg = crate::i18n::tr(format!("Picked {}", theme_edit::hex(c)));
        }
        self.studio_rebuild(ctx);
    }

    pub(crate) fn studio_picture(&mut self, ctx: &egui::Context, path: Option<String>) {
        let now = ctx.input(|i| i.time);
        if let (Some(st), Some(path)) = (self.studio.as_mut(), path) {
            let mut layers = st.layers();
            let mut t = toml::Table::new();
            t.insert("type".into(), Value::String("picture".into()));
            t.insert("file".into(), Value::String(path.replace('\\', "/")));
            t.insert("frosted".into(), Value::Boolean(true));
            // Right above the sky, under everything else.
            let at = layers.iter().position(|l| l.get("type").and_then(|v| v.as_str()) == Some("sky")).map(|i| i + 1).unwrap_or(0);
            layers.insert(at, t);
            st.edit("add-picture", now, |d| d.set_layers(layers));
            st.expanded = Some(at);
        }
        self.studio_rebuild(ctx);
    }

    /// Draws the Studio panel (and the eyedropper's click catcher).
    pub(crate) fn studio_panel(&mut self, ctx: &egui::Context, screen: Rect) {
        let Some(mut st) = self.studio.take() else { return };
        let th = self.themes[self.theme].clone();
        let now = ctx.input(|i| i.time);
        let w = 410.0f32.min(screen.width() * 0.46);
        let panel = Rect::from_min_max(pos2(screen.max.x - w - 10.0, screen.min.y + 10.0), pos2(screen.max.x - 10.0, screen.max.y - 10.0));

        // Eyedropper: the next click on the app picks the color under it.
        if let Some(key) = st.eyedropper {
            egui::Area::new(egui::Id::unique("studio-eyedropper")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
                let resp = ui.allocate_response(screen.size(), Sense::click());
                let resp = resp.on_hover_cursor(egui::CursorIcon::Crosshair);
                if resp.clicked()
                    && let Some(pos) = resp.interact_pointer_pos()
                {
                    let ppp = ctx.pixels_per_point();
                    let tx = self.notifier();
                    let ctx2 = ctx.clone();
                    ctx.request_screenshot(move |img| {
                        let (x, y) = ((pos.x * ppp) as usize, (pos.y * ppp) as usize);
                        if x < img.size[0] && y < img.size[1] {
                            let c = img.pixels[y * img.size[0] + x];
                            tx.send(Msg::StudioPixel(Color32::from_rgb(c.r(), c.g(), c.b())));
                            ctx2.request_repaint();
                        }
                    });
                    st.picking = Some(key);
                    st.eyedropper = None;
                }
            });
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                st.eyedropper = None;
            }
        }

        let mut close: Option<Option<String>> = None;
        let mut save = false;
        let mut pick_picture = false;
        egui::Area::new(egui::Id::unique("theme-studio")).order(egui::Order::Foreground).fixed_pos(panel.min).show(ctx, |ui| {
            ui.allocate_exact_size(panel.size(), Sense::click());
            let p = ui.painter().clone();
            // A solid frosted backing, so the app behind doesn't show through the controls.
            let backing = if th.dark { aero::darken(th.glass, 0.25) } else { aero::lerp(Color32::WHITE, th.glass, 0.25) };
            aero::rrect_grad(&p, panel, 18.0, &[(0.0, with_alpha(backing, 240)), (1.0, with_alpha(backing, 248))]);
            aero::glass_panel(&p, panel, 18.0, &th);
            let inner = panel.shrink(16.0);
            embossed(&p, pos2(inner.min.x, inner.min.y + 12.0), Align2::LEFT_CENTER, "🎨 Theme Studio", bold(19.0), th.text);
            let base = st.draft.base();
            aero::fit_text(&p, pos2(inner.max.x, inner.min.y + 12.0), Align2::RIGHT_CENTER, &crate::i18n::tr(format!("built on {base}")), regular(12.0), th.soft, 170.0);

            ui.scope_builder(egui::UiBuilder::new().max_rect(Rect::from_min_max(pos2(inner.min.x, inner.min.y + 30.0), inner.max)), |ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                ui.set_max_width(inner.width());
                // Name.
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(crate::i18n::tr("Name")).strong());
                    let resp = ui.add(egui::TextEdit::singleline(&mut st.name_buf).desired_width(ui.available_width()));
                    if resp.changed() {
                        let name = st.name_buf.clone();
                        st.edit("name", now, |d| d.set(&["meta", "name"], Value::String(name)));
                    }
                });
                // Tools (they wrap onto a second line in longer languages).
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(st.draft.can_undo(), egui::Button::new(crate::i18n::tr("↩ Undo"))).clicked() {
                        st.draft.undo();
                        st.name_buf = st.draft.name();
                        st.changed = true;
                    }
                    if ui.add_enabled(st.draft.can_redo(), egui::Button::new(crate::i18n::tr("↪ Redo"))).clicked() {
                        st.draft.redo();
                        st.name_buf = st.draft.name();
                        st.changed = true;
                    }
                    if ui.button(crate::i18n::tr("🎲 Randomize")).on_hover_text(crate::i18n::tr("New harmonious colors, sky and hills; text stays readable")).clicked() {
                        let resolved = st.base_resolved();
                        let dark = st.built.as_ref().is_some_and(|t| t.dark);
                        let seed = (now * 1000.0) as u64;
                        st.edit("randomize", now, |d| theme_edit::randomize(d, &resolved, dark, seed));
                        st.last_edit.0.clear();
                    }
                    if ui.button(crate::i18n::tr("Reset")).on_hover_text(crate::i18n::tr("Undo every change: back to the base theme")).clicked() {
                        st.draft.reset_to_base();
                        st.changed = true;
                    }
                    if ui.button(crate::i18n::tr("Duplicate")).on_hover_text(crate::i18n::tr("Save as a new theme next time, keeping this one")).clicked() {
                        st.draft.checkpoint();
                        st.draft.file = None;
                        st.name_buf = format!("{} copy", st.draft.name());
                        let name = st.name_buf.clone();
                        st.draft.set(&["meta", "name"], Value::String(name));
                        st.changed = true;
                    }
                });
                // Tabs.
                ui.horizontal_wrapped(|ui| {
                    for (tab, label) in [(StudioTab::Colors, "Colors"), (StudioTab::Scene, "Scene"), (StudioTab::Material, "Material"), (StudioTab::Motion, "Motion & Sound")] {
                        if ui.selectable_label(st.tab == tab, crate::i18n::tr(label)).clicked() {
                            st.tab = tab;
                        }
                    }
                });
                ui.separator();

                let footer_h = 92.0;
                let body_h = (ui.available_height() - footer_h).max(80.0);
                egui::ScrollArea::vertical().max_height(body_h).auto_shrink([false, false]).show(ui, |ui| match st.tab {
                    StudioTab::Colors => colors_tab(ui, &mut st, now),
                    StudioTab::Scene => pick_picture = scene_tab(ui, &mut st, now),
                    StudioTab::Material => material_tab(ui, &mut st, now),
                    StudioTab::Motion => motion_tab(ui, &mut st, now),
                });
                ui.separator();
                // Messages.
                let (line, color) = if !st.error.is_empty() {
                    (format!("⚠ {}", crate::i18n::tr(&st.error)), rgba(220, 70, 50, 255))
                } else if let Some(w) = st.warnings.first() {
                    (format!("⚠ {}", crate::i18n::tr(w)), rgba(220, 140, 30, 255))
                } else if st.eyedropper.is_some() {
                    (crate::i18n::tr("Click anywhere in the app to pick its color (Esc cancels)"), th.text)
                } else {
                    (crate::i18n::tr(&st.msg), th.soft)
                };
                ui.add(egui::Label::new(egui::RichText::new(line).color(color).size(12.0)).wrap());
                ui.horizontal(|ui| {
                    let label = if st.draft.dirty { crate::i18n::tr("✔ Save") } else { crate::i18n::tr("✔ Saved") };
                    if ui.add_enabled(st.draft.dirty && st.error.is_empty(), egui::Button::new(egui::RichText::new(label).strong())).clicked() {
                        save = true;
                    }
                    let close_label = if st.confirm_close { crate::i18n::tr("Discard changes") } else { crate::i18n::tr("Close") };
                    if ui.button(close_label).clicked() {
                        if st.draft.dirty && !st.confirm_close {
                            st.confirm_close = true;
                            st.msg = crate::i18n::tr("You have unsaved changes: click again to close without saving");
                        } else {
                            close = Some(None);
                        }
                    }
                    if ui
                        .add_enabled(st.error.is_empty(), egui::Button::new(crate::i18n::tr("📤 Export")))
                        .on_hover_text(crate::i18n::tr("Save one file to share, with any pictures inside"))
                        .clicked()
                    {
                        st.msg = match st.draft.build(&st.known).and_then(|p| crate::theme_share::export(&p.resolved, st.draft.file.as_deref())) {
                            Ok(text) => {
                                let home = crate::platform::home();
                                let dir = [home.join("Documents"), home.clone()].into_iter().find(|d| d.is_dir()).unwrap_or(home);
                                let path = dir.join(crate::theme::file_name(&st.draft.name()));
                                match std::fs::write(&path, text) {
                                    Ok(()) => {
                                        tools::open_folder(&path);
                                        crate::i18n::tr(format!("Saved {} (ready to share)", path.display()))
                                    }
                                    Err(e) => crate::i18n::tr(format!("Could not save: {e}")),
                                }
                            }
                            Err(e) => crate::i18n::tr(format!("Could not save: {e}")),
                        };
                    }
                    if ui.button(crate::i18n::tr("📂 Themes folder")).clicked() {
                        let dir = crate::theme::user_dir();
                        let _ = std::fs::create_dir_all(&dir);
                        crate::platform::open_dir(&dir);
                    }
                });
            });
        });

        if save {
            match st.draft.save(&crate::theme::user_dir()) {
                Ok(path) => {
                    st.msg = crate::i18n::tr(format!("Saved {}", path.display()));
                    let name = st.built.as_ref().map(|t| t.name.clone()).unwrap_or_else(|| st.draft.name());
                    crate::theme::save_name(&name);
                    self.studio = Some(st);
                    self.reload_themes(Some(&name));
                    let st = self.studio.as_mut().unwrap();
                    st.changed = true;
                    self.studio_rebuild(ctx);
                    return;
                }
                Err(e) => st.error = format!("Could not save: {e}"),
            }
        }
        if pick_picture {
            let tx = self.notifier();
            let title = crate::i18n::tr("Choose a picture for the background");
            std::thread::spawn(move || tx.send(Msg::StudioPicture(crate::platform::pick_file(&title))));
        }
        let saved_name = (!st.draft.dirty && st.draft.file.is_some()).then(|| st.draft.name());
        self.studio = Some(st);
        if let Some(sel) = close {
            // A saved theme stays chosen; otherwise go back to the previous one.
            self.close_studio(ctx, sel.or(saved_name));
            return;
        }
        self.studio_rebuild(ctx);
    }
}

/// Contrast badge: ratio and whether it's comfortable to read.
fn contrast_badge(ui: &mut egui::Ui, ratio: f32, good: f32) {
    let (text, color) = if ratio >= good { (format!("✔ {ratio:.1}"), rgba(40, 150, 70, 255)) } else { (format!("⚠ {ratio:.1}"), rgba(210, 70, 40, 255)) };
    ui.label(egui::RichText::new(text).color(color).strong()).on_hover_text(crate::i18n::tr("Contrast with the glass behind it (4.5 or more reads comfortably)"));
}

fn colors_tab(ui: &mut egui::Ui, st: &mut Studio, now: f64) {
    let Some(t) = st.built.clone() else { return };
    let behind = if t.dark { t.glass } else { aero::lerp(Color32::WHITE, t.glass, 0.3) };
    for (key, label) in COLOR_KEYS {
        ui.horizontal(|ui| {
            ui.add_sized(vec2(110.0, 20.0), egui::Label::new(crate::i18n::tr(*label)));
            let mut c = color_of(&t, key);
            if egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::Opaque).changed() {
                st.edit(key, now, |d| d.set(&["colors", key], Value::String(theme_edit::hex(c))));
            }
            ui.monospace(theme_edit::hex(c));
            if ui.small_button("💧").on_hover_text(crate::i18n::tr("Pick this color from anywhere in the app")).clicked() {
                st.eyedropper = Some(key);
            }
            match *key {
                "text" => contrast_badge(ui, crate::theme::contrast(t.text, behind), 4.5),
                "soft" => contrast_badge(ui, crate::theme::contrast(t.soft, behind), 3.0),
                _ => {}
            }
        });
    }
    ui.add_space(4.0);
    ui.label(egui::RichText::new(crate::i18n::tr("Build matching colors from the accent")).strong());
    ui.horizontal(|ui| {
        for (kind, label) in [(Harmony::Complementary, "Complementary"), (Harmony::Analogous, "Analogous"), (Harmony::Triad, "Triad")] {
            if ui.button(crate::i18n::tr(label)).clicked() {
                let colors = theme_edit::harmony(t.accent, kind);
                st.edit(label, now, |d| {
                    for (k, c) in colors {
                        d.set(&["colors", k], Value::String(theme_edit::hex(c)));
                    }
                });
            }
        }
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(crate::i18n::tr("Preview as seen with"));
        let names = [(Vision::Normal, "typical color vision"), (Vision::Protanopia, "protanopia"), (Vision::Deuteranopia, "deuteranopia"), (Vision::Tritanopia, "tritanopia")];
        let current = names.iter().find(|(v, _)| *v == st.vision).map(|(_, n)| *n).unwrap_or("");
        egui::ComboBox::from_id_salt("studio-vision").selected_text(crate::i18n::tr(current)).show_ui(ui, |ui| {
            for (v, n) in names {
                if ui.selectable_label(st.vision == v, crate::i18n::tr(n)).clicked() {
                    st.vision = v;
                    st.changed = true;
                }
            }
        });
    });
}

/// A slider bound to a number in a layer's settings. Returns true when changed.
fn layer_slider(ui: &mut egui::Ui, t: &mut toml::Table, key: &str, label: &str, range: std::ops::RangeInclusive<f32>, default: f32, whole: bool) -> bool {
    let mut v = num(t, key, default);
    let mut s = egui::Slider::new(&mut v, range).text(crate::i18n::tr(label));
    if whole {
        s = s.integer();
    }
    if ui.add(s).changed() {
        t.insert(key.into(), if whole { Value::Integer(v.round() as i64) } else { Value::Float((v as f64 * 1000.0).round() / 1000.0) });
        return true;
    }
    false
}

fn layer_color(ui: &mut egui::Ui, t: &mut toml::Table, key: &str, label: &str, current: Color32) -> bool {
    let mut c = current;
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(crate::i18n::tr(label));
        if egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::Opaque).changed() {
            t.insert(key.into(), Value::String(theme_edit::hex(c)));
            changed = true;
        }
    });
    changed
}

fn stops_editor(ui: &mut egui::Ui, t: &mut toml::Table, key: &str, label: &str, stops: &[(f32, Color32)]) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.label(crate::i18n::tr(label));
        let mut s: Vec<(f32, Color32)> = stops.to_vec();
        for (_, c) in s.iter_mut() {
            if egui::color_picker::color_edit_button_srgba(ui, c, egui::color_picker::Alpha::Opaque).changed() {
                changed = true;
            }
        }
        if changed {
            t.insert(key.into(), theme_edit::stops_value(&s));
        }
    });
    changed
}

/// The scene: weight meter, layers (drag :: or the arrows to reorder), add a layer.
/// Returns true when the user asked to add a picture.
fn scene_tab(ui: &mut egui::Ui, st: &mut Studio, now: f64) -> bool {
    let Some(t) = st.built.clone() else { return false };
    let weight = crate::theme::scene_weight(&t.scene);
    let (label, color) = match weight {
        0..=150 => ("light", rgba(40, 150, 70, 255)),
        151..=250 => ("medium", rgba(200, 140, 30, 255)),
        _ => ("heavy: draws lighter on battery", rgba(210, 70, 40, 255)),
    };
    ui.horizontal(|ui| {
        ui.label(crate::i18n::tr("Scene weight"));
        ui.add(egui::ProgressBar::new((weight as f32 / 400.0).min(1.0)).desired_width(120.0).fill(color).text(format!("{weight}")));
        ui.label(egui::RichText::new(crate::i18n::tr(label)).color(color));
    });
    let mut layers = st.layers();
    let mut action: Option<(&str, usize)> = None;
    let mut moved: Option<(usize, usize)> = None;
    let mut changed_layer = false;
    let count = layers.len();
    #[allow(clippy::needless_range_loop)] // rows also reorder and remove by index
    for i in 0..count {
        let ty = layers[i].get("type").and_then(|v| v.as_str()).unwrap_or("?").to_string();
        let built = t.scene.get(i).filter(|l| l.kind.type_name() == ty).cloned();
        let frame = egui::Frame::group(ui.style()).inner_margin(6.0);
        let resp = frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.dnd_drag_source(egui::Id::unique(("studio-layer", i)), i, |ui| {
                    ui.label("::");
                })
                .response
                .on_hover_text(crate::i18n::tr("Drag to reorder"));
                let mut on = layers[i].get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                if ui.checkbox(&mut on, "").on_hover_text(crate::i18n::tr("Show this layer")).changed() {
                    layers[i].insert("enabled".into(), Value::Boolean(on));
                    changed_layer = true;
                }
                let open = st.expanded == Some(i);
                if ui.selectable_label(open, egui::RichText::new(crate::i18n::tr(layer_label(&ty))).strong()).clicked() {
                    st.expanded = if open { None } else { Some(i) };
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("🗑").on_hover_text(crate::i18n::tr("Remove this layer")).clicked() {
                        action = Some(("remove", i));
                    }
                    if ui.add_enabled(i + 1 < count, egui::Button::new("⬇").small()).clicked() {
                        moved = Some((i, i + 1));
                    }
                    if ui.add_enabled(i > 0, egui::Button::new("⬆").small()).clicked() {
                        moved = Some((i, i - 1));
                    }
                });
            });
            if st.expanded == Some(i) {
                let l = &mut layers[i];
                changed_layer |= layer_slider(ui, l, "opacity", "Opacity", 0.0..=1.0, 1.0, false);
                changed_layer |= layer_slider(ui, l, "speed", "Speed", 0.0..=5.0, 1.0, false);
                if let Some(b) = &built {
                    changed_layer |= layer_settings(ui, l, &b.kind, st);
                }
            }
        });
        // Dropping a dragged layer here moves it to this place.
        if let Some(from) = resp.response.dnd_release_payload::<usize>() {
            moved = Some((*from, i));
        }
    }
    let mut add_picture = false;
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("studio-add-layer").selected_text(crate::i18n::tr("+ Add a layer")).show_ui(ui, |ui| {
            for kind in theme_edit::ADDABLE {
                if ui.selectable_label(false, crate::i18n::tr(layer_label(kind))).clicked()
                    && let Some(t) = theme_edit::new_layer(kind)
                {
                    layers.push(t);
                    st.expanded = Some(layers.len() - 1);
                    changed_layer = true;
                }
            }
            if ui.selectable_label(false, crate::i18n::tr("Picture…")).clicked() {
                add_picture = true;
            }
        });
    });
    if let Some(("remove", i)) = action {
        layers.remove(i);
        st.expanded = None;
        changed_layer = true;
    }
    if let Some((from, to)) = moved
        && from != to
        && from < layers.len()
        && to < layers.len()
    {
        let l = layers.remove(from);
        layers.insert(to, l);
        st.expanded = st.expanded.map(|e| if e == from { to } else { e });
        changed_layer = true;
    }
    if changed_layer {
        let key = format!("layers-{:?}", st.expanded);
        st.edit(&key, now, |d| d.set_layers(layers));
    }
    add_picture
}

fn layer_label(ty: &str) -> &'static str {
    match ty {
        "sky" => "Sky",
        "sun" => "Sun",
        "moon" => "Moon",
        "rays" => "Light rays",
        "stars" => "Stars",
        "aurora" => "Aurora",
        "hills" => "Hills",
        "bubbles" => "Bubbles",
        "clouds" => "Clouds",
        "fish" => "Fish",
        "fireflies" => "Fireflies",
        "petals" => "Petals",
        "snow" => "Snow",
        "leaves" => "Leaves",
        "picture" => "Picture",
        _ => "Layer",
    }
}

/// A layer's own settings. Returns true when one changed.
fn layer_settings(ui: &mut egui::Ui, l: &mut toml::Table, kind: &Layer, st: &Studio) -> bool {
    let _ = st;
    let mut ch = false;
    match kind {
        Layer::Sky { stops } => ch |= stops_editor(ui, l, "stops", "Colors, top to bottom", stops),
        Layer::Sun { pos, color } | Layer::Moon { pos, color } => {
            ch |= layer_slider(ui, l, "x", "Horizontal", -0.2..=1.2, pos.x, false);
            ch |= layer_slider(ui, l, "y", "Vertical", -0.3..=1.0, pos.y, false);
            ch |= layer_color(ui, l, "color", "Color", *color);
        }
        Layer::Rays { pos, opacity } => {
            ch |= layer_slider(ui, l, "strength", "Brightness", 0.0..=120.0, *opacity as f32, true);
            ch |= layer_slider(ui, l, "x", "Horizontal", -0.2..=1.2, pos.x, false);
            ch |= layer_slider(ui, l, "y", "Vertical", -0.5..=1.0, pos.y, false);
        }
        Layer::Stars { count } => ch |= layer_slider(ui, l, "count", "How many", 0.0..=400.0, *count as f32, true),
        Layer::Aurora => {
            ui.label(crate::i18n::tr("Green, blue and violet curtains of light."));
        }
        Layer::Hills { back, front } => {
            ch |= stops_editor(ui, l, "back", "Back hill", back);
            ch |= stops_editor(ui, l, "front", "Front hill", front);
        }
        Layer::Bubbles { color, count } => {
            ch |= layer_slider(ui, l, "count", "How many", 0.0..=80.0, *count as f32, true);
            ch |= layer_color(ui, l, "color", "Color", *color);
        }
        Layer::Clouds { color, count, y, size } => {
            ch |= layer_slider(ui, l, "count", "How many", 0.0..=20.0, *count as f32, true);
            ch |= layer_slider(ui, l, "y", "Height", -0.2..=1.2, *y, false);
            ch |= layer_slider(ui, l, "size", "Size", 0.2..=3.0, *size, false);
            ch |= layer_color(ui, l, "color", "Color", *color);
        }
        Layer::Fish { color, count, y } => {
            ch |= layer_slider(ui, l, "count", "How many", 0.0..=30.0, *count as f32, true);
            ch |= layer_slider(ui, l, "y", "Height", 0.0..=1.0, *y, false);
            ch |= layer_color(ui, l, "color", "Color", *color);
        }
        Layer::Fireflies { color, count } | Layer::Petals { color, count } | Layer::Leaves { color, count } => {
            ch |= layer_slider(ui, l, "count", "How many", 0.0..=150.0, *count as f32, true);
            ch |= layer_color(ui, l, "color", "Color", *color);
        }
        Layer::Snow { color, count } => {
            ch |= layer_slider(ui, l, "count", "How many", 0.0..=300.0, *count as f32, true);
            ch |= layer_color(ui, l, "color", "Color", *color);
        }
        Layer::Picture { path, frosted } => {
            ui.label(egui::RichText::new(path.display().to_string()).monospace().size(11.0));
            let mut f = *frosted;
            if ui.checkbox(&mut f, crate::i18n::tr("Frosted (softer, easier to read over)")).changed() {
                l.insert("frosted".into(), Value::Boolean(f));
                ch = true;
            }
        }
    }
    ch
}

fn material_tab(ui: &mut egui::Ui, st: &mut Studio, now: f64) {
    let Some(t) = st.built.clone() else { return };
    let m = t.material;
    let mut dark = t.dark;
    if ui.checkbox(&mut dark, crate::i18n::tr("Dark smoked glass")).changed() {
        st.edit("dark", now, |d| d.set(&["material", "dark_glass"], Value::Boolean(dark)));
    }
    let slider = |ui: &mut egui::Ui, st: &mut Studio, key: &str, path: [&'static str; 2], label: &str, v: f32, range: std::ops::RangeInclusive<f32>| {
        let mut v = v;
        if ui.add(egui::Slider::new(&mut v, range).text(crate::i18n::tr(label))).changed() {
            st.edit(key, now, |d| d.set(&path, Value::Float((v as f64 * 100.0).round() / 100.0)));
        }
    };
    slider(ui, st, "corners", ["material", "corners"], "Roundness", m.corners, 0.0..=2.0);
    slider(ui, st, "gloss", ["material", "gloss"], "Gloss", m.gloss, 0.0..=1.5);
    slider(ui, st, "glass", ["material", "glass"], "Glass opacity", m.glass, 0.3..=1.5);
    ui.horizontal(|ui| {
        ui.label(crate::i18n::tr("Buttons"));
        for (style, key, label) in
            [(crate::theme::ButtonStyle::Aqua, "aqua", "Aqua"), (crate::theme::ButtonStyle::Soft, "soft", "Soft"), (crate::theme::ButtonStyle::Flat, "flat", "Flat")]
        {
            if ui.radio(m.buttons == style, crate::i18n::tr(label)).clicked() {
                st.edit("buttons", now, |d| d.set(&["material", "buttons"], Value::String(key.into())));
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label(crate::i18n::tr("Font"));
        for (choice, key, label) in [(crate::theme::FontChoice::System, "system", "System"), (crate::theme::FontChoice::Builtin, "builtin", "Built-in")] {
            if ui.radio(t.font == choice, crate::i18n::tr(label)).clicked() {
                st.edit("font", now, |d| d.set(&["font", "family"], Value::String(key.into())));
            }
        }
    });
    slider(ui, st, "font-size", ["font", "size"], "Text size", m.font_size, 0.8..=1.3);
}

fn motion_tab(ui: &mut egui::Ui, st: &mut Studio, now: f64) {
    let Some(t) = st.built.clone() else { return };
    let mut speed = t.motion_speed;
    if ui.add(egui::Slider::new(&mut speed, 0.0..=3.0).text(crate::i18n::tr("Animation speed"))).changed() {
        st.edit("speed", now, |d| d.set(&["motion", "speed"], Value::Float((speed as f64 * 100.0).round() / 100.0)));
    }
    ui.horizontal(|ui| {
        ui.label(crate::i18n::tr("Page transition"));
        for (tr, key, label) in
            [(crate::theme::Transition::None, "none", "None"), (crate::theme::Transition::Fade, "fade", "Fade"), (crate::theme::Transition::Slide, "slide", "Glassy slide")]
        {
            if ui.radio(t.transition == tr, crate::i18n::tr(label)).clicked() {
                st.edit("transition", now, |d| d.set(&["motion", "transition"], Value::String(key.into())));
            }
        }
    });
    ui.label(egui::RichText::new(crate::i18n::tr("Reduce motion (in Settings) always turns animations and transitions off.")).size(12.0).weak());
    ui.add_space(8.0);
    ui.label(egui::RichText::new(crate::i18n::tr("Sounds")).strong());
    ui.label(egui::RichText::new(crate::i18n::tr("Sound packs come in a later update. Theme files can already name one.")).size(12.0).weak());
}
