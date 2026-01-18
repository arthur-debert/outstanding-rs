//! Shared core state and functionality for App and LocalApp.
//!
//! This module contains [`AppCore`], which holds the common configuration
//! and behavior shared between thread-safe [`App`](super::App) and
//! single-threaded [`LocalApp`](super::LocalApp).
//!
//! By extracting shared logic here, we avoid code duplication and ensure
//! feature parity between both app types.

use std::collections::HashMap;
use std::sync::Arc;

use clap::{Arg, ArgAction, ArgMatches, Command};
use minijinja::Environment;
use serde::Serialize;

use crate::context::{ContextRegistry, RenderContext};
use crate::rendering::template::filters::register_filters;
use crate::rendering::theme::detect_color_mode;
use crate::setup::SetupError;
use crate::{OutputMode, StylesheetRegistry, TemplateRegistry, Theme};

use super::app::get_terminal_width;
use super::hooks::Hooks;

/// Shared core state for App and LocalApp.
///
/// This struct contains all configuration and state that is common between
/// the thread-safe `App` and single-threaded `LocalApp`. By sharing this
/// core, we ensure feature parity and avoid code duplication.
///
/// # Fields
///
/// - **Output configuration**: `output_flag`, `output_file_flag`, `output_mode`
/// - **Theme and styles**: `theme`, `stylesheet_registry`
/// - **Templates**: `template_registry`
/// - **Hooks**: `command_hooks`
/// - **Default command**: `default_command`
/// - **Context**: `context_registry`
pub struct AppCore {
    /// Name of the output mode flag (e.g., "output" for `--output`).
    /// Set to None to disable the flag.
    pub(crate) output_flag: Option<String>,

    /// Name of the output file flag (e.g., "output-file-path" for `--output-file-path`).
    /// Set to None to disable the flag.
    pub(crate) output_file_flag: Option<String>,

    /// Current output mode (Auto, Term, Text, Json, etc.).
    pub(crate) output_mode: OutputMode,

    /// Default theme for rendering.
    pub(crate) theme: Option<Theme>,

    /// Per-command hooks for pre/post processing.
    pub(crate) command_hooks: HashMap<String, Hooks>,

    /// Default command to run when no subcommand is provided.
    pub(crate) default_command: Option<String>,

    /// Template registry for embedded/file-based templates.
    /// Wrapped in Arc for efficient sharing across dispatch closures.
    pub(crate) template_registry: Option<Arc<TemplateRegistry>>,

    /// Stylesheet registry for runtime theme access.
    pub(crate) stylesheet_registry: Option<StylesheetRegistry>,

    /// Context registry for template variable injection.
    pub(crate) context_registry: ContextRegistry,
}

impl Default for AppCore {
    fn default() -> Self {
        Self::new()
    }
}

impl AppCore {
    /// Creates a new AppCore with default settings.
    ///
    /// Default configuration:
    /// - `--output` flag enabled
    /// - `--output-file-path` flag enabled
    /// - Auto output mode
    /// - No theme (uses default)
    /// - No hooks
    /// - No default command
    /// - No template registry
    /// - No stylesheet registry
    /// - Empty context registry
    pub fn new() -> Self {
        Self {
            output_flag: Some("output".to_string()),
            output_file_flag: Some("output-file-path".to_string()),
            output_mode: OutputMode::Auto,
            theme: None,
            command_hooks: HashMap::new(),
            default_command: None,
            template_registry: None,
            stylesheet_registry: None,
            context_registry: ContextRegistry::new(),
        }
    }

    // =========================================================================
    // Accessors
    // =========================================================================

    /// Returns the current output mode.
    pub fn output_mode(&self) -> OutputMode {
        self.output_mode
    }

    /// Returns the hooks registered for a specific command path.
    pub fn get_hooks(&self, path: &str) -> Option<&Hooks> {
        self.command_hooks.get(path)
    }

    /// Returns the default theme, if configured.
    pub fn theme(&self) -> Option<&Theme> {
        self.theme.as_ref()
    }

    /// Returns the default command name, if configured.
    pub fn default_command(&self) -> Option<&str> {
        self.default_command.as_deref()
    }

