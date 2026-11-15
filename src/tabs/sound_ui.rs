//! Sounds: playing them, and their part of the Settings screen.

use super::prelude::*;
use crate::settings::Settings;
use crate::sound::{self, Event, Pack};

impl App {
    /// Plays an event's sound, if sounds are on, that one isn't turned off,
    /// and the window is in front. The theme picks the voice.
    pub(crate) fn play_sound(&self, event: Event) {
        let s = &self.settings;
        if !s.sound_on || s.sound_off.contains(&event) || !self.window_focused {
            return;
        }
        self.play_sound_now(event);
    }

    /// Plays it whatever the settings say (the Settings screen's ▶ buttons).
    pub(crate) fn play_sound_now(&self, event: Event) {
        let th = &self.themes[self.theme];
        // Themes ask for 0.5 by default: that's "as loud as your setting".
        let volume = (self.settings.sound_volume as f32 / 100.0 * th.sound.1 * 2.0).clamp(0.0, 1.0);
        sound::play(event, Pack::from_name(&th.sound.0), volume);
    }
}

fn label(e: Event) -> (&'static str, &'static str, &'static str) {
    match e {
        Event::CleanDone => ("✨", "Clean finished", "A rising chime when space has been freed"),
        Event::ScanDone => ("🔍", "Scan finished", "Two soft notes (off at first: a scan runs every time AeroClean opens)"),
        Event::Click => ("🖱", "Switching screens", "A gentle click"),
        Event::Warning => ("⚠", "Something went wrong", "A friendly two-tone, never harsh"),
    }
}

/// What the user did in the section.
pub(crate) enum SoundAction {
    Changed,
    Preview(Event),
}

/// The SOUNDS section of the Settings screen.
pub(crate) fn sounds_section(ui: &mut egui::Ui, w: f32, s: &mut Settings, th: &Theme) -> Option<SoundAction> {
    let mut action = None;
    let resp =
        super::settings_toggle_row(ui, w, 0.0, s.sound_on, "🔔", "Play sounds", "An original sound pack made for AeroClean. Silent when the window is in the background.", th);
    if resp.clicked() {
        s.sound_on = !s.sound_on;
        action = Some(SoundAction::Changed);
        if s.sound_on {
            action = Some(SoundAction::Preview(Event::CleanDone));
        }
    }
    if !s.sound_on {
        return action;
    }
    let pack = Pack::from_name(&th.sound.0);
    let voice = match pack {
        Pack::Glass => "Glassy bells (from your theme)",
        Pack::Soft => "Soft marimba (from your theme)",
        Pack::None => "Your theme is silent: pick another theme to hear sounds",
    };
    if super::settings_option_row(ui, w - 40.0, 40.0, "🔊", "Volume", voice, &format!("{}%", s.sound_volume), "Change the volume", th) {
        s.sound_volume = match s.sound_volume {
            v if v < 20 => 20,
            v if v < 40 => 40,
            v if v < 60 => 60,
            v if v < 80 => 80,
            v if v < 100 => 100,
            _ => 10,
        };
        action = Some(SoundAction::Preview(Event::ScanDone));
    }
    for e in Event::ALL {
        let (icon, title, desc) = label(e);
        let on = !s.sound_off.contains(&e);
        let resp = super::settings_toggle_row(ui, w, 40.0, on, icon, title, desc, th);
        // A ▶ button on the right to hear it.
        let r = resp.rect;
        let play = Rect::from_min_size(pos2(r.max.x - 52.0, r.center().y - 15.0), vec2(44.0, 30.0));
        let heard = ui.push_id(("sound-try", e.code()), |ui| aero::aqua_button(ui, play, "▶", th.accent, pack != Pack::None).on_hover_text(crate::i18n::tr("Listen"))).inner;
        let spoken = crate::i18n::tr(format!("Listen: {}", crate::i18n::tr(title)));
        heard.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, pack != Pack::None, spoken.clone()));
        if heard.clicked() {
            action = Some(SoundAction::Preview(e));
        } else if resp.clicked() {
            if on {
                s.sound_off.push(e);
            } else {
                s.sound_off.retain(|x| *x != e);
            }
            action = Some(SoundAction::Changed);
        }
    }
    action
}
