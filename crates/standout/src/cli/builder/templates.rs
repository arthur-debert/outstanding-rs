use super::AppBuilder;
use crate::setup::SetupError;
use crate::TemplateRegistry;
use crate::TEMPLATE_EXTENSIONS;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

pub(crate) type SharedTemplateEngine =
    Rc<RefCell<Box<dyn standout_render::template::TemplateEngine>>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TemplateRef {
    Named(String),
    Inline(String),
    Absent(TemplateAbsence),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TemplateAbsence {
    Silent,
    StructuredOnly,
    Binary,
}

impl TemplateRef {
    pub(crate) fn convention(command_path: &str) -> Self {
        Self::Named(command_path.replace('.', "/"))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TemplateRefreshError {
    name: String,
    location: String,
    message: String,
}

impl TemplateRefreshError {
    fn new(
        name: impl Into<String>,
        registry: &TemplateRegistry,
        message: impl Into<String>,
    ) -> Self {
        let name = name.into();
        Self {
            location: template_location(registry, &name),
            name,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for TemplateRefreshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.name.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(
                f,
                "template `{}`{} could not be refreshed: {}",
                self.name, self.location, self.message
            )
        }
    }
}

impl std::error::Error for TemplateRefreshError {}

pub(crate) fn template_location(registry: &TemplateRegistry, name: &str) -> String {
    match registry.get(name) {
        Ok(standout_render::template::ResolvedTemplate::File(path)) => {
            format!(" at `{}`", path.display())
        }
        Ok(standout_render::template::ResolvedTemplate::Inline(_)) | Err(_) => String::new(),
    }
}

pub(crate) fn refresh_engine_templates(
    engine: &mut dyn standout_render::template::TemplateEngine,
    registry: &TemplateRegistry,
) -> Result<(), TemplateRefreshError> {
    for name in registry.names() {
        let content = registry
            .get_content(name)
            .map_err(|error| TemplateRefreshError::new(name, registry, error.to_string()))?;
        engine
            .add_template(name, &content)
            .map_err(|error| TemplateRefreshError::new(name, registry, error.to_string()))?;
    }
    Ok(())
}

pub(crate) fn refresh_named_template(
    registry: &TemplateRegistry,
    name: &str,
) -> Result<(), TemplateRefreshError> {
    match registry.get_content(name) {
        Ok(_) => Ok(()),
        Err(standout_render::RegistryError::NotFound { .. }) => {
            let mut refreshed = registry.clone();
            refreshed
                .refresh()
                .map_err(|error| TemplateRefreshError::new(name, registry, error.to_string()))?;
            refreshed
                .get_content(name)
                .map_err(|error| TemplateRefreshError::new(name, &refreshed, error.to_string()))?;
            Ok(())
        }
        Err(error) => Err(TemplateRefreshError::new(name, registry, error.to_string())),
    }
}

fn missing_event_template_message(
    command_path: &str,
    template_name: &str,
    event_name: &str,
) -> String {
    format!(
        "command `{command_path}` produces its result while it runs, so it renders each event from template `{event_name}`, but that template is not registered; add it beside `{template_name}`, or drop the `Results` parameter if the command produces one batch value instead"
    )
}

fn missing_template_message(
    command_path: &str,
    template_name: &str,
    registry: Option<&TemplateRegistry>,
) -> String {
    let has_application_templates =
        registry.is_some_and(TemplateRegistry::has_application_templates);
    let mut message = if has_application_templates {
        format!(
            "command `{command_path}` references template `{template_name}`, but that template is not registered; add it with .templates(embed_templates!(\"src/templates\")) or .templates_dir(\"path/to/templates\")"
        )
    } else {
        format!(
            "command `{command_path}` references template `{template_name}`, but no application templates are configured; add .templates(embed_templates!(\"src/templates\")) or .templates_dir(\"path/to/templates\") before .build(), or declare no presentation with .structured_only(), .silent(), or .binary()"
        )
    };

    let Some(registry) = registry else {
        return message;
    };
    if !has_application_templates {
        return message;
    }

    let suggestions = nearest_template_names(template_name, registry);
    if !suggestions.is_empty() {
        message.push_str("; did you mean ");
        message.push_str(&suggestions.join(", "));
        message.push('?');
    } else {
        let available = available_template_names(registry);
        if !available.is_empty() {
            message.push_str("; available templates: ");
            message.push_str(&available.join(", "));
        }
    }
    message
}

fn available_template_names(registry: &TemplateRegistry) -> Vec<String> {
    canonical_template_names(registry)
        .into_iter()
        .take(5)
        .map(|candidate| format!("`{candidate}`"))
        .collect()
}

fn nearest_template_names(name: &str, registry: &TemplateRegistry) -> Vec<String> {
    let mut candidates: Vec<(usize, String)> = canonical_template_names(registry)
        .into_iter()
        .map(|candidate| (edit_distance(name, &candidate), candidate))
        .filter(|(distance, candidate)| {
            *distance <= 3 || candidate.contains(name) || name.contains(candidate)
        })
        .collect();
    candidates.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    candidates.dedup_by(|left, right| left.1 == right.1);
    candidates
        .into_iter()
        .take(3)
        .map(|(_, candidate)| format!("`{candidate}`"))
        .collect()
}

fn canonical_template_names(registry: &TemplateRegistry) -> Vec<String> {
    let mut names = BTreeMap::<String, String>::new();
    for name in registry.names() {
        let key = template_alias_key(name).to_string();
        match names.entry(key) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(name.to_string());
            }
            std::collections::btree_map::Entry::Occupied(mut entry)
                if standout_render::extension_priority(name, TEMPLATE_EXTENSIONS)
                    < standout_render::extension_priority(entry.get(), TEMPLATE_EXTENSIONS) =>
            {
                entry.insert(name.to_string());
            }
            std::collections::btree_map::Entry::Occupied(_) => {}
        }
    }
    names.into_values().collect()
}

fn template_alias_key(name: &str) -> &str {
    for extension in TEMPLATE_EXTENSIONS {
        if let Some(stripped) = name.strip_suffix(*extension) {
            return stripped;
        }
    }
    name
}

fn edit_distance(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    let mut costs: Vec<usize> = (0..=right.len()).collect();

    for (i, left_char) in left.iter().enumerate() {
        let mut previous = costs[0];
        costs[0] = i + 1;
        for (j, right_char) in right.iter().enumerate() {
            let substitution = previous + usize::from(left_char != right_char);
            previous = costs[j + 1];
            costs[j + 1] = (costs[j + 1] + 1).min(costs[j] + 1).min(substitution);
        }
    }

    costs[right.len()]
}

fn unique_unknown_tag_names<'a>(
    errors: impl IntoIterator<Item = &'a standout_bbparser::UnknownTagError>,
) -> Vec<String> {
    let mut names: Vec<String> = errors.into_iter().map(|error| error.tag.clone()).collect();
    names.sort_unstable();
    names.dedup();
    names
}

