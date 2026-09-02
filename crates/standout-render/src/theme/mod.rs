//! Adaptive themes with automatic light/dark mode support.
//!
//! Themes are named collections of styles that adapt to the OS color scheme.
//! Adaptation happens at the style level, not the theme level: a style has
//! base attributes plus optional light/dark overrides, so shared attributes
//! are declared once and only the differences need overriding per mode.
//!
//! Resolving a style merges the active mode's overrides onto the base:
//! present values in the override replace the base, missing values fall
//! through to it. Color-scheme mode is a fact on [`crate::TargetProperties`];
//! convenience wrappers and `App::run` detect it, tests construct
//! `TargetProperties` with an explicit [`ColorMode`].
//!
//! See [`crate::style`] for the style primitives and stylesheet formats.

mod adaptive;
mod icon_def;
mod icon_mode;
#[allow(clippy::module_inception)]
mod theme;

pub(crate) use adaptive::probe_color_mode;
pub use adaptive::ColorMode;
pub use icon_def::{IconDefinition, IconSet};
pub(crate) use icon_mode::probe_icon_mode;
pub use icon_mode::IconMode;
pub use theme::Theme;
