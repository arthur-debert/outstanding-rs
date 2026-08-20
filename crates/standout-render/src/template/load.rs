//! Load named templates and their include/import/extends tree into an engine.
//!
//! [`RenderRequest::registry`](crate::RenderRequest::registry) is the explicit
//! dependency for named templates and includes. Direct [`crate::render_request`]
//! callers must not have to pre-populate the engine: this module recursively
//! loads the named template and its static dependencies, and refreshes the
//! registry when a dependency is dynamic.

use std::collections::HashSet;

use super::engine::TemplateEngine;
use super::registry::{RegistryError, ResolvedTemplate, TemplateRegistry};
use crate::error::RenderError;

/// Loads `name` and its static include/import/extends dependencies into `engine`.
///
/// If the named template (or a static dependency) is missing, the registry is
/// refreshed once and the load is retried, so a file-backed template that
/// appeared on disk is picked up. A dynamic include/import/extends expression
/// refreshes the registry and loads every registered template, because the
/// referenced name is not known until render.
pub fn load_named_template(
    engine: &mut dyn TemplateEngine,
    registry: &TemplateRegistry,
    name: &str,
) -> Result<(), RenderError> {
    load_with_missing_refresh(engine, registry, |engine, registry, allow_optional_skip| {
        let mut seen = HashSet::new();
        load_tree(engine, registry, name, &mut seen, true, allow_optional_skip)
    })
}

/// Loads static include/import/extends dependencies of inline source.
///
/// Same refresh rules as [`load_named_template`]: a missing static dependency
/// refreshes the registry once; a dynamic expression loads every registered
/// template.
pub(crate) fn load_inline_dependencies(
    engine: &mut dyn TemplateEngine,
    registry: &TemplateRegistry,
    source: &str,
) -> Result<(), RenderError> {
    load_with_missing_refresh(engine, registry, |engine, registry, allow_optional_skip| {
        let mut seen = HashSet::new();
        load_source_tree(engine, registry, source, &mut seen, allow_optional_skip)
    })
}

fn load_with_missing_refresh(
    engine: &mut dyn TemplateEngine,
    registry: &TemplateRegistry,
    attempt: impl Fn(&mut dyn TemplateEngine, &TemplateRegistry, bool) -> Result<(), RenderError>,
) -> Result<(), RenderError> {
    match attempt(engine, registry, false) {
        Err(error) if is_not_found(&error) => {
            let mut refreshed = registry.clone();
            refreshed.refresh().map_err(registry_error)?;
            match attempt(engine, &refreshed, true) {
                Err(RenderError::TemplateNotFound(name)) => Err(refresh_error(
                    &name,
                    &refreshed,
                    format!("Template not found: \"{name}\""),
                )),
                other => other,
            }
        }
        other => other,
    }
}

fn load_tree(
    engine: &mut dyn TemplateEngine,
    registry: &TemplateRegistry,
    name: &str,
    seen: &mut HashSet<String>,
    required: bool,
    allow_optional_skip: bool,
) -> Result<(), RenderError> {
    if seen.contains(name) {
        return Ok(());
    }

    let content = match registry.get_content(name) {
        Ok(content) => content,
        Err(RegistryError::NotFound { name }) => {
            if required || !allow_optional_skip {
                return Err(RenderError::TemplateNotFound(name));
            }
            return Ok(());
        }
        Err(error) => return Err(refresh_error(name, registry, error)),
    };
    seen.insert(name.to_string());
    engine
        .add_template(name, &content)
        .map_err(|error| refresh_error(name, registry, error))?;
    load_source_tree(engine, registry, &content, seen, allow_optional_skip)
}

fn load_source_tree(
    engine: &mut dyn TemplateEngine,
    registry: &TemplateRegistry,
    source: &str,
    seen: &mut HashSet<String>,
    allow_optional_skip: bool,
) -> Result<(), RenderError> {
    let dependencies = template_dependencies(source);
    if dependencies.dynamic {
        let mut refreshed = registry.clone();
        refreshed.refresh().map_err(registry_error)?;
        return load_all(engine, &refreshed);
    }

    for dependency in dependencies.dependencies {
        match dependency {
            Dependency::Required(name) => {
                load_tree(engine, registry, &name, seen, true, allow_optional_skip)?;
            }
            Dependency::Optional(name) => {
                load_tree(engine, registry, &name, seen, false, allow_optional_skip)?;
            }
            Dependency::Alternatives { names, optional } => {
                load_first_alternative(
                    engine,
                    registry,
                    &names,
                    optional,
                    seen,
                    allow_optional_skip,
                )?;
            }
        }
    }

    Ok(())
}

