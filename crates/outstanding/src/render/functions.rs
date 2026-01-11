//! Core rendering functions.
//!
//! This module provides the main rendering entry points:
//!
//! - [`render`]: Simple rendering with automatic color detection
//! - [`render_with_output`]: Rendering with explicit output mode
//! - [`render_with_context`]: Rendering with injected context objects
//! - [`render_or_serialize`]: Render or serialize to JSON based on mode

use minijinja::{Environment, Error, Value};
use serde::Serialize;
use std::collections::HashMap;

use super::filters::register_filters;
use crate::context::{ContextRegistry, RenderContext};
use crate::output::OutputMode;
use crate::table::FlatDataSpec;
use crate::theme::{detect_color_mode, Theme};

/// Renders a template with automatic terminal color detection.
///
/// This is the simplest way to render styled output. It automatically detects
/// whether stdout supports colors and applies styles accordingly. Color mode
/// (light/dark) is detected from OS settings.
///
/// # Arguments
///
/// * `template` - A minijinja template string
/// * `data` - Any serializable data to pass to the template
/// * `theme` - Theme definitions to use for the `style` filter
///
/// # Example
///
/// ```rust
/// use outstanding::{render, Theme};
/// use console::Style;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Data { message: String }
///
/// let theme = Theme::new().add("ok", Style::new().green());
/// let output = render(
///     r#"{{ message | style("ok") }}"#,
///     &Data { message: "Success!".into() },
///     &theme,
/// ).unwrap();
/// ```
pub fn render<T: Serialize>(template: &str, data: &T, theme: &Theme) -> Result<String, Error> {
    render_with_output(template, data, theme, OutputMode::Auto)
}

/// Renders a template with explicit output mode control.
///
/// Use this when you need to override automatic terminal detection,
/// for example when honoring a `--output=text` CLI flag. Color mode
/// (light/dark) is detected from OS settings.
///
/// # Arguments
///
/// * `template` - A minijinja template string
/// * `data` - Any serializable data to pass to the template
/// * `theme` - Theme definitions to use for the `style` filter
/// * `mode` - Output mode: `Auto`, `Term`, or `Text`
///
/// # Example
///
/// ```rust
/// use outstanding::{render_with_output, Theme, OutputMode};
/// use console::Style;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Data { status: String }
///
/// let theme = Theme::new().add("ok", Style::new().green());
///
/// // Force plain text output
/// let plain = render_with_output(
///     r#"{{ status | style("ok") }}"#,
///     &Data { status: "done".into() },
///     &theme,
///     OutputMode::Text,
/// ).unwrap();
/// assert_eq!(plain, "done"); // No ANSI codes
///
/// // Force terminal output (with ANSI codes)
/// let term = render_with_output(
///     r#"{{ status | style("ok") }}"#,
///     &Data { status: "done".into() },
///     &theme,
///     OutputMode::Term,
/// ).unwrap();
/// // Contains ANSI codes for green
/// ```
pub fn render_with_output<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
    mode: OutputMode,
) -> Result<String, Error> {
    // Validate style aliases before rendering
    theme
        .validate()
        .map_err(|e| Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string()))?;

    // Detect color mode and resolve styles for that mode
    let color_mode = crate::theme::detect_color_mode();
    let styles = theme.resolve_styles(Some(color_mode));

    let mut env = Environment::new();
    register_filters(&mut env, styles, mode);

    env.add_template_owned("_inline".to_string(), template.to_string())?;
    let tmpl = env.get_template("_inline")?;
    tmpl.render(data)
}

