//! Core rendering functions.
//!
//! # Function Hierarchy
//!
//! The render functions form a layered hierarchy, from simple to fully explicit:
//!
//! ## Basic Rendering (template → styled string)
//!
//! | Function | Output Mode | Color Mode | Use When |
//! |----------|-------------|------------|----------|
//! | [`render`] | Auto-detect | Auto-detect | Simple cases, let Standout decide |
//! | [`render_with_output`] | Explicit | Auto-detect | Honoring `--output` CLI flag |
//! | [`render_with_mode`] | Explicit | Explicit | Tests, or forcing light/dark mode |
//!
//! ## Auto-Dispatch (render or serialize based on mode)
//!
//! For structured modes (Json, Yaml, Csv, Xml), these skip templating and
//! serialize data directly. For text modes, they render the template.
//!
//! | Function | Extra Features |
//! |----------|----------------|
//! | [`render_auto`] | Basic auto-dispatch |
//! | [`render_auto_with_spec`] | CSV column specification |
//! | [`render_auto_with_context`] | Context injection |
//!
//! ## With Context Injection
//!
//! Inject additional values (beyond handler data) into templates:
//!
//! | Function | Structured Output |
//! |----------|-------------------|
//! | [`render_with_context`] | No (template only) |
//! | [`render_auto_with_context`] | Yes (auto-dispatch) |
//!
//! # Two-Pass Rendering
//!
//! Templates use tag-based syntax for styling: `[name]content[/name]`
//!
//! The rendering process works in two passes:
//! 1. MiniJinja pass: Variable substitution and template logic
//! 2. BBParser pass: Style tag processing (`[tag]...[/tag]`)
//!
//! This allows templates like:
//! ```text
//! [title]{{ data.title }}[/title]: [count]{{ items | length }}[/count] items
//! ```
//!
//! # Feature Support Matrix
//!
//! Different rendering approaches support different features:
//!
//! | Approach | Includes | Per-call Mode | Styles | Use Case |
//! |----------|----------|---------------|--------|----------|
//! | [`Renderer`] | ✓ | ✓* | ✓ | Pre-compiled templates, hot reload |
//! | [`App::render`] | ✓ | ✓ | ✓ | CLI apps with embedded templates |
//! | [`render`] / [`render_auto`] | ✗ | ✓ | ✓ | One-off template strings |
//!
//! *Use [`Renderer::set_output_mode`] to change mode between renders.
//!
//! ## Template Includes
//!
//! Template includes (`{% include "partial" %}`) require a template registry.
//! The standalone `render*` functions take a template string, not a name,
//! so they cannot resolve includes to other templates.
//!
//! For includes, use either:
//! - [`Renderer`] with [`add_template`](Renderer::add_template) or
//!   [`with_embedded_source`](Renderer::with_embedded_source)
//! - `standout_render::cli::App` with embedded templates via the builder (requires `standout` crate)
//!
//! [`Renderer`]: super::renderer::Renderer
//! [`Renderer::set_output_mode`]: super::renderer::Renderer::set_output_mode

use serde::Serialize;
use standout_bbparser::{BBParser, TagTransform, UnknownTagBehavior};
use std::collections::HashMap;

use super::engine::{MiniJinjaEngine, TemplateEngine};
use crate::context::{ContextRegistry, RenderContext};
use crate::error::RenderError;
use crate::output::OutputMode;
use crate::style::Styles;
use crate::tabular::FlatDataSpec;
use crate::theme::{detect_color_mode, ColorMode, Theme};

/// Maps OutputMode to BBParser's TagTransform.
fn output_mode_to_transform(mode: OutputMode) -> TagTransform {
    match mode {
        OutputMode::Auto => {
            if mode.should_use_color() {
                TagTransform::Apply
            } else {
                TagTransform::Remove
            }
        }
        OutputMode::Term => TagTransform::Apply,
        OutputMode::Text => TagTransform::Remove,
        OutputMode::TermDebug => TagTransform::Keep,
        // Structured modes shouldn't reach here (filtered out before)
        OutputMode::Json | OutputMode::Yaml | OutputMode::Xml | OutputMode::Csv => {
            TagTransform::Remove
        }
    }
}

