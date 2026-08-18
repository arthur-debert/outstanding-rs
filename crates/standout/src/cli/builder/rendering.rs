//! Rendering methods for App.
//!
//! This module contains methods for rendering templates and serializing data
//! across all output modes (Term, Text, JSON, YAML, XML, CSV).

use serde::Serialize;
use std::collections::HashMap;

use super::App;
use crate::context::RenderContext;
use crate::setup::SetupError;
use crate::{detect_color_mode, detect_icon_mode, OutputMode};

impl App {
    // =========================================================================
    // Public Rendering API
    // =========================================================================

    /// Renders a template by name with the given data.
    ///
    /// Looks up the template in the registry and renders it.
    /// Supports `{% include %}` directives via the template registry.
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

    // =========================================================================
    // Internal Rendering
    // =========================================================================

    /// Internal: renders template content with full support for includes and context.
    fn render_template_content<T: Serialize>(
        &self,
        template: &str,
        data: &T,
        mode: OutputMode,
    ) -> Result<String, SetupError> {
        use standout_bbparser::{TagTransform, UnknownTagBehavior};

        let color_mode = detect_color_mode();
        let theme = self.theme.clone().unwrap_or_default();
        let styles = theme.resolve_styles(Some(color_mode));

        // Validate style aliases before rendering
        styles
            .validate()
            .map_err(|e| SetupError::Config(e.to_string()))?;

        // Build render context for context providers
        let json_data =
            serde_json::to_value(data).map_err(|e| SetupError::Config(e.to_string()))?;
        let ambiguous_width =
            standout_render::detect_ambiguous_width_override().unwrap_or(self.ambiguous_width);
        let render_ctx = RenderContext::with_ambiguous_width(
            mode,
            standout_render::detect_terminal_width(),
            ambiguous_width,
            &theme,
            &json_data,
        );

        // Build combined context: context providers + data
        let combined_minijinja_map = self.build_combined_context(data, &render_ctx)?;

        // Convert minijinja::Value map to serde_json::Value map for the template engine
        let combined_json_map: serde_json::Map<String, serde_json::Value> = combined_minijinja_map
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    serde_json::to_value(v).unwrap_or(serde_json::Value::Null),
                )
            })
            .collect();

        // Pass 1: Template rendering via engine
        let minijinja_output = self
            .template_engine
            .borrow()
            .render_template_with_render_widths(
                template,
                &serde_json::Value::Object(combined_json_map),
                render_ctx.terminal_width,
                ambiguous_width,
            )
            .map_err(|e| SetupError::Template(e.to_string()))?;

        // Pass 2: BBParser style tag processing
        let transform = match mode {
            OutputMode::Term | OutputMode::Auto => TagTransform::Apply,
            OutputMode::TermDebug => TagTransform::Keep,
            _ => TagTransform::Remove,
        };
        let final_output = standout_render::diagnostics::resolve_tags(
            &minijinja_output,
            styles.to_resolved_map(),
            transform,
            UnknownTagBehavior::Strip,
        );

        Ok(final_output)
    }

    /// Internal: builds combined context from icons, context providers, and data.
    fn build_combined_context<T: Serialize>(
        &self,
        data: &T,
        render_ctx: &RenderContext,
    ) -> Result<HashMap<String, serde_json::Value>, SetupError> {
        // Start with icon context (lowest priority)
        let mut combined: HashMap<String, serde_json::Value> = HashMap::new();
        if let Some(theme) = &self.theme {
            if !theme.icons().is_empty() {
                let icon_mode = detect_icon_mode();
                let resolved_icons = theme.resolve_icons(icon_mode);
                if let Ok(icons_value) = serde_json::to_value(resolved_icons) {
                    combined.insert("icons".to_string(), icons_value);
                }
            }
        }

        // Resolve all context providers (medium priority)
        let context_values_minijinja = self.context_registry.resolve(render_ctx);

        // Convert data to a map of values
        let data_value =
            serde_json::to_value(data).map_err(|e| SetupError::Config(e.to_string()))?;

        // Add context values (medium priority)
        for (key, val) in context_values_minijinja {
            let json_val = serde_json::to_value(val).map_err(|e| {
                SetupError::Config(format!("Failed to convert context value: {}", e))
            })?;
            combined.insert(key, json_val);
        }

        // Add data values (highest priority - overwrites context)
        if let Some(obj) = data_value.as_object() {
            for (key, value) in obj {
                combined.insert(key.clone(), value.clone());
            }
        }

        Ok(combined)
    }

    /// Internal: serializes data for structured output modes.
    pub(crate) fn serialize_data<T: Serialize>(
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
                crate::serialize_to_xml(data).map_err(|e| SetupError::Config(e.to_string()))
            }
            OutputMode::Csv => {
                let value =
                    serde_json::to_value(data).map_err(|e| SetupError::Config(e.to_string()))?;
                let (headers, rows) = crate::flatten_json_for_csv(&value);

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
