use serde::Serialize;
use standout_bbparser::{BBParser, TagTransform, UnknownTagBehavior};
use std::collections::HashMap;

use super::engine::{MiniJinjaEngine, TemplateEngine};
use crate::context::{ContextRegistry, RenderContext};
use crate::error::RenderError;
use crate::output::{Representation, StyleMode};
use crate::request::{convenience_request, ColorPolicy, TargetProperties};
use crate::style::Styles;
use crate::tabular::FlatDataSpec;
use crate::theme::{ColorMode, IconMode, Theme};
use crate::{render_request, TemplateRef};

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
    super::presentation::render_final(&super::presentation::parse_markup(output), &resolved, style)
}

#[derive(Debug, Clone)]
pub struct RenderResult {
    pub formatted: String,
    pub raw: String,
}

impl RenderResult {
    pub fn new(formatted: String, raw: String) -> Self {
        Self { formatted, raw }
    }

    pub fn plain(text: String) -> Self {
        Self {
            formatted: text.clone(),
            raw: text,
        }
    }
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

pub fn render<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
) -> Result<String, RenderError> {
    render_with_output(
        template,
        data,
        theme,
        Representation::Human,
        ColorPolicy::Auto,
    )
}

fn detect_then_render<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
    format: Representation,
    color_policy: ColorPolicy,
    overlay: impl FnOnce(&mut TargetProperties),
) -> Result<String, RenderError> {
    let mut target = TargetProperties::detect();
    overlay(&mut target);
    let request = convenience_request(
        TemplateRef::Inline(template.to_string()),
        standout_types::RenderData::from_serialize(data)?,
        theme.clone(),
        format,
        color_policy,
        target,
        None,
        None,
        None,
    );
    render_request(&request)
}

pub fn render_with_output<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
    representation: Representation,
    color_policy: ColorPolicy,
) -> Result<String, RenderError> {
    detect_then_render(template, data, theme, representation, color_policy, |_| {})
}

pub fn render_with_mode<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
    representation: Representation,
    color_policy: ColorPolicy,
    color_mode: ColorMode,
) -> Result<String, RenderError> {
    detect_then_render(
        template,
        data,
        theme,
        representation,
        color_policy,
        |target| {
            target.color_scheme = color_mode;
        },
    )
}

pub fn render_with_vars<T, K, V, I>(
    template: &str,
    data: &T,
    theme: &Theme,
    representation: Representation,
    color_policy: ColorPolicy,
    vars: I,
) -> Result<String, RenderError>
where
    T: Serialize,
    K: AsRef<str>,
    V: Into<standout_types::RenderData>,
    I: IntoIterator<Item = (K, V)>,
{
    let mut registry = ContextRegistry::new();
    for (key, value) in vars {
        registry.add_static(key.as_ref(), value.into());
    }
    let request = convenience_request(
        TemplateRef::Inline(template.to_string()),
        standout_types::RenderData::from_serialize(data)?,
        theme.clone(),
        representation,
        color_policy,
        TargetProperties::detect(),
        Some(registry),
        None,
        None,
    );
    render_request(&request)
}

pub fn render_auto<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
    representation: Representation,
    color_policy: ColorPolicy,
) -> Result<String, RenderError> {
    detect_then_render(template, data, theme, representation, color_policy, |_| {})
}

pub fn render_auto_with_spec<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
    representation: Representation,
    color_policy: ColorPolicy,
    spec: Option<&FlatDataSpec>,
) -> Result<String, RenderError> {
    if representation == Representation::Csv {
        if let Some(s) = spec {
            let value = standout_types::RenderData::from_serialize(data)?;
            let headers = s.extract_header();
            let rows: Vec<Vec<String>> = match value {
                standout_types::RenderData::Array(items) => items
                    .iter()
                    .map(|item| s.extract_row(&item.to_json()))
                    .collect(),
                _ => vec![s.extract_row(&value.to_json())],
            };
            let mut wtr = csv::Writer::from_writer(Vec::new());
            wtr.write_record(&headers)?;
            for row in rows {
                wtr.write_record(&row)?;
            }
            let bytes = wtr.into_inner()?;
            return Ok(String::from_utf8(bytes)?);
        }
    }
    detect_then_render(template, data, theme, representation, color_policy, |_| {})
}

#[allow(clippy::too_many_arguments)]
pub fn render_with_context<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
    representation: Representation,
    color_policy: ColorPolicy,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
    template_registry: Option<&super::TemplateRegistry>,
) -> Result<String, RenderError> {
    let mut target = TargetProperties::detect();
    target.width = render_context.terminal_width;
    target.ambiguous_width = render_context.ambiguous_width();
    let named = template_registry.is_some_and(|registry| registry.get_content(template).is_ok());
    let template_ref = if named {
        TemplateRef::Named(template.to_string())
    } else {
        TemplateRef::Inline(template.to_string())
    };
    let mut request = convenience_request(
        template_ref,
        standout_types::RenderData::from_serialize(data)?,
        theme.clone(),
        representation,
        color_policy,
        target,
        Some(context_registry.clone()),
        template_registry.map(|registry| std::rc::Rc::new(registry.clone())),
        None,
    );
    request.extras = render_context.extras.clone();
    render_request(&request)
}

#[allow(clippy::too_many_arguments)]
pub fn render_auto_with_context<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
    representation: Representation,
    color_policy: ColorPolicy,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
    template_registry: Option<&super::TemplateRegistry>,
) -> Result<String, RenderError> {
    render_with_context(
        template,
        data,
        theme,
        representation,
        color_policy,
        context_registry,
        render_context,
        template_registry,
    )
}