fn validate_framework_template_content(
    name: &str,
    content: &str,
    parser: &standout_bbparser::BBParser,
) -> Result<(), SetupError> {
    use standout_bbparser::UnknownTagKind;

    let Err(errors) = parser.validate(content) else {
        return Ok(());
    };

    let malformed = unique_unknown_tag_names(errors.errors.iter().filter(|error| {
        matches!(
            error.kind,
            UnknownTagKind::Unbalanced | UnknownTagKind::UnexpectedClose
        )
    }));
    if !malformed.is_empty() {
        return Err(SetupError::Template(format!(
            "framework template `{name}` contains malformed style markup involving tag(s): {}; fix the template source or disable framework templates with .include_framework_templates(false) if this app does not use them",
            malformed.join(", ")
        )));
    }

    let missing = unique_unknown_tag_names(
        errors
            .errors
            .iter()
            .filter(|error| !parser.styles().contains_key(&error.tag)),
    );
    if !missing.is_empty() {
        return Err(SetupError::Template(format!(
            "framework template `{name}` emits style tag(s) not defined by the resolved theme: {}; enable framework styles with .include_framework_styles(true), define the tag with .theme(...) or .styles(...), or disable framework templates with .include_framework_templates(false)",
            missing.join(", ")
        )));
    }

    Ok(())
}