/// Post-processes rendered output with BBParser to apply style tags.
///
/// This is the second pass of the two-pass rendering system.
pub(crate) fn apply_style_tags(output: &str, styles: &Styles, mode: OutputMode) -> String {
    let transform = output_mode_to_transform(mode);
    let resolved_styles = styles.to_resolved_map();
    let parser =
        BBParser::new(resolved_styles, transform).unknown_behavior(UnknownTagBehavior::Passthrough);
    parser.parse(output)
}

/// Validates a template for unknown style tags.
///
/// This function renders the template (performing variable substitution) and then
/// checks for any style tags that are not defined in the theme. Use this during
/// development or CI to catch typos in templates.
///
/// # Arguments
///
/// * `template` - A minijinja template string
/// * `data` - Any serializable data to pass to the template
/// * `theme` - Theme definitions that define valid style names
///
/// # Returns
///
/// Returns `Ok(())` if all style tags in the template are defined in the theme.
/// Returns `Err` with the list of unknown tags if any are found.
///
/// # Example
///
/// ```rust
/// use standout_render::{validate_template, Theme};
/// use console::Style;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Data { name: String }
///
/// let theme = Theme::new().add("title", Style::new().bold());
///
/// // Valid template passes
/// let result = validate_template(
///     "[title]{{ name }}[/title]",
///     &Data { name: "Hello".into() },
///     &theme,
/// );
/// assert!(result.is_ok());
///
/// // Unknown tag fails validation
/// let result = validate_template(
///     "[unknown]{{ name }}[/unknown]",
///     &Data { name: "Hello".into() },
///     &theme,
/// );
/// assert!(result.is_err());
/// ```
pub fn validate_template<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
) -> Result<(), Box<dyn std::error::Error>> {
    let color_mode = detect_color_mode();
    let styles = theme.resolve_styles(Some(color_mode));

    // First render with the engine to get the final output
    let engine = MiniJinjaEngine::new();
    let data_value = serde_json::to_value(data)?;
    let minijinja_output = engine.render_template(template, &data_value)?;

    // Now validate the style tags
    let resolved_styles = styles.to_resolved_map();
    let parser = BBParser::new(resolved_styles, TagTransform::Remove);
    parser.validate(&minijinja_output)?;

    Ok(())
}

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
/// use standout_render::{render, Theme};
/// use console::Style;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Data { message: String }
///
/// let theme = Theme::new().add("ok", Style::new().green());
/// let output = render(
///     r#"[ok]{{ message }}[/ok]"#,
///     &Data { message: "Success!".into() },
///     &theme,
/// ).unwrap();
/// ```
pub fn render<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
) -> Result<String, RenderError> {
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
/// * `theme` - Theme definitions to use for styling
/// * `mode` - Output mode: `Auto`, `Term`, or `Text`
///
/// # Example
///
/// ```rust
/// use standout_render::{render_with_output, Theme, OutputMode};
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
///     r#"[ok]{{ status }}[/ok]"#,
///     &Data { status: "done".into() },
///     &theme,
///     OutputMode::Text,
/// ).unwrap();
/// assert_eq!(plain, "done"); // No ANSI codes
///
/// // Force terminal output (with ANSI codes)
/// let term = render_with_output(
///     r#"[ok]{{ status }}[/ok]"#,
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
) -> Result<String, RenderError> {
    // Detect color mode and render with explicit mode
    let color_mode = detect_color_mode();
    render_with_mode(template, data, theme, mode, color_mode)
}

