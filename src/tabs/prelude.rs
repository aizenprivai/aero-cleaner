//! Imports shared by all tab modules.
pub(super) use crate::aero::{self, bold, embossed, regular, rgba, with_alpha};
pub(super) use crate::cleaner::{format_bytes, CATEGORIES};
pub(super) use crate::theme::{self, Theme};
pub(super) use crate::tools::{self, BigFile};
pub(super) use crate::{settings, App, Msg, Phase};
pub(super) use eframe::egui::{self, pos2, vec2, Align2, Rect, Sense, Stroke, Vec2};
