//! Theme system for organizing and selecting style collections.
//!
//! This module provides:
//!
//! - [`Theme`]: A named collection of adaptive styles with fluent builder API
//! - [`ColorMode`]: Light or dark color mode enum
//! - [`detect_color_mode`]: Detect the user's preferred color mode from OS
//! - [`set_theme_detector`]: Override color mode detection for testing
//!
//! # Adaptive Themes
//!
//! Themes in Outstanding are inherently adaptive. Individual styles can define
//! mode-specific variations that are automatically selected based on the user's
//! OS color mode (light/dark).
//!
//! ## Programmatic Construction
//!
//! ```rust
//! use outstanding::{Theme, ColorMode};
//! use console::Style;
//!
//! let theme = Theme::new()
//!     // Non-adaptive style (same in all modes)
//!     .add("muted", Style::new().dim())
//!     // Adaptive style with light/dark variants
//!     .add_adaptive(
//!         "panel",
//!         Style::new(),                          // Base
//!         Some(Style::new().fg(console::Color::Black)), // Light mode
//!         Some(Style::new().fg(console::Color::White)), // Dark mode
//!     );
//! ```
//!
//! ## From YAML
//!
//! ```rust,ignore
//! use outstanding::Theme;
//!
//! let theme = Theme::from_yaml(r#"
//! panel:
//!   fg: gray
//!   light:
//!     fg: black
//!   dark:
//!     fg: white
//! "#)?;
//! ```

mod adaptive;
#[allow(clippy::module_inception)]
mod theme;

pub use adaptive::{detect_color_mode, set_theme_detector, ColorMode};
pub use theme::Theme;