fn build_icon_context(
    theme: &Theme,
    icon_mode: IconMode,
) -> HashMap<String, standout_types::RenderData> {
    if theme.icons().is_empty() {
        return HashMap::new();
    }
    let resolved = theme.resolve_icons(icon_mode);
    let mut ctx = HashMap::new();
    ctx.insert(
        "icons".to_string(),
        standout_types::RenderData::from_serialize(resolved).unwrap(),
    );
    ctx
}

fn build_combined_context<T: Serialize>(
    data: &T,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
    icon_context: HashMap<String, standout_types::RenderData>,
) -> Result<HashMap<String, standout_types::RenderData>, RenderError> {
    let context_values = context_registry.resolve(render_context);

    let data_value = standout_types::RenderData::from_serialize(data)?;

    let mut combined: HashMap<String, standout_types::RenderData> = icon_context;

    for (key, value) in context_values {
        combined.insert(key, value);
    }

    if let Some(obj) = data_value.as_object() {
        for (key, value) in obj {
            combined.insert(key.clone(), value.clone());
        }
    }

    Ok(combined)
}

pub fn render_auto_with_engine(
    engine: &dyn super::TemplateEngine,
    template: &str,
    data: &standout_types::RenderData,
    theme: &Theme,
    style: StyleMode,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
) -> Result<String, RenderError> {
    Ok(render_auto_with_engine_split(
        engine,
        template,
        data,
        theme,
        style,
        context_registry,
        render_context,
    )?
    .formatted)
}

pub fn render_auto_with_engine_split(
    engine: &dyn super::TemplateEngine,
    template: &str,
    data: &standout_types::RenderData,
    theme: &Theme,
    style: StyleMode,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
) -> Result<RenderResult, RenderError> {
    let detected = TargetProperties::detect();
    render_auto_with_engine_split_kind(
        engine,
        TemplateIdentity::Auto(template),
        data,
        theme,
        style,
        context_registry,
        render_context,
        detected.color_scheme,
        detected.icon_mode,
    )
}