/// Renders data using a template, or serializes directly for structured output modes.
///
/// This is the recommended function when you want to support both human-readable
/// output (terminal, text) and machine-readable output (JSON). For structured modes
/// like `Json`, the data is serialized directly, skipping template rendering entirely.
///
/// # Arguments
///
/// * `template` - A minijinja template string (ignored for structured modes)
/// * `data` - Any serializable data to render or serialize
/// * `theme` - Theme definitions for the `style` filter (ignored for structured modes)
/// * `mode` - Output mode determining the output format
///
/// # Example
///
/// ```rust
/// use outstanding::{render_or_serialize, Theme, OutputMode};
/// use console::Style;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Report { title: String, count: usize }
///
/// let theme = Theme::new().add("title", Style::new().bold());
/// let data = Report { title: "Summary".into(), count: 42 };
///
/// // Terminal output uses the template
/// let term = render_or_serialize(
///     r#"{{ title | style("title") }}: {{ count }}"#,
///     &data,
///     &theme,
///     OutputMode::Text,
/// ).unwrap();
/// assert_eq!(term, "Summary: 42");
///
/// // JSON output serializes directly
/// let json = render_or_serialize(
///     r#"{{ title | style("title") }}: {{ count }}"#,
///     &data,
///     &theme,
///     OutputMode::Json,
/// ).unwrap();
/// assert!(json.contains("\"title\": \"Summary\""));
/// assert!(json.contains("\"count\": 42"));
/// ```
pub fn render_or_serialize<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
    mode: OutputMode,
) -> Result<String, Error> {
    if mode.is_structured() {
        match mode {
            OutputMode::Json => serde_json::to_string_pretty(data)
                .map_err(|e| Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())),
            OutputMode::Yaml => serde_yaml::to_string(data)
                .map_err(|e| Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())),
            OutputMode::Xml => quick_xml::se::to_string(data)
                .map_err(|e| Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())),
            OutputMode::Csv => {
                let value = serde_json::to_value(data).map_err(|e| {
                    Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())
                })?;
                let (headers, rows) = crate::util::flatten_json_for_csv(&value);

                let mut wtr = csv::Writer::from_writer(Vec::new());
                wtr.write_record(&headers).map_err(|e| {
                    Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())
                })?;
                for row in rows {
                    wtr.write_record(&row).map_err(|e| {
                        Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())
                    })?;
                }
                let bytes = wtr.into_inner().map_err(|e| {
                    Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())
                })?;
                String::from_utf8(bytes)
                    .map_err(|e| Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string()))
            }
            _ => unreachable!("is_structured() returned true for non-structured mode"),
        }
    } else {
        render_with_output(template, data, theme, mode)
    }
}

/// Renders data using a template, or serializes with granular control.
///
/// Similar to `render_or_serialize`, but allows passing an optional `FlatDataSpec`.
/// This is particularly useful for controlling CSV output structure (columns, headers)
/// instead of relying on automatic JSON flattening.
///
/// # Arguments
///
/// * `template` - A minijinja template string
/// * `data` - Any serializable data to render or serialize
/// * `theme` - Theme definitions for the `style` filter
/// * `mode` - Output mode determining the output format
/// * `spec` - Optional `FlatDataSpec` for defining CSV/Table structure
pub fn render_or_serialize_with_spec<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
    mode: OutputMode,
    spec: Option<&FlatDataSpec>,
) -> Result<String, Error> {
    if mode.is_structured() {
        match mode {
            OutputMode::Json => serde_json::to_string_pretty(data)
                .map_err(|e| Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())),
            OutputMode::Yaml => serde_yaml::to_string(data)
                .map_err(|e| Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())),
            OutputMode::Xml => quick_xml::se::to_string(data)
                .map_err(|e| Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())),
            OutputMode::Csv => {
                let value = serde_json::to_value(data).map_err(|e| {
                    Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())
                })?;

                let (headers, rows) = if let Some(s) = spec {
                    // Use the spec for explicit extraction
                    let headers = s.extract_header();
                    let rows: Vec<Vec<String>> = match value {
                        serde_json::Value::Array(items) => {
                            items.iter().map(|item| s.extract_row(item)).collect()
                        }
                        _ => vec![s.extract_row(&value)],
                    };
                    (headers, rows)
                } else {
                    // Use automatic flattening
                    crate::util::flatten_json_for_csv(&value)
                };

                let mut wtr = csv::Writer::from_writer(Vec::new());
                wtr.write_record(&headers).map_err(|e| {
                    Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())
                })?;
                for row in rows {
                    wtr.write_record(&row).map_err(|e| {
                        Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())
                    })?;
                }
                let bytes = wtr.into_inner().map_err(|e| {
                    Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())
                })?;
                String::from_utf8(bytes)
                    .map_err(|e| Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string()))
            }
            _ => unreachable!("is_structured() returned true for non-structured mode"),
        }
    } else {
        render_with_output(template, data, theme, mode)
    }
}

