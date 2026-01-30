//! Local (single-threaded) app builder for mutable handlers.
//!
//! This module provides [`LocalAppBuilder`] for building CLI applications
//! that use `FnMut` handlers with `&mut self` access to state.
//!
//! # When to Use
//!
//! Use `LocalAppBuilder` when your handlers need mutable access to state
//! without interior mutability wrappers (`Arc<Mutex<_>>`).
//!
//! # Example
//!
//! ```rust,ignore
//! use standout::cli::{LocalApp, Output, HandlerResult};
//!
//! struct Database {
//!     records: Vec<Record>,
//! }
//!
//! impl Database {
//!     fn add(&mut self, r: Record) { self.records.push(r); }
//!     fn list(&self) -> &[Record] { &self.records }
//! }
//!
//! let mut db = Database { records: vec![] };
//!
//! LocalApp::builder()
//!     .command("add", |m, ctx| {
//!         let name = m.get_one::<String>("name").unwrap();
//!         db.add(Record { name: name.clone() });
//!         Ok(Output::Silent)
//!     }, "")
//!     .command("list", |m, ctx| {
//!         Ok(Output::Render(db.list().to_vec()))
//!     }, "{{ records }}")
//!     .build()
//!     .run(cmd, args);
//! ```

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use clap::ArgMatches;
use serde::Serialize;

use crate::context::ContextRegistry;
use standout_dispatch::Extensions;

use crate::TemplateRegistry;
use crate::{OutputMode, Theme};
use standout_render::template::TemplateEngine;

use super::dispatch::{render_handler_output, LocalDispatchFn};
use super::handler::{CommandContext, HandlerResult, LocalFnHandler, LocalHandler};
use super::hooks::Hooks;
use crate::setup::SetupError;

use super::app::App;
use super::mode::Local;
use crate::topics::TopicRegistry;

/// Recipe for creating local dispatch closures.
trait LocalCommandRecipe {
    fn create_dispatch(
        self: Box<Self>,
        template: &str,
        context_registry: &ContextRegistry,
        theme: &Theme,
        template_engine: Arc<Box<dyn TemplateEngine>>,
    ) -> LocalDispatchFn;
}

/// Recipe for closure-based local handlers.
struct LocalClosureRecipe<F, T>
where
    F: FnMut(&ArgMatches, &CommandContext) -> HandlerResult<T> + 'static,
    T: Serialize + 'static,
{
    handler: F,
    _phantom: std::marker::PhantomData<T>,
}

