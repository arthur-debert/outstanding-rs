use serde::Serialize;
use std::collections::HashMap;

use super::{refresh_named_template, App};
use crate::setup::SetupError;
use crate::{
    render_request, ColorPolicy, OutputMode, RenderRequest, TargetProperties, TemplateRef,
};
use standout_render::RegistryError;

impl App {
    pub fn render<T: Serialize>(
        &self,
        template: &str,
        data: &T,
        mode: OutputMode,
    ) -> Result<String, SetupError> {
        self.render_named(template, data, mode, detected_target(self.ambiguous_width))
    }

    pub fn render_with<T: Serialize>(
        &self,
        template: &str,
        data: &T,
        mode: OutputMode,
        mut target: TargetProperties,
    ) -> Result<String, SetupError> {
        target.ambiguous_width = self.ambiguous_width;
        self.render_named(template, data, mode, target)
    }

    fn render_named<T: Serialize>(
        &self,
        template: &str,
        data: &T,
        mode: OutputMode,
        target: TargetProperties,
    ) -> Result<String, SetupError> {
        if mode.is_structured() {
            return self.render_named_or_inline(TemplateRef::Absent, data, mode, None, target);
        }

        let registry = self.template_registry.as_ref().ok_or_else(|| {
            SetupError::Config(format!(
                "render({template:?}, ...) needs a template registry; add .templates(embed_templates!(\"src/templates\")) or .templates_dir(\"path/to/templates\") before .build(), or call render_inline(...) for inline template source"
            ))
        })?;

        refresh_named_template(registry, template).map_err(|error| {
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

        self.render_named_or_inline(
            TemplateRef::Named(template.to_string()),
            data,
            mode,
            Some(registry.clone()),
            target,
        )
    }

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
            detected_target(self.ambiguous_width),
        )
    }

    pub fn render_inline_with<T: Serialize>(
        &self,
        template: &str,
        data: &T,
        mode: OutputMode,
        mut target: TargetProperties,
    ) -> Result<String, SetupError> {
        target.ambiguous_width = self.ambiguous_width;
        self.render_named_or_inline(
            TemplateRef::Inline(template.to_string()),
            data,
            mode,
            self.template_registry.clone(),
            target,
        )
    }

    fn render_named_or_inline<T: Serialize>(
        &self,
        template: TemplateRef,
        data: &T,
        mode: OutputMode,
        registry: Option<std::rc::Rc<crate::TemplateRegistry>>,
        target: TargetProperties,
    ) -> Result<String, SetupError> {
        let theme = self.theme.clone();
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
            warnings: None,
        };
        render_request(&request).map_err(|e| SetupError::Template(e.to_string()))
    }
}

fn detected_target(ambiguous_width: crate::AmbiguousWidth) -> TargetProperties {
    let mut target = TargetProperties::detect();
    target.ambiguous_width = ambiguous_width;
    target
}
