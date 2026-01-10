//! Theme system for organizing and selecting style collections.
//!
//! This module provides:
//!
//! - [`Theme`]: A named collection of styles with fluent builder API
//! - [`AdaptiveTheme`]: Light/dark theme pairs with OS detection
//! - [`ThemeChoice`]: Reference type for selecting themes at render time
//! - [`ColorMode`]: Light or dark color mode enum
//!
//! Themes wrap the style system and provide a higher-level API for
//! building and selecting style collections.

mod adaptive;
mod choice;
#[allow(clippy::module_inception)]
mod theme;

pub use adaptive::{set_theme_detector, AdaptiveTheme, ColorMode};
pub use choice::ThemeChoice;
pub use theme::Theme;
