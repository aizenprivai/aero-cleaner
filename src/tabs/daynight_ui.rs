//! Day and night: switching themes on their own, with a crossfade.

use super::prelude::*;
use crate::daynight::{self, Mode, Sun};

impl App {
    /// Shows theme `i`, crossfading from the current one (unless motion is reduced).
    pub(crate) fn switch_theme(&mut self, i: usize, ctx: &egui::Context) {
        if i == self.theme || i >= self.themes.len() {
            return;
        }
        if !self.settings.reduce_motion {
            self.fade = Some((self.themes[self.theme].clone(), ctx.input(|inp| inp.time)));
        }
        self.theme = i;
        self.apply_visuals(ctx);
    }

    fn system_dark(ctx: &egui::Context) -> Option<bool> {
        let egui_says = ctx.system_theme().map(|t| t == egui::Theme::Dark);
        #[cfg(target_os = "linux")]
        return daynight::linux_dark().or(egui_says);
        #[cfg(not(target_os = "linux"))]
        egui_says
    }

    /// Whether it's night for the chosen mode, right now.
    pub(crate) fn night_now(&self, ctx: &egui::Context) -> Option<bool> {
        let s = &self.settings;
        let now = daynight::now();
        let coords = daynight::coords(daynight::zone_name().as_deref(), now.offset);
        let dark = if s.auto_theme == Mode::System { Self::system_dark(ctx) } else { None };
        daynight::is_night(s.auto_theme, now, (s.day_at, s.night_at), dark, coords)
    }

    /// Every few seconds: if day turned to night (or back), show that theme.
    /// A theme picked by hand stays until the next change.
    pub(crate) fn auto_theme_tick(&mut self, ctx: &egui::Context, time: f64) {
        if self.settings.auto_theme == Mode::Off {
            self.auto_night = None;
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(5));
        if time - self.auto_check_at < 4.0 && self.auto_night.is_some() {
            return;
        }
        // Not while a theme is being edited or previewed.
        if self.studio.is_some() || self.gallery.preview.is_some() {
            return;
        }
        self.auto_check_at = time;
        let Some(night) = self.night_now(ctx) else { return };
        if self.auto_night == Some(night) {
            return;
        }
        self.auto_night = Some(night);
        let name = if night { &self.settings.night_theme } else { &self.settings.day_theme };
        if let Some(i) = self.themes.iter().position(|t| &t.name == name) {
            self.switch_theme(i, ctx);
        }
    }

    /// Today's touches (worked out at most every 30 seconds), or None when
    /// they're all off or a theme is being edited or previewed.
    pub(crate) fn current_touches(&mut self, time: f64) -> Option<crate::touches::Touches> {
        let s = &self.settings;
        if !(s.seasons || s.live_sky || s.fun_dates) || self.studio.is_some() || self.gallery.preview.is_some() {
            self.touches = None;
            return None;
        }
        ctx_repaint_soon(&self.ctx);
        if self.touches.is_none() || time - self.touches_at > 30.0 {
            self.touches_at = time;
            let now = daynight::now();
            let coords = daynight::coords(daynight::zone_name().as_deref(), now.offset);
            self.touches = crate::touches::compute(s.seasons, s.live_sky, s.fun_dates, now, coords);
        }
        self.touches.clone()
    }

