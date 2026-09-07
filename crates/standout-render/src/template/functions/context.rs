use serde::Serialize;
use std::collections::HashMap;

use crate::context::{ContextRegistry, RenderContext};
use crate::error::RenderError;
use crate::output::Representation;
use crate::request::{convenience_request, ColorPolicy, TargetProperties};
use crate::theme::{IconMode, Theme};
use crate::{render_request, TemplateRef};

#[allow(clippy::too_many_arguments)]
pub fn render_with_context<T: Serialize>(
    template: &str,
    data: &T,
    theme: &Theme,
    representation: Representation,
    color_policy: ColorPolicy,
    context_registry: &ContextRegistry,
    render_context: &RenderContext,
    template_registry: Option<&crate::template::TemplateRegistry>,
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
    template_registry: Option<&crate::template::TemplateRegistry>,
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

pub(super) fn build_icon_context(
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

pub(super) fn build_combined_context<T: Serialize>(
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::output::{Representation, StyleMode};
    use crate::{ColorPolicy, Theme};

    use crate::test_data as json;
    use serde::Serialize;

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
}
