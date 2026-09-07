use crate::context::ContextRegistry;
use crate::error::RenderError;
use crate::output::Representation;
use crate::request::{convenience_request, ColorPolicy, TargetProperties};
use crate::tabular::FlatDataSpec;
use crate::theme::{ColorMode, Theme};
use crate::{render_request, TemplateRef};
use serde::Serialize;

mod context;
mod engine;
mod styles;
pub use context::{render_auto_with_context, render_with_context};
pub use engine::{
    render_auto_with_engine, render_auto_with_engine_split, render_auto_with_engine_split_inline,
    render_auto_with_engine_split_named,
};
pub(crate) use engine::{render_engine_split_inline, render_engine_split_named};
pub use styles::{apply_style_tags, apply_style_tags_with, validate_template};

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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::output::Representation;
    use crate::tabular::{Column, FlatDataSpec, Width};
    use crate::template::functions::test_data::ListData;
    use crate::template::functions::test_data::SimpleData;
    use crate::test_data as json;
    use crate::{ColorPolicy, Theme};
    use console::Style;
    use serde::Serialize;

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
}

#[cfg(test)]
mod icons;

#[cfg(test)]
mod test_data {
    use serde::Serialize;
    #[derive(Serialize)]
    pub(super) struct SimpleData {
        pub(super) message: String,
    }

    #[derive(Serialize)]
    pub(super) struct ListData {
        pub(super) items: Vec<String>,
        pub(super) count: usize,
    }
}