/// Loads the first include-list candidate that exists, then stops.
///
/// Before the registry has been refreshed, a missing candidate is treated as
/// not-found so newly added files can appear. After that refresh, a required
/// list with no candidates errors; an `ignore missing` list succeeds.
fn load_first_alternative(
    engine: &mut dyn TemplateEngine,
    registry: &TemplateRegistry,
    names: &[String],
    optional: bool,
    seen: &mut HashSet<String>,
    allow_optional_skip: bool,
) -> Result<(), RenderError> {
    for name in names {
        if seen.contains(name) {
            return Ok(());
        }
        let content = match registry.get_content(name) {
            Ok(content) => content,
            Err(RegistryError::NotFound { name }) => {
                if !allow_optional_skip {
                    return Err(RenderError::TemplateNotFound(name));
                }
                continue;
            }
            Err(error) => return Err(refresh_error(name, registry, error)),
        };
        seen.insert(name.clone());
        engine
            .add_template(name, &content)
            .map_err(|error| refresh_error(name, registry, error))?;
        return load_source_tree(engine, registry, &content, seen, allow_optional_skip);
    }

    if optional {
        Ok(())
    } else {
        Err(RenderError::TemplateNotFound(
            names
                .first()
                .cloned()
                .unwrap_or_else(|| "include-list".into()),
        ))
    }
}

fn load_all(
    engine: &mut dyn TemplateEngine,
    registry: &TemplateRegistry,
) -> Result<(), RenderError> {
    for name in registry.names() {
        let content = match registry.get_content(name) {
            Ok(content) => content,
            Err(RegistryError::NotFound { name }) => {
                return Err(RenderError::TemplateNotFound(name));
            }
            Err(error) => return Err(refresh_error(name, registry, error)),
        };
        engine
            .add_template(name, &content)
            .map_err(|error| refresh_error(name, registry, error))?;
    }
    Ok(())
}

fn registry_error(error: RegistryError) -> RenderError {
    match error {
        RegistryError::NotFound { name } => RenderError::TemplateNotFound(name),
        other => RenderError::OperationError(other.to_string()),
    }
}

fn refresh_error(
    name: &str,
    registry: &TemplateRegistry,
    error: impl std::fmt::Display,
) -> RenderError {
    let location = match registry.get(name) {
        Ok(ResolvedTemplate::File(path)) => format!(" at `{}`", path.display()),
        Ok(ResolvedTemplate::Inline(_)) | Err(_) => String::new(),
    };
    RenderError::OperationError(format!(
        "template `{name}`{location} could not be refreshed: {error}"
    ))
}

fn is_not_found(error: &RenderError) -> bool {
    matches!(error, RenderError::TemplateNotFound(_))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Dependency {
    /// `{% extends %}`, `{% import %}`, `{% from %}`, or a bare `{% include %}`.
    Required(String),
    /// `{% include 'name' ignore missing %}`.
    Optional(String),
    /// `{% include ['first', 'second'] %}`: first existing candidate wins.
    Alternatives { names: Vec<String>, optional: bool },
}

#[derive(Default)]
struct TemplateDependencies {
    dependencies: Vec<Dependency>,
    dynamic: bool,
}

fn template_dependencies(source: &str) -> TemplateDependencies {
    let mut dependencies = TemplateDependencies::default();
    let mut cursor = 0;

    while let Some(open) = find_next_template_syntax(source, cursor) {
        if source[open..].starts_with("{#") {
            let Some(close) = source[open + 2..].find("#}") else {
                break;
            };
            cursor = open + 2 + close + 2;
            continue;
        }

        if source[open..].starts_with("{{") {
            let after_start = open + 2;
            let Some(close) = find_closing_delimiter(source, after_start, b"}}") else {
                break;
            };
            cursor = close + 2;
            continue;
        }

        let Some((tag, close)) = read_tag(source, open) else {
            break;
        };
        let (keyword, body) = split_tag(&tag);
        if keyword == "raw" {
            cursor = find_endraw(source, close).unwrap_or(source.len());
            continue;
        }

        let expression = match keyword {
            "include" | "extends" | "import" => Some(body),
            "from" => body
                .split_once(" import ")
                .map(|(template, _imports)| template.trim()),
            _ => None,
        };

        if matches!(keyword, "include" | "extends" | "import" | "from") {
            match expression.and_then(|expression| static_dependency(keyword, expression)) {
                Some(dependency) => dependencies.dependencies.push(dependency),
                None => dependencies.dynamic = true,
            }
        }
        cursor = close;
    }

    dependencies
}

fn find_next_template_syntax(source: &str, cursor: usize) -> Option<usize> {
    let rest = &source[cursor..];
    ["{%", "{{", "{#"]
        .into_iter()
        .filter_map(|marker| rest.find(marker).map(|index| cursor + index))
        .min()
}

fn read_tag(source: &str, open: usize) -> Option<(String, usize)> {
    if !source[open..].starts_with("{%") {
        return None;
    }
    let after_start = open + 2;
    let tag_end = find_closing_delimiter(source, after_start, b"%}")?;
    let close = tag_end + 2;
    Some((normalize_tag(&source[after_start..tag_end]), close))
}

fn find_closing_delimiter(source: &str, start: usize, delimiter: &[u8]) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut cursor = start;
    let mut quote = None;
    let mut escaped = false;

    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
            cursor += 1;
            continue;
        }

        if matches!(byte, b'\'' | b'"') {
            quote = Some(byte);
            cursor += 1;
            continue;
        }

        if bytes[cursor..].starts_with(delimiter) {
            return Some(cursor);
        }
        cursor += 1;
    }

    None
}