/// Renders a template with additional context objects injected.
///
/// This is the most flexible rendering function, allowing you to inject
/// additional objects into the template context beyond the serialized data.
/// Use this when templates need access to utilities, formatters, or runtime
/// values that cannot be represented as JSON.
///
/// # Arguments
///
/// * `template` - A minijinja template string
/// * `data` - Any serializable data to pass to the template
/// * `theme` - Theme definitions for the `style` filter
/// * `mode` - Output mode: `Auto`, `Term`, `Text`, etc.
/// * `context_registry` - Additional context objects to inject
/// * `render_context` - Information about the render environment
///
/// # Context Resolution
///
/// Context objects are resolved from the registry using the provided
/// `RenderContext`. Each registered provider is called to produce a value,
/// which is then merged into the template context.
///
/// If a context key conflicts with a data field, the **data field wins**.
/// Context is supplementary to the handler's data, not a replacement.
///
/// # Example
///
/// ```rust
/// use outstanding::{render_with_context, Theme, OutputMode};
/// use outstanding::context::{RenderContext, ContextRegistry};
/// use minijinja::Value;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Data { name: String }
///
/// let theme = Theme::new();
/// let data = Data { name: "Alice".into() };
///
/// // Create context with a static value
/// let mut registry = ContextRegistry::new();
/// registry.add_static("version", Value::from("1.0.0"));
///
/// // Create render context
/// let json_data = serde_json::to_value(&data).unwrap();
/// let render_ctx = RenderContext::new(
///     OutputMode::Text,
///     Some(80),
///     &theme,
///     &json_data,
/// );
///
/// let output = render_with_context(
///     "{{ name }} (v{{ version }})",
///     &data,
///     &theme,
///     OutputMode::Text,
///     &registry,
///     &render_ctx,
/// ).unwrap();
///
/// assert_eq!(output, "Alice (v1.0.0)");
/// ```
pub fn render_with_context<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
    mode: OutputMode,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
) -> Result<String, Error> {
    let color_mode = detect_color_mode();
    let theme = theme.resolve_styles(Some(color_mode));

    // Validate style aliases before rendering
    theme
        .validate()
        .map_err(|e| Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string()))?;

    let mut env = Environment::new();
    register_filters(&mut env, theme, mode);

    env.add_template_owned("_inline".to_string(), template.to_string())?;
    let tmpl = env.get_template("_inline")?;

    // Build the combined context: data + injected context
    // Data fields take precedence over context fields
    let combined = build_combined_context(data, context_registry, render_context)?;

    tmpl.render(&combined)
}

/// Renders with context, or serializes directly for structured output modes.
///
/// This combines `render_with_context` with JSON serialization support.
/// For structured modes like `Json`, the data is serialized directly,
/// skipping template rendering (and context injection).
///
/// # Arguments
///
/// * `template` - A minijinja template string (ignored for structured modes)
/// * `data` - Any serializable data to render or serialize
/// * `theme` - Theme definitions for the `style` filter
/// * `mode` - Output mode determining the output format
/// * `context_registry` - Additional context objects to inject
/// * `render_context` - Information about the render environment
///
/// # Example
///
/// ```rust
/// use outstanding::{render_or_serialize_with_context, Theme, OutputMode};
/// use outstanding::context::{RenderContext, ContextRegistry};
/// use minijinja::Value;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Report { title: String, count: usize }
///
/// let theme = Theme::new();
/// let data = Report { title: "Summary".into(), count: 42 };
///
/// let mut registry = ContextRegistry::new();
/// registry.add_provider("terminal_width", |ctx: &RenderContext| {
///     Value::from(ctx.terminal_width.unwrap_or(80))
/// });
///
/// let json_data = serde_json::to_value(&data).unwrap();
/// let render_ctx = RenderContext::new(
///     OutputMode::Text,
///     Some(120),
///     &theme,
///     &json_data,
/// );
///
/// // Text mode uses the template with context
/// let text = render_or_serialize_with_context(
///     "{{ title }} (width={{ terminal_width }}): {{ count }}",
///     &data,
///     &theme,
///     OutputMode::Text,
///     &registry,
///     &render_ctx,
/// ).unwrap();
/// assert_eq!(text, "Summary (width=120): 42");
///
/// // JSON mode ignores template and context, serializes data directly
/// let json = render_or_serialize_with_context(
///     "unused",
///     &data,
///     &theme,
///     OutputMode::Json,
///     &registry,
///     &render_ctx,
/// ).unwrap();
/// assert!(json.contains("\"title\": \"Summary\""));
/// ```
pub fn render_or_serialize_with_context<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
    mode: OutputMode,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
) -> Result<String, Error> {
    if mode.is_structured() {
        match mode {
            OutputMode::Json => serde_json::to_string_pretty(data)
                .map_err(|e| Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())),
            _ => unreachable!("is_structured() returned true for non-structured mode"),
        }
    } else {
        render_with_context(
            template,
            data,
            theme,
            mode,
            context_registry,
            render_context,
        )
    }
}

