//! Load a template registry into an engine, once per registry generation.
//!
//! [`RenderRequest::registry`](crate::RenderRequest::registry) is the explicit
//! dependency for named templates and includes. Direct [`crate::render_request`]
//! callers must not have to pre-populate the engine: this module copies every
//! registered template into the engine, and refreshes the registry when a
//! named template is missing so a file that appeared on disk is picked up.
//!
//! The copy is cached on (engine identity, registry identity, generation)
//! when the registry has no file sources. File-backed registries reread on
//! every render (ADR-0019). This is not MiniJinja's `Environment::set_loader`:
//! that callback is `Send + Sync + 'static`, and the engine is deliberately
//! `!Send`/`!Sync`.

use std::cell::Cell;

use super::engine::TemplateEngine;
use super::registry::{RegistryError, ResolvedTemplate, TemplateRegistry, TEMPLATE_EXTENSIONS};
use crate::error::RenderError;

/// Identity of the last registry loaded into an engine on this thread.
///
/// Raw addresses are compared, never dereferenced. A generation change or a
/// different engine/registry pair is a miss. Pointer reuse of a dropped engine
/// is rejected by checking that a registered name is already on the engine.
#[derive(Clone, Copy, PartialEq, Eq)]
struct LoadCacheKey {
    engine: usize,
    registry: usize,
    generation: u64,
}

thread_local! {
    static LAST_LOADED: Cell<Option<LoadCacheKey>> = const { Cell::new(None) };
}

fn engine_addr(engine: &dyn TemplateEngine) -> usize {
    engine as *const dyn TemplateEngine as *const () as usize
}

fn cache_key(engine: &dyn TemplateEngine, registry: &TemplateRegistry) -> LoadCacheKey {
    LoadCacheKey {
        engine: engine_addr(engine),
        registry: std::ptr::from_ref(registry) as usize,
        generation: registry.generation(),
    }
}

fn already_loaded(engine: &dyn TemplateEngine, registry: &TemplateRegistry) -> bool {
    // File-backed entries reread on every render (ADR-0019). The cache is
    // for inline/embedded registries, where a second add is wasted work.
    if registry.has_file_sources() {
        return false;
    }
    let key = cache_key(engine, registry);
    if LAST_LOADED.with(|cell| cell.get()) != Some(key) {
        return false;
    }
    match registry.names().next() {
        Some(name) => engine.has_template(name),
        None => true,
    }
}

fn remember_loaded(engine: &dyn TemplateEngine, registry: &TemplateRegistry) {
    LAST_LOADED.with(|cell| cell.set(Some(cache_key(engine, registry))));
}

/// Loads every template in `registry` into `engine`.
///
/// Cached for inline/embedded registries: a second call with the same engine,
/// registry identity, and [`TemplateRegistry::generation`] does not call
/// [`TemplateEngine::add_template`]. File-backed registries always re-walk
/// and reread so includes pick up disk changes (ADR-0019). A missing named
/// lookup still refreshes once (see [`load_named_template`]).
pub(crate) fn load_registry_templates(
    engine: &mut dyn TemplateEngine,
    registry: &TemplateRegistry,
) -> Result<(), RenderError> {
    if already_loaded(engine, registry) {
        return Ok(());
    }
    let mut original_names: Vec<String> = registry.names().map(str::to_string).collect();
    original_names.sort_by_key(|name| is_extension_alias(name));
    let mut working = registry.clone();
    working.refresh().map_err(registry_error)?;
    load_all(engine, &working)?;
    for name in &original_names {
        if working.get(name).is_err() {
            if let Err(error) = registry.get_content(name) {
                return Err(refresh_error(name, registry, error));
            }
        }
    }
    remember_loaded(engine, registry);
    Ok(())
}

/// Loads `name` and every other registered template into `engine`.
///
/// If `name` is missing, the registry is refreshed once and the load is
/// retried, so a file-backed template that appeared on disk is picked up. A
/// second miss produces the [`refresh_error`] diagnostic.
pub(crate) fn load_named_template(
    engine: &mut dyn TemplateEngine,
    registry: &TemplateRegistry,
    name: &str,
) -> Result<(), RenderError> {
    load_registry_templates(engine, registry)?;
    if engine.has_template(name) {
        // Cached copy is enough, but a registered file that disappeared or
        // became unreadable must still error (ADR-0019), not render stale.
        match registry.get_content(name) {
            Ok(_) | Err(RegistryError::NotFound { .. }) => return Ok(()),
            Err(error) => return Err(refresh_error(name, registry, error)),
        }
    }
    if let Ok(content) = registry.get_content(name) {
        return add_named(engine, registry, name, &content);
    }

    let mut refreshed = registry.clone();
    refreshed.refresh().map_err(registry_error)?;
    load_all(engine, &refreshed)?;
    remember_loaded(engine, registry);
    if engine.has_template(name) {
        return Ok(());
    }
    match refreshed.get_content(name) {
        Ok(content) => add_named(engine, &refreshed, name, &content),
        Err(RegistryError::NotFound { name }) => Err(refresh_error(
            &name,
            &refreshed,
            format!("Template not found: \"{name}\""),
        )),
        Err(error) => Err(refresh_error(name, &refreshed, error)),
    }
}

/// Loads every registered template so inline source can `{% include %}`.
///
/// Same cache as [`load_registry_templates`]: the first render against a
/// registry walks and copies; later renders against the same generation do
/// not.
pub(crate) fn load_inline_dependencies(
    engine: &mut dyn TemplateEngine,
    registry: &TemplateRegistry,
) -> Result<(), RenderError> {
    load_registry_templates(engine, registry)
}

fn add_named(
    engine: &mut dyn TemplateEngine,
    registry: &TemplateRegistry,
    name: &str,
    content: &str,
) -> Result<(), RenderError> {
    engine
        .add_template(name, content)
        .map_err(|error| refresh_error(name, registry, error))
}

fn is_extension_alias(name: &str) -> bool {
    TEMPLATE_EXTENSIONS.iter().any(|ext| name.ends_with(ext))
}

fn load_all(
    engine: &mut dyn TemplateEngine,
    registry: &TemplateRegistry,
) -> Result<(), RenderError> {
    // Extensionless names first so a compile error names the include key
    // (`_partial`) rather than the file alias (`_partial.jinja`).
    let mut names: Vec<String> = registry.names().map(str::to_string).collect();
    names.sort_by_key(|name| is_extension_alias(name));
    for name in &names {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::MiniJinjaEngine;

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
    }

    #[test]
    fn missing_named_template_refreshes_once_then_errors() {
        let mut registry = TemplateRegistry::new();
        registry.add_inline("list", "ok");
        let mut engine: Box<dyn TemplateEngine> = Box::new(MiniJinjaEngine::new());

        let error = load_named_template(&mut *engine, &registry, "missing").unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("could not be refreshed") && message.contains("missing"),
            "{message}"
        );
    }
}