fn normalize_tag(tag: &str) -> String {
    let tag = tag.trim();
    let tag = tag
        .strip_prefix('-')
        .or_else(|| tag.strip_prefix('+'))
        .unwrap_or(tag)
        .trim_start();
    let tag = tag
        .strip_suffix('-')
        .or_else(|| tag.strip_suffix('+'))
        .unwrap_or(tag)
        .trim_end();
    tag.to_string()
}

fn split_tag(tag: &str) -> (&str, &str) {
    let mut words = tag.splitn(2, char::is_whitespace);
    let keyword = words.next().unwrap_or_default();
    let body = words.next().unwrap_or_default().trim();
    (keyword, body)
}

fn find_endraw(source: &str, cursor: usize) -> Option<usize> {
    let mut cursor = cursor;
    while let Some(open) = source[cursor..].find("{%").map(|index| cursor + index) {
        match read_tag(source, open) {
            Some((tag, close)) => {
                if split_tag(&tag).0 == "endraw" {
                    return Some(close);
                }
                cursor = close;
            }
            None => {
                // Inside `{% raw %}`, unclosed `{%` is literal content.
                cursor = open + 2;
            }
        }
    }
    None
}

fn static_dependency(keyword: &str, expression: &str) -> Option<Dependency> {
    let expression = expression.trim();
    if keyword == "include" && expression.starts_with('[') {
        let list_end = expression.find(']')?;
        let mut names = Vec::new();
        for item in expression[1..list_end].split(',') {
            let (name, remainder) = quoted_string(item.trim())?;
            if !remainder.trim().is_empty() {
                return None;
            }
            names.push(name);
        }
        let suffix = &expression[list_end + 1..];
        return (!names.is_empty() && is_static_dependency_suffix(keyword, suffix)).then_some(
            Dependency::Alternatives {
                names,
                optional: include_ignores_missing(suffix),
            },
        );
    }

    let (name, remainder) = quoted_string(expression)?;
    if !is_static_dependency_suffix(keyword, remainder) {
        return None;
    }
    if keyword == "include" && include_ignores_missing(remainder) {
        Some(Dependency::Optional(name))
    } else {
        Some(Dependency::Required(name))
    }
}

fn include_ignores_missing(suffix: &str) -> bool {
    suffix
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(2)
        .any(|pair| pair == ["ignore", "missing"])
}

fn is_static_dependency_suffix(keyword: &str, suffix: &str) -> bool {
    let suffix = suffix.trim();
    match keyword {
        "extends" | "from" => suffix.is_empty(),
        "import" => suffix.starts_with("as "),
        "include" => suffix
            .split_whitespace()
            .all(|word| matches!(word, "ignore" | "missing" | "with" | "without" | "context")),
        _ => false,
    }
}