/// Builds a combined context from data and injected context.
///
/// Data fields take precedence over context fields.
fn build_combined_context<T: Serialize>(
    data: &T,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
) -> Result<HashMap<String, Value>, Error> {
    // First, resolve all context providers
    let context_values = context_registry.resolve(render_context);

    // Convert data to a map of values
    let data_value = serde_json::to_value(data)
        .map_err(|e| Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string()))?;

    let mut combined: HashMap<String, Value> = HashMap::new();

    // Add context values first (lower priority)
    for (key, value) in context_values {
        combined.insert(key, value);
    }

    // Add data values (higher priority - overwrites context)
    if let Some(obj) = data_value.as_object() {
        for (key, value) in obj {
            let minijinja_value = json_to_minijinja(value);
            combined.insert(key.clone(), minijinja_value);
        }
    }

    Ok(combined)
}

/// Converts a serde_json::Value to a minijinja::Value.
fn json_to_minijinja(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::from(()),
        serde_json::Value::Bool(b) => Value::from(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::from(i)
            } else if let Some(f) = n.as_f64() {
                Value::from(f)
            } else {
                Value::from(n.to_string())
            }
        }
        serde_json::Value::String(s) => Value::from(s.clone()),
        serde_json::Value::Array(arr) => {
            let items: Vec<Value> = arr.iter().map(json_to_minijinja).collect();
            Value::from(items)
        }
        serde_json::Value::Object(obj) => {
            let map: HashMap<String, Value> = obj
                .iter()
                .map(|(k, v)| (k.clone(), json_to_minijinja(v)))
                .collect();
            Value::from_iter(map)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::{Column, FlatDataSpec, Width};
    use crate::theme::Theme;
    use console::Style;
    use serde::Serialize;
    use serde_json::json;

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
            r#"{{ message | style("red") }}"#,
            &data,
            &theme,
            OutputMode::Text,
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
            r#"{{ message | style("green") }}"#,
            &data,
            &theme,
            OutputMode::Term,
        )
        .unwrap();

        assert!(output.contains("success"));
        assert!(output.contains("\x1b["));
    }

    #[test]
    fn test_render_unknown_style_shows_indicator() {
        let theme = Theme::new();
        let data = SimpleData {
            message: "hello".into(),
        };

        let output = render_with_output(
            r#"{{ message | style("unknown") }}"#,
            &data,
            &theme,
            OutputMode::Term,
        )
        .unwrap();

        assert_eq!(output, "(!?) hello");
    }

    #[test]
    fn test_render_unknown_style_shows_indicator_no_color() {
        let theme = Theme::new();
        let data = SimpleData {
            message: "hello".into(),
        };

        let output = render_with_output(
            r#"{{ message | style("unknown") }}"#,
            &data,
            &theme,
            OutputMode::Text,
        )
        .unwrap();

        assert_eq!(output, "(!?) hello");
    }

    #[test]
    fn test_render_template_with_loop() {
        let theme = Theme::new().add("item", Style::new().cyan());
        let data = ListData {
            items: vec!["one".into(), "two".into()],
            count: 2,
        };

        let template = r#"{% for item in items %}{{ item | style("item") }}
{% endfor %}"#;

        let output = render_with_output(template, &data, &theme, OutputMode::Text).unwrap();
        assert_eq!(output, "one\ntwo\n");
    }

    #[test]
    fn test_render_mixed_styled_and_plain() {
        let theme = Theme::new().add("count", Style::new().bold());
        let data = ListData {
            items: vec![],
            count: 42,
        };

        let template = r#"Total: {{ count | style("count") }} items"#;
        let output = render_with_output(template, &data, &theme, OutputMode::Text).unwrap();

        assert_eq!(output, "Total: 42 items");
    }

    #[test]
    fn test_render_literal_string_styled() {
        let theme = Theme::new().add("header", Style::new().bold());

        #[derive(Serialize)]
        struct Empty {}

        let output = render_with_output(
            r#"{{ "Header" | style("header") }}"#,
            &Empty {},
            &theme,
            OutputMode::Text,
        )
        .unwrap();

        assert_eq!(output, "Header");
    }

    #[test]
    fn test_empty_template() {
        let theme = Theme::new();

        #[derive(Serialize)]
        struct Empty {}

        let output = render_with_output("", &Empty {}, &theme, OutputMode::Text).unwrap();
        assert_eq!(output, "");
    }

    #[test]
    fn test_template_syntax_error() {
        let theme = Theme::new();

        #[derive(Serialize)]
        struct Empty {}

        let result = render_with_output("{{ unclosed", &Empty {}, &theme, OutputMode::Text);
        assert!(result.is_err());
    }

    #[test]
    fn test_style_filter_with_nested_data() {
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

        let template = r#"{% for item in items %}{{ item.name | style("name") }}={{ item.value }}
{% endfor %}"#;

        let output = render_with_output(template, &data, &theme, OutputMode::Text).unwrap();
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
            r#"{{ name | style("title") }}: {{ value | style("count") }}"#,
            &data,
            &theme,
            OutputMode::TermDebug,
        )
        .unwrap();

        assert_eq!(output, "[title]Test[/title]: [count]42[/count]");
    }

    #[test]
    fn test_render_with_output_term_debug_missing_style() {
        let theme = Theme::new().add("known", Style::new().bold());

        #[derive(Serialize)]
        struct Data {
            message: String,
        }

        let data = Data {
            message: "hello".into(),
        };

        let output = render_with_output(
            r#"{{ message | style("unknown") }}"#,
            &data,
            &theme,
            OutputMode::TermDebug,
        )
        .unwrap();

        assert_eq!(output, "(!?) hello");

        let output = render_with_output(
            r#"{{ message | style("known") }}"#,
            &data,
            &theme,
            OutputMode::TermDebug,
        )
        .unwrap();

        assert_eq!(output, "[known]hello[/known]");
    }

    #[test]
    fn test_render_or_serialize_json_mode() {
        use serde_json::json;

        let theme = Theme::new();
        let data = json!({"name": "test", "count": 42});

        let output =
            render_or_serialize("unused template", &data, &theme, OutputMode::Json).unwrap();

        assert!(output.contains("\"name\": \"test\""));
        assert!(output.contains("\"count\": 42"));
    }

    #[test]
    fn test_render_or_serialize_text_mode_uses_template() {
        use serde_json::json;

        let theme = Theme::new();
        let data = json!({"name": "test"});

        let output =
            render_or_serialize("Name: {{ name }}", &data, &theme, OutputMode::Text).unwrap();

        assert_eq!(output, "Name: test");
    }

    #[test]
    fn test_render_or_serialize_term_mode_uses_template() {
        use serde_json::json;

        let theme = Theme::new().add("bold", Style::new().bold().force_styling(true));
        let data = json!({"name": "test"});

        let output = render_or_serialize(
            r#"{{ name | style("bold") }}"#,
            &data,
            &theme,
            OutputMode::Term,
        )
        .unwrap();

        assert!(output.contains("\x1b[1m"));
        assert!(output.contains("test"));
    }

    #[test]
    fn test_render_or_serialize_json_with_struct() {
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

        let output = render_or_serialize("unused", &data, &theme, OutputMode::Json).unwrap();

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
            r#"{{ "text" | style("alias") }}"#,
            &serde_json::json!({}),
            &theme,
            OutputMode::Text,
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
            r#"{{ "12:00" | style("timestamp") }}"#,
            &serde_json::json!({}),
            &theme,
            OutputMode::Text,
        )
        .unwrap();

        assert_eq!(output, "12:00");
    }

    #[test]
    fn test_render_fails_with_dangling_alias() {
        let theme = Theme::new().add("orphan", "missing");

        let result = render_with_output(
            r#"{{ "text" | style("orphan") }}"#,
            &serde_json::json!({}),
            &theme,
            OutputMode::Text,
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
            r#"{{ "text" | style("a") }}"#,
            &serde_json::json!({}),
            &theme,
            OutputMode::Text,
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
            r#"{{ time | style("timestamp") }} - {{ name | style("title") }}"#,
            &serde_json::json!({"time": "12:00", "name": "Report"}),
            &theme,
            OutputMode::Text,
        )
        .unwrap();

        assert_eq!(output, "12:00 - Report");
    }

    // ============================================================================
    // YAML/XML/CSV Output Tests
    // ============================================================================

    #[test]
    fn test_render_or_serialize_yaml_mode() {
        use serde_json::json;

        let theme = Theme::new();
        let data = json!({"name": "test", "count": 42});

        let output =
            render_or_serialize("unused template", &data, &theme, OutputMode::Yaml).unwrap();

        assert!(output.contains("name: test"));
        assert!(output.contains("count: 42"));
    }

    #[test]
    fn test_render_or_serialize_xml_mode() {
        let theme = Theme::new();

        #[derive(Serialize)]
        #[serde(rename = "root")]
        struct Data {
            name: String,
            count: usize,
        }

        let data = Data {
            name: "test".into(),
            count: 42,
        };

        let output =
            render_or_serialize("unused template", &data, &theme, OutputMode::Xml).unwrap();

        assert!(output.contains("<root>"));
        assert!(output.contains("<name>test</name>"));
    }

    #[test]
    fn test_render_or_serialize_csv_mode_auto_flatten() {
        use serde_json::json;

        let theme = Theme::new();
        let data = json!([
            {"name": "Alice", "stats": {"score": 10}},
            {"name": "Bob", "stats": {"score": 20}}
        ]);

        let output = render_or_serialize("unused", &data, &theme, OutputMode::Csv).unwrap();

        assert!(output.contains("name,stats.score"));
        assert!(output.contains("Alice,10"));
        assert!(output.contains("Bob,20"));
    }

    #[test]
    fn test_render_or_serialize_csv_mode_with_spec() {
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

        let output =
            render_or_serialize_with_spec("unused", &data, &theme, OutputMode::Csv, Some(&spec))
                .unwrap();

        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines[0], "name,Role");
        assert!(lines.contains(&"Alice,admin"));
        assert!(lines.contains(&"Bob,user"));
        assert!(!output.contains("30"));
    }

    // ============================================================================
    // Context Injection Tests
    // ============================================================================

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
        let json_data = serde_json::to_value(&data).unwrap();

        let mut registry = ContextRegistry::new();
        registry.add_static("version", Value::from("1.0.0"));

        let render_ctx = RenderContext::new(OutputMode::Text, Some(80), &theme, &json_data);

        let output = render_with_context(
            "{{ name }} (v{{ version }})",
            &data,
            &theme,
            OutputMode::Text,
            &registry,
            &render_ctx,
        )
        .unwrap();

        assert_eq!(output, "Alice (v1.0.0)");
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
        let json_data = serde_json::to_value(&data).unwrap();

        let mut registry = ContextRegistry::new();
        registry.add_provider("terminal_width", |ctx: &RenderContext| {
            Value::from(ctx.terminal_width.unwrap_or(80))
        });

        let render_ctx = RenderContext::new(OutputMode::Text, Some(120), &theme, &json_data);

        let output = render_with_context(
            "{{ message }} (width={{ terminal_width }})",
            &data,
            &theme,
            OutputMode::Text,
            &registry,
            &render_ctx,
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
        let json_data = serde_json::to_value(&data).unwrap();

        let mut registry = ContextRegistry::new();
        registry.add_static("value", Value::from("from_context"));

        let render_ctx = RenderContext::new(OutputMode::Text, None, &theme, &json_data);

        let output = render_with_context(
            "{{ value }}",
            &data,
            &theme,
            OutputMode::Text,
            &registry,
            &render_ctx,
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
        let json_data = serde_json::to_value(&data).unwrap();

        let registry = ContextRegistry::new();
        let render_ctx = RenderContext::new(OutputMode::Text, None, &theme, &json_data);

        let output = render_with_context(
            "{{ name }}",
            &data,
            &theme,
            OutputMode::Text,
            &registry,
            &render_ctx,
        )
        .unwrap();

        assert_eq!(output, "Test");
    }

    #[test]
    fn test_render_or_serialize_with_context_json_mode() {
        use crate::context::{ContextRegistry, RenderContext};

        #[derive(Serialize)]
        struct Data {
            count: usize,
        }

        let theme = Theme::new();
        let data = Data { count: 42 };
        let json_data = serde_json::to_value(&data).unwrap();

        let mut registry = ContextRegistry::new();
        registry.add_static("extra", Value::from("ignored"));

        let render_ctx = RenderContext::new(OutputMode::Json, None, &theme, &json_data);

        let output = render_or_serialize_with_context(
            "unused template {{ extra }}",
            &data,
            &theme,
            OutputMode::Json,
            &registry,
            &render_ctx,
        )
        .unwrap();

        assert!(output.contains("\"count\": 42"));
        assert!(!output.contains("ignored"));
    }

    #[test]
    fn test_render_or_serialize_with_context_text_mode() {
        use crate::context::{ContextRegistry, RenderContext};

        #[derive(Serialize)]
        struct Data {
            count: usize,
        }

        let theme = Theme::new();
        let data = Data { count: 42 };
        let json_data = serde_json::to_value(&data).unwrap();

        let mut registry = ContextRegistry::new();
        registry.add_static("label", Value::from("Items"));

        let render_ctx = RenderContext::new(OutputMode::Text, None, &theme, &json_data);

        let output = render_or_serialize_with_context(
            "{{ label }}: {{ count }}",
            &data,
            &theme,
            OutputMode::Text,
            &registry,
            &render_ctx,
        )
        .unwrap();

        assert_eq!(output, "Items: 42");
    }

    #[test]
    fn test_render_with_context_provider_uses_output_mode() {
        use crate::context::{ContextRegistry, RenderContext};

        #[derive(Serialize)]
        struct Data {}

        let theme = Theme::new();
        let data = Data {};
        let json_data = serde_json::to_value(&data).unwrap();

        let mut registry = ContextRegistry::new();
        registry.add_provider("mode", |ctx: &RenderContext| {
            Value::from(format!("{:?}", ctx.output_mode))
        });

        let render_ctx = RenderContext::new(OutputMode::Term, None, &theme, &json_data);

        let output = render_with_context(
            "Mode: {{ mode }}",
            &data,
            &theme,
            OutputMode::Term,
            &registry,
            &render_ctx,
        )
        .unwrap();

        assert_eq!(output, "Mode: Term");
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
        let json_data = serde_json::to_value(&data).unwrap();

        let mut registry = ContextRegistry::new();
        registry.add_static("prefix", Value::from("- "));

        let render_ctx = RenderContext::new(OutputMode::Text, None, &theme, &json_data);

        let output = render_with_context(
            "{% for item in items %}{{ prefix }}{{ item.name }}\n{% endfor %}",
            &data,
            &theme,
            OutputMode::Text,
            &registry,
            &render_ctx,
        )
        .unwrap();

        assert_eq!(output, "- one\n- two\n");
    }
}
