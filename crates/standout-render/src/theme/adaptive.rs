//! Color mode detection for adaptive themes.
//!
//! [`probe_color_mode`] queries the OS for the user's preferred scheme. Callers
//! detect at the crate edge via [`crate::TargetProperties::detect`]; tests
//! construct [`crate::TargetProperties`] with an explicit
//! [`ColorMode`](ColorMode). Template functions never call this probe.

use dark_light::{detect as detect_os_theme, Mode as OsThemeMode};

/// The user's preferred color mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// Light mode (light background, dark text).
    Light,
    /// Dark mode (dark background, light text).
    Dark,
}

/// Detects the user's preferred color mode from the OS.
///
/// Uses the `dark-light` crate to query the OS for the current theme preference.
///
/// # Returns
///
/// - [`ColorMode::Light`] if the OS is in light mode
/// - [`ColorMode::Dark`] if the OS is in dark mode
pub(crate) fn probe_color_mode() -> ColorMode {
    match detect_os_theme() {
        OsThemeMode::Dark => ColorMode::Dark,
        OsThemeMode::Light => ColorMode::Light,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::MiniJinjaEngine;
    use crate::{
        render_request, ColorPolicy, OutputMode, RenderRequest, SharedTemplateEngine,
        TargetProperties, TemplateRef, Theme,
    };
    use console::Style;
    use serde::Serialize;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    #[derive(Serialize)]
    struct SimpleData {
        message: String,
    }

    fn engine() -> SharedTemplateEngine {
        Rc::new(RefCell::new(Box::new(MiniJinjaEngine::new())))
    }

    fn target_with_scheme(color_scheme: ColorMode) -> TargetProperties {
        TargetProperties {
            width: Some(80),
            stdout_is_terminal: true,
            stderr_is_terminal: true,
            stdout_color_capability: true,
            stderr_color_capability: true,
            color_scheme,
            icon_mode: crate::IconMode::Classic,
            ambiguous_width: crate::AmbiguousWidth::Narrow,
        }
    }

    #[test]
    fn adaptive_theme_follows_request_color_scheme() {
        let theme = Theme::new().add_adaptive(
            "tone",
            Style::new(),
            Some(Style::new().green()),
            Some(Style::new().red()),
        );
        let data = serde_json::to_value(SimpleData {
            message: "hi".into(),
        })
        .unwrap();

        let dark = RenderRequest {
            data: data.clone(),
            template: TemplateRef::Inline("[tone]{{ message }}[/tone]".into()),
            theme: theme.clone(),
            format: OutputMode::Term,
            color_policy: ColorPolicy::Auto,
            target: target_with_scheme(ColorMode::Dark),
            engine: engine(),
            registry: None,
            context_registry: None,
            csv_projection: None,
            extras: HashMap::new(),
            warnings: None,
        };
        let dark_output = render_request(&dark).unwrap();
        assert!(
            dark_output.contains("\x1b[31"),
            "Expected red color in dark mode, got: {dark_output}"
        );

        let light = RenderRequest {
            target: target_with_scheme(ColorMode::Light),
            engine: engine(),
            ..dark
        };
        let light_output = render_request(&light).unwrap();
        assert!(
            light_output.contains("\x1b[32"),
            "Expected green color in light mode, got: {light_output}"
        );
    }

    #[test]
    fn probe_color_mode_returns_a_variant() {
        match probe_color_mode() {
            ColorMode::Light | ColorMode::Dark => {}
        }
    }
}