fn quoted_string(input: &str) -> Option<(String, &str)> {
    let mut chars = input.char_indices();
    let (_, quote) = chars.next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }

    for (index, ch) in chars {
        if ch == '\\' {
            return None;
        }
        if ch == quote {
            return Some((
                input[quote.len_utf8()..index].to_string(),
                &input[index + ch.len_utf8()..],
            ));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::MiniJinjaEngine;

    fn flattened_names(dependencies: &TemplateDependencies) -> Vec<&str> {
        dependencies
            .dependencies
            .iter()
            .flat_map(|dependency| match dependency {
                Dependency::Required(name) | Dependency::Optional(name) => {
                    vec![name.as_str()]
                }
                Dependency::Alternatives { names, .. } => {
                    names.iter().map(String::as_str).collect()
                }
            })
            .collect()
    }

    #[test]
    fn template_dependencies_support_whitespace_controls() {
        let dependencies = template_dependencies(
            "{%+ extends 'base' +%}{%- include 'partial' -%}{%+ import 'macros' as macros -%}{%- from 'forms' import field +%}",
        );

        assert_eq!(
            flattened_names(&dependencies),
            ["base", "partial", "macros", "forms"]
        );
        assert!(!dependencies.dynamic);
    }

    #[test]
    fn escaped_template_dependency_uses_full_registry_refresh() {
        let dependencies = template_dependencies(r#"{% include "a\\b" %}"#);

        assert!(dependencies.dependencies.is_empty());
        assert!(dependencies.dynamic);
    }

    #[test]
    fn template_dependencies_ignore_delimiters_inside_quoted_strings() {
        let dependencies = template_dependencies(concat!(
            r#"{{ "}} {% include 'variable-double' %}" }}"#,
            r#"{{ '}} {% import "variable-single" as dep %}' }}"#,
            r#"{% set marker = "%} {% from 'statement-double' import dep %}" %}"#,
            r#"{% set marker = '%} {% include "statement-single" %}' %}"#,
            r#"{{ "escaped quote: \" }} {% include 'escaped' %}" }}"#,
            r#"{% include 'actual' %}"#,
        ));

        assert_eq!(flattened_names(&dependencies), ["actual"]);
        assert!(!dependencies.dynamic);
    }

    #[test]
    fn ignore_missing_include_is_optional() {
        let dependencies = template_dependencies("{% include 'optional' ignore missing %}");
        assert_eq!(
            dependencies.dependencies,
            [Dependency::Optional("optional".into())]
        );
        assert!(!dependencies.dynamic);
    }

    #[test]
    fn include_list_is_ordered_alternatives() {
        let dependencies = template_dependencies("{% include ['override', 'default'] %}");
        assert_eq!(
            dependencies.dependencies,
            [Dependency::Alternatives {
                names: vec!["override".into(), "default".into()],
                optional: false,
            }]
        );
        assert!(!dependencies.dynamic);
    }

    #[test]
    fn include_list_ignore_missing_is_optional() {
        let dependencies =
            template_dependencies("{% include ['override', 'default'] ignore missing %}");
        assert_eq!(
            dependencies.dependencies,
            [Dependency::Alternatives {
                names: vec!["override".into(), "default".into()],
                optional: true,
            }]
        );
        assert!(!dependencies.dynamic);
    }

    #[test]
    fn raw_block_with_unclosed_tag_still_finds_later_includes() {
        // An unclosed quoted `{%` inside raw swallows `%}` until the quote
        // ends; `read_tag` returns None and must not abort the endraw scan.
        let dependencies = template_dependencies(concat!(
            r#"{% raw %}{% "unclosed {% endraw %}"#,
            "{% include 'actual' %}",
        ));
        assert_eq!(flattened_names(&dependencies), ["actual"]);
        assert!(!dependencies.dynamic);
    }

    #[test]
    fn load_named_template_adds_static_includes_to_the_engine() {
        let mut registry = TemplateRegistry::new();
        registry.add_inline("list", "{% include 'partial' %}!");
        registry.add_inline("partial", "hi");
        let mut engine: Box<dyn TemplateEngine> = Box::new(MiniJinjaEngine::new());

        load_named_template(&mut *engine, &registry, "list").unwrap();

        assert!(engine.has_template("list"));
        assert!(engine.has_template("partial"));
        assert_eq!(
            engine.render_named("list", &serde_json::json!({})).unwrap(),
            "hi!"
        );
    }

    #[test]
    fn load_named_template_loads_all_templates_for_a_dynamic_include() {
        let mut registry = TemplateRegistry::new();
        registry.add_inline("list", "{% include extra %}");
        registry.add_inline("hello", "Ada");
        let mut engine: Box<dyn TemplateEngine> = Box::new(MiniJinjaEngine::new());

        load_named_template(&mut *engine, &registry, "list").unwrap();

        assert!(engine.has_template("hello"));
        assert_eq!(
            engine
                .render_named("list", &serde_json::json!({"extra": "hello"}))
                .unwrap(),
            "Ada"
        );
    }

    #[test]
    fn load_named_template_skips_absent_ignore_missing_include() {
        let mut registry = TemplateRegistry::new();
        registry.add_inline("list", "{% include 'optional' ignore missing %}ok");
        let mut engine: Box<dyn TemplateEngine> = Box::new(MiniJinjaEngine::new());

        load_named_template(&mut *engine, &registry, "list").unwrap();
        assert_eq!(
            engine.render_named("list", &serde_json::json!({})).unwrap(),
            "ok"
        );
    }

    #[test]
    fn load_named_template_loads_present_fallback_from_include_list() {
        let mut registry = TemplateRegistry::new();
        registry.add_inline("list", "{% include ['override', 'default'] %}");
        registry.add_inline("default", "fallback");
        let mut engine: Box<dyn TemplateEngine> = Box::new(MiniJinjaEngine::new());

        load_named_template(&mut *engine, &registry, "list").unwrap();
        assert!(!engine.has_template("override"));
        assert!(engine.has_template("default"));
        assert_eq!(
            engine.render_named("list", &serde_json::json!({})).unwrap(),
            "fallback"
        );
    }

    #[test]
    fn load_named_template_stops_at_the_first_existing_include_list_candidate() {
        let mut registry = TemplateRegistry::new();
        registry.add_inline("list", "{% include ['override', 'default'] %}");
        registry.add_inline("override", "good");
        registry.add_inline("default", "{% if");
        let mut engine: Box<dyn TemplateEngine> = Box::new(MiniJinjaEngine::new());

        load_named_template(&mut *engine, &registry, "list").unwrap();
        assert!(engine.has_template("override"));
        assert!(!engine.has_template("default"));
        assert_eq!(
            engine.render_named("list", &serde_json::json!({})).unwrap(),
            "good"
        );
    }

    #[test]
    fn required_include_list_errors_when_no_candidate_exists() {
        let mut registry = TemplateRegistry::new();
        registry.add_inline("list", "{% include ['missing-a', 'missing-b'] %}");
        let mut engine: Box<dyn TemplateEngine> = Box::new(MiniJinjaEngine::new());

        let error = load_named_template(&mut *engine, &registry, "list").unwrap_err();
        assert!(
            matches!(error, RenderError::OperationError(ref message) if message.contains("missing-a")),
            "{error:?}"
        );
    }

    #[test]
    fn optional_include_list_succeeds_when_no_candidate_exists() {
        let mut registry = TemplateRegistry::new();
        registry.add_inline(
            "list",
            "{% include ['missing-a', 'missing-b'] ignore missing %}ok",
        );
        let mut engine: Box<dyn TemplateEngine> = Box::new(MiniJinjaEngine::new());

        load_named_template(&mut *engine, &registry, "list").unwrap();
        assert_eq!(
            engine.render_named("list", &serde_json::json!({})).unwrap(),
            "ok"
        );
    }

    #[test]
    fn file_backed_optional_include_is_discovered_after_the_initial_scan() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("list.jinja"),
            "{% include 'optional' ignore missing %}end",
        )
        .unwrap();
        let mut registry = TemplateRegistry::new();
        registry.add_template_dir(dir.path()).unwrap();
        std::fs::write(dir.path().join("optional.jinja"), "yes").unwrap();
        let mut engine: Box<dyn TemplateEngine> = Box::new(MiniJinjaEngine::new());

        load_named_template(&mut *engine, &registry, "list").unwrap();
        assert_eq!(
            engine.render_named("list", &serde_json::json!({})).unwrap(),
            "yesend"
        );
    }

    #[test]
    fn file_backed_include_list_discovers_an_earlier_candidate_after_the_initial_scan() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("list.jinja"),
            "{% include ['override', 'default'] %}",
        )
        .unwrap();
        std::fs::write(dir.path().join("default.jinja"), "fallback").unwrap();
        let mut registry = TemplateRegistry::new();
        registry.add_template_dir(dir.path()).unwrap();
        std::fs::write(dir.path().join("override.jinja"), "chosen").unwrap();
        let mut engine: Box<dyn TemplateEngine> = Box::new(MiniJinjaEngine::new());

        load_named_template(&mut *engine, &registry, "list").unwrap();
        assert_eq!(
            engine.render_named("list", &serde_json::json!({})).unwrap(),
            "chosen"
        );
        assert!(!engine.has_template("default"));
    }
}
