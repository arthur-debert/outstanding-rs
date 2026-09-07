use serde::Serialize;
use standout_bbparser::{BBParser, TagTransform, UnknownTagBehavior};

use crate::output::StyleMode;
use crate::request::TargetProperties;
use crate::style::Styles;
use crate::template::engine::{MiniJinjaEngine, TemplateEngine};
use crate::theme::Theme;

fn style_to_transform(style: StyleMode) -> TagTransform {
    match style {
        StyleMode::Ansi => TagTransform::Apply,
        StyleMode::Debug => TagTransform::Keep,
        StyleMode::Plain => TagTransform::Remove,
    }
}

pub fn apply_style_tags(output: &str, styles: &Styles, style: StyleMode) -> String {
    apply_style_tags_with(output, styles, style, None)
}

pub fn apply_style_tags_with(
    output: &str,
    styles: &Styles,
    style: StyleMode,
    warnings: Option<&crate::warnings::WarningBuffer>,
) -> String {
    let transform = style_to_transform(style);
    let mut resolved = styles.to_resolved_map();
    if transform == TagTransform::Apply {
        // Forces ANSI regardless of console::colors_enabled(), so a non-TTY
        // test process still emits escapes when the request asked for them.
        for style in resolved.values_mut() {
            *style = style.clone().force_styling(true);
        }
    }
    crate::diagnostics::resolve_tags_with(
        output,
        resolved.clone(),
        transform,
        UnknownTagBehavior::Strip,
        warnings,
    );
    crate::template::presentation::render_final(
        &crate::template::presentation::parse_markup(output),
        &resolved,
        style,
    )
}

pub fn validate_template<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
) -> Result<(), Box<dyn std::error::Error>> {
    let color_mode = TargetProperties::detect().color_scheme;
    let styles = theme.resolve_styles(Some(color_mode));

    let engine = MiniJinjaEngine::new();
    let data_value = standout_types::RenderData::from_serialize(data)?;
    let minijinja_output = engine.render_template(template, &data_value)?;

    let resolved_styles = styles.to_resolved_map();
    let parser = BBParser::new(resolved_styles, TagTransform::Remove);
    parser.validate(&minijinja_output)?;

    Ok(())
}

#[cfg(test)]
mod tests {

    use crate::output::Representation;
    use crate::template::functions::*;
    use crate::{ColorPolicy, Theme};
    use console::Style;

    #[test]
    fn test_render_with_alias() {
        let theme = Theme::new()
            .add("base", Style::new().bold())
            .add("alias", "base");

        let output = render_with_output(
            r#"[alias]text[/alias]"#,
            &crate::test_data!({}),
            &theme,
            Representation::Human,
            ColorPolicy::Never,
        )
        .unwrap();

        assert_eq!(output, "text");
    }

    #[test]
    fn test_render_with_alias_chain() {
        let theme = Theme::new()
            .add("muted", Style::new().dim())
            .add("disabled", "muted")
            .add("timestamp", "disabled");

        let output = render_with_output(
            r#"[timestamp]12:00[/timestamp]"#,
            &crate::test_data!({}),
            &theme,
            Representation::Human,
            ColorPolicy::Never,
        )
        .unwrap();

        assert_eq!(output, "12:00");
    }

    #[test]
    fn test_render_fails_with_dangling_alias() {
        let theme = Theme::new().add("orphan", "missing");

        let result = render_with_output(
            r#"[orphan]text[/orphan]"#,
            &crate::test_data!({}),
            &theme,
            Representation::Human,
            ColorPolicy::Never,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("orphan"));
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn test_render_fails_with_cycle() {
        let theme = Theme::new().add("a", "b").add("b", "a");

        let result = render_with_output(
            r#"[a]text[/a]"#,
            &crate::test_data!({}),
            &theme,
            Representation::Human,
            ColorPolicy::Never,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cycle"));
    }

    #[test]
    fn test_three_layer_styling_pattern() {
        let theme = Theme::new()
            .add("dim_style", Style::new().dim())
            .add("cyan_bold", Style::new().cyan().bold())
            .add("yellow_bg", Style::new().on_yellow())
            .add("muted", "dim_style")
            .add("accent", "cyan_bold")
            .add("highlighted", "yellow_bg")
            .add("timestamp", "muted")
            .add("title", "accent")
            .add("selected_item", "highlighted");

        assert!(theme.validate().is_ok());

        let output = render_with_output(
            r#"[timestamp]{{ time }}[/timestamp] - [title]{{ name }}[/title]"#,
            &crate::test_data!({"time": "12:00", "name": "Report"}),
            &theme,
            Representation::Human,
            ColorPolicy::Never,
        )
        .unwrap();

        assert_eq!(output, "12:00 - Report");
    }
}

#[cfg(test)]
mod tag_syntax;
#[cfg(test)]
mod validation;