/// Renders a template with explicit output mode and color mode control.
///
/// Use this when you need to force a specific color mode (light/dark),
/// for example in tests or when honoring user preferences.
///
/// # Arguments
///
/// * `template` - A minijinja template string
/// * `data` - Any serializable data to pass to the template
/// * `theme` - Theme definitions to use for the `style` filter
/// * `output_mode` - Output mode: `Auto`, `Term`, `Text`, etc.
/// * `color_mode` - Color mode: `Light` or `Dark`
///
/// # Example
///
/// ```rust
/// use standout_render::{render_with_mode, Theme, OutputMode, ColorMode};
/// use console::Style;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Data { status: String }
///
/// let theme = Theme::new()
///     .add_adaptive(
///         "panel",
///         Style::new(),
///         Some(Style::new().black()),  // Light mode
///         Some(Style::new().white()),  // Dark mode
///     );
///
/// // Force dark mode rendering
/// let dark = render_with_mode(
///     r#"[panel]{{ status }}[/panel]"#,
///     &Data { status: "test".into() },
///     &theme,
///     OutputMode::Term,
///     ColorMode::Dark,
/// ).unwrap();
///
/// // Force light mode rendering
/// let light = render_with_mode(
///     r#"[panel]{{ status }}[/panel]"#,
///     &Data { status: "test".into() },
///     &theme,
///     OutputMode::Term,
///     ColorMode::Light,
/// ).unwrap();
/// ```
pub fn render_with_mode<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
    output_mode: OutputMode,
    color_mode: ColorMode,
) -> Result<String, RenderError> {
    // Validate style aliases before rendering
    theme
        .validate()
        .map_err(|e| RenderError::StyleError(e.to_string()))?;

    // Resolve styles for the specified color mode
    let styles = theme.resolve_styles(Some(color_mode));

    // Pass 1: Template rendering
    let engine = MiniJinjaEngine::new();
    let data_value = serde_json::to_value(data)?;
    let template_output = engine.render_template(template, &data_value)?;

    // Pass 2: BBParser style tag processing
    let final_output = apply_style_tags(&template_output, &styles, output_mode);

    Ok(final_output)
}

/// Renders a template with additional variables injected into the context.
///
/// This is a convenience function for adding simple key-value pairs to the template
/// context without the complexity of the full [`ContextRegistry`] system. The data
/// fields take precedence over the injected variables.
///
/// # Arguments
///
/// * `template` - A minijinja template string
/// * `data` - The primary serializable data to render
/// * `theme` - Theme definitions for style tag processing
/// * `vars` - Additional variables to inject into the template context
///
/// # Example
///
/// ```rust
/// use standout_render::{render_with_vars, Theme, OutputMode};
/// use serde::Serialize;
/// use std::collections::HashMap;
///
/// #[derive(Serialize)]
/// struct User { name: String }
///
/// let theme = Theme::new();
/// let user = User { name: "Alice".into() };
///
/// let mut vars = HashMap::new();
/// vars.insert("version", "1.0.0");
/// vars.insert("app_name", "MyApp");
///
/// let output = render_with_vars(
///     "{{ name }} - {{ app_name }} v{{ version }}",
///     &user,
///     &theme,
///     OutputMode::Text,
///     vars,
/// ).unwrap();
///
/// assert_eq!(output, "Alice - MyApp v1.0.0");
/// ```
pub fn render_with_vars<T, K, V, I>(
    template: &str,
    data: &T,
    theme: &Theme,
    mode: OutputMode,
    vars: I,
) -> Result<String, RenderError>
where
    T: Serialize,
    K: AsRef<str>,
    V: Into<serde_json::Value>,
    I: IntoIterator<Item = (K, V)>,
{
    let color_mode = detect_color_mode();
    let styles = theme.resolve_styles(Some(color_mode));

    // Validate style aliases before rendering
    styles
        .validate()
        .map_err(|e| RenderError::StyleError(e.to_string()))?;

    // Build context from vars
    let mut context: HashMap<String, serde_json::Value> = HashMap::new();
    for (key, value) in vars {
        context.insert(key.as_ref().to_string(), value.into());
    }

    // Pass 1: Template rendering with context
    let engine = MiniJinjaEngine::new();
    let data_value = serde_json::to_value(data)?;
    let template_output = engine.render_with_context(template, &data_value, context)?;

    // Pass 2: BBParser style tag processing
    let final_output = apply_style_tags(&template_output, &styles, mode);

    Ok(final_output)
}

