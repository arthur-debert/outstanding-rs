//! Adaptive themes that respond to system color mode.

use dark_light::{detect as detect_os_theme, Mode as OsThemeMode};
use once_cell::sync::Lazy;
use std::sync::Mutex;

use super::theme::Theme;

/// The user's preferred color mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Light,
    Dark,
}

/// A theme that adapts based on the user's display mode.
///
/// Contains separate themes for light and dark modes, automatically
/// selecting the appropriate one based on OS settings.
///
/// # Example
///
/// ```rust
/// use outstanding::{AdaptiveTheme, Theme, ThemeChoice, OutputMode};
/// use console::Style;
///
/// let light = Theme::new().add("tone", Style::new().green());
/// let dark = Theme::new().add("tone", Style::new().yellow().italic());
/// let adaptive = AdaptiveTheme::new(light, dark);
///
/// // Automatically renders with the user's OS theme
/// let banner = outstanding::render_with_output(
///     r#"Mode: {{ "active" | style("tone") }}"#,
///     &serde_json::json!({}),
///     ThemeChoice::Adaptive(&adaptive),
///     OutputMode::Term,
/// ).unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct AdaptiveTheme {
    light: Theme,
    dark: Theme,
}

impl AdaptiveTheme {
    /// Creates an adaptive theme with separate light and dark variants.
    pub fn new(light: Theme, dark: Theme) -> Self {
        Self { light, dark }
    }

    /// Resolves to the appropriate theme based on the current color mode.
    pub(crate) fn resolve(&self) -> Theme {
        match detect_color_mode() {
            ColorMode::Light => self.light.clone(),
            ColorMode::Dark => self.dark.clone(),
        }
    }
}

type ThemeDetector = fn() -> ColorMode;

static THEME_DETECTOR: Lazy<Mutex<ThemeDetector>> = Lazy::new(|| Mutex::new(os_theme_detector));

/// Overrides the detector used to determine whether the user prefers a light or dark theme.
///
/// This is useful for testing or when you want to force a specific color mode.
pub fn set_theme_detector(detector: ThemeDetector) {
    let mut guard = THEME_DETECTOR.lock().unwrap();
    *guard = detector;
}

pub(crate) fn detect_color_mode() -> ColorMode {
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
    use console::Style;
    use serde::Serialize;

    #[derive(Serialize)]
    struct SimpleData {
        message: String,
    }

    #[test]
    fn test_adaptive_theme_uses_detector() {
        use crate::{render_with_output, OutputMode, ThemeChoice};

        console::set_colors_enabled(true);
        let light = Theme::new().add("tone", Style::new().green().force_styling(true));
        let dark = Theme::new().add("tone", Style::new().red().force_styling(true));
        let adaptive = AdaptiveTheme::new(light, dark);
        let data = SimpleData {
            message: "hi".into(),
        };

        set_theme_detector(|| ColorMode::Dark);
        let dark_output = render_with_output(
            r#"{{ message | style("tone") }}"#,
            &data,
            ThemeChoice::Adaptive(&adaptive),
            OutputMode::Term,
        )
        .unwrap();
        assert!(dark_output.contains("\x1b[31"));

        set_theme_detector(|| ColorMode::Light);
        let light_output = render_with_output(
            r#"{{ message | style("tone") }}"#,
            &data,
            ThemeChoice::Adaptive(&adaptive),
            OutputMode::Term,
        )
        .unwrap();
        assert!(light_output.contains("\x1b[32"));

        // Reset to default for other tests
        set_theme_detector(|| ColorMode::Light);
    }
}
