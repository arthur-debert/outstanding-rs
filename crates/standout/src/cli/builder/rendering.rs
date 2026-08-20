//! Rendering methods for App.
//!
//! [`App::render`] and [`App::render_inline`] build a [`crate::RenderRequest`]
//! and call [`standout_render::render_request`], the same pipeline dispatch
//! uses.

use serde::Serialize;
use std::collections::HashMap;

use super::{refresh_named_template, App};
use crate::setup::SetupError;
use crate::{
    render_request, ColorPolicy, OutputMode, RenderRequest, TargetProperties, TemplateRef,
};
use standout_render::RegistryError;

impl App {
    /// Renders a template by name with the given data.
    ///
    /// Looks up the template in the registry and renders it through
    /// [`render_request`]. Supports `{% include %}` directives via the
    /// template registry.
    ///
    /// Detects destination facts at this edge and overwrites ambiguous-width
    /// with the application's configured policy.
    ///
    /// Structured modes (JSON/YAML/XML/CSV) serialize `data` through
    /// [`render_request`] with [`TemplateRef::Absent`] and do not look up or
    /// refresh a named template.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No template registry is configured for a named human-mode render
    /// - The template is not found
    /// - Rendering fails
    pub fn render<T: Serialize>(
        &self,
        template: &str,
        data: &T,
        mode: OutputMode,
    ) -> Result<String, SetupError> {
        if mode.is_structured() {
            return self.render_named_or_inline(TemplateRef::Absent, data, mode, None);
        }

        let registry = self.template_registry.as_ref().ok_or_else(|| {
            SetupError::Config(format!(
                "render({template:?}, ...) needs a template registry; add .templates(embed_templates!(\"src/templates\")) or .templates_dir(\"path/to/templates\") before .build(), or call render_inline(...) for inline template source"
            ))
        })?;

        {
            let mut engine = self.template_engine.borrow_mut();
            refresh_named_template(&mut **engine, registry, template).map_err(|error| {
                if matches!(registry.get(template), Err(RegistryError::NotFound { .. })) {
                    SetupError::Template(format!(
                        "render({template:?}, ...) could not find the named template; add it with .templates(embed_templates!(\"src/templates\")) or .templates_dir(\"path/to/templates\"): {error}"
                    ))
                } else {
                    SetupError::Template(format!(
                        "render({template:?}, ...) could not refresh the registered template: {error}"
                    ))
                }
            })?;
        }

        self.render_named_or_inline(
            TemplateRef::Named(template.to_string()),
            data,
            mode,
            Some(registry.clone()),
        )
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
        self.render_named_or_inline(
            TemplateRef::Inline(template.to_string()),
            data,
            mode,
            self.template_registry.clone(),
        )
    }

    fn render_named_or_inline<T: Serialize>(
        &self,
        template: TemplateRef,
        data: &T,
        mode: OutputMode,
        registry: Option<std::rc::Rc<crate::TemplateRegistry>>,
    ) -> Result<String, SetupError> {
        let mut target = TargetProperties::detect();
        target.ambiguous_width = self.ambiguous_width;
        let theme = self.theme.clone().unwrap_or_default();
        let request = RenderRequest {
            data: serde_json::to_value(data).map_err(|e| SetupError::Config(e.to_string()))?,
            template,
            theme,
            format: mode,
            color_policy: ColorPolicy::Auto,
            target,
            engine: self.template_engine.clone(),
            registry,
            context_registry: Some(self.context_registry.clone()),
            csv_projection: None,
            extras: HashMap::new(),
        };
        render_request(&request).map_err(|e| SetupError::Template(e.to_string()))
    }
}