    /// The settings, above the theme cards. Returns the room left for the cards.
    pub(crate) fn daynight_panel(&mut self, ui: &mut egui::Ui, area: Rect) -> Rect {
        let th = self.themes[self.theme].clone();
        // As tall as its content was last frame (it wraps in narrow windows and long languages).
        let height_id = egui::Id::unique("daynight-panel-height");
        let h = ui.ctx().data(|d| d.get_temp::<f32>(height_id)).unwrap_or(80.0);
        let r = Rect::from_min_size(area.min, vec2(area.width(), h + 24.0));
        aero::surface(ui.painter(), r, 14.0, &th, false, None);
        let names: Vec<String> = self.themes.iter().take(self.studio.as_ref().map(|s| s.preview_index).unwrap_or(self.themes.len())).map(|t| t.name.clone()).collect();
        let before = (self.settings.auto_theme, self.settings.day_theme.clone(), self.settings.night_theme.clone(), self.settings.day_at, self.settings.night_at);
        let touches_before = (self.settings.seasons, self.settings.live_sky, self.settings.fun_dates);
        let touches_status = self.touches_status();
        let status = self.daynight_status(ui.ctx());
        let used = ui.scope_builder(egui::UiBuilder::new().max_rect(r.shrink2(vec2(16.0, 12.0))), |ui| {
            ui.style_mut().visuals.override_text_color = Some(th.text);
            ui.spacing_mut().item_spacing = vec2(10.0, 8.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(crate::i18n::tr("Switch themes")).strong());
                for m in Mode::ALL {
                    ui.radio_value(&mut self.settings.auto_theme, m, crate::i18n::tr(m.label()));
                }
            });
            let on = self.settings.auto_theme != Mode::Off;
            ui.add_enabled_ui(on, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(crate::i18n::tr("☀ Day theme"));
                    theme_combo(ui, "daynight-day", &mut self.settings.day_theme, &names);
                    ui.add_space(12.0);
                    ui.label(crate::i18n::tr("🌙 Night theme"));
                    theme_combo(ui, "daynight-night", &mut self.settings.night_theme, &names);
                });
                if self.settings.auto_theme == Mode::Schedule {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(crate::i18n::tr("Day from"));
                        time_combo(ui, "daynight-day-at", &mut self.settings.day_at);
                        ui.add_space(6.0);
                        ui.label(crate::i18n::tr("night from"));
                        time_combo(ui, "daynight-night-at", &mut self.settings.night_at);
                    });
                }
            });
            ui.label(egui::RichText::new(status).color(th.soft).size(12.5));
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(crate::i18n::tr("Live touches")).strong());
                ui.checkbox(&mut self.settings.seasons, crate::i18n::tr("❄ Seasons"))
                    .on_hover_text(crate::i18n::tr("Snow in winter, petals in spring, fireflies on summer nights, leaves in autumn"));
                ui.checkbox(&mut self.settings.live_sky, crate::i18n::tr("☀ Live sky")).on_hover_text(crate::i18n::tr("The sun and its light follow the real time of day"));
                ui.checkbox(&mut self.settings.fun_dates, crate::i18n::tr("🎉 Fun dates"))
                    .on_hover_text(crate::i18n::tr("Small touches on New Year, Valentine's Day, Halloween and Christmas"));
            });
            if !touches_status.is_empty() {
                ui.label(egui::RichText::new(touches_status).color(th.soft).size(12.5));
            }
        });
        let content = used.response.rect.height();
        if (content - h).abs() > 0.5 {
            ui.ctx().data_mut(|d| d.insert_temp(height_id, content));
            ui.ctx().request_repaint();
        }
        let after = (self.settings.auto_theme, self.settings.day_theme.clone(), self.settings.night_theme.clone(), self.settings.day_at, self.settings.night_at);
        if (self.settings.seasons, self.settings.live_sky, self.settings.fun_dates) != touches_before {
            self.settings.save();
            self.touches = None;
        }
        if after != before {
            self.settings.save();
            // Apply the new settings right away.
            self.auto_night = None;
        }
        Rect::from_min_max(pos2(area.min.x, r.max.y + 14.0), area.max)
    }

    /// "Now: winter snow. The sun follows the time of day."
    fn touches_status(&self) -> String {
        let s = &self.settings;
        let Some(t) = &self.touches else { return String::new() };
        let mut out = vec![];
        if let (true, Some(season)) = (s.seasons, t.season) {
            out.push(crate::i18n::tr(format!("Now: {}.", crate::i18n::tr(season.label()))));
        }
        if s.live_sky {
            out.push(crate::i18n::tr("The sun follows the time of day."));
        }
        if let (true, Some(d)) = (s.fun_dates, t.date) {
            out.push(crate::i18n::tr(d.label()));
        }
        out.join(" ")
    }

    /// "It's night now (sunset was at 17:14)."
    fn daynight_status(&self, ctx: &egui::Context) -> String {
        let s = &self.settings;
        let now = daynight::now();
        let night = self.night_now(ctx);
        let which = match night {
            Some(true) => crate::i18n::tr(format!("It's night now: {}.", s.night_theme)),
            Some(false) => crate::i18n::tr(format!("It's day now: {}.", s.day_theme)),
            None => String::new(),
        };
        let missing = [&s.day_theme, &s.night_theme].into_iter().find(|n| !self.themes.iter().any(|t| &t.name == *n));
        let extra = match s.auto_theme {
            Mode::Off => crate::i18n::tr("AeroClean keeps the theme you pick."),
            Mode::System if night.is_none() => crate::i18n::tr("Your system doesn't say whether it's in dark mode."),
            Mode::System => String::new(),
            Mode::Schedule => String::new(),
            Mode::Sun => {
                let zone = daynight::zone_name();
                let (lat, lon) = daynight::coords(zone.as_deref(), now.offset);
                let place = zone.unwrap_or_else(|| crate::i18n::tr("your time zone"));
                match daynight::sun(now.day, lat, lon, now.offset) {
                    Sun::RiseSet(rise, set) => crate::i18n::tr(format!("Today: sunrise {}, sunset {} (estimated for {place}).", daynight::clock(rise), daynight::clock(set))),
                    Sun::AlwaysUp => crate::i18n::tr("The sun doesn't set today."),
                    Sun::AlwaysDown => crate::i18n::tr("The sun doesn't rise today."),
                }
            }
        };
        let mut out = [which, extra].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ");
        if let Some(n) = missing
            && s.auto_theme != Mode::Off
        {
            out = crate::i18n::tr(format!("⚠ There's no theme called {n} any more: pick another."));
        }
        out
    }
}

fn theme_combo(ui: &mut egui::Ui, id: &str, value: &mut String, names: &[String]) {
    egui::ComboBox::from_id_salt(id).selected_text(value.as_str()).width(150.0).show_ui(ui, |ui| {
        for n in names {
            ui.selectable_value(value, n.clone(), n.as_str());
        }
    });
}

/// Every half hour of the day.
fn time_combo(ui: &mut egui::Ui, id: &str, value: &mut u32) {
    egui::ComboBox::from_id_salt(id).selected_text(daynight::clock(*value as f64)).width(70.0).show_ui(ui, |ui| {
        for m in (0..48).map(|h| h * 30) {
            ui.selectable_value(value, m, daynight::clock(m as f64));
        }
    });
}

/// The sky moves slowly: a frame every half minute is enough when nothing else is moving.
fn ctx_repaint_soon(ctx: &egui::Context) {
    ctx.request_repaint_after(std::time::Duration::from_secs(30));
}