/// Auto-dispatches between template rendering and direct serialization.
///
/// This is the recommended function when you want to support both human-readable
/// output (terminal, text) and machine-readable output (JSON, YAML, etc.). For
/// structured modes like `Json`, the data is serialized directly, skipping
/// template rendering entirely.
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
/// use standout_render::{render_auto, Theme, OutputMode};
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
/// let term = render_auto(
///     r#"[title]{{ title }}[/title]: {{ count }}"#,
///     &data,
///     &theme,
///     OutputMode::Text,
/// ).unwrap();
/// assert_eq!(term, "Summary: 42");
///
/// // JSON output serializes directly
/// let json = render_auto(
///     r#"[title]{{ title }}[/title]: {{ count }}"#,
///     &data,
///     &theme,
///     OutputMode::Json,
/// ).unwrap();
/// assert!(json.contains("\"title\": \"Summary\""));
/// assert!(json.contains("\"count\": 42"));
/// ```
pub fn render_auto<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
    mode: OutputMode,
) -> Result<String, RenderError> {
    if mode.is_structured() {
        match mode {
            OutputMode::Json => Ok(serde_json::to_string_pretty(data)?),
            OutputMode::Yaml => Ok(serde_yaml::to_string(data)?),
            OutputMode::Xml => Ok(quick_xml::se::to_string(data)?),
            OutputMode::Csv => {
                let value = serde_json::to_value(data)?;
                let (headers, rows) = crate::util::flatten_json_for_csv(&value);

                let mut wtr = csv::Writer::from_writer(Vec::new());
                wtr.write_record(&headers)?;
                for row in rows {
                    wtr.write_record(&row)?;
                }
                let bytes = wtr.into_inner()?;
                Ok(String::from_utf8(bytes)?)
            }
            _ => unreachable!("is_structured() returned true for non-structured mode"),
        }
    } else {
        render_with_output(template, data, theme, mode)
    }
}

