use std::collections::HashMap;
use std::rc::Rc;

use clap::Command;
use serde::Serialize;
use standout_bbparser::{BBParser, TagTransform, UnknownTagKind};

use crate::assets::HELP_TEMPLATE_NAME;
use crate::topics::TopicRegistry;
use crate::{
    default_template_engine, render_request, ColorPolicy, OutputMode, RenderError, RenderRequest,
    SharedTemplateEngine, TargetProperties, TemplateRef, Theme,
};

use super::config::{default_help_theme, HelpConfig};
use super::data::{extract_help_data, extract_help_data_with_topics};

pub(crate) const DEFAULT_HELP_TEMPLATE: &str = include_str!("template.txt");

pub(crate) fn human_help_format(mode: OutputMode) -> OutputMode {
    if mode.is_structured() {
        OutputMode::Auto
    } else {
        mode
    }
}

fn resolve_help_theme(configured: Option<Theme>) -> Theme {
    match configured {
        Some(theme) => default_help_theme().merge(theme),
        None => default_help_theme(),
    }
}

pub(crate) fn inline_template_ref(
    source: &str,
    theme: &Theme,
    name: &str,
) -> Result<TemplateRef, RenderError> {
    validate_inline_template_tags(name, source, theme)?;
    Ok(TemplateRef::Inline(source.to_string()))
}

pub(crate) fn named_or_inline_template(
    registry: Option<&crate::TemplateRegistry>,
    named: &str,
    default_source: &str,
    theme: &Theme,
) -> Result<TemplateRef, RenderError> {
    match registry {
        Some(registry) => match registry.get(named) {
            Ok(_) => Ok(TemplateRef::Named(named.to_string())),
            Err(crate::RegistryError::NotFound { .. }) => {
                inline_template_ref(default_source, theme, named)
            }
            Err(error) => Err(RenderError::OperationError(error.to_string())),
        },
        None => inline_template_ref(default_source, theme, named),
    }
}

pub(crate) fn validate_inline_template_tags(
    name: &str,
    source: &str,
    theme: &Theme,
) -> Result<(), RenderError> {
    let styles = theme.resolve_styles(None).to_resolved_map();
    let parser = BBParser::new(styles, TagTransform::Remove);
    let Err(errors) = parser.validate(source) else {
        return Ok(());
    };

    let malformed = unique_tag_names(errors.errors.iter().filter(|error| {
        matches!(
            error.kind,
            UnknownTagKind::Unbalanced | UnknownTagKind::UnexpectedClose
        )
    }));
    if !malformed.is_empty() {
        return Err(RenderError::TemplateError(format!(
            "template `{name}` contains malformed style markup involving tag(s): {}",
            malformed.join(", ")
        )));
    }

    let missing = unique_tag_names(
        errors
            .errors
            .iter()
            .filter(|error| !parser.styles().contains_key(&error.tag)),
    );
    if !missing.is_empty() {
        return Err(RenderError::StyleError(format!(
            "template `{name}` emits style tag(s) not defined by the resolved theme: {}",
            missing.join(", ")
        )));
    }

    Ok(())
}