    /// Returns the names of all available templates.
    ///
    /// Returns an empty iterator if no template registry is configured.
    pub fn template_names(&self) -> impl Iterator<Item = &str> {
        self.template_registry
            .as_ref()
            .map(|r| r.names())
            .into_iter()
            .flatten()
    }

    /// Returns the names of all available themes.
    ///
    /// Returns an empty vector if no stylesheet registry is configured.
    pub fn theme_names(&self) -> Vec<String> {
        self.stylesheet_registry
            .as_ref()
            .map(|r| r.names().map(String::from).collect())
            .unwrap_or_default()
    }

    /// Gets a theme by name from the stylesheet registry.
    ///
    /// This allows using themes other than the default at runtime.
    ///
    /// # Errors
    ///
    /// Returns an error if no stylesheet registry is configured or if the theme
    /// is not found.
    pub fn get_theme(&mut self, name: &str) -> Result<Theme, SetupError> {
        self.stylesheet_registry
            .as_mut()
            .ok_or_else(|| SetupError::Config("No stylesheet registry configured".into()))?
            .get(name)
            .map_err(|_| SetupError::ThemeNotFound(name.to_string()))
    }

    // =========================================================================
    // Command augmentation
    // =========================================================================

    /// Augments a clap Command with Standout's global flags.
    ///
    /// Adds `--output` and `--output-file-path` flags if configured.
    /// These flags are global (apply to all subcommands).
    pub fn augment_command(&self, mut cmd: Command) -> Command {
        if let Some(ref flag_name) = self.output_flag {
            let flag: &'static str = Box::leak(flag_name.clone().into_boxed_str());
            cmd = cmd.arg(
                Arg::new("_output_mode")
                    .long(flag)
                    .value_name("MODE")
                    .global(true)
                    .value_parser([
                        "auto",
                        "term",
                        "text",
                        "term-debug",
                        "json",
                        "yaml",
                        "xml",
                        "csv",
                    ])
                    .default_value("auto")
                    .help("Output mode: auto, term, text, term-debug, json, yaml, xml, or csv"),
            );
        }

        if let Some(ref flag_name) = self.output_file_flag {
            let flag: &'static str = Box::leak(flag_name.clone().into_boxed_str());
            cmd = cmd.arg(
                Arg::new("_output_file_path")
                    .long(flag)
                    .value_name("PATH")
                    .global(true)
                    .action(ArgAction::Set)
                    .help("Write output to file instead of stdout"),
            );
        }

        cmd
    }

    /// Extracts the output mode from parsed ArgMatches.
    ///
    /// Reads the `_output_mode` argument value and converts it to an OutputMode.
    /// Returns Auto if the flag is disabled or the value is unrecognized.
    pub fn extract_output_mode(&self, matches: &ArgMatches) -> OutputMode {
        if self.output_flag.is_some() {
            match matches
                .get_one::<String>("_output_mode")
                .map(|s| s.as_str())
            {
                Some("term") => OutputMode::Term,
                Some("text") => OutputMode::Text,
                Some("term-debug") => OutputMode::TermDebug,
                Some("json") => OutputMode::Json,
                Some("yaml") => OutputMode::Yaml,
                Some("xml") => OutputMode::Xml,
                Some("csv") => OutputMode::Csv,
                _ => OutputMode::Auto,
            }
        } else {
            OutputMode::Auto
        }
    }

    // =========================================================================
    // Rendering
    // =========================================================================

    /// Renders a template by name with the given data.
    ///
    /// Looks up the template in the registry and renders it.
    /// Supports `{% include %}` directives via the template registry.
    ///
    /// # Arguments
    ///
    /// * `template` - Template name to look up in the registry
    /// * `data` - Serializable data to render
    /// * `mode` - Output mode (Term, Text, Json, etc.)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No template registry is configured
    /// - The template is not found
    /// - Rendering fails
    pub fn render<T: Serialize>(
        &self,
        template: &str,
        data: &T,
        mode: OutputMode,
    ) -> Result<String, SetupError> {
        // For JSON/YAML/XML/CSV modes, serialize directly
        if mode.is_structured() {
            return self.serialize_data(data, mode);
        }

        // Get template registry
        let registry = self
            .template_registry
            .as_ref()
            .ok_or_else(|| SetupError::Config("No template registry configured".into()))?;

        // Get template content
        let template_content = registry
            .get_content(template)
            .map_err(|e| SetupError::Template(e.to_string()))?;

        // Render the template content
        self.render_template_content(&template_content, data, mode)
    }

