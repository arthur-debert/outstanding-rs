//! Load a template registry into an engine, once per registry generation.
//!
//! [`RenderRequest::registry`](crate::RenderRequest::registry) is the explicit
//! dependency for named templates and includes. Direct [`crate::render_request`]
//! callers must not have to pre-populate the engine: this module copies every
//! registered template into the engine, and refreshes the registry when a
//! named template is missing so a file that appeared on disk is picked up.
//!
//! The copy is cached on (engine identity, registry id, generation) when
//! the registry has no file sources. File-backed registries reread on every
//! render (ADR-0019). This is not MiniJinja's `Environment::set_loader`:
//! that callback is `Send + Sync + 'static`, and the engine is deliberately
//! `!Send`/`!Sync`.

use std::cell::Cell;

use super::engine::TemplateEngine;
use super::registry::{RegistryError, ResolvedTemplate, TemplateRegistry, TEMPLATE_EXTENSIONS};
use crate::error::RenderError;

/// Identity of the last registry generation loaded into an engine on this thread.
///
/// Engine addresses are compared, never dereferenced. Registry identity is
/// [`TemplateRegistry::id`], so a new registry is never a hit just because
/// it reused a heap address. [`TemplateRegistry::generation`] is a globally
/// unique revision, so sibling clones that diverge cannot share a key. A
/// generation change or a different engine is a miss. One slot is
/// deliberate: a shared engine overwrites same-named templates, so returning
/// to a previous registry must reload. Pointer reuse of a dropped engine is
/// rejected by checking that a registered name is already on the engine.
#[derive(Clone, Copy, PartialEq, Eq)]
struct LoadCacheKey {
    engine: usize,
    registry_id: u64,
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
        registry_id: registry.id(),
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
/// [`TemplateRegistry::id`], and [`TemplateRegistry::generation`] does not
/// call [`TemplateEngine::add_template`]. File-backed registries always
/// re-walk and reread so includes pick up disk changes (ADR-0019). A missing
/// named lookup still refreshes once (see [`load_named_template`]).
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
        if let Some(error) = disappeared_file_error(name, registry, &working) {
            return Err(error);
        }
    }
    remember_loaded(engine, registry);
    Ok(())
}

