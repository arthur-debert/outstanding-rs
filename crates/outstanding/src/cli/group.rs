//! Nested command group builder for declarative dispatch.
//!
//! This module provides [`GroupBuilder`] for creating nested command hierarchies
//! with a fluent API, and [`CommandConfig`] for inline command configuration.

use crate::context::{ContextRegistry, RenderContext};
use crate::{render_auto_with_context, Theme};
use clap::ArgMatches;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

use super::app::get_terminal_width;
use super::dispatch::{DispatchFn, DispatchOutput};
use crate::cli::handler::{
    CommandContext, FnHandler, Handler, HandlerResult, Output as HandlerOutput,
};
use crate::cli::hooks::Hooks;

// ============================================================================
// CommandRecipe - Deferred dispatch closure creation
// ============================================================================

/// A recipe for creating a dispatch closure.
///
/// Unlike `ErasedCommandConfig::register` which consumes self, this trait
/// allows creating dispatch closures on demand without consuming the recipe.
/// This enables deferred closure creation where the theme and context_registry
/// are captured at dispatch time rather than at registration time.
pub(crate) trait CommandRecipe: Send + Sync {
    /// Returns the template for this command, if explicitly set.
    #[allow(dead_code)]
    fn template(&self) -> Option<&str>;

    /// Returns hooks for this command, if set.
    #[allow(dead_code)]
    fn hooks(&self) -> Option<&Hooks>;

    /// Takes ownership of hooks (for registration with AppBuilder).
    #[allow(dead_code)]
    fn take_hooks(&mut self) -> Option<Hooks>;

    /// Creates a dispatch closure with the given configuration.
    ///
    /// This can be called multiple times (unlike ErasedCommandConfig::register).
    fn create_dispatch(
        &self,
        template: &str,
        context_registry: &ContextRegistry,
        theme: &Theme,
    ) -> DispatchFn;
}

/// Recipe for closure-based command handlers.
pub(crate) struct ClosureRecipe<F, T>
where
    F: Fn(&ArgMatches, &CommandContext) -> HandlerResult<T> + Send + Sync + 'static,
    T: Serialize + Send + Sync + 'static,
{
    handler: Arc<FnHandler<F, T>>,
    template: Option<String>,
    hooks: Option<Hooks>,
}

