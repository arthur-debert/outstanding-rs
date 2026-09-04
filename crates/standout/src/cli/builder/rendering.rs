use serde::Serialize;
use std::collections::HashMap;

use super::{refresh_named_template, App};
use crate::setup::SetupError;
use crate::{
    render_request, ColorPolicy, RenderRequest, Representation, TargetProperties, TemplateRef,
};
use standout_render::RegistryError;

impl App {
    pub fn render_with<T: Serialize>(
        &self,
        template: TemplateRef,
        data: &T,
        mode: Representation,
        mut target: TargetProperties,
    ) -> Result<String, SetupError> {
        target.ambiguous_width = self.ambiguous_width;

        if mode.is_structured() {
            return self.render_resolved(TemplateRef::Absent, data, mode, None, target);
        }

        let TemplateRef::Named(name) = &template else {
            return self.render_resolved(
                template,
                data,
                mode,
                self.template_registry.clone(),
                target,
            );
        };

        let registry = self.template_registry.as_ref().ok_or_else(|| {
            SetupError::Config(format!(
                "render_with(TemplateRef::Named({name:?}), ...) needs a template registry; add .templates(embed_templates!(\"src/templates\")) or .templates_dir(\"path/to/templates\") before .build(), or pass TemplateRef::Inline for inline template source"
            ))
        })?;

        refresh_named_template(registry, name).map_err(|error| {
            if matches!(registry.get(name), Err(RegistryError::NotFound { .. })) {
                SetupError::Template(format!(
                    "render_with(TemplateRef::Named({name:?}), ...) could not find the named template; add it with .templates(embed_templates!(\"src/templates\")) or .templates_dir(\"path/to/templates\"): {error}"
                ))
            } else {
                SetupError::Template(format!(
                    "render_with(TemplateRef::Named({name:?}), ...) could not refresh the registered template: {error}"
                ))
            }
        })?;

        let registry = registry.clone();
        self.render_resolved(template, data, mode, Some(registry), target)
    }

    fn render_resolved<T: Serialize>(
        &self,
        template: TemplateRef,
        data: &T,
        mode: Representation,
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