/// Loads `name` and every other registered template into `engine`.
///
/// [`load_registry_templates`] copies the current registry into the engine
/// (and errors if a registered file disappeared or became unreadable). This
/// function then requires the *current* registry to resolve `name`; engine
/// membership alone is not enough, because a shared engine can still hold a
/// template from a previous registry. A name that resolves only via extension
/// fallback is added under the requested name so `render_named` hits. If
/// `name` is missing, the registry is refreshed once and the load is retried,
/// so a file-backed template that appeared on disk is picked up. A second
/// miss produces the [`refresh_error`] diagnostic.
pub(crate) fn load_named_template(
    engine: &mut dyn TemplateEngine,
    registry: &TemplateRegistry,
    name: &str,
) -> Result<(), RenderError> {
    load_registry_templates(engine, registry)?;
    if let Some(result) = ensure_requested_name(engine, registry, name) {
        return result;
    }

    let mut refreshed = registry.clone();
    refreshed.refresh().map_err(registry_error)?;
    load_all(engine, &refreshed)?;
    remember_loaded(engine, registry);
    if let Some(result) = ensure_requested_name(engine, &refreshed, name) {
        return result;
    }
    Err(refresh_error(
        name,
        &refreshed,
        format!("Template not found: \"{name}\""),
    ))
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

fn is_extension_alias(name: &str) -> bool {
    TEMPLATE_EXTENSIONS.iter().any(|ext| name.ends_with(ext))
}

/// If `registry` resolves `name`, ensure the engine has that exact name.
///
/// Exact registered names were copied by [`load_all`]. Extension fallback
/// (`show.j2` → `show`) copies the resolved content under the requested name
/// so `render_named` hits, and overwrites a leftover from a previous registry.
/// Engine membership alone is never treated as proof that this registry
/// supplied `name`.
fn ensure_requested_name(
    engine: &mut dyn TemplateEngine,
    registry: &TemplateRegistry,
    name: &str,
) -> Option<Result<(), RenderError>> {
    match registry.get(name) {
        Err(RegistryError::NotFound { .. }) => None,
        Err(error) => Some(Err(refresh_error(name, registry, error))),
        Ok(_) => {
            if registry.names().any(|registered| registered == name) {
                return Some(Ok(()));
            }
            Some(match registry.get_content(name) {
                Ok(content) => add_named(engine, registry, name, &content),
                Err(error) => Err(refresh_error(name, registry, error)),
            })
        }
    }
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

/// A file-backed name that refresh dropped must error (ADR-0019), even when a
/// framework or inline entry of the same name would still resolve.
fn disappeared_file_error(
    name: &str,
    original: &TemplateRegistry,
    working: &TemplateRegistry,
) -> Option<RenderError> {
    match original.get(name) {
        Ok(ResolvedTemplate::File(_)) => match working.get(name) {
            Ok(ResolvedTemplate::File(_)) => None,
            _ => Some(match original.get_content(name) {
                Err(error) => refresh_error(name, original, error),
                Ok(_) => refresh_error(name, original, format!("Template not found: \"{name}\"")),
            }),
        },
        _ => None,
    }
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

    #[test]
    fn shared_engine_does_not_accept_a_stale_template_from_another_registry() {
        let mut engine: Box<dyn TemplateEngine> = Box::new(MiniJinjaEngine::new());
        let mut first = TemplateRegistry::new();
        first.add_inline("shared", "from-a");
        first.add_inline("foo", "stale");
        load_named_template(&mut *engine, &first, "foo").unwrap();

        let mut second = TemplateRegistry::new();
        second.add_inline("shared", "from-b");
        let error = load_named_template(&mut *engine, &second, "foo").unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("foo")
                && (message.contains("not found") || message.contains("could not be refreshed")),
            "{message}"
        );

        load_named_template(&mut *engine, &second, "shared").unwrap();
        assert_eq!(
            engine
                .render_named("shared", &serde_json::json!({}))
                .unwrap(),
            "from-b"
        );
    }

    #[test]
    fn extension_fallback_adds_the_requested_name_and_overwrites_a_stale_engine_entry() {
        let mut engine: Box<dyn TemplateEngine> = Box::new(MiniJinjaEngine::new());
        let mut first = TemplateRegistry::new();
        first.add_inline("show.j2", "stale");
        load_named_template(&mut *engine, &first, "show.j2").unwrap();
        assert_eq!(
            engine
                .render_named("show.j2", &serde_json::json!({}))
                .unwrap(),
            "stale"
        );

        let mut second = TemplateRegistry::new();
        second.add_inline("show", "fresh");
        load_named_template(&mut *engine, &second, "show.j2").unwrap();
        assert_eq!(
            engine
                .render_named("show.j2", &serde_json::json!({}))
                .unwrap(),
            "fresh"
        );
    }

    #[test]
    fn replacing_inline_template_reloads_on_a_shared_engine() {
        let mut engine: Box<dyn TemplateEngine> = Box::new(MiniJinjaEngine::new());
        let mut registry = TemplateRegistry::new();
        registry.add_inline("list", "one");
        load_named_template(&mut *engine, &registry, "list").unwrap();
        assert_eq!(
            engine.render_named("list", &serde_json::json!({})).unwrap(),
            "one"
        );

        registry.add_inline("list", "two");
        load_named_template(&mut *engine, &registry, "list").unwrap();
        assert_eq!(
            engine.render_named("list", &serde_json::json!({})).unwrap(),
            "two"
        );
    }

    #[test]
    fn sibling_clones_with_different_content_reload_on_a_shared_engine() {
        let parent = TemplateRegistry::new();
        let mut first = parent.clone();
        let mut second = parent.clone();
        first.add_inline("x", "from-a");
        second.add_inline("x", "from-b");
        let mut engine: Box<dyn TemplateEngine> = Box::new(MiniJinjaEngine::new());

        load_named_template(&mut *engine, &first, "x").unwrap();
        assert_eq!(
            engine.render_named("x", &serde_json::json!({})).unwrap(),
            "from-a"
        );
        load_named_template(&mut *engine, &second, "x").unwrap();
        assert_eq!(
            engine.render_named("x", &serde_json::json!({})).unwrap(),
            "from-b"
        );
    }

    #[test]
    fn adding_then_removing_inline_and_framework_templates_reloads_on_a_shared_engine() {
        let mut engine: Box<dyn TemplateEngine> = Box::new(MiniJinjaEngine::new());
        let mut registry = TemplateRegistry::new();
        registry.add_inline("list", "{% include 'partial' %}");
        registry.add_inline("partial", "old");
        load_named_template(&mut *engine, &registry, "list").unwrap();
        assert_eq!(
            engine.render_named("list", &serde_json::json!({})).unwrap(),
            "old"
        );

        registry.add_inline("partial", "new");
        load_named_template(&mut *engine, &registry, "list").unwrap();
        assert_eq!(
            engine.render_named("list", &serde_json::json!({})).unwrap(),
            "new"
        );

        registry.add_framework("standout/x", "fw-one");
        load_named_template(&mut *engine, &registry, "standout/x").unwrap();
        assert_eq!(
            engine
                .render_named("standout/x", &serde_json::json!({}))
                .unwrap(),
            "fw-one"
        );

        registry.add_framework("standout/x", "fw-two");
        load_named_template(&mut *engine, &registry, "standout/x").unwrap();
        assert_eq!(
            engine
                .render_named("standout/x", &serde_json::json!({}))
                .unwrap(),
            "fw-two"
        );

        registry.clear_framework();
        let error = load_named_template(&mut *engine, &registry, "standout/x").unwrap_err();
        assert!(error.to_string().contains("standout/x"), "{}", error);

        registry.clear();
        let error = load_named_template(&mut *engine, &registry, "list").unwrap_err();
        assert!(error.to_string().contains("list"), "{error}");
    }

    #[test]
    fn disappeared_file_is_not_replaced_by_a_framework_template_of_the_same_name() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("standout");
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(nested.join("help.jinja"), "from-disk").unwrap();
        let mut registry = TemplateRegistry::new();
        registry.add_template_dir(dir.path()).unwrap();
        registry.refresh().unwrap();
        registry.add_framework("standout/help", "from-framework");
        let mut engine: Box<dyn TemplateEngine> = Box::new(MiniJinjaEngine::new());

        load_named_template(&mut *engine, &registry, "standout/help").unwrap();
        assert_eq!(
            engine
                .render_named("standout/help", &serde_json::json!({}))
                .unwrap(),
            "from-disk"
        );

        std::fs::remove_file(nested.join("help.jinja")).unwrap();
        let error = load_named_template(&mut *engine, &registry, "standout/help").unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("standout/help") && message.contains("could not be refreshed"),
            "{message}"
        );
    }
}
