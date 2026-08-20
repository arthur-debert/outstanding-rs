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
    load_with_missing_refresh(engine, registry, |engine, registry| {
        let mut seen = HashSet::new();
        load_tree(engine, registry, name, &mut seen)
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
    load_with_missing_refresh(engine, registry, |engine, registry| {
        let mut seen = HashSet::new();
        load_source_tree(engine, registry, source, &mut seen)
    })
}

fn load_with_missing_refresh(
    engine: &mut dyn TemplateEngine,
    registry: &TemplateRegistry,
    attempt: impl Fn(&mut dyn TemplateEngine, &TemplateRegistry) -> Result<(), RenderError>,
) -> Result<(), RenderError> {
    match attempt(engine, registry) {
        Err(error) if is_not_found(&error) => {
            let mut refreshed = registry.clone();
            refreshed.refresh().map_err(registry_error)?;
            match attempt(engine, &refreshed) {
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
) -> Result<(), RenderError> {
    if !seen.insert(name.to_string()) {
        return Ok(());
    }

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
    load_source_tree(engine, registry, &content, seen)
}

fn load_source_tree(
    engine: &mut dyn TemplateEngine,
    registry: &TemplateRegistry,
    source: &str,
    seen: &mut HashSet<String>,
) -> Result<(), RenderError> {
    let dependencies = template_dependencies(source);
    if dependencies.dynamic {
        let mut refreshed = registry.clone();
        refreshed.refresh().map_err(registry_error)?;
        return load_all(engine, &refreshed);
    }

    for dependency in dependencies.names {
        load_tree(engine, registry, &dependency, seen)?;
    }

    Ok(())
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

#[derive(Default)]
struct TemplateDependencies {
    names: Vec<String>,
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
            match expression.and_then(|expression| static_template_names(keyword, expression)) {
                Some(names) => dependencies.names.extend(names),
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
        let (tag, close) = read_tag(source, open)?;
        if split_tag(&tag).0 == "endraw" {
            return Some(close);
        }
        cursor = close;
    }
    None
}

fn static_template_names(keyword: &str, expression: &str) -> Option<Vec<String>> {
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
        return (!names.is_empty()
            && is_static_dependency_suffix(keyword, &expression[list_end + 1..]))
        .then_some(names);
    }

    let (name, remainder) = quoted_string(expression)?;
    is_static_dependency_suffix(keyword, remainder).then_some(vec![name])
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

    #[test]
    fn template_dependencies_support_whitespace_controls() {
        let dependencies = template_dependencies(
            "{%+ extends 'base' +%}{%- include 'partial' -%}{%+ import 'macros' as macros -%}{%- from 'forms' import field +%}",
        );

        assert_eq!(dependencies.names, ["base", "partial", "macros", "forms"]);
        assert!(!dependencies.dynamic);
    }

    #[test]
    fn escaped_template_dependency_uses_full_registry_refresh() {
        let dependencies = template_dependencies(r#"{% include "a\\b" %}"#);

        assert!(dependencies.names.is_empty());
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

        assert_eq!(dependencies.names, ["actual"]);
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
}