impl AppBuilder {
    pub(super) fn validate_command_templates(&self) -> Result<(), SetupError> {
        for (path, pending) in self.pending_commands.borrow().iter() {
            let name = match &pending.template {
                TemplateRef::Named(name) => name.clone(),
                TemplateRef::Inline(_) | TemplateRef::Absent(_) => continue,
            };
            let Some(registry) = self.template_registry.as_ref() else {
                return Err(SetupError::Template(missing_template_message(
                    path, &name, None,
                )));
            };
            if pending.recipe.emits_events() {
                let event_name = format!("{name}.event");
                if let Err(error) = registry.get_content(&event_name) {
                    let message = match error {
                        standout_render::RegistryError::NotFound { .. } => {
                            missing_event_template_message(path, &name, &event_name)
                        }
                        _ => TemplateRefreshError::new(&event_name, registry, error.to_string())
                            .to_string(),
                    };
                    return Err(SetupError::Template(message));
                }
                // A handler returning `Output::Silent` renders no summary, and
                // the build cannot read which variant it returns, so a missing
                // summary template is a render error on the run instead.
                if matches!(
                    registry.get_content(&name),
                    Err(standout_render::RegistryError::NotFound { .. })
                ) {
                    continue;
                }
            }
            registry.get_content(&name).map_err(|error| {
                let message = match error {
                    standout_render::RegistryError::NotFound { .. } => {
                        missing_template_message(path, &name, Some(registry))
                    }
                    _ => TemplateRefreshError::new(&name, registry, error.to_string()).to_string(),
                };
                SetupError::Template(message)
            })?;
        }
        Ok(())
    }

    pub(super) fn validate_framework_template_styles(&self) -> Result<(), SetupError> {
        use standout_bbparser::{BBParser, TagTransform};

        let Some(registry) = &self.template_registry else {
            return Ok(());
        };
        let Some(theme) = &self.theme else {
            return Ok(());
        };

        let styles = theme.resolve_styles(None).to_resolved_map();
        let parser = BBParser::new(styles, TagTransform::Remove);

        for name in registry.framework_names() {
            let content = registry.get_content(name).map_err(|error| {
                SetupError::Template(
                    TemplateRefreshError::new(name, registry, error.to_string()).to_string(),
                )
            })?;
            validate_framework_template_content(name, &content, &parser)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use console::Style;
    use std::collections::HashMap;
    #[test]
    fn framework_template_validation_reports_malformed_markup_separately() {
        let parser = standout_bbparser::BBParser::new(
            HashMap::from([("known".to_string(), Style::new())]),
            standout_bbparser::TagTransform::Remove,
        );

        let error =
            validate_framework_template_content("standout/broken", "[known]unclosed", &parser)
                .unwrap_err()
                .to_string();

        assert!(error.contains("malformed style markup"), "{error}");
        assert!(error.contains("known"), "{error}");
        assert!(
            !error.contains("not defined by the resolved theme"),
            "{error}"
        );
    }

    #[test]
    fn framework_template_validation_reports_only_missing_styles() {
        let parser = standout_bbparser::BBParser::new(
            HashMap::from([("known".to_string(), Style::new())]),
            standout_bbparser::TagTransform::Remove,
        );

        let error = validate_framework_template_content(
            "standout/missing",
            "[missing]text[/missing]",
            &parser,
        )
        .unwrap_err()
        .to_string();

        assert!(
            error.contains("not defined by the resolved theme"),
            "{error}"
        );
        assert!(error.contains("missing"), "{error}");
        assert!(!error.contains("malformed style markup"), "{error}");
    }
}
