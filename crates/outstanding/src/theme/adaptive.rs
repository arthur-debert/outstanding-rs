//! Color mode detection for adaptive themes.
//!
//! This module provides color mode detection for themes that adapt to the
//! user's OS display mode (light/dark).
//!
//! # Usage
//!
//! Color mode detection is typically handled automatically by the render
//! functions. Use [`set_theme_detector`] to override detection for testing.
//!
//! ```rust
//! use outstanding::{Theme, ColorMode, set_theme_detector};
//! use console::Style;
//!
//! // Create an adaptive theme
//! let theme = Theme::new()
//!     .add_adaptive(
//!         "panel",
//!         Style::new(),
//!         Some(Style::new().fg(console::Color::Black)), // Light mode
//!         Some(Style::new().fg(console::Color::White)), // Dark mode
//!     );
//!
//! // For testing, override the detector
//! set_theme_detector(|| ColorMode::Dark);
//! ```

use dark_light::{detect as detect_os_theme, Mode as OsThemeMode};
use once_cell::sync::Lazy;
use std::sync::Mutex;

/// The user's preferred color mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// Light mode (light background, dark text).
    Light,
    /// Dark mode (dark background, light text).
    Dark,
}

type ThemeDetector = fn() -> ColorMode;

static THEME_DETECTOR: Lazy<Mutex<ThemeDetector>> = Lazy::new(|| Mutex::new(os_theme_detector));

/// Overrides the detector used to determine whether the user prefers a light or dark theme.
///
/// This is useful for testing or when you want to force a specific color mode.
///
/// # Example
///
/// ```rust
/// use outstanding::{ColorMode, set_theme_detector};
///
/// // Force dark mode for testing
/// set_theme_detector(|| ColorMode::Dark);
///
/// // Reset to OS detection (if needed)
/// // Note: There's no direct way to reset to OS detection,
/// // but tests should restore their changes.
/// ```
pub fn set_theme_detector(detector: ThemeDetector) {
    let mut guard = THEME_DETECTOR.lock().unwrap();
    *guard = detector;
}

/// Detects the user's preferred color mode from the OS.
///
/// Uses the `dark-light` crate to query the OS for the current theme preference.
/// The detector can be overridden via [`set_theme_detector`] for testing.
///
/// # Returns
///
/// - [`ColorMode::Light`] if the OS is in light mode
/// - [`ColorMode::Dark`] if the OS is in dark mode
pub fn detect_color_mode() -> ColorMode {
    let detector = THEME_DETECTOR.lock().unwrap();
    (*detector)()
}

fn os_theme_detector() -> ColorMode {
    match detect_os_theme() {
        OsThemeMode::Dark => ColorMode::Dark,
        OsThemeMode::Light => ColorMode::Light,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{render_with_output, OutputMode, Theme};
    use console::Style;
    use serde::Serialize;

    #[derive(Serialize)]
    struct SimpleData {
        message: String,
    }

    #[test]
    fn test_adaptive_theme_uses_detector() {
        console::set_colors_enabled(true);

        // Create an adaptive theme with different colors for light/dark modes
        let theme = Theme::new().add_adaptive(
            "tone",
            Style::new(), // base (unused since we always have overrides)
            Some(Style::new().green().force_styling(true)), // Light mode
            Some(Style::new().red().force_styling(true)), // Dark mode
        );

        let data = SimpleData {
            message: "hi".into(),
        };

        // Test dark mode
        set_theme_detector(|| ColorMode::Dark);
        let dark_output = render_with_output(
            r#"[tone]{{ message }}[/tone]"#,
            &data,
            &theme,
            OutputMode::Term,
        )
        .unwrap();
        assert!(
            dark_output.contains("\x1b[31"),
            "Expected red color in dark mode, got: {}",
            dark_output
        );

        // Test light mode
        set_theme_detector(|| ColorMode::Light);
        let light_output = render_with_output(
            r#"[tone]{{ message }}[/tone]"#,
            &data,
            &theme,
            OutputMode::Term,
        )
        .unwrap();
        assert!(
            light_output.contains("\x1b[32"),
            "Expected green color in light mode, got: {}",
            light_output
        );

        // Reset to light for other tests
        set_theme_detector(|| ColorMode::Light);
    }

    #[test]
    fn test_detect_color_mode_uses_override() {
        set_theme_detector(|| ColorMode::Dark);
        assert_eq!(detect_color_mode(), ColorMode::Dark);

        set_theme_detector(|| ColorMode::Light);
        assert_eq!(detect_color_mode(), ColorMode::Light);
    }
}
