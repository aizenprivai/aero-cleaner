//! Imports shared by all tab modules.
pub(super) use crate::aero::{self, bold, embossed, regular, rgba, with_alpha};
pub(super) use crate::cleaner::{CATEGORIES, format_bytes};
pub(super) use crate::theme::{self, Theme};
pub(super) use crate::tools::{self, BigFile};
pub(super) use crate::{App, Msg, Phase, settings};
pub(super) use eframe::egui::{self, Align2, Rect, Sense, Stroke, Vec2, pos2, vec2};