fn unique_tag_names<'a>(
    errors: impl IntoIterator<Item = &'a standout_bbparser::UnknownTagError>,
) -> Vec<String> {
    let mut names: Vec<String> = errors.into_iter().map(|error| error.tag.clone()).collect();
    names.sort_unstable();
    names.dedup();
    names
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_via_request<T: Serialize>(
    data: &T,
    template: TemplateRef,
    theme: Theme,
    format: OutputMode,
    target: TargetProperties,
    engine: SharedTemplateEngine,
    registry: Option<Rc<crate::TemplateRegistry>>,
    context_registry: Option<crate::context::ContextRegistry>,
    warnings: Option<standout_render::warnings::WarningBuffer>,
) -> Result<String, RenderError> {
    let request = RenderRequest {
        data: serde_json::to_value(data)?,
        template,
        theme,
        format: human_help_format(format),
        color_policy: ColorPolicy::Auto,
        target,
        engine,
        registry,
        context_registry,
        csv_projection: None,
        extras: HashMap::new(),
        warnings,
    };
    render_request(&request)
}

pub fn render_help(cmd: &Command, config: Option<HelpConfig>) -> Result<String, RenderError> {
    let config = config.unwrap_or_default();
    let theme = resolve_help_theme(config.theme);
    let template = match config.template.as_deref() {
        Some(source) => inline_template_ref(source, &theme, HELP_TEMPLATE_NAME)?,
        None => inline_template_ref(DEFAULT_HELP_TEMPLATE, &theme, HELP_TEMPLATE_NAME)?,
    };
    let target = TargetProperties::detect();
    let data = extract_help_data(
        cmd,
        config.command_groups.as_deref(),
        config.length,
        &target,
    );
    render_via_request(
        &data,
        template,
        theme,
        config.output_mode.unwrap_or(OutputMode::Auto),
        target,
        default_template_engine(),
        None,
        None,
        None,
    )
}

pub fn render_help_with_topics(
    cmd: &Command,
    registry: &TopicRegistry,
    config: Option<HelpConfig>,
) -> Result<String, RenderError> {
    let config = config.unwrap_or_default();
    let theme = resolve_help_theme(config.theme);
    let template = match config.template.as_deref() {
        Some(source) => inline_template_ref(source, &theme, HELP_TEMPLATE_NAME)?,
        None => inline_template_ref(DEFAULT_HELP_TEMPLATE, &theme, HELP_TEMPLATE_NAME)?,
    };
    let target = TargetProperties::detect();
    let data = extract_help_data_with_topics(
        cmd,
        registry,
        config.command_groups.as_deref(),
        config.length,
        &target,
    );
    render_via_request(
        &data,
        template,
        theme,
        config.output_mode.unwrap_or(OutputMode::Auto),
        target,
        default_template_engine(),
        None,
        None,
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Command;
    use console::Style;

    fn cmd() -> Command {
        Command::new("app").about("Demo")
    }

    #[test]
    fn structured_modes_still_print_human_help() {
        for mode in [
            OutputMode::Json,
            OutputMode::Yaml,
            OutputMode::Csv,
            OutputMode::Xml,
        ] {
            let output = render_help(
                &cmd(),
                Some(HelpConfig {
                    output_mode: Some(mode),
                    ..Default::default()
                }),
            )
            .unwrap();
            assert!(
                output.contains("USAGE"),
                "{mode:?} must print human help, got:\n{output}"
            );
            assert!(
                !output.trim_start().starts_with('{'),
                "{mode:?} must not emit a JSON help document:\n{output}"
            );
        }
    }

    #[test]
    fn custom_template_unknown_tag_fails_at_construction() {
        let err = render_help(
            &cmd(),
            Some(HelpConfig {
                template: Some("[nope]hello[/nope]".into()),
                output_mode: Some(OutputMode::Text),
                ..Default::default()
            }),
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("nope"), "{msg}");
        assert!(msg.contains("not defined by the resolved theme"), "{msg}");
    }

    #[test]
    fn custom_template_known_tag_renders() {
        let output = render_help(
            &cmd(),
            Some(HelpConfig {
                template: Some("[header]HELLO[/header]".into()),
                output_mode: Some(OutputMode::Text),
                theme: Some(Theme::new().add("header", Style::new().bold())),
                ..Default::default()
            }),
        )
        .unwrap();
        assert!(output.contains("HELLO"), "{output}");
    }

    #[test]
    fn human_help_format_maps_structured_to_auto() {
        assert_eq!(human_help_format(OutputMode::Json), OutputMode::Auto);
        assert_eq!(human_help_format(OutputMode::Yaml), OutputMode::Auto);
        assert_eq!(human_help_format(OutputMode::Csv), OutputMode::Auto);
        assert_eq!(human_help_format(OutputMode::Xml), OutputMode::Auto);
        assert_eq!(human_help_format(OutputMode::Term), OutputMode::Term);
        assert_eq!(human_help_format(OutputMode::Text), OutputMode::Text);
        assert_eq!(human_help_format(OutputMode::Auto), OutputMode::Auto);
    }

    #[test]
    fn registered_file_override_is_named_without_reading_content() {
        let mut registry = crate::TemplateRegistry::new();
        registry
            .add_from_files(vec![crate::TemplateFile::new(
                HELP_TEMPLATE_NAME,
                "standout/help.jinja",
                "/missing/standout/help.jinja",
                "/missing",
            )])
            .unwrap();

        let theme = default_help_theme();
        let template = named_or_inline_template(
            Some(&registry),
            HELP_TEMPLATE_NAME,
            DEFAULT_HELP_TEMPLATE,
            &theme,
        )
        .unwrap();
        assert_eq!(
            template,
            TemplateRef::Named(HELP_TEMPLATE_NAME.to_string()),
            "an unreadable registered override must stay Named so load can surface the read error"
        );
    }

    #[test]
    fn missing_named_template_falls_back_to_inline_default() {
        let registry = crate::TemplateRegistry::new();
        let theme = default_help_theme();
        let template = named_or_inline_template(
            Some(&registry),
            HELP_TEMPLATE_NAME,
            DEFAULT_HELP_TEMPLATE,
            &theme,
        )
        .unwrap();
        assert!(
            matches!(template, TemplateRef::Inline(_)),
            "NotFound must fall back to the inline default, got {template:?}"
        );
    }

    #[test]
    fn no_registry_falls_back_to_inline_default() {
        let theme = default_help_theme();
        let template =
            named_or_inline_template(None, HELP_TEMPLATE_NAME, DEFAULT_HELP_TEMPLATE, &theme)
                .unwrap();
        assert!(
            matches!(template, TemplateRef::Inline(_)),
            "no registry must fall back to the inline default, got {template:?}"
        );
    }
}