/// Auto-dispatches with granular control over structured output.
///
/// Similar to `render_auto`, but allows passing an optional `FlatDataSpec`.
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
pub fn render_auto_with_spec<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
    mode: OutputMode,
    spec: Option<&FlatDataSpec>,
) -> Result<String, RenderError> {
    if mode.is_structured() {
        match mode {
            OutputMode::Json => Ok(serde_json::to_string_pretty(data)?),
            OutputMode::Yaml => Ok(serde_yaml::to_string(data)?),
            OutputMode::Xml => Ok(quick_xml::se::to_string(data)?),
            OutputMode::Csv => {
                let value = serde_json::to_value(data)?;

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
                wtr.write_record(&headers)?;
                for row in rows {
                    wtr.write_record(&row)?;
                }
                let bytes = wtr.into_inner()?;
                Ok(String::from_utf8(bytes)?)
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
/// If a context key conflicts with a data field, the data field wins.
/// Context is supplementary to the handler's data, not a replacement.
///
/// # Example
///
/// ```rust
/// use standout_render::{render_with_context, Theme, OutputMode};
/// use standout_render::context::{RenderContext, ContextRegistry};
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
///     None,
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
    template_registry: Option<&super::TemplateRegistry>,
) -> Result<String, RenderError> {
    let color_mode = detect_color_mode();
    let styles = theme.resolve_styles(Some(color_mode));

    // Validate style aliases before rendering
    styles
        .validate()
        .map_err(|e| RenderError::StyleError(e.to_string()))?;

    let mut engine = MiniJinjaEngine::new();

    // Check if template is a registry key (name) or inline content.
    // If the registry contains a template with this name, use its content.
    // Otherwise, treat the template string as inline content.
    let template_content = if let Some(registry) = template_registry {
        if let Ok(content) = registry.get_content(template) {
            content
        } else {
            template.to_string()
        }
    } else {
        template.to_string()
    };

    // Load all templates from registry if available (enables {% include %})
    if let Some(registry) = template_registry {
        for name in registry.names() {
            if let Ok(content) = registry.get_content(name) {
                engine.add_template(name, &content)?;
            }
        }
    }

    // Build the combined context: data + injected context
    // Data fields take precedence over context fields
    let context = build_combined_context(data, context_registry, render_context)?;

    // Pass 1: Template rendering with context
    let data_value = serde_json::to_value(data)?;
    let template_output = engine.render_with_context(&template_content, &data_value, context)?;

    // Pass 2: BBParser style tag processing
    let final_output = apply_style_tags(&template_output, &styles, mode);

    Ok(final_output)
}

/// Auto-dispatches with context injection support.
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
/// use standout_render::{render_auto_with_context, Theme, OutputMode};
/// use standout_render::context::{RenderContext, ContextRegistry};
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
/// let text = render_auto_with_context(
///     "{{ title }} (width={{ terminal_width }}): {{ count }}",
///     &data,
///     &theme,
///     OutputMode::Text,
///     &registry,
///     &render_ctx,
///     None,
/// ).unwrap();
/// assert_eq!(text, "Summary (width=120): 42");
///
/// // JSON mode ignores template and context, serializes data directly
/// let json = render_auto_with_context(
///     "unused",
///     &data,
///     &theme,
///     OutputMode::Json,
///     &registry,
///     &render_ctx,
///     None,
/// ).unwrap();
/// assert!(json.contains("\"title\": \"Summary\""));
/// ```
pub fn render_auto_with_context<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
    mode: OutputMode,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
    template_registry: Option<&super::TemplateRegistry>,
) -> Result<String, RenderError> {
    if mode.is_structured() {
        match mode {
            OutputMode::Json => Ok(serde_json::to_string_pretty(data)?),
            OutputMode::Yaml => Ok(serde_yaml::to_string(data)?),
            OutputMode::Xml => Ok(quick_xml::se::to_string(data)?),
            OutputMode::Csv => {
                let value = serde_json::to_value(data)?;
                let (headers, rows) = crate::util::flatten_json_for_csv(&value);

                let mut wtr = csv::Writer::from_writer(Vec::new());
                wtr.write_record(&headers)?;
                for row in rows {
                    wtr.write_record(&row)?;
                }
                let bytes = wtr.into_inner()?;
                Ok(String::from_utf8(bytes)?)
            }
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
            template_registry,
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
) -> Result<HashMap<String, serde_json::Value>, RenderError> {
    // First, resolve all context providers
    let context_values = context_registry.resolve(render_context);

    // Convert data to a map of values
    let data_value = serde_json::to_value(data)?;

    let mut combined: HashMap<String, serde_json::Value> = HashMap::new();

    // Add context values first (lower priority)
    for (key, value) in context_values {
        // Convert minijinja::Value to serde_json::Value
        // This is a bit inefficient but necessary for the abstraction
        // In the future, ContextRegistry should probably return serde_json::Value
        let json_val =
            serde_json::to_value(value).map_err(|e| RenderError::ContextError(e.to_string()))?;
        combined.insert(key, json_val);
    }

    // Add data values (higher priority - overwrites context)
    if let Some(obj) = data_value.as_object() {
        for (key, value) in obj {
            combined.insert(key.clone(), value.clone());
        }
    }

    Ok(combined)
}

/// Auto-dispatches rendering using a provided TemplateEngine.
///
/// This is similar to `render_auto_with_context` but allows using a pre-configured
/// `TemplateEngine` trait object instead of creating a new dictionary-based engine.
pub fn render_auto_with_engine(
    engine: &dyn super::TemplateEngine,
    template: &str,
    data: &serde_json::Value,
    theme: &Theme,
    mode: OutputMode,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
) -> Result<String, RenderError> {
    if mode.is_structured() {
        match mode {
            OutputMode::Json => Ok(serde_json::to_string_pretty(data)?),
            OutputMode::Yaml => Ok(serde_yaml::to_string(data)?),
            OutputMode::Xml => Ok(quick_xml::se::to_string(data)?),
            OutputMode::Csv => {
                let (headers, rows) = crate::util::flatten_json_for_csv(data);

                let mut wtr = csv::Writer::from_writer(Vec::new());
                wtr.write_record(&headers)?;
                for row in rows {
                    wtr.write_record(&row)?;
                }
                let bytes = wtr.into_inner()?;
                Ok(String::from_utf8(bytes)?)
            }
            _ => unreachable!("is_structured() returned true for non-structured mode"),
        }
    } else {
        let color_mode = detect_color_mode();
        let styles = theme.resolve_styles(Some(color_mode));

        // Validate style aliases before rendering
        styles
            .validate()
            .map_err(|e| RenderError::StyleError(e.to_string()))?;

        // Build the combined context: data + injected context
        // Note: data is already Value, but build_combined_context expects T: Serialize
        // We can pass &data directly since Value implements Serialize
        let context_map = build_combined_context(data, context_registry, render_context)?;

        // Merge into a single Value for the engine
        let combined_value = serde_json::Value::Object(context_map.into_iter().collect());

        // Render template
        let template_output = if engine.has_template(template) {
            engine.render_named(template, &combined_value)?
        } else {
            engine.render_template(template, &combined_value)?
        };

        // Apply styles
        let final_output = apply_style_tags(&template_output, &styles, mode);

        Ok(final_output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabular::{Column, FlatDataSpec, Width};
    use crate::Theme;
    use console::Style;
    use minijinja::Value;
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
            r#"[red]{{ message }}[/red]"#,
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
            r#"[green]{{ message }}[/green]"#,
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
            r#"[unknown]{{ message }}[/unknown]"#,
            &data,
            &theme,
            OutputMode::Term,
        )
        .unwrap();

        // Unknown tags in passthrough mode get ? marker on both open and close tags
        assert_eq!(output, "[unknown?]hello[/unknown?]");
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
            OutputMode::Text,
        )
        .unwrap();

        // In text mode (Remove), unknown tags are stripped like known tags
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

        let template = r#"Total: [count]{{ count }}[/count] items"#;
        let output = render_with_output(template, &data, &theme, OutputMode::Text).unwrap();

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
            r#"[title]{{ name }}[/title]: [count]{{ value }}[/count]"#,
            &data,
            &theme,
            OutputMode::TermDebug,
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

        // In TermDebug (Keep mode), unknown tags are preserved as-is
        let output = render_with_output(
            r#"[unknown]{{ message }}[/unknown]"#,
            &data,
            &theme,
            OutputMode::TermDebug,
        )
        .unwrap();

        assert_eq!(output, "[unknown]hello[/unknown]");

        // Known tags are also preserved as-is in debug mode
        let output = render_with_output(
            r#"[known]{{ message }}[/known]"#,
            &data,
            &theme,
            OutputMode::TermDebug,
        )
        .unwrap();

        assert_eq!(output, "[known]hello[/known]");
    }

    #[test]
    fn test_render_auto_json_mode() {
        use serde_json::json;

        let theme = Theme::new();
        let data = json!({"name": "test", "count": 42});

        let output = render_auto("unused template", &data, &theme, OutputMode::Json).unwrap();

        assert!(output.contains("\"name\": \"test\""));
        assert!(output.contains("\"count\": 42"));
    }

    #[test]
    fn test_render_auto_text_mode_uses_template() {
        use serde_json::json;

        let theme = Theme::new();
        let data = json!({"name": "test"});

        let output = render_auto("Name: {{ name }}", &data, &theme, OutputMode::Text).unwrap();

        assert_eq!(output, "Name: test");
    }

    #[test]
    fn test_render_auto_term_mode_uses_template() {
        use serde_json::json;

        let theme = Theme::new().add("bold", Style::new().bold().force_styling(true));
        let data = json!({"name": "test"});

        let output = render_auto(
            r#"[bold]{{ name }}[/bold]"#,
            &data,
            &theme,
            OutputMode::Term,
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

        let output = render_auto("unused", &data, &theme, OutputMode::Json).unwrap();

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
            r#"[timestamp]12:00[/timestamp]"#,
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
            r#"[orphan]text[/orphan]"#,
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
            r#"[a]text[/a]"#,
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
            r#"[timestamp]{{ time }}[/timestamp] - [title]{{ name }}[/title]"#,
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
    fn test_render_auto_yaml_mode() {
        use serde_json::json;

        let theme = Theme::new();
        let data = json!({"name": "test", "count": 42});

        let output = render_auto("unused template", &data, &theme, OutputMode::Yaml).unwrap();

        assert!(output.contains("name: test"));
        assert!(output.contains("count: 42"));
    }

    #[test]
    fn test_render_auto_xml_mode() {
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

        let output = render_auto("unused template", &data, &theme, OutputMode::Xml).unwrap();

        assert!(output.contains("<root>"));
        assert!(output.contains("<name>test</name>"));
    }

    #[test]
    fn test_render_auto_csv_mode_auto_flatten() {
        use serde_json::json;

        let theme = Theme::new();
        let data = json!([
            {"name": "Alice", "stats": {"score": 10}},
            {"name": "Bob", "stats": {"score": 20}}
        ]);

        let output = render_auto("unused", &data, &theme, OutputMode::Csv).unwrap();

        assert!(output.contains("name,stats.score"));
        assert!(output.contains("Alice,10"));
        assert!(output.contains("Bob,20"));
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

        let output =
            render_auto_with_spec("unused", &data, &theme, OutputMode::Csv, Some(&spec)).unwrap();

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
            None,
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
        let json_data = serde_json::to_value(&data).unwrap();

        let mut registry = ContextRegistry::new();
        registry.add_static("extra", Value::from("ignored"));

        let render_ctx = RenderContext::new(OutputMode::Json, None, &theme, &json_data);

        let output = render_auto_with_context(
            "unused template {{ extra }}",
            &data,
            &theme,
            OutputMode::Json,
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
        let json_data = serde_json::to_value(&data).unwrap();

        let mut registry = ContextRegistry::new();
        registry.add_static("label", Value::from("Items"));

        let render_ctx = RenderContext::new(OutputMode::Text, None, &theme, &json_data);

        let output = render_auto_with_context(
            "{{ label }}: {{ count }}",
            &data,
            &theme,
            OutputMode::Text,
            &registry,
            &render_ctx,
            None,
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
            None,
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

        // Create an adaptive theme with different colors for light/dark
        // Note: force_styling(true) is needed in tests since there's no TTY
        let theme = Theme::new().add_adaptive(
            "status",
            Style::new(),                                   // Base
            Some(Style::new().black().force_styling(true)), // Light mode
            Some(Style::new().white().force_styling(true)), // Dark mode
        );

        let data = Data {
            status: "test".into(),
        };

        // Force dark mode
        let dark_output = render_with_mode(
            r#"[status]{{ status }}[/status]"#,
            &data,
            &theme,
            OutputMode::Term,
            ColorMode::Dark,
        )
        .unwrap();

        // Force light mode
        let light_output = render_with_mode(
            r#"[status]{{ status }}[/status]"#,
            &data,
            &theme,
            OutputMode::Term,
            ColorMode::Light,
        )
        .unwrap();

        // They should be different (different colors applied)
        assert_ne!(dark_output, light_output);

        // Dark mode should use white (ANSI 37)
        assert!(
            dark_output.contains("\x1b[37"),
            "Expected white (37) in dark mode"
        );

        // Light mode should use black (ANSI 30)
        assert!(
            light_output.contains("\x1b[30"),
            "Expected black (30) in light mode"
        );
    }

    // ============================================================================
    // BBParser Tag Syntax Tests
    // ============================================================================

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
            OutputMode::Text,
        )
        .unwrap();

        // Tags should be stripped in text mode
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
            OutputMode::Term,
        )
        .unwrap();

        // Should contain ANSI bold codes
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
            OutputMode::TermDebug,
        )
        .unwrap();

        // Tags should be preserved in debug mode
        assert_eq!(output, "[title]Hello[/title]");
    }

    #[test]
    fn test_tag_syntax_unknown_tag_passthrough() {
        // Passthrough with ? marker only applies in Apply mode (Term)
        let theme = Theme::new().add("known", Style::new().bold());

        #[derive(Serialize)]
        struct Data {
            name: String,
        }

        // In Term mode, unknown tags get ? marker
        let output = render_with_output(
            "[unknown]{{ name }}[/unknown]",
            &Data {
                name: "Hello".into(),
            },
            &theme,
            OutputMode::Term,
        )
        .unwrap();

        // Unknown tags get ? marker in passthrough mode
        assert!(output.contains("[unknown?]"));
        assert!(output.contains("[/unknown?]"));
        assert!(output.contains("Hello"));

        // In Text mode, all tags are stripped (Remove transform)
        let text_output = render_with_output(
            "[unknown]{{ name }}[/unknown]",
            &Data {
                name: "Hello".into(),
            },
            &theme,
            OutputMode::Text,
        )
        .unwrap();

        // Text mode strips all tags
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
            OutputMode::Term,
        )
        .unwrap();

        // Should contain both bold and red ANSI codes
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
            OutputMode::Text,
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
            OutputMode::Text,
        )
        .unwrap();

        assert_eq!(output, "one\ntwo\n");
    }

    #[test]
    fn test_tag_syntax_literal_brackets() {
        // Tags that don't match our pattern should pass through
        let theme = Theme::new();

        #[derive(Serialize)]
        struct Data {
            msg: String,
        }

        let output = render_with_output(
            "Array: [1, 2, 3] and {{ msg }}",
            &Data { msg: "done".into() },
            &theme,
            OutputMode::Text,
        )
        .unwrap();

        // Non-tag brackets preserved
        assert_eq!(output, "Array: [1, 2, 3] and done");
    }

    // ============================================================================
    // Template Validation Tests
    // ============================================================================

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
        // Only unknown tags should be reported
        assert_eq!(errors.len(), 2);
        assert!(errors.errors.iter().any(|e| e.tag == "unknown"));
    }

    #[test]
    fn test_validate_template_syntax_error_fails() {
        let theme = Theme::new();
        #[derive(Serialize)]
        struct Data {}

        // Missing closing braces
        let result = validate_template("{{ unclosed", &Data {}, &theme);
        assert!(result.is_err());

        let err = result.unwrap_err();
        // Should NOT be UnknownTagErrors
        assert!(err
            .downcast_ref::<standout_bbparser::UnknownTagErrors>()
            .is_none());
        // Should be a minijinja error
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
        use serde_json::json;

        let theme = Theme::new();
        let data = json!({"name": "test", "count": 42});

        // Setup context registry (though strictly not used for structured output)
        let registry = ContextRegistry::new();
        let render_ctx = RenderContext::new(OutputMode::Yaml, Some(80), &theme, &data);

        // This call previously panicked
        let output = render_auto_with_context(
            "unused template",
            &data,
            &theme,
            OutputMode::Yaml,
            &registry,
            &render_ctx,
            None,
        )
        .unwrap();

        assert!(output.contains("name: test"));
        assert!(output.contains("count: 42"));
    }
}
