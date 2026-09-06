use dark_light::{detect as detect_os_theme, Mode as OsThemeMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Light,
    Dark,
}

pub(crate) fn probe_color_mode() -> ColorMode {
    match detect_os_theme() {
        Ok(OsThemeMode::Dark) => ColorMode::Dark,
        _ => ColorMode::Light,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::MiniJinjaEngine;
    use crate::{
        render_request, ColorPolicy, RenderRequest, Representation, SharedTemplateEngine,
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
            format: Representation::Human,
            color_policy: ColorPolicy::Always,
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