impl<F, T> ClosureRecipe<F, T>
where
    F: Fn(&ArgMatches, &CommandContext) -> HandlerResult<T> + Send + Sync + 'static,
    T: Serialize + Send + Sync + 'static,
{
    pub fn new(handler: FnHandler<F, T>) -> Self {
        Self {
            handler: Arc::new(handler),
            template: None,
            hooks: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_template(mut self, template: String) -> Self {
        self.template = Some(template);
        self
    }

    #[allow(dead_code)]
    pub fn with_hooks(mut self, hooks: Hooks) -> Self {
        self.hooks = Some(hooks);
        self
    }
}

impl<F, T> CommandRecipe for ClosureRecipe<F, T>
where
    F: Fn(&ArgMatches, &CommandContext) -> HandlerResult<T> + Send + Sync + 'static,
    T: Serialize + Send + Sync + 'static,
{
    fn template(&self) -> Option<&str> {
        self.template.as_deref()
    }

    fn hooks(&self) -> Option<&Hooks> {
        self.hooks.as_ref()
    }

    fn take_hooks(&mut self) -> Option<Hooks> {
        self.hooks.take()
    }

    fn create_dispatch(
        &self,
        template: &str,
        context_registry: &ContextRegistry,
        theme: &Theme,
    ) -> DispatchFn {
        let handler = self.handler.clone();
        let template = template.to_string();
        let context_registry = context_registry.clone();
        let theme = theme.clone();

        Arc::new(
            move |matches: &ArgMatches, ctx: &CommandContext, hooks: Option<&Hooks>| {
                let result = handler.handle(matches, ctx);

                match result {
                    Ok(HandlerOutput::Render(data)) => {
                        let mut json_data = serde_json::to_value(&data)
                            .map_err(|e| format!("Failed to serialize handler result: {}", e))?;

                        if let Some(hooks) = hooks {
                            json_data = hooks
                                .run_post_dispatch(matches, ctx, json_data)
                                .map_err(|e| format!("Hook error: {}", e))?;
                        }

                        let render_ctx = RenderContext::new(
                            ctx.output_mode,
                            get_terminal_width(),
                            &theme,
                            &json_data,
                        );

                        let output = render_auto_with_context(
                            &template,
                            &json_data,
                            &theme,
                            ctx.output_mode,
                            &context_registry,
                            &render_ctx,
                        )
                        .map_err(|e| e.to_string())?;
                        Ok(DispatchOutput::Text(output))
                    }
                    Err(e) => Err(format!("Error: {}", e)),
                    Ok(HandlerOutput::Silent) => Ok(DispatchOutput::Silent),
                    Ok(HandlerOutput::Binary { data, filename }) => {
                        Ok(DispatchOutput::Binary(data, filename))
                    }
                }
            },
        )
    }
}

/// Recipe for struct-based command handlers.
pub(crate) struct StructRecipe<H, T>
where
    H: Handler<Output = T> + Send + Sync + 'static,
    T: Serialize + Send + Sync + 'static,
{
    handler: Arc<H>,
    #[allow(dead_code)]
    template: Option<String>,
    hooks: Option<Hooks>,
    _phantom: std::marker::PhantomData<T>,
}

impl<H, T> StructRecipe<H, T>
where
    H: Handler<Output = T> + Send + Sync + 'static,
    T: Serialize + Send + Sync + 'static,
{
    pub fn new(handler: H) -> Self {
        Self {
            handler: Arc::new(handler),
            template: None,
            hooks: None,
            _phantom: std::marker::PhantomData,
        }
    }

    #[allow(dead_code)]
    pub fn with_template(mut self, template: String) -> Self {
        self.template = Some(template);
        self
    }

    #[allow(dead_code)]
    pub fn with_hooks(mut self, hooks: Hooks) -> Self {
        self.hooks = Some(hooks);
        self
    }
}

impl<H, T> CommandRecipe for StructRecipe<H, T>
where
    H: Handler<Output = T> + Send + Sync + 'static,
    T: Serialize + Send + Sync + 'static,
{
    fn template(&self) -> Option<&str> {
        self.template.as_deref()
    }

    fn hooks(&self) -> Option<&Hooks> {
        self.hooks.as_ref()
    }

    fn take_hooks(&mut self) -> Option<Hooks> {
        self.hooks.take()
    }

    fn create_dispatch(
        &self,
        template: &str,
        context_registry: &ContextRegistry,
        theme: &Theme,
    ) -> DispatchFn {
        let handler = self.handler.clone();
        let template = template.to_string();
        let context_registry = context_registry.clone();
        let theme = theme.clone();

        Arc::new(
            move |matches: &ArgMatches, ctx: &CommandContext, hooks: Option<&Hooks>| {
                let result = handler.handle(matches, ctx);

                match result {
                    Ok(HandlerOutput::Render(data)) => {
                        let mut json_data = serde_json::to_value(&data)
                            .map_err(|e| format!("Failed to serialize handler result: {}", e))?;

                        if let Some(hooks) = hooks {
                            json_data = hooks
                                .run_post_dispatch(matches, ctx, json_data)
                                .map_err(|e| format!("Hook error: {}", e))?;
                        }

                        let render_ctx = RenderContext::new(
                            ctx.output_mode,
                            get_terminal_width(),
                            &theme,
                            &json_data,
                        );

                        let output = render_auto_with_context(
                            &template,
                            &json_data,
                            &theme,
                            ctx.output_mode,
                            &context_registry,
                            &render_ctx,
                        )
                        .map_err(|e| e.to_string())?;
                        Ok(DispatchOutput::Text(output))
                    }
                    Err(e) => Err(format!("Error: {}", e)),
                    Ok(HandlerOutput::Silent) => Ok(DispatchOutput::Silent),
                    Ok(HandlerOutput::Binary { data, filename }) => {
                        Ok(DispatchOutput::Binary(data, filename))
                    }
                }
            },
        )
    }
}

/// Wrapper around ErasedCommandConfig that implements CommandRecipe.
///
/// This allows group-registered commands to use the deferred closure pattern.
/// The inner config is wrapped in a Mutex to allow interior mutability.
pub(crate) struct ErasedConfigRecipe {
    config: std::sync::Mutex<Option<Box<dyn ErasedCommandConfig + Send>>>,
    #[allow(dead_code)]
    template: Option<String>,
    #[allow(dead_code)]
    hooks: std::sync::Mutex<Option<Hooks>>,
}

impl ErasedConfigRecipe {
    /// Creates a new recipe from an existing boxed handler (for group registration).
    pub fn from_handler(mut handler: Box<dyn ErasedCommandConfig + Send>) -> Self {
        let template = handler.template().map(String::from);
        let hooks = handler.take_hooks();
        Self {
            config: std::sync::Mutex::new(Some(handler)),
            template,
            hooks: std::sync::Mutex::new(hooks),
        }
    }
}

impl CommandRecipe for ErasedConfigRecipe {
    fn template(&self) -> Option<&str> {
        self.template.as_deref()
    }

    fn hooks(&self) -> Option<&Hooks> {
        // Can't return reference through mutex, but hooks are extracted during construction
        None
    }

    fn take_hooks(&mut self) -> Option<Hooks> {
        self.hooks.lock().unwrap().take()
    }

    fn create_dispatch(
        &self,
        template: &str,
        context_registry: &ContextRegistry,
        theme: &Theme,
    ) -> DispatchFn {
        let config = self
            .config
            .lock()
            .unwrap()
            .take()
            .expect("ErasedConfigRecipe::create_dispatch called more than once");
        config.register(
            "",
            template.to_string(),
            context_registry.clone(),
            theme.clone(),
        )
    }
}

/// Configuration for a single command.
///
/// Used internally to collect handler, template, and hooks before
/// registering with the builder.
pub struct CommandConfig<H> {
    pub(crate) handler: H,
    pub(crate) template: Option<String>,
    pub(crate) hooks: Option<Hooks>,
}

impl<H> CommandConfig<H> {
    /// Creates a new command config with the given handler.
    pub fn new(handler: H) -> Self {
        Self {
            handler,
            template: None,
            hooks: None,
        }
    }

    /// Sets an explicit template for this command.
    ///
    /// If not set, the template will be derived from the command path
    /// using the configured template directory and extension.
    pub fn template(mut self, template: impl Into<String>) -> Self {
        self.template = Some(template.into());
        self
    }

    /// Sets hooks for this command.
    pub fn hooks(mut self, hooks: Hooks) -> Self {
        self.hooks = Some(hooks);
        self
    }

    /// Adds a pre-dispatch hook for this command.
    pub fn pre_dispatch<F>(mut self, f: F) -> Self
    where
        F: Fn(&ArgMatches, &CommandContext) -> Result<(), crate::cli::hooks::HookError>
            + Send
            + Sync
            + 'static,
    {
        let hooks = self.hooks.take().unwrap_or_default();
        self.hooks = Some(hooks.pre_dispatch(f));
        self
    }

    /// Adds a post-dispatch hook for this command.
    pub fn post_dispatch<F>(mut self, f: F) -> Self
    where
        F: Fn(
                &ArgMatches,
                &CommandContext,
                serde_json::Value,
            ) -> Result<serde_json::Value, crate::cli::hooks::HookError>
            + Send
            + Sync
            + 'static,
    {
        let hooks = self.hooks.take().unwrap_or_default();
        self.hooks = Some(hooks.post_dispatch(f));
        self
    }

    /// Adds a post-output hook for this command.
    pub fn post_output<F>(mut self, f: F) -> Self
    where
        F: Fn(
                &ArgMatches,
                &CommandContext,
                crate::cli::hooks::RenderedOutput,
            )
                -> Result<crate::cli::hooks::RenderedOutput, crate::cli::hooks::HookError>
            + Send
            + Sync
            + 'static,
    {
        let hooks = self.hooks.take().unwrap_or_default();
        self.hooks = Some(hooks.post_output(f));
        self
    }
}

/// Entry in the group builder - either a command or a nested group.
pub(crate) enum GroupEntry {
    /// A leaf command with handler, optional template, and optional hooks
    Command {
        handler: Box<dyn ErasedCommandConfig + Send + Sync>,
    },
    /// A nested group
    Group { builder: GroupBuilder },
}

/// Type-erased command configuration for storage.
pub(crate) trait ErasedCommandConfig {
    fn template(&self) -> Option<&str>;
    #[allow(dead_code)]
    fn hooks(&self) -> Option<&Hooks>;
    fn take_hooks(&mut self) -> Option<Hooks>;
    fn register(
        self: Box<Self>,
        path: &str,
        template: String,
        context_registry: ContextRegistry,
        theme: Theme,
    ) -> DispatchFn;
}

/// Builder for a group of related commands.
///
/// Groups allow organizing commands hierarchically:
///
/// ```rust,ignore
/// App::builder()
///     .group("db", |g| g
///         .command("migrate", db::migrate)
///         .command("backup", db::backup))
///     .group("app", |g| g
///         .command("start", app::start)
///         .group("config", |g| g
///             .command("get", app::config_get)
///             .command("set", app::config_set)))
///     .build()
/// ```
#[derive(Default)]
pub struct GroupBuilder {
    pub(crate) entries: HashMap<String, GroupEntry>,
    /// The default command to use when no subcommand is specified
    pub(crate) default_command: Option<String>,
}

impl GroupBuilder {
    /// Creates a new empty group builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if a command or group with the given name is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// Returns the number of registered entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if no entries are registered.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the default command name, if one is set.
    pub fn get_default_command(&self) -> Option<&str> {
        self.default_command.as_deref()
    }

    /// Registers a command handler (closure) in this group.
    ///
    /// The template will be derived from the command path if not explicitly set.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// .group("db", |g| g
    ///     .command("migrate", |_m, _ctx| {
    ///         Ok(HandlerOutput::Render(json!({"status": "done"})))
    ///     }))
    /// ```
    pub fn command<F, T>(self, name: &str, handler: F) -> Self
    where
        F: Fn(&ArgMatches, &CommandContext) -> HandlerResult<T> + Send + Sync + 'static,
        T: Serialize + Send + Sync + 'static,
    {
        self.command_with(name, handler, |cfg| cfg)
    }

    /// Registers a command handler with configuration.
    ///
    /// Use this to set explicit template or hooks inline:
    ///
    /// ```rust,ignore
    /// .group("db", |g| g
    ///     .command_with("migrate", handler, |cfg| cfg
    ///         .template("custom/migrate.j2")
    ///         .pre_dispatch(validate_db)))
    /// ```
    pub fn command_with<F, T, C>(mut self, name: &str, handler: F, configure: C) -> Self
    where
        F: Fn(&ArgMatches, &CommandContext) -> HandlerResult<T> + Send + Sync + 'static,
        T: Serialize + Send + Sync + 'static,
        C: FnOnce(CommandConfig<FnHandler<F, T>>) -> CommandConfig<FnHandler<F, T>>,
    {
        let config = CommandConfig::new(FnHandler::new(handler));
        let config = configure(config);
        self.entries.insert(
            name.to_string(),
            GroupEntry::Command {
                handler: Box::new(ClosureCommandConfig {
                    handler: config.handler,
                    template: config.template,
                    hooks: config.hooks,
                }),
            },
        );
        self
    }

    /// Registers a struct handler in this group.
    pub fn handler<H, T>(self, name: &str, handler: H) -> Self
    where
        H: Handler<Output = T> + 'static,
        T: Serialize + 'static,
    {
        self.handler_with(name, handler, |cfg| cfg)
    }

    /// Registers a struct handler with configuration.
    pub fn handler_with<H, T, C>(mut self, name: &str, handler: H, configure: C) -> Self
    where
        H: Handler<Output = T> + 'static,
        T: Serialize + 'static,
        C: FnOnce(CommandConfig<H>) -> CommandConfig<H>,
    {
        let config = CommandConfig::new(handler);
        let config = configure(config);
        self.entries.insert(
            name.to_string(),
            GroupEntry::Command {
                handler: Box::new(StructCommandConfig {
                    handler: config.handler,
                    template: config.template,
                    hooks: config.hooks,
                }),
            },
        );
        self
    }

    /// Creates a nested group within this group.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// .group("app", |g| g
    ///     .group("config", |g| g
    ///         .command("get", get_handler)
    ///         .command("set", set_handler)))
    /// ```
    pub fn group<F>(mut self, name: &str, configure: F) -> Self
    where
        F: FnOnce(GroupBuilder) -> GroupBuilder,
    {
        let builder = configure(GroupBuilder::new());
        self.entries
            .insert(name.to_string(), GroupEntry::Group { builder });
        self
    }

    /// Sets a command as the default command for this group.
    ///
    /// When the CLI is invoked without a subcommand (a "naked" invocation),
    /// the default command is automatically used.
    ///
    /// # Panics
    ///
    /// Panics if a default command has already been set, as only one
    /// default command can be defined.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// .group("app", |g| g
    ///     .command("list", list_handler)
    ///     .command("add", add_handler)
    ///     .default_command("list"))  // "list" is used when no command specified
    /// ```
    pub fn default_command(mut self, name: &str) -> Self {
        if self.default_command.is_some() {
            panic!(
                "Only one default command can be defined. '{}' is already set as default.",
                self.default_command.as_ref().unwrap()
            );
        }
        self.default_command = Some(name.to_string());
        self
    }
}

/// Internal: closure-based command config that implements ErasedCommandConfig
struct ClosureCommandConfig<F, T>
where
    F: Fn(&ArgMatches, &CommandContext) -> HandlerResult<T> + Send + Sync + 'static,
    T: Serialize + Send + Sync + 'static,
{
    handler: FnHandler<F, T>,
    template: Option<String>,
    hooks: Option<Hooks>,
}

impl<F, T> ErasedCommandConfig for ClosureCommandConfig<F, T>
where
    F: Fn(&ArgMatches, &CommandContext) -> HandlerResult<T> + Send + Sync + 'static,
    T: Serialize + Send + Sync + 'static,
{
    fn template(&self) -> Option<&str> {
        self.template.as_deref()
    }

    fn hooks(&self) -> Option<&Hooks> {
        self.hooks.as_ref()
    }

    fn take_hooks(&mut self) -> Option<Hooks> {
        self.hooks.take()
    }

    fn register(
        self: Box<Self>,
        _path: &str,
        template: String,
        context_registry: ContextRegistry,
        theme: Theme,
    ) -> DispatchFn {
        let handler = Arc::new(self.handler);

        Arc::new(
            move |matches: &ArgMatches, ctx: &CommandContext, hooks: Option<&Hooks>| {
                let result = handler.handle(matches, ctx);

                match result {
                    Ok(HandlerOutput::Render(data)) => {
                        let mut json_data = serde_json::to_value(&data)
                            .map_err(|e| format!("Failed to serialize handler result: {}", e))?;

                        if let Some(hooks) = hooks {
                            json_data = hooks
                                .run_post_dispatch(matches, ctx, json_data)
                                .map_err(|e| format!("Hook error: {}", e))?;
                        }

                        let render_ctx = RenderContext::new(
                            ctx.output_mode,
                            get_terminal_width(),
                            &theme,
                            &json_data,
                        );

                        let output = render_auto_with_context(
                            &template,
                            &json_data,
                            &theme,
                            ctx.output_mode,
                            &context_registry,
                            &render_ctx,
                        )
                        .map_err(|e| e.to_string())?;
                        Ok(DispatchOutput::Text(output))
                    }
                    Err(e) => Err(format!("Error: {}", e)),
                    Ok(HandlerOutput::Silent) => Ok(DispatchOutput::Silent),
                    Ok(HandlerOutput::Binary {
                        data: bytes,
                        filename,
                    }) => Ok(DispatchOutput::Binary(bytes, filename)),
                }
            },
        )
    }
}

/// Internal: struct-based command config that implements ErasedCommandConfig
struct StructCommandConfig<H, T>
where
    H: Handler<Output = T> + 'static,
    T: Serialize + 'static,
{
    handler: H,
    template: Option<String>,
    hooks: Option<Hooks>,
}

impl<H, T> ErasedCommandConfig for StructCommandConfig<H, T>
where
    H: Handler<Output = T> + 'static,
    T: Serialize + 'static,
{
    fn template(&self) -> Option<&str> {
        self.template.as_deref()
    }

    fn hooks(&self) -> Option<&Hooks> {
        self.hooks.as_ref()
    }

    fn take_hooks(&mut self) -> Option<Hooks> {
        self.hooks.take()
    }

    fn register(
        self: Box<Self>,
        _path: &str,
        template: String,
        context_registry: ContextRegistry,
        theme: Theme,
    ) -> DispatchFn {
        let handler = Arc::new(self.handler);

        Arc::new(
            move |matches: &ArgMatches, ctx: &CommandContext, hooks: Option<&Hooks>| {
                let result = handler.handle(matches, ctx);

                match result {
                    Ok(HandlerOutput::Render(data)) => {
                        let mut json_data = serde_json::to_value(&data)
                            .map_err(|e| format!("Failed to serialize handler result: {}", e))?;

                        if let Some(hooks) = hooks {
                            json_data = hooks
                                .run_post_dispatch(matches, ctx, json_data)
                                .map_err(|e| format!("Hook error: {}", e))?;
                        }

                        let render_ctx = RenderContext::new(
                            ctx.output_mode,
                            get_terminal_width(),
                            &theme,
                            &json_data,
                        );

                        let output = render_auto_with_context(
                            &template,
                            &json_data,
                            &theme,
                            ctx.output_mode,
                            &context_registry,
                            &render_ctx,
                        )
                        .map_err(|e| e.to_string())?;
                        Ok(DispatchOutput::Text(output))
                    }
                    Err(e) => Err(format!("Error: {}", e)),
                    Ok(HandlerOutput::Silent) => Ok(DispatchOutput::Silent),
                    Ok(HandlerOutput::Binary {
                        data: bytes,
                        filename,
                    }) => Ok(DispatchOutput::Binary(bytes, filename)),
                }
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_group_builder_creation() {
        let group = GroupBuilder::new();
        assert!(group.entries.is_empty());
    }

    #[test]
    fn test_group_builder_command() {
        let group = GroupBuilder::new().command("test", |_m, _ctx| {
            Ok(HandlerOutput::Render(json!({"ok": true})))
        });

        assert!(group.entries.contains_key("test"));
    }

    #[test]
    fn test_group_builder_nested() {
        let group = GroupBuilder::new()
            .command("top", |_m, _ctx| Ok(HandlerOutput::Render(json!({}))))
            .group("nested", |g| {
                g.command("inner", |_m, _ctx| Ok(HandlerOutput::Render(json!({}))))
            });

        assert!(group.entries.contains_key("top"));
        assert!(group.entries.contains_key("nested"));
    }

    #[test]
    fn test_command_config_template() {
        let config =
            CommandConfig::new(FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
                Ok(HandlerOutput::Render(json!({})))
            }))
            .template("custom.j2");

        assert_eq!(config.template, Some("custom.j2".to_string()));
    }

    #[test]
    fn test_command_config_hooks() {
        let config =
            CommandConfig::new(FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
                Ok(HandlerOutput::Render(json!({})))
            }))
            .pre_dispatch(|_, _| Ok(()));

        assert!(config.hooks.is_some());
    }

    #[test]
    fn test_group_builder_default_command() {
        let group = GroupBuilder::new()
            .command("list", |_m, _ctx| Ok(HandlerOutput::Render(json!({}))))
            .command("add", |_m, _ctx| Ok(HandlerOutput::Render(json!({}))))
            .default_command("list");

        assert_eq!(group.default_command, Some("list".to_string()));
    }

    #[test]
    #[should_panic(expected = "Only one default command can be defined")]
    fn test_group_builder_duplicate_default_command_panics() {
        let _ = GroupBuilder::new()
            .command("list", |_m, _ctx| Ok(HandlerOutput::Render(json!({}))))
            .command("add", |_m, _ctx| Ok(HandlerOutput::Render(json!({}))))
            .default_command("list")
            .default_command("add");
    }
}