    /// Renders an inline template string with the given data.
    ///
    /// Unlike `render`, this takes the template content directly.
    /// Still supports `{% include %}` if a template registry is configured.
    pub fn render_inline<T: Serialize>(
        &self,
        template: &str,
        data: &T,
        mode: OutputMode,
    ) -> Result<String, SetupError> {
        // For JSON/YAML/XML/CSV modes, serialize directly
        if mode.is_structured() {
            return self.serialize_data(data, mode);
        }

        self.render_template_content(template, data, mode)
    }

    /// Internal: renders template content with full support for includes and context.
    fn render_template_content<T: Serialize>(
        &self,
        template: &str,
        data: &T,
        mode: OutputMode,
    ) -> Result<String, SetupError> {
        use standout_bbparser::{BBParser, TagTransform, UnknownTagBehavior};

        let color_mode = detect_color_mode();
        let theme = self.theme.clone().unwrap_or_default();
        let styles = theme.resolve_styles(Some(color_mode));

        // Validate style aliases before rendering
        styles
            .validate()
            .map_err(|e| SetupError::Config(e.to_string()))?;

        // Build MiniJinja environment
        let mut env = Environment::new();
        register_filters(&mut env);

        // Load all templates from registry if available (enables {% include %})
        if let Some(registry) = &self.template_registry {
            for name in registry.names() {
                if let Ok(content) = registry.get_content(name) {
                    env.add_template_owned(name.to_string(), content)
                        .map_err(|e| SetupError::Template(e.to_string()))?;
                }
            }
        }

        // Add the inline template
        env.add_template_owned("_inline".to_string(), template.to_string())
            .map_err(|e| SetupError::Template(e.to_string()))?;

        let tmpl = env
            .get_template("_inline")
            .map_err(|e| SetupError::Template(e.to_string()))?;

        // Build render context for context providers
        let json_data =
            serde_json::to_value(data).map_err(|e| SetupError::Config(e.to_string()))?;
        let render_ctx = RenderContext::new(mode, get_terminal_width(), &theme, &json_data);

        // Build combined context: context providers + data
        let combined = self.build_combined_context(data, &render_ctx)?;

        // Pass 1: MiniJinja template rendering
        let minijinja_output = tmpl
            .render(&combined)
            .map_err(|e| SetupError::Template(e.to_string()))?;

        // Pass 2: BBParser style tag processing
        let transform = match mode {
            OutputMode::Term | OutputMode::Auto => TagTransform::Apply,
            OutputMode::TermDebug => TagTransform::Keep,
            _ => TagTransform::Remove,
        };
        let resolved_styles = styles.to_resolved_map();
        let parser = BBParser::new(resolved_styles, transform)
            .unknown_behavior(UnknownTagBehavior::Passthrough);
        let final_output = parser.parse(&minijinja_output);

        Ok(final_output)
    }

