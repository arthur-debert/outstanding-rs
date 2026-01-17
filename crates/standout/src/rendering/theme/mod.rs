//! Adaptive themes with automatic light/dark mode support.
//!
//! Themes are named collections of styles that automatically adapt to the user's
//! OS color scheme. Unlike systems with separate "light theme" and "dark theme"
//! files, Standout's themes define mode-specific variations at the style level,
//! eliminating duplication for styles that don't change between modes.
//!
//! ## Design Decision: Style-Level Adaptation
//!
//! Most styles (bold, italic, semantic colors) look fine in both modes. Only a
//! handful need adjustment — typically foreground colors for contrast. By making
//! adaptation per-style rather than per-theme, you define shared styles once and
//! override only what differs:
//!
//! ```yaml
//! # Shared across all modes
//! header:
//!   fg: cyan
//!   bold: true
//!
//! # Mode-specific overrides
//! panel:
//!   fg: gray          # Base (fallback)
//!   light:
//!     fg: black       # Override for light mode
//!   dark:
//!     fg: white       # Override for dark mode
//! ```
//!
//! ## How Merging Works
//!
//! When resolving a style in Dark mode:
//! 1. Start with base attributes (`fg: gray`)
//! 2. Merge dark overrides — each attribute in `dark:` replaces the base
//! 3. Result: `fg: white` (from dark), other attributes preserved from base
//!
//! This is additive: `Some` values in overrides replace, missing values preserve base.
//!
//! ## Color Mode Detection
//!
//! [`detect_color_mode`] queries the OS for the user's preferred scheme. Override
//! it for testing with [`set_theme_detector`]:
//!
//! ```rust,ignore
//! standout::set_theme_detector(|| ColorMode::Dark);
//! ```
//!
//! ## Construction
//!
//! **Programmatic** (for compile-time themes):
//! ```rust
//! use standout::Theme;
//! use console::Style;
//!
//! let theme = Theme::new()
//!     .add("header", Style::new().bold().cyan())
//!     .add_adaptive("panel", Style::new(),
//!         Some(Style::new().fg(console::Color::Black)),
//!         Some(Style::new().fg(console::Color::White)));
//! ```
//!
//! **YAML** (for user-customizable themes):
//! ```rust
//! let theme = standout::Theme::from_yaml(r#"
//! header: { fg: cyan, bold: true }
//! panel:
//!   fg: gray
//!   light: { fg: black }
//!   dark: { fg: white }
//! "#).unwrap();
//! ```
//!
//! ## See Also
//!
//! - [`crate::stylesheet`]: YAML parsing details and color format reference
//! - [`crate::style`]: Low-level style primitives and aliasing

mod adaptive;
#[allow(clippy::module_inception)]
mod theme;

pub use adaptive::{detect_color_mode, set_theme_detector, ColorMode};
pub use theme::Theme;
