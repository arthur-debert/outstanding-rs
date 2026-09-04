use std::collections::HashMap;
use std::rc::Rc;

use clap::Command;
use serde::Serialize;
use standout_bbparser::{BBParser, TagTransform, UnknownTagKind};

use crate::assets::HELP_TEMPLATE_NAME;
use crate::topics::TopicRegistry;
use crate::{
    default_template_engine, render_request, ColorPolicy, RenderError, RenderRequest,
    Representation, SharedTemplateEngine, TargetProperties, TemplateRef, Theme,
};

use super::config::{default_help_theme, HelpConfig, HelpLength};
use super::data::{extract_help_data, extract_help_data_with_topics};
use super::document::HelpDocument;

pub(crate) const DEFAULT_HELP_TEMPLATE: &str = include_str!("template.txt");

/// `csv` has no help projection and is a render error.
pub(crate) fn help_is_a_document(mode: Representation) -> bool {
    matches!(
        mode,
        Representation::Json | Representation::Yaml | Representation::Csv
    )
}

/// A structured mode with no help document falls back to the terminal's resolution.
pub(crate) fn human_help_format(mode: Representation) -> Representation {
    if mode.is_structured() {
        Representation::Human
    } else {
        mode
    }
}

/// Without the newline the process edge appends; `None` when no command sits at `path`.
pub(crate) fn render_help_document(
    root: &Command,
    path: &[&str],
    length: HelpLength,
    mode: Representation,
) -> Result<Option<String>, RenderError> {
    if !matches!(mode, Representation::Json | Representation::Yaml) {
        return Err(RenderError::OperationError(format!(
            "the help document has no {mode:?} projection"
        )));
    }
    let Some(document) = HelpDocument::extract(root, path, length) else {
        return Ok(None);
    };
    let mut text = standout_render::serialize_document(&document, mode)?;
    while text.ends_with('\n') {
        text.pop();
    }
    Ok(Some(text))
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
    format: Representation,
    color_policy: ColorPolicy,
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
        color_policy,
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

fn standalone_document(cmd: &Command, config: &HelpConfig) -> Option<Result<String, RenderError>> {
    let mode = config.output_mode.unwrap_or(Representation::Human);
    help_is_a_document(mode).then(|| {
        render_help_document(cmd, &[], config.length, mode)
            .map(|document| document.expect("the root is always at the empty path"))
    })
}

pub fn render_help(cmd: &Command, config: Option<HelpConfig>) -> Result<String, RenderError> {
    let config = config.unwrap_or_default();
    if let Some(document) = standalone_document(cmd, &config) {
        return document;
    }
    let theme = resolve_help_theme(config.theme);
    let template = match config.template.as_deref() {
        Some(source) => inline_template_ref(source, &theme, HELP_TEMPLATE_NAME)?,
        None => inline_template_ref(DEFAULT_HELP_TEMPLATE, &theme, HELP_TEMPLATE_NAME)?,
    };
    let target = TargetProperties::detect();
    let data = extract_help_data(
        cmd,
        &[],
        config.command_groups.as_deref(),
        config.length,
        &target,
    )
    .expect("the root is always at the empty path");
    render_via_request(
        &data,
        template,
        theme,
        config.output_mode.unwrap_or(Representation::Human),
        config.color,
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
    if let Some(document) = standalone_document(cmd, &config) {
        return document;
    }
    let theme = resolve_help_theme(config.theme);
    let template = match config.template.as_deref() {
        Some(source) => inline_template_ref(source, &theme, HELP_TEMPLATE_NAME)?,
        None => inline_template_ref(DEFAULT_HELP_TEMPLATE, &theme, HELP_TEMPLATE_NAME)?,
    };
    let target = TargetProperties::detect();
    let data = extract_help_data_with_topics(
        cmd,
        &[],
        registry,
        config.command_groups.as_deref(),
        config.length,
        &target,
    )
    .expect("the root is always at the empty path");
    render_via_request(
        &data,
        template,
        theme,
        config.output_mode.unwrap_or(Representation::Human),
        config.color,
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

    fn help_in(mode: Representation) -> Result<String, RenderError> {
        render_help(
            &cmd(),
            Some(HelpConfig {
                output_mode: Some(mode),
                ..Default::default()
            }),
        )
    }

    #[test]
    fn json_and_yaml_answer_with_the_help_document() {
        let json = help_in(Representation::Json).unwrap();
        let document: HelpDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(document.schema_version, 1);
        assert_eq!(document.path, ["app"]);
        assert!(!json.contains("USAGE"), "{json}");
        assert!(!json.ends_with('\n'), "{json:?}");

        let yaml = help_in(Representation::Yaml).unwrap();
        assert!(yaml.starts_with("schema_version: 1\nname: app\n"), "{yaml}");
    }

    #[test]
    fn csv_has_no_help_projection() {
        let error = help_in(Representation::Csv).unwrap_err().to_string();
        assert!(error.contains("no Csv projection"), "{error}");
    }

    #[test]
    fn ndjson_prints_the_human_help_page() {
        let output = help_in(Representation::Ndjson).unwrap();
        assert!(output.contains("USAGE"), "{output}");
        assert!(!output.trim_start().starts_with('{'), "{output}");
    }

    #[test]
    fn a_mode_is_either_the_page_or_the_document() {
        assert!(help_is_a_document(Representation::Json));
        assert!(help_is_a_document(Representation::Yaml));
        assert!(help_is_a_document(Representation::Csv));
        assert!(!help_is_a_document(Representation::Ndjson));
        assert!(!help_is_a_document(Representation::Human));
        assert!(!help_is_a_document(Representation::Human));
    }

    #[test]
    fn custom_template_unknown_tag_fails_at_construction() {
        let err = render_help(
            &cmd(),
            Some(HelpConfig {
                template: Some("[nope]hello[/nope]".into()),
                output_mode: Some(Representation::Human),
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
                output_mode: Some(Representation::Human),
                theme: Some(Theme::new().add("header", Style::new().bold())),
                ..Default::default()
            }),
        )
        .unwrap();
        assert!(output.contains("HELLO"), "{output}");
    }

    #[test]
    fn human_help_format_maps_structured_to_auto() {
        assert_eq!(
            human_help_format(Representation::Json),
            Representation::Human
        );
        assert_eq!(
            human_help_format(Representation::Yaml),
            Representation::Human
        );
        assert_eq!(
            human_help_format(Representation::Csv),
            Representation::Human
        );
        assert_eq!(
            human_help_format(Representation::Ndjson),
            Representation::Human
        );
        assert_eq!(
            human_help_format(Representation::Human),
            Representation::Human
        );
        assert_eq!(
            human_help_format(Representation::Human),
            Representation::Human
        );
        assert_eq!(
            human_help_format(Representation::Human),
            Representation::Human
        );
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