pub fn render_auto_with_engine_split_inline(
    engine: &dyn super::TemplateEngine,
    template: &str,
    data: &standout_types::RenderData,
    theme: &Theme,
    style: StyleMode,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
) -> Result<RenderResult, RenderError> {
    let detected = TargetProperties::detect();
    render_engine_split_inline(
        engine,
        template,
        data,
        theme,
        style,
        context_registry,
        render_context,
        detected.color_scheme,
        detected.icon_mode,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_engine_split_inline(
    engine: &dyn super::TemplateEngine,
    template: &str,
    data: &standout_types::RenderData,
    theme: &Theme,
    style: StyleMode,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
    color_mode: ColorMode,
    icon_mode: IconMode,
) -> Result<RenderResult, RenderError> {
    render_auto_with_engine_split_kind(
        engine,
        TemplateIdentity::Inline(template),
        data,
        theme,
        style,
        context_registry,
        render_context,
        color_mode,
        icon_mode,
    )
}

pub fn render_auto_with_engine_split_named(
    engine: &dyn super::TemplateEngine,
    name: &str,
    data: &standout_types::RenderData,
    theme: &Theme,
    style: StyleMode,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
) -> Result<RenderResult, RenderError> {
    let detected = TargetProperties::detect();
    render_engine_split_named(
        engine,
        name,
        data,
        theme,
        style,
        context_registry,
        render_context,
        detected.color_scheme,
        detected.icon_mode,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_engine_split_named(
    engine: &dyn super::TemplateEngine,
    name: &str,
    data: &standout_types::RenderData,
    theme: &Theme,
    style: StyleMode,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
    color_mode: ColorMode,
    icon_mode: IconMode,
) -> Result<RenderResult, RenderError> {
    render_auto_with_engine_split_kind(
        engine,
        TemplateIdentity::Named(name),
        data,
        theme,
        style,
        context_registry,
        render_context,
        color_mode,
        icon_mode,
    )
}

enum TemplateIdentity<'a> {
    Auto(&'a str),
    Inline(&'a str),
    Named(&'a str),
}

#[allow(clippy::too_many_arguments)]
fn render_auto_with_engine_split_kind(
    engine: &dyn super::TemplateEngine,
    template: TemplateIdentity<'_>,
    data: &standout_types::RenderData,
    theme: &Theme,
    style: StyleMode,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
    color_mode: ColorMode,
    icon_mode: IconMode,
) -> Result<RenderResult, RenderError> {
    {
        let styles = theme.resolve_styles(Some(color_mode));

        styles
            .validate()
            .map_err(|e| RenderError::StyleError(e.to_string()))?;

        let icon_context = build_icon_context(theme, icon_mode);
        let context_map =
            build_combined_context(data, context_registry, render_context, icon_context)?;

        let combined_value = standout_types::RenderData::Object(context_map.into_iter().collect());

        let raw_output = match template {
            TemplateIdentity::Auto(template) if engine.has_template(template) => engine
                .render_named_with_render_widths(
                    template,
                    &combined_value,
                    render_context.terminal_width,
                    render_context.ambiguous_width(),
                )?,
            TemplateIdentity::Auto(template) | TemplateIdentity::Inline(template) => engine
                .render_template_with_render_widths(
                    template,
                    &combined_value,
                    render_context.terminal_width,
                    render_context.ambiguous_width(),
                )?,
            TemplateIdentity::Named(name) => engine.render_named_with_render_widths(
                name,
                &combined_value,
                render_context.terminal_width,
                render_context.ambiguous_width(),
            )?,
        };

        // Unresolved-tag warnings are recorded once, on this formatted pass.
        let formatted_output = apply_style_tags_with(
            &raw_output,
            &styles,
            style,
            render_context.warnings.as_ref(),
        );

        let stripped_output = apply_style_tags(&raw_output, &styles, StyleMode::Plain);

        Ok(RenderResult::new(formatted_output, stripped_output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabular::{Column, FlatDataSpec, Width};
    use crate::test_data as json;
    use crate::{ColorPolicy, Theme};
    use console::Style;
    use serde::Serialize;

    #[derive(Serialize)]
    struct SimpleData {
        message: String,
    }

    #[derive(Serialize)]
    struct ListData {
        items: Vec<String>,
        count: usize,
    }

    #[test]
    fn test_render_with_output_text_no_ansi() {
        let theme = Theme::new().add("red", Style::new().red());
        let data = SimpleData {
            message: "test".into(),
        };

        let output = render_with_output(
            r#"[red]{{ message }}[/red]"#,
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Never,
        )
        .unwrap();

        assert_eq!(output, "test");
        assert!(!output.contains("\x1b["));
    }

    #[test]
    fn test_render_with_output_term_has_ansi() {
        let theme = Theme::new().add("green", Style::new().green().force_styling(true));
        let data = SimpleData {
            message: "success".into(),
        };

        let output = render_with_output(
            r#"[green]{{ message }}[/green]"#,
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Always,
        )
        .unwrap();

        assert!(output.contains("success"));
        assert!(output.contains("\x1b["));
    }

    #[test]
    fn test_render_unknown_style_degrades_to_text() {
        let theme = Theme::new();
        let data = SimpleData {
            message: "hello".into(),
        };

        let output = render_with_output(
            r#"[unknown]{{ message }}[/unknown]"#,
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Always,
        )
        .unwrap();

        assert_eq!(output, "hello");
    }

    #[test]
    fn test_render_unknown_style_stripped_in_text_mode() {
        let theme = Theme::new();
        let data = SimpleData {
            message: "hello".into(),
        };

        let output = render_with_output(
            r#"[unknown]{{ message }}[/unknown]"#,
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Never,
        )
        .unwrap();

        assert_eq!(output, "hello");
    }

    #[test]
    fn test_render_template_with_loop() {
        let theme = Theme::new().add("item", Style::new().cyan());
        let data = ListData {
            items: vec!["one".into(), "two".into()],
            count: 2,
        };

        let template = r#"{% for item in items %}[item]{{ item }}[/item]
{% endfor %}"#;

        let output = render_with_output(
            template,
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Never,
        )
        .unwrap();
        assert_eq!(output, "one\ntwo\n");
    }

    #[test]
    fn test_render_mixed_styled_and_plain() {
        let theme = Theme::new().add("count", Style::new().bold());
        let data = ListData {
            items: vec![],
            count: 42,
        };

        let template = r#"Total: [count]{{ count }}[/count] items"#;
        let output = render_with_output(
            template,
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Never,
        )
        .unwrap();

        assert_eq!(output, "Total: 42 items");
    }

    #[test]
    fn test_render_literal_string_styled() {
        let theme = Theme::new().add("header", Style::new().bold());

        #[derive(Serialize)]
        struct Empty {}

        let output = render_with_output(
            r#"[header]Header[/header]"#,
            &Empty {},
            &theme,
            Representation::Human,
            ColorPolicy::Never,
        )
        .unwrap();

        assert_eq!(output, "Header");
    }

    #[test]
    fn test_empty_template() {
        let theme = Theme::new();

        #[derive(Serialize)]
        struct Empty {}

        let output = render_with_output(
            "",
            &Empty {},
            &theme,
            Representation::Human,
            ColorPolicy::Never,
        )
        .unwrap();
        assert_eq!(output, "");
    }

    #[test]
    fn test_template_syntax_error() {
        let theme = Theme::new();

        #[derive(Serialize)]
        struct Empty {}

        let result = render_with_output(
            "{{ unclosed",
            &Empty {},
            &theme,
            Representation::Human,
            ColorPolicy::Never,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_style_tag_with_nested_data() {
        #[derive(Serialize)]
        struct Item {
            name: String,
            value: i32,
        }

        #[derive(Serialize)]
        struct Container {
            items: Vec<Item>,
        }

        let theme = Theme::new().add("name", Style::new().bold());
        let data = Container {
            items: vec![
                Item {
                    name: "foo".into(),
                    value: 1,
                },
                Item {
                    name: "bar".into(),
                    value: 2,
                },
            ],
        };

        let template = r#"{% for item in items %}[name]{{ item.name }}[/name]={{ item.value }}
{% endfor %}"#;

        let output = render_with_output(
            template,
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Never,
        )
        .unwrap();
        assert_eq!(output, "foo=1\nbar=2\n");
    }

    #[test]
    fn test_render_with_output_term_debug() {
        let theme = Theme::new()
            .add("title", Style::new().bold())
            .add("count", Style::new().cyan());

        #[derive(Serialize)]
        struct Data {
            name: String,
            value: usize,
        }

        let data = Data {
            name: "Test".into(),
            value: 42,
        };

        let output = render_with_output(
            r#"[title]{{ name }}[/title]: [count]{{ value }}[/count]"#,
            &data,
            &theme,
            Representation::TermDebug,
            ColorPolicy::Auto,
        )
        .unwrap();

        assert_eq!(output, "[title]Test[/title]: [count]42[/count]");
    }

    #[test]
    fn test_render_with_output_term_debug_preserves_tags() {
        let theme = Theme::new().add("known", Style::new().bold());

        #[derive(Serialize)]
        struct Data {
            message: String,
        }

        let data = Data {
            message: "hello".into(),
        };

        let output = render_with_output(
            r#"[unknown]{{ message }}[/unknown]"#,
            &data,
            &theme,
            Representation::TermDebug,
            ColorPolicy::Auto,
        )
        .unwrap();

        assert_eq!(output, "[unknown]hello[/unknown]");

        let output = render_with_output(
            r#"[known]{{ message }}[/known]"#,
            &data,
            &theme,
            Representation::TermDebug,
            ColorPolicy::Auto,
        )
        .unwrap();

        assert_eq!(output, "[known]hello[/known]");
    }

    #[test]
    fn test_render_auto_json_mode() {
        use crate::test_data as json;

        let theme = Theme::new();
        let data = json!({"name": "test", "count": 42});

        let output = render_auto(
            "unused template",
            &data,
            &theme,
            Representation::Json,
            ColorPolicy::Auto,
        )
        .unwrap();

        assert!(output.contains("\"name\": \"test\""));
        assert!(output.contains("\"count\": 42"));
    }

    #[test]
    fn test_render_auto_text_mode_uses_template() {
        use crate::test_data as json;

        let theme = Theme::new();
        let data = json!({"name": "test"});

        let output = render_auto(
            "Name: {{ name }}",
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Never,
        )
        .unwrap();

        assert_eq!(output, "Name: test");
    }

    #[test]
    fn test_render_auto_term_mode_uses_template() {
        use crate::test_data as json;

        let theme = Theme::new().add("bold", Style::new().bold().force_styling(true));
        let data = json!({"name": "test"});

        let output = render_auto(
            r#"[bold]{{ name }}[/bold]"#,
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Always,
        )
        .unwrap();

        assert!(output.contains("\x1b[1m"));
        assert!(output.contains("test"));
    }

    #[test]
    fn test_render_auto_json_with_struct() {
        #[derive(Serialize)]
        struct Report {
            title: String,
            items: Vec<String>,
        }

        let theme = Theme::new();
        let data = Report {
            title: "Summary".into(),
            items: vec!["one".into(), "two".into()],
        };

        let output = render_auto(
            "unused",
            &data,
            &theme,
            Representation::Json,
            ColorPolicy::Auto,
        )
        .unwrap();

        assert!(output.contains("\"title\": \"Summary\""));
        assert!(output.contains("\"items\""));
        assert!(output.contains("\"one\""));
    }

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

    #[test]
    fn test_render_auto_yaml_mode() {
        use crate::test_data as json;

        let theme = Theme::new();
        let data = json!({"name": "test", "count": 42});

        let output = render_auto(
            "unused template",
            &data,
            &theme,
            Representation::Yaml,
            ColorPolicy::Auto,
        )
        .unwrap();

        assert!(output.contains("name: test"));
        assert!(output.contains("count: 42"));
    }

    #[test]
    fn test_render_auto_csv_mode_flat_records() {
        use crate::test_data as json;

        let theme = Theme::new();
        let data = json!([
            {"name": "Alice", "score": 10},
            {"name": "Bob", "score": 20}
        ]);

        let output = render_auto(
            "unused",
            &data,
            &theme,
            Representation::Csv,
            ColorPolicy::Auto,
        )
        .unwrap();

        assert_eq!(output, "name,score\nAlice,10\nBob,20\n");
    }

    #[test]
    fn test_render_auto_csv_mode_refuses_a_nested_value() {
        use crate::test_data as json;

        let theme = Theme::new();
        let data = json!([
            {"name": "Alice", "stats": {"score": 10}},
            {"name": "Bob", "stats": {"score": 20}}
        ]);

        let error = render_auto(
            "unused",
            &data,
            &theme,
            Representation::Csv,
            ColorPolicy::Auto,
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("`[0].stats` is an object"), "{message}");
        assert!(message.contains("CsvProjection"), "{message}");
    }

    #[test]
    fn test_render_auto_csv_mode_with_spec() {
        let theme = Theme::new();
        let data = json!([
            {"name": "Alice", "meta": {"age": 30, "role": "admin"}},
            {"name": "Bob", "meta": {"age": 25, "role": "user"}}
        ]);

        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)).key("name"))
            .column(
                Column::new(Width::Fixed(10))
                    .key("meta.role")
                    .header("Role"),
            )
            .build();

        let output = render_auto_with_spec(
            "unused",
            &data,
            &theme,
            Representation::Csv,
            ColorPolicy::Auto,
            Some(&spec),
        )
        .unwrap();

        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines[0], "name,Role");
        assert!(lines.contains(&"Alice,admin"));
        assert!(lines.contains(&"Bob,user"));
        assert!(!output.contains("30"));
    }

    #[test]
    fn test_render_with_context_basic() {
        use crate::context::{ContextRegistry, RenderContext};

        #[derive(Serialize)]
        struct Data {
            name: String,
        }

        let theme = Theme::new();
        let data = Data {
            name: "Alice".into(),
        };
        let json_data = standout_types::RenderData::from_serialize(&data).unwrap();

        let mut registry = ContextRegistry::new();
        registry.add_static("version", standout_types::RenderData::from("1.0.0"));

        let render_ctx = RenderContext::new(
            Representation::Human,
            StyleMode::Plain,
            Some(80),
            &theme,
            &json_data,
        );

        let output = render_with_context(
            "{{ name }} (v{{ version }})",
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Never,
            &registry,
            &render_ctx,
            None,
        )
        .unwrap();

        assert_eq!(output, "Alice (v1.0.0)");
    }

    #[test]
    fn render_with_context_preserves_caller_extras_for_providers() {
        use crate::context::{ContextRegistry, RenderContext};

        let theme = Theme::new();
        let data = json!({"name": "Ada"});
        let mut registry = ContextRegistry::new();
        registry.add_provider("label", |ctx: &RenderContext| {
            standout_types::RenderData::from(ctx.get_extra("label").unwrap_or("missing"))
        });
        let render_ctx = RenderContext::new(
            Representation::Human,
            StyleMode::Plain,
            Some(80),
            &theme,
            &data,
        )
        .with_extra("label", "from-extra");
        let output = render_with_context(
            "{{ name }} {{ label }}",
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Never,
            &registry,
            &render_ctx,
            None,
        )
        .unwrap();
        assert_eq!(output, "Ada from-extra");
    }

    #[test]
    fn test_render_with_context_dynamic_provider() {
        use crate::context::{ContextRegistry, RenderContext};

        #[derive(Serialize)]
        struct Data {
            message: String,
        }

        let theme = Theme::new();
        let data = Data {
            message: "Hello".into(),
        };
        let json_data = standout_types::RenderData::from_serialize(&data).unwrap();

        let mut registry = ContextRegistry::new();
        registry.add_provider("terminal_width", |ctx: &RenderContext| {
            standout_types::RenderData::from(ctx.terminal_width.unwrap_or(80))
        });

        let render_ctx = RenderContext::new(
            Representation::Human,
            StyleMode::Plain,
            Some(120),
            &theme,
            &json_data,
        );

        let output = render_with_context(
            "{{ message }} (width={{ terminal_width }})",
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Never,
            &registry,
            &render_ctx,
            None,
        )
        .unwrap();

        assert_eq!(output, "Hello (width=120)");
    }

    #[test]
    fn test_render_with_context_data_takes_precedence() {
        use crate::context::{ContextRegistry, RenderContext};

        #[derive(Serialize)]
        struct Data {
            value: String,
        }

        let theme = Theme::new();
        let data = Data {
            value: "from_data".into(),
        };
        let json_data = standout_types::RenderData::from_serialize(&data).unwrap();

        let mut registry = ContextRegistry::new();
        registry.add_static("value", standout_types::RenderData::from("from_context"));

        let render_ctx = RenderContext::new(
            Representation::Human,
            StyleMode::Plain,
            None,
            &theme,
            &json_data,
        );

        let output = render_with_context(
            "{{ value }}",
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Never,
            &registry,
            &render_ctx,
            None,
        )
        .unwrap();

        assert_eq!(output, "from_data");
    }

    #[test]
    fn test_render_with_context_empty_registry() {
        use crate::context::{ContextRegistry, RenderContext};

        #[derive(Serialize)]
        struct Data {
            name: String,
        }

        let theme = Theme::new();
        let data = Data {
            name: "Test".into(),
        };
        let json_data = standout_types::RenderData::from_serialize(&data).unwrap();

        let registry = ContextRegistry::new();
        let render_ctx = RenderContext::new(
            Representation::Human,
            StyleMode::Plain,
            None,
            &theme,
            &json_data,
        );

        let output = render_with_context(
            "{{ name }}",
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Never,
            &registry,
            &render_ctx,
            None,
        )
        .unwrap();

        assert_eq!(output, "Test");
    }

    #[test]
    fn test_render_auto_with_context_json_mode() {
        use crate::context::{ContextRegistry, RenderContext};

        #[derive(Serialize)]
        struct Data {
            count: usize,
        }

        let theme = Theme::new();
        let data = Data { count: 42 };
        let json_data = standout_types::RenderData::from_serialize(&data).unwrap();

        let mut registry = ContextRegistry::new();
        registry.add_static("extra", standout_types::RenderData::from("ignored"));

        let render_ctx = RenderContext::new(
            Representation::Json,
            StyleMode::Plain,
            None,
            &theme,
            &json_data,
        );

        let output = render_auto_with_context(
            "unused template {{ extra }}",
            &data,
            &theme,
            Representation::Json,
            ColorPolicy::Auto,
            &registry,
            &render_ctx,
            None,
        )
        .unwrap();

        assert!(output.contains("\"count\": 42"));
        assert!(!output.contains("ignored"));
    }

    #[test]
    fn test_render_auto_with_context_text_mode() {
        use crate::context::{ContextRegistry, RenderContext};

        #[derive(Serialize)]
        struct Data {
            count: usize,
        }

        let theme = Theme::new();
        let data = Data { count: 42 };
        let json_data = standout_types::RenderData::from_serialize(&data).unwrap();

        let mut registry = ContextRegistry::new();
        registry.add_static("label", standout_types::RenderData::from("Items"));

        let render_ctx = RenderContext::new(
            Representation::Human,
            StyleMode::Plain,
            None,
            &theme,
            &json_data,
        );

        let output = render_auto_with_context(
            "{{ label }}: {{ count }}",
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Never,
            &registry,
            &render_ctx,
            None,
        )
        .unwrap();

        assert_eq!(output, "Items: 42");
    }

    #[test]
    fn test_render_with_context_provider_uses_the_representation() {
        use crate::context::{ContextRegistry, RenderContext};

        #[derive(Serialize)]
        struct Data {}

        let theme = Theme::new();
        let data = Data {};
        let json_data = standout_types::RenderData::from_serialize(&data).unwrap();

        let mut registry = ContextRegistry::new();
        registry.add_provider("mode", |ctx: &RenderContext| {
            standout_types::RenderData::from(format!("{:?}", ctx.representation))
        });

        let render_ctx = RenderContext::new(
            Representation::Human,
            StyleMode::Ansi,
            None,
            &theme,
            &json_data,
        );

        let output = render_with_context(
            "Mode: {{ mode }}",
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Always,
            &registry,
            &render_ctx,
            None,
        )
        .unwrap();

        assert_eq!(output, "Mode: Human");
    }

    #[test]
    fn test_render_with_context_nested_data() {
        use crate::context::{ContextRegistry, RenderContext};

        #[derive(Serialize)]
        struct Item {
            name: String,
        }

        #[derive(Serialize)]
        struct Data {
            items: Vec<Item>,
        }

        let theme = Theme::new();
        let data = Data {
            items: vec![Item { name: "one".into() }, Item { name: "two".into() }],
        };
        let json_data = standout_types::RenderData::from_serialize(&data).unwrap();

        let mut registry = ContextRegistry::new();
        registry.add_static("prefix", standout_types::RenderData::from("- "));

        let render_ctx = RenderContext::new(
            Representation::Human,
            StyleMode::Plain,
            None,
            &theme,
            &json_data,
        );

        let output = render_with_context(
            "{% for item in items %}{{ prefix }}{{ item.name }}\n{% endfor %}",
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Never,
            &registry,
            &render_ctx,
            None,
        )
        .unwrap();

        assert_eq!(output, "- one\n- two\n");
    }

    #[test]
    fn test_render_with_mode_forces_color_mode() {
        use console::Style;

        #[derive(Serialize)]
        struct Data {
            status: String,
        }

        let theme = Theme::new().add_adaptive(
            "status",
            Style::new(),                                   // Base
            Some(Style::new().black().force_styling(true)), // Light mode
            Some(Style::new().white().force_styling(true)), // Dark mode
        );

        let data = Data {
            status: "test".into(),
        };

        let dark_output = render_with_mode(
            r#"[status]{{ status }}[/status]"#,
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Always,
            ColorMode::Dark,
        )
        .unwrap();

        let light_output = render_with_mode(
            r#"[status]{{ status }}[/status]"#,
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Always,
            ColorMode::Light,
        )
        .unwrap();

        assert_ne!(dark_output, light_output);

        assert!(
            dark_output.contains("\x1b[37"),
            "Expected white (37) in dark mode"
        );

        assert!(
            light_output.contains("\x1b[30"),
            "Expected black (30) in light mode"
        );
    }

    #[test]
    fn test_tag_syntax_text_mode() {
        let theme = Theme::new().add("title", Style::new().bold());

        #[derive(Serialize)]
        struct Data {
            name: String,
        }

        let output = render_with_output(
            "[title]{{ name }}[/title]",
            &Data {
                name: "Hello".into(),
            },
            &theme,
            Representation::Human,
            ColorPolicy::Never,
        )
        .unwrap();

        assert_eq!(output, "Hello");
    }

    #[test]
    fn test_tag_syntax_term_mode() {
        let theme = Theme::new().add("bold", Style::new().bold().force_styling(true));

        #[derive(Serialize)]
        struct Data {
            name: String,
        }

        let output = render_with_output(
            "[bold]{{ name }}[/bold]",
            &Data {
                name: "Hello".into(),
            },
            &theme,
            Representation::Human,
            ColorPolicy::Always,
        )
        .unwrap();

        assert!(output.contains("\x1b[1m"));
        assert!(output.contains("Hello"));
    }

    #[test]
    fn test_tag_syntax_debug_mode() {
        let theme = Theme::new().add("title", Style::new().bold());

        #[derive(Serialize)]
        struct Data {
            name: String,
        }

        let output = render_with_output(
            "[title]{{ name }}[/title]",
            &Data {
                name: "Hello".into(),
            },
            &theme,
            Representation::TermDebug,
            ColorPolicy::Auto,
        )
        .unwrap();

        assert_eq!(output, "[title]Hello[/title]");
    }

    #[test]
    fn test_tag_syntax_unknown_tag_degrades_to_text() {
        let theme = Theme::new().add("known", Style::new().bold());

        #[derive(Serialize)]
        struct Data {
            name: String,
        }

        let output = render_with_output(
            "[unknown]{{ name }}[/unknown]",
            &Data {
                name: "Hello".into(),
            },
            &theme,
            Representation::Human,
            ColorPolicy::Always,
        )
        .unwrap();

        assert_eq!(output, "Hello");

        let text_output = render_with_output(
            "[unknown]{{ name }}[/unknown]",
            &Data {
                name: "Hello".into(),
            },
            &theme,
            Representation::Human,
            ColorPolicy::Never,
        )
        .unwrap();

        assert_eq!(text_output, "Hello");
    }

    #[test]
    fn test_tag_syntax_nested() {
        let theme = Theme::new()
            .add("bold", Style::new().bold().force_styling(true))
            .add("red", Style::new().red().force_styling(true));

        #[derive(Serialize)]
        struct Data {
            word: String,
        }

        let output = render_with_output(
            "[bold][red]{{ word }}[/red][/bold]",
            &Data {
                word: "test".into(),
            },
            &theme,
            Representation::Human,
            ColorPolicy::Always,
        )
        .unwrap();

        assert!(output.contains("\x1b[1m")); // Bold
        assert!(output.contains("\x1b[31m")); // Red
        assert!(output.contains("test"));
    }

    #[test]
    fn test_tag_syntax_multiple_styles() {
        let theme = Theme::new()
            .add("title", Style::new().bold())
            .add("count", Style::new().cyan());

        #[derive(Serialize)]
        struct Data {
            name: String,
            num: usize,
        }

        let output = render_with_output(
            r#"[title]{{ name }}[/title]: [count]{{ num }}[/count]"#,
            &Data {
                name: "Items".into(),
                num: 42,
            },
            &theme,
            Representation::Human,
            ColorPolicy::Never,
        )
        .unwrap();

        assert_eq!(output, "Items: 42");
    }

    #[test]
    fn test_tag_syntax_in_loop() {
        let theme = Theme::new().add("item", Style::new().cyan());

        #[derive(Serialize)]
        struct Data {
            items: Vec<String>,
        }

        let output = render_with_output(
            "{% for item in items %}[item]{{ item }}[/item]\n{% endfor %}",
            &Data {
                items: vec!["one".into(), "two".into()],
            },
            &theme,
            Representation::Human,
            ColorPolicy::Never,
        )
        .unwrap();

        assert_eq!(output, "one\ntwo\n");
    }

    #[test]
    fn test_tag_syntax_literal_brackets() {
        let theme = Theme::new();

        #[derive(Serialize)]
        struct Data {
            msg: String,
        }

        let output = render_with_output(
            "Array: [1, 2, 3] and {{ msg }}",
            &Data { msg: "done".into() },
            &theme,
            Representation::Human,
            ColorPolicy::Never,
        )
        .unwrap();

        assert_eq!(output, "Array: [1, 2, 3] and done");
    }

    #[test]
    fn test_validate_template_all_known_tags() {
        let theme = Theme::new()
            .add("title", Style::new().bold())
            .add("count", Style::new().cyan());

        #[derive(Serialize)]
        struct Data {
            name: String,
        }

        let result = validate_template(
            "[title]{{ name }}[/title]",
            &Data {
                name: "Hello".into(),
            },
            &theme,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_template_unknown_tag_fails() {
        let theme = Theme::new().add("known", Style::new().bold());

        #[derive(Serialize)]
        struct Data {
            name: String,
        }

        let result = validate_template(
            "[unknown]{{ name }}[/unknown]",
            &Data {
                name: "Hello".into(),
            },
            &theme,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        let errors = err
            .downcast_ref::<standout_bbparser::UnknownTagErrors>()
            .expect("Expected UnknownTagErrors");
        assert_eq!(errors.len(), 2); // open and close tags
    }

    #[test]
    fn test_validate_template_multiple_unknown_tags() {
        let theme = Theme::new().add("known", Style::new().bold());

        #[derive(Serialize)]
        struct Data {
            a: String,
            b: String,
        }

        let result = validate_template(
            "[foo]{{ a }}[/foo] and [bar]{{ b }}[/bar]",
            &Data {
                a: "x".into(),
                b: "y".into(),
            },
            &theme,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        let errors = err
            .downcast_ref::<standout_bbparser::UnknownTagErrors>()
            .expect("Expected UnknownTagErrors");
        assert_eq!(errors.len(), 4); // foo open/close + bar open/close
    }

    #[test]
    fn test_validate_template_plain_text_passes() {
        let theme = Theme::new();

        #[derive(Serialize)]
        struct Data {
            msg: String,
        }

        let result = validate_template("Just plain {{ msg }}", &Data { msg: "hi".into() }, &theme);

        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_template_mixed_known_and_unknown() {
        let theme = Theme::new().add("known", Style::new().bold());

        #[derive(Serialize)]
        struct Data {
            a: String,
            b: String,
        }

        let result = validate_template(
            "[known]{{ a }}[/known] [unknown]{{ b }}[/unknown]",
            &Data {
                a: "x".into(),
                b: "y".into(),
            },
            &theme,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        let errors = err
            .downcast_ref::<standout_bbparser::UnknownTagErrors>()
            .expect("Expected UnknownTagErrors");
        assert_eq!(errors.len(), 2);
        assert!(errors.errors.iter().any(|e| e.tag == "unknown"));
    }

    #[test]
    fn test_validate_template_syntax_error_fails() {
        let theme = Theme::new();
        #[derive(Serialize)]
        struct Data {}

        let result = validate_template("{{ unclosed", &Data {}, &theme);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err
            .downcast_ref::<standout_bbparser::UnknownTagErrors>()
            .is_none());
        let msg = err.to_string();
        assert!(
            msg.contains("syntax error") || msg.contains("unexpected"),
            "Got: {}",
            msg
        );
    }

    #[test]
    fn test_render_auto_with_context_yaml_mode() {
        use crate::context::{ContextRegistry, RenderContext};
        use crate::test_data as json;

        let theme = Theme::new();
        let data = json!({"name": "test", "count": 42});

        let registry = ContextRegistry::new();
        let render_ctx = RenderContext::new(
            Representation::Yaml,
            StyleMode::Plain,
            Some(80),
            &theme,
            &data,
        );

        let output = render_auto_with_context(
            "unused template",
            &data,
            &theme,
            Representation::Yaml,
            ColorPolicy::Auto,
            &registry,
            &render_ctx,
            None,
        )
        .unwrap();

        assert!(output.contains("name: test"));
        assert!(output.contains("count: 42"));
    }

    #[test]
    fn test_render_with_icons_classic() {
        use crate::request::convenience_engine;
        use crate::{AmbiguousWidth, IconDefinition, IconMode};

        let theme = Theme::new()
            .add_icon(
                "check",
                IconDefinition::new("[ok]").with_nerdfont("\u{f00c}"),
            )
            .add_icon("arrow", IconDefinition::new(">>"));

        let request = crate::RenderRequest {
            data: standout_types::RenderData::from_serialize(SimpleData {
                message: "done".into(),
            })
            .unwrap(),
            template: TemplateRef::Inline(
                "{{ icons.check }} {{ message }} {{ icons.arrow }}".into(),
            ),
            theme,
            format: Representation::Human,
            color_policy: ColorPolicy::Never,
            target: TargetProperties {
                width: Some(80),
                stdout_is_terminal: false,
                stderr_is_terminal: false,
                stdout_color_capability: false,
                stderr_color_capability: false,
                color_scheme: ColorMode::Dark,
                icon_mode: IconMode::Classic,
                ambiguous_width: AmbiguousWidth::Narrow,
            },
            engine: convenience_engine(),
            registry: None,
            context_registry: None,
            csv_projection: None,
            extras: HashMap::new(),
            warnings: None,
        };
        assert_eq!(render_request(&request).unwrap(), "[ok] done >>");
    }

    #[test]
    fn test_render_with_icons_nerdfont() {
        use crate::request::convenience_engine;
        use crate::{AmbiguousWidth, IconDefinition, IconMode};

        let theme = Theme::new().add_icon(
            "check",
            IconDefinition::new("[ok]").with_nerdfont("\u{f00c}"),
        );

        let request = crate::RenderRequest {
            data: standout_types::RenderData::from_serialize(SimpleData {
                message: "done".into(),
            })
            .unwrap(),
            template: TemplateRef::Inline("{{ icons.check }} {{ message }}".into()),
            theme,
            format: Representation::Human,
            color_policy: ColorPolicy::Never,
            target: TargetProperties {
                width: Some(80),
                stdout_is_terminal: false,
                stderr_is_terminal: false,
                stdout_color_capability: false,
                stderr_color_capability: false,
                color_scheme: ColorMode::Dark,
                icon_mode: IconMode::NerdFont,
                ambiguous_width: AmbiguousWidth::Narrow,
            },
            engine: convenience_engine(),
            registry: None,
            context_registry: None,
            csv_projection: None,
            extras: HashMap::new(),
            warnings: None,
        };
        assert_eq!(render_request(&request).unwrap(), "\u{f00c} done");
    }

    #[test]
    fn test_render_without_icons_no_overhead() {
        let theme = Theme::new();
        let data = SimpleData {
            message: "hello".into(),
        };

        let output = render_with_output(
            "{{ message }}",
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Never,
        )
        .unwrap();

        assert_eq!(output, "hello");
    }

    #[test]
    fn test_render_with_icons_and_styles() {
        use crate::IconDefinition;

        let theme = Theme::new()
            .add("title", Style::new().bold())
            .add_icon("bullet", IconDefinition::new("-"));

        let data = SimpleData {
            message: "item".into(),
        };

        let output = render_with_output(
            "{{ icons.bullet }} [title]{{ message }}[/title]",
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Never,
        )
        .unwrap();

        assert_eq!(output, "- item");
    }

    #[test]
    fn test_render_with_vars_includes_icons() {
        use crate::IconDefinition;

        let theme = Theme::new().add_icon("star", IconDefinition::new("*"));

        let data = SimpleData {
            message: "hello".into(),
        };

        let vars = std::collections::HashMap::from([("version", "1.0")]);

        let output = render_with_vars(
            "{{ icons.star }} {{ message }} v{{ version }}",
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Never,
            vars,
        )
        .unwrap();

        assert_eq!(output, "* hello v1.0");
    }

    #[test]
    fn test_render_with_context_includes_icons() {
        use crate::context::{ContextRegistry, RenderContext};
        use crate::IconDefinition;

        let theme = Theme::new().add_icon("dot", IconDefinition::new("."));

        let data = SimpleData {
            message: "test".into(),
        };

        let mut registry = ContextRegistry::new();
        registry.add_static("extra", standout_types::RenderData::from("ctx"));

        let json_data = standout_types::RenderData::from_serialize(&data).unwrap();
        let render_ctx = RenderContext::new(
            Representation::Human,
            StyleMode::Plain,
            Some(80),
            &theme,
            &json_data,
        );

        let output = render_with_context(
            "{{ icons.dot }} {{ message }} {{ extra }}",
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Never,
            &registry,
            &render_ctx,
            None,
        )
        .unwrap();

        assert_eq!(output, ". test ctx");
    }

    #[test]
    fn test_render_yaml_from_theme_with_icons() {
        let theme = Theme::from_yaml(
            r#"
            title:
                fg: cyan
                bold: true
            icons:
                check:
                    classic: "[ok]"
                    nerdfont: "nf"
            "#,
        )
        .unwrap();

        let data = SimpleData {
            message: "done".into(),
        };

        let output = render_with_output(
            "{{ icons.check }} [title]{{ message }}[/title]",
            &data,
            &theme,
            Representation::Human,
            ColorPolicy::Never,
        )
        .unwrap();

        assert_eq!(output, "[ok] done");
    }
}
