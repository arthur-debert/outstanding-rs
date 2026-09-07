use std::collections::HashMap;

use super::*;

use crate::output::{Representation, StyleMode};
use crate::{ColorPolicy, Theme};
use console::Style;

use crate::template::functions::test_data::SimpleData;

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
        template: TemplateRef::Inline("{{ icons.check }} {{ message }} {{ icons.arrow }}".into()),
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