    /// Internal: builds combined context from context providers and data.
    fn build_combined_context<T: Serialize>(
        &self,
        data: &T,
        render_ctx: &RenderContext,
    ) -> Result<HashMap<String, minijinja::Value>, SetupError> {
        use minijinja::Value;

        // Resolve all context providers
        let context_values = self.context_registry.resolve(render_ctx);

        // Convert data to a map of values
        let data_value =
            serde_json::to_value(data).map_err(|e| SetupError::Config(e.to_string()))?;

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

    /// Internal: serializes data for structured output modes.
    fn serialize_data<T: Serialize>(
        &self,
        data: &T,
        mode: OutputMode,
    ) -> Result<String, SetupError> {
        match mode {
            OutputMode::Json => {
                serde_json::to_string_pretty(data).map_err(|e| SetupError::Config(e.to_string()))
            }
            OutputMode::Yaml => {
                serde_yaml::to_string(data).map_err(|e| SetupError::Config(e.to_string()))
            }
            OutputMode::Xml => {
                quick_xml::se::to_string(data).map_err(|e| SetupError::Config(e.to_string()))
            }
            OutputMode::Csv => {
                let value =
                    serde_json::to_value(data).map_err(|e| SetupError::Config(e.to_string()))?;
                let (headers, rows) = crate::util::flatten_json_for_csv(&value);

                let mut wtr = csv::Writer::from_writer(Vec::new());
                wtr.write_record(&headers)
                    .map_err(|e| SetupError::Config(e.to_string()))?;
                for row in rows {
                    wtr.write_record(&row)
                        .map_err(|e| SetupError::Config(e.to_string()))?;
                }
                let bytes = wtr
                    .into_inner()
                    .map_err(|e| SetupError::Config(e.to_string()))?;
                String::from_utf8(bytes).map_err(|e| SetupError::Config(e.to_string()))
            }
            _ => Err(SetupError::Config(format!(
                "Unexpected output mode: {:?}",
                mode
            ))),
        }
    }
}

/// Converts a serde_json::Value to a minijinja::Value.
fn json_to_minijinja(value: &serde_json::Value) -> minijinja::Value {
    match value {
        serde_json::Value::Null => minijinja::Value::UNDEFINED,
        serde_json::Value::Bool(b) => minijinja::Value::from(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                minijinja::Value::from(i)
            } else if let Some(u) = n.as_u64() {
                minijinja::Value::from(u)
            } else if let Some(f) = n.as_f64() {
                minijinja::Value::from(f)
            } else {
                minijinja::Value::UNDEFINED
            }
        }
        serde_json::Value::String(s) => minijinja::Value::from(s.as_str()),
        serde_json::Value::Array(arr) => {
            let items: Vec<minijinja::Value> = arr.iter().map(json_to_minijinja).collect();
            minijinja::Value::from(items)
        }
        serde_json::Value::Object(obj) => {
            let map: std::collections::BTreeMap<String, minijinja::Value> = obj
                .iter()
                .map(|(k, v)| (k.clone(), json_to_minijinja(v)))
                .collect();
            minijinja::Value::from(map)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_core_default() {
        let core = AppCore::new();
        assert_eq!(core.output_flag, Some("output".to_string()));
        assert_eq!(core.output_file_flag, Some("output-file-path".to_string()));
        assert_eq!(core.output_mode, OutputMode::Auto);
        assert!(core.theme.is_none());
        assert!(core.command_hooks.is_empty());
        assert!(core.default_command.is_none());
    }

    #[test]
    fn test_app_core_accessors() {
        let mut core = AppCore::new();
        core.output_mode = OutputMode::Json;
        core.default_command = Some("list".to_string());

        assert_eq!(core.output_mode(), OutputMode::Json);
        assert_eq!(core.default_command(), Some("list"));
    }

    #[test]
    fn test_extract_output_mode() {
        let core = AppCore::new();
        let cmd = core.augment_command(Command::new("test"));

        let matches = cmd
            .try_get_matches_from(["test", "--output", "json"])
            .unwrap();
        assert_eq!(core.extract_output_mode(&matches), OutputMode::Json);
    }

    #[test]
    fn test_extract_output_mode_disabled() {
        let mut core = AppCore::new();
        core.output_flag = None;

        let cmd = Command::new("test");
        let matches = cmd.try_get_matches_from(["test"]).unwrap();
        assert_eq!(core.extract_output_mode(&matches), OutputMode::Auto);
    }

    #[test]
    fn test_render_inline_json_mode() {
        let core = AppCore::new();
        let data = serde_json::json!({"name": "test", "count": 42});

        let result = core
            .render_inline("ignored", &data, OutputMode::Json)
            .unwrap();
        assert!(result.contains("\"name\": \"test\""));
        assert!(result.contains("\"count\": 42"));
    }

    #[test]
    fn test_render_inline_text_mode() {
        let core = AppCore::new();
        let data = serde_json::json!({"name": "Alice"});

        let result = core
            .render_inline("Hello, {{ name }}!", &data, OutputMode::Text)
            .unwrap();
        assert_eq!(result, "Hello, Alice!");
    }

    #[test]
    fn test_render_inline_yaml_mode() {
        let core = AppCore::new();
        let data = serde_json::json!({"name": "test", "count": 42});

        let result = core
            .render_inline("ignored", &data, OutputMode::Yaml)
            .unwrap();
        assert!(result.contains("name: test"));
        assert!(result.contains("count: 42"));
    }

    #[test]
    fn test_render_inline_xml_mode() {
        use serde::Serialize;

        #[derive(Serialize)]
        struct Data {
            name: String,
            count: i32,
        }

        let core = AppCore::new();
        let data = Data {
            name: "test".to_string(),
            count: 42,
        };

        let result = core
            .render_inline("ignored", &data, OutputMode::Xml)
            .unwrap();
        assert!(result.contains("<name>test</name>"));
        assert!(result.contains("<count>42</count>"));
    }

    #[test]
    fn test_render_inline_csv_mode() {
        let core = AppCore::new();
        let data = serde_json::json!({"name": "test", "count": 42});

        let result = core
            .render_inline("ignored", &data, OutputMode::Csv)
            .unwrap();
        // CSV should have headers and values
        assert!(result.contains("name"));
        assert!(result.contains("count"));
        assert!(result.contains("test"));
        assert!(result.contains("42"));
    }

    #[test]
    fn test_render_inline_with_include() {
        use crate::TemplateRegistry;

        // Create a template registry with a partial
        let mut registry = TemplateRegistry::new();
        registry.add_inline("_header.j2", "=== {{ title }} ===");

        let mut core = AppCore::new();
        core.template_registry = Some(Arc::new(registry));

        let data = serde_json::json!({"title": "My Title", "body": "Content here"});

        // Template that includes the header partial
        let template = r#"{% include "_header.j2" %}
{{ body }}"#;

        let result = core
            .render_inline(template, &data, OutputMode::Text)
            .unwrap();
        assert!(result.contains("=== My Title ==="));
        assert!(result.contains("Content here"));
    }

    #[test]
    fn test_render_with_template_registry() {
        use crate::TemplateRegistry;

        // Create a template registry with main template and partial
        let mut registry = TemplateRegistry::new();
        // Note: include inherits loop context, so we use item.name/item.value
        registry.add_inline("_item.j2", "- {{ item.name }}: {{ item.value }}");
        registry.add_inline(
            "list.j2",
            r#"Items:
{% for item in items %}{% include "_item.j2" %}
{% endfor %}"#,
        );

        let mut core = AppCore::new();
        core.template_registry = Some(Arc::new(registry));

        let data = serde_json::json!({
            "items": [
                {"name": "foo", "value": 1},
                {"name": "bar", "value": 2}
            ]
        });

        let result = core.render("list.j2", &data, OutputMode::Text).unwrap();
        assert!(result.contains("Items:"));
        assert!(result.contains("- foo: 1"));
        assert!(result.contains("- bar: 2"));
    }

    #[test]
    fn test_theme_accessor() {
        let mut core = AppCore::new();
        assert!(core.theme().is_none());

        core.theme = Some(Theme::default());
        assert!(core.theme().is_some());
    }

    #[test]
    fn test_theme_names_empty_without_registry() {
        let core = AppCore::new();
        assert!(core.theme_names().is_empty());
    }

    #[test]
    fn test_template_names_empty_without_registry() {
        let core = AppCore::new();
        assert_eq!(core.template_names().count(), 0);
    }

    #[test]
    fn test_template_names_with_registry() {
        use crate::TemplateRegistry;

        let mut registry = TemplateRegistry::new();
        registry.add_inline("foo.j2", "foo");
        registry.add_inline("bar.j2", "bar");

        let mut core = AppCore::new();
        core.template_registry = Some(Arc::new(registry));

        let names: Vec<_> = core.template_names().collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"foo.j2"));
        assert!(names.contains(&"bar.j2"));
    }

    #[test]
    fn test_get_hooks() {
        let mut core = AppCore::new();
        assert!(core.get_hooks("list").is_none());

        core.command_hooks.insert("list".to_string(), Hooks::new());
        assert!(core.get_hooks("list").is_some());
        assert!(core.get_hooks("other").is_none());
    }

    #[test]
    fn test_default_command() {
        let mut core = AppCore::new();
        assert!(core.default_command().is_none());

        core.default_command = Some("list".to_string());
        assert_eq!(core.default_command(), Some("list"));
    }
}