impl<F, T> LocalClosureRecipe<F, T>
where
    F: FnMut(&ArgMatches, &CommandContext) -> HandlerResult<T> + 'static,
    T: Serialize + 'static,
{
    fn new(handler: F) -> Self {
        Self {
            handler,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<F, T> LocalCommandRecipe for LocalClosureRecipe<F, T>
where
    F: FnMut(&ArgMatches, &CommandContext) -> HandlerResult<T> + 'static,
    T: Serialize + 'static,
{
    fn create_dispatch(
        self: Box<Self>,
        template: &str,
        context_registry: &ContextRegistry,
        theme: &Theme,
        template_engine: Arc<Box<dyn TemplateEngine>>,
    ) -> LocalDispatchFn {
        let mut handler = LocalFnHandler::new(self.handler);
        let template = template.to_string();
        let context_registry = context_registry.clone();
        let theme = theme.clone();

        Rc::new(RefCell::new(
            move |matches: &ArgMatches,
                  ctx: &CommandContext,
                  hooks: Option<&Hooks>,
                  output_mode: crate::OutputMode| {
                let result = handler.handle(matches, ctx).map_err(|e| e.to_string());
                render_handler_output(
                    result,
                    matches,
                    ctx,
                    hooks,
                    &template,
                    &theme,
                    &context_registry,
                    &**template_engine,
                    output_mode,
                )
            },
        ))
    }
}

/// Recipe for struct-based local handlers.
struct LocalStructRecipe<H, T>
where
    H: LocalHandler<Output = T> + 'static,
    T: Serialize + 'static,
{
    handler: H,
    _phantom: std::marker::PhantomData<T>,
}

impl<H, T> LocalStructRecipe<H, T>
where
    H: LocalHandler<Output = T> + 'static,
    T: Serialize + 'static,
{
    fn new(handler: H) -> Self {
        Self {
            handler,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<H, T> LocalCommandRecipe for LocalStructRecipe<H, T>
where
    H: LocalHandler<Output = T> + 'static,
    T: Serialize + 'static,
{
    fn create_dispatch(
        mut self: Box<Self>,
        template: &str,
        context_registry: &ContextRegistry,
        theme: &Theme,
        template_engine: Arc<Box<dyn TemplateEngine>>,
    ) -> LocalDispatchFn {
        let template = template.to_string();
        let context_registry = context_registry.clone();
        let theme = theme.clone();

        Rc::new(RefCell::new(
            move |matches: &ArgMatches,
                  ctx: &CommandContext,
                  hooks: Option<&Hooks>,
                  output_mode: crate::OutputMode| {
                let result = self.handler.handle(matches, ctx).map_err(|e| e.to_string());
                render_handler_output(
                    result,
                    matches,
                    ctx,
                    hooks,
                    &template,
                    &theme,
                    &context_registry,
                    &**template_engine,
                    output_mode,
                )
            },
        ))
    }
}

/// Pending command for deferred dispatch creation.
struct PendingLocalCommand {
    recipe: Box<dyn LocalCommandRecipe>,
    template: String,
}

/// Builder for local (single-threaded) CLI applications.
///
/// Unlike [`AppBuilder`](super::AppBuilder), this builder accepts `FnMut` handlers
/// that can capture mutable state without requiring `Send + Sync`.
///
/// # Example
///
/// ```rust,ignore
/// use standout::cli::{LocalApp, Output};
///
/// let mut counter = 0u32;
///
/// LocalApp::builder()
///     .command("increment", |m, ctx| {
///         counter += 1;  // FnMut allows mutation!
///         Ok(Output::Render(counter))
///     }, "{{ count }}")
///     .build()?
///     .run(cmd, args);
/// ```
///
/// # Differences from AppBuilder
///
/// | Aspect | `AppBuilder` | `LocalAppBuilder` |
/// |--------|--------------|-------------------|
/// | Handler type | `Fn` | `FnMut` |
/// | Thread bounds | `Send + Sync` | None |
/// | State mutation | Interior mutability | Direct |
/// | Storage | `Arc<dyn Fn>` | `Rc<RefCell<dyn FnMut>>` |
pub struct LocalAppBuilder {
    // pub(crate) registry: TopicRegistry, // Unused
    pub(crate) output_flag: Option<String>,
    pub(crate) output_file_flag: Option<String>,
    pub(crate) theme: Option<Theme>,
    pub(crate) stylesheet_registry: Option<crate::StylesheetRegistry>,
    pub(crate) template_registry: Option<TemplateRegistry>,
    pub(crate) default_theme_name: Option<String>,
    pending_commands: RefCell<HashMap<String, PendingLocalCommand>>,
    finalized_commands: RefCell<Option<HashMap<String, LocalDispatchFn>>>,
    pub(crate) command_hooks: HashMap<String, Hooks>,
    pub(crate) context_registry: ContextRegistry,
    pub(crate) template_dir: Option<std::path::PathBuf>,
    pub(crate) template_ext: String,
    pub(crate) default_command: Option<String>,
    /// App-level state shared across all dispatches.
    pub(crate) app_state: Arc<Extensions>,
    pub(crate) template_engine: Arc<Box<dyn TemplateEngine>>,
}

impl Default for LocalAppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalAppBuilder {
    /// Creates a new builder with default settings.
    pub fn new() -> Self {
        Self {
            // registry: TopicRegistry::new(),
            output_flag: Some("output".to_string()),
            output_file_flag: Some("output-file-path".to_string()),
            theme: None,
            stylesheet_registry: None,
            template_registry: None,
            default_theme_name: None,
            pending_commands: RefCell::new(HashMap::new()),
            finalized_commands: RefCell::new(None),
            command_hooks: HashMap::new(),
            context_registry: ContextRegistry::new(),
            template_dir: None,
            template_ext: ".j2".to_string(),
            default_command: None,
            app_state: Arc::new(Extensions::new()),
            template_engine: Arc::new(Box::new(standout_render::template::MiniJinjaEngine::new())),
        }
    }

    /// Adds app-level state that will be available to all handlers.
    ///
    /// App state is immutable and shared across all dispatches via `Arc<Extensions>`.
    /// Use for long-lived resources like database connections, configuration, and
    /// API clients.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use standout::cli::LocalApp;
    ///
    /// struct Config { debug: bool }
    ///
    /// let app = LocalApp::builder()
    ///     .app_state(Config { debug: true })
    ///     .command("list", |matches, ctx| {
    ///         let config = ctx.app_state.get_required::<Config>()?;
    ///         Ok(Output::Render(vec!["item"]))
    ///     }, "{{ items }}")
    ///     .build()?;
    /// ```
    pub fn app_state<T: Send + Sync + 'static>(mut self, value: T) -> Self {
        Arc::get_mut(&mut self.app_state)
            .expect("app_state Arc should be exclusively owned during builder phase")
            .insert(value);
        self
    }

    // ============================================================================
    // Command Registration
    // ============================================================================

    /// sets a custom template engine to be used for rendering.
    ///
    /// If not set, the default MiniJinja engine will be used.
    pub fn template_engine(
        mut self,
        engine: Box<dyn standout_render::template::TemplateEngine>,
    ) -> Self {
        self.template_engine = Arc::new(engine);
        self
    }

    /// Registers a command handler (FnMut closure) with a template.
    ///
    /// Unlike [`AppBuilder::command`](super::AppBuilder::command), this accepts
    /// `FnMut` closures that can capture mutable state.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut data = Vec::new();
    ///
    /// LocalApp::builder()
    ///     .command("add", |m, ctx| {
    ///         data.push(m.get_one::<String>("item").unwrap().clone());
    ///         Ok(Output::Render(data.len()))
    ///     }, "Added. Total: {{ count }}")
    /// ```
    pub fn command<F, T>(self, path: &str, handler: F, template: &str) -> Result<Self, SetupError>
    where
        F: FnMut(&ArgMatches, &CommandContext) -> HandlerResult<T> + 'static,
        T: Serialize + 'static,
    {
        let template_str = if template.is_empty() {
            self.resolve_template(path)
        } else {
            template.to_string()
        };

        let recipe = LocalClosureRecipe::new(handler);

        if self.pending_commands.borrow().contains_key(path) {
            return Err(SetupError::DuplicateCommand(path.to_string()));
        }

        self.pending_commands.borrow_mut().insert(
            path.to_string(),
            PendingLocalCommand {
                recipe: Box::new(recipe),
                template: template_str,
            },
        );

        Ok(self)
    }

    /// Registers a struct handler implementing [`LocalHandler`].
    ///
    /// Use this when your handler needs `&mut self` access.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// struct Counter { count: u32 }
    ///
    /// impl LocalHandler for Counter {
    ///     type Output = u32;
    ///     fn handle(&mut self, _m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<u32> {
    ///         self.count += 1;
    ///         Ok(Output::Render(self.count))
    ///     }
    /// }
    ///
    /// LocalApp::builder()
    ///     .command_handler("count", Counter { count: 0 }, "{{ count }}")
    /// ```
    pub fn command_handler<H, T>(
        self,
        path: &str,
        handler: H,
        template: &str,
    ) -> Result<Self, SetupError>
    where
        H: LocalHandler<Output = T> + 'static,
        T: Serialize + 'static,
    {
        let template_str = if template.is_empty() {
            self.resolve_template(path)
        } else {
            template.to_string()
        };

        let recipe = LocalStructRecipe::new(handler);

        if self.pending_commands.borrow().contains_key(path) {
            return Err(SetupError::DuplicateCommand(path.to_string()));
        }

        self.pending_commands.borrow_mut().insert(
            path.to_string(),
            PendingLocalCommand {
                recipe: Box::new(recipe),
                template: template_str,
            },
        );

        Ok(self)
    }

    /// Registers hooks for a specific command path.
    pub fn hooks(mut self, path: &str, hooks: Hooks) -> Self {
        self.command_hooks.insert(path.to_string(), hooks);
        self
    }

    // ============================================================================
    // Configuration (mirrors AppBuilder)
    // ============================================================================

    /// Sets a custom theme for rendering.
    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = Some(theme);
        self
    }

    /// Sets embedded templates.
    pub fn templates(mut self, templates: crate::EmbeddedTemplates) -> Self {
        self.template_registry = Some(TemplateRegistry::from(templates));
        self
    }

    /// Sets embedded styles.
    pub fn styles(mut self, styles: crate::EmbeddedStyles) -> Self {
        self.stylesheet_registry = Some(crate::StylesheetRegistry::from(styles));
        self
    }

    /// Sets the default theme name.
    pub fn default_theme(mut self, name: &str) -> Self {
        self.default_theme_name = Some(name.to_string());
        self
    }

    /// Sets the output flag name.
    pub fn output_flag(mut self, name: Option<&str>) -> Self {
        self.output_flag = Some(name.unwrap_or("output").to_string());
        self
    }

    /// Disables the output flag.
    pub fn no_output_flag(mut self) -> Self {
        self.output_flag = None;
        self
    }

    /// Sets a default command.
    pub fn default_command(mut self, name: &str) -> Self {
        self.default_command = Some(name.to_string());
        self
    }

    // ============================================================================
    // Build and Dispatch
    // ============================================================================

    fn resolve_template(&self, command_path: &str) -> String {
        let file_path = command_path.replace('.', "/");
        let template_name = format!("{}{}", file_path, self.template_ext);

        if let Some(ref registry) = self.template_registry {
            if let Ok(content) = registry.get_content(&template_name) {
                return content;
            }
        }

        if let Some(ref dir) = self.template_dir {
            return format!("{}/{}", dir.display(), template_name);
        }

        String::new()
    }

    fn ensure_commands_finalized(
        &self,
        theme: &Theme,
        template_engine: Arc<Box<dyn TemplateEngine>>,
    ) {
        if self.finalized_commands.borrow().is_some() {
            return;
        }

        let context_registry = &self.context_registry;

        let mut commands = HashMap::new();
        let mut pending = self.pending_commands.borrow_mut();

        // Drain the pending commands (take ownership)
        for (path, pending_cmd) in pending.drain() {
            let dispatch = pending_cmd.recipe.create_dispatch(
                &pending_cmd.template,
                context_registry,
                theme,
                template_engine.clone(),
            );
            commands.insert(path, dispatch);
        }

        *self.finalized_commands.borrow_mut() = Some(commands);
    }

    /// Builds the LocalApp instance.
    pub fn build(mut self) -> Result<App<Local>, SetupError> {
        use super::core::AppCore;

        // Resolve theme
        let theme = if let Some(theme) = self.theme.take() {
            Some(theme)
        } else if let Some(ref mut registry) = self.stylesheet_registry {
            if let Some(name) = &self.default_theme_name {
                let theme = registry
                    .get(name)
                    .map_err(|_| SetupError::ThemeNotFound(name.to_string()))?;
                Some(theme)
            } else {
                registry
                    .get("default")
                    .or_else(|_| registry.get("theme"))
                    .or_else(|_| registry.get("base"))
                    .ok()
            }
        } else {
            None
        };

        // Populate engine with templates from registry
        if let Some(registry) = &self.template_registry {
            if let Some(engine_box) = Arc::get_mut(&mut self.template_engine) {
                for name in registry.names() {
                    if let Ok(content) = registry.get_content(name) {
                        let _ = engine_box.add_template(name, &content);
                    }
                }
            }
        }

        // Finalize commands before building
        // Use the resolved theme (failed previously because self.theme was taken)
        let effective_theme = theme.clone().unwrap_or_default();

        self.ensure_commands_finalized(&effective_theme, self.template_engine.clone());

        // Wrap template registry in Arc (for AppCore)
        let template_registry = self.template_registry.take().map(Arc::new);

        // Build the AppCore with all shared configuration
        let core = AppCore {
            output_flag: self.output_flag,
            output_file_flag: self.output_file_flag,
            output_mode: OutputMode::Auto,
            theme,
            command_hooks: self.command_hooks,
            default_command: self.default_command,
            template_registry,
            stylesheet_registry: self.stylesheet_registry,
            context_registry: self.context_registry,
            app_state: self.app_state,
            template_engine: self.template_engine,
        };

        Ok(App {
            core,
            registry: TopicRegistry::new(),
            commands: self.finalized_commands.take().unwrap_or_default(),
        })
    }

    /// Test helper: Check if a command path is registered.
    #[cfg(test)]
    pub(crate) fn has_command(&self, path: &str) -> bool {
        self.pending_commands.borrow().contains_key(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::handler::Output;
    use serde_json::json;

    #[test]
    fn test_local_builder_command() {
        let mut counter = 0u32;

        let builder = LocalAppBuilder::new()
            .command(
                "increment",
                move |_m, _ctx| {
                    counter += 1;
                    Ok(Output::Render(json!({"count": counter})))
                },
                "{{ count }}",
            )
            .unwrap();

        assert!(builder.has_command("increment"));
    }

    #[test]
    fn test_local_builder_struct_handler() {
        struct Counter {
            count: u32,
        }

        impl LocalHandler for Counter {
            type Output = u32;

            fn handle(&mut self, _m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<u32> {
                self.count += 1;
                Ok(Output::Render(self.count))
            }
        }

        let builder = LocalAppBuilder::new()
            .command_handler("count", Counter { count: 0 }, "{{ . }}")
            .unwrap();

        assert!(builder.has_command("count"));
    }

    #[test]
    fn test_local_builder_multiple_commands() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let state = Rc::new(RefCell::new(Vec::new()));
        let state_add = state.clone();
        let state_list = state.clone();

        let builder = LocalAppBuilder::new()
            .command(
                "add",
                move |_m, _ctx| {
                    state_add.borrow_mut().push("item");
                    Ok(Output::Render(json!({"count": state_add.borrow().len()})))
                },
                "",
            )
            .unwrap()
            .command(
                "list",
                move |_m, _ctx| {
                    Ok(Output::Render(
                        json!({"items": state_list.borrow().clone()}),
                    ))
                },
                "",
            )
            .unwrap();

        assert!(builder.has_command("add"));
        assert!(builder.has_command("list"));
    }
}
