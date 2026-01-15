//! Command registration methods for AppBuilder.
//!
//! This module contains methods for registering commands and handlers:
//! - Simple command registration with closures
//! - Struct-based handler registration
//! - Command groups for nested hierarchies
//! - Hook registration

use clap::ArgMatches;
use serde::Serialize;

use super::{AppBuilder, PendingCommand};
use crate::cli::group::{
    ClosureRecipe, CommandConfig, ErasedConfigRecipe, GroupBuilder, GroupEntry, StructRecipe,
};
use crate::cli::handler::{CommandContext, FnHandler, Handler, HandlerResult};
use crate::cli::hooks::Hooks;

impl AppBuilder {
    /// Creates a command group for organizing related commands.
    ///
    /// Groups allow nested command hierarchies with a fluent API:
    ///
    /// ```rust,ignore
    /// App::builder()
    ///     .template_dir("templates")
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
    ///
    /// Commands within groups use dot notation for paths:
    /// - `db.migrate`, `db.backup`
    /// - `app.start`, `app.config.get`, `app.config.set`
    pub fn group<F>(mut self, name: &str, configure: F) -> Self
    where
        F: FnOnce(GroupBuilder) -> GroupBuilder,
    {
        let builder = configure(GroupBuilder::new());
        self.register_group(name, builder);
        self
    }

    /// Registers a command handler with inline configuration.
    ///
    /// Use this to set explicit template or hooks without using `.hooks()` separately:
    ///
    /// ```rust,ignore
    /// App::builder()
    ///     .command_with("list", handler, |cfg| cfg
    ///         .template("custom/list.j2")
    ///         .pre_dispatch(validate_auth)
    ///         .post_output(copy_to_clipboard))
    ///     .build()
    /// ```
    pub fn command_with<F, T, C>(mut self, path: &str, handler: F, configure: C) -> Self
    where
        F: Fn(&ArgMatches, &CommandContext) -> HandlerResult<T> + Send + Sync + 'static,
        T: Serialize + Send + Sync + 'static,
        C: FnOnce(CommandConfig<FnHandler<F, T>>) -> CommandConfig<FnHandler<F, T>>,
    {
        let config = CommandConfig::new(FnHandler::new(handler));
        let mut config = configure(config);

        // Resolve template
        let template = config
            .template
            .clone()
            .unwrap_or_else(|| self.resolve_template(path));

        // Register hooks if present
        if let Some(hooks) = config.hooks.take() {
            self.command_hooks.insert(path.to_string(), hooks);
        }

        // Create a recipe for deferred closure creation using the handler
        let recipe = ClosureRecipe::new(config.handler);

        // Store pending command - closure will be created at dispatch time
        self.pending_commands.borrow_mut().insert(
            path.to_string(),
            PendingCommand {
                recipe: Box::new(recipe),
                template,
            },
        );

        self
    }

    /// Helper to register a group's commands recursively.
    pub(crate) fn register_group(&mut self, prefix: &str, builder: GroupBuilder) {
        for (name, entry) in builder.entries {
            let path = format!("{}.{}", prefix, name);

            match entry {
                GroupEntry::Command { mut handler } => {
                    // Resolve template
                    let template = handler
                        .template()
                        .map(String::from)
                        .unwrap_or_else(|| self.resolve_template(&path));

                    // Extract and register hooks
                    if let Some(hooks) = handler.take_hooks() {
                        self.command_hooks.insert(path.clone(), hooks);
                    }

                    // Create a recipe for deferred closure creation
                    let recipe = ErasedConfigRecipe::from_handler(handler);

                    // Store pending command
                    self.pending_commands.borrow_mut().insert(
                        path,
                        PendingCommand {
                            recipe: Box::new(recipe),
                            template,
                        },
                    );
                }
                GroupEntry::Group { builder: nested } => {
                    self.register_group(&path, nested);
                }
            }
        }
    }

    /// Resolves a template from a command path using conventions.
    ///
    /// Resolution order:
    /// 1. If template_registry is set, look up by command path (e.g., "db/migrate.j2")
    /// 2. If template_dir is set, return the file path for runtime loading
    /// 3. Otherwise return empty string (JSON serialization fallback)
    pub(crate) fn resolve_template(&self, command_path: &str) -> String {
        let file_path = command_path.replace('.', "/");
        let template_name = format!("{}{}", file_path, self.template_ext);

        // First, try to get content from embedded templates
        if let Some(ref registry) = self.template_registry {
            if let Ok(content) = registry.get_content(&template_name) {
                return content;
            }
        }

        // Fall back to file path if template_dir is configured
        if let Some(ref dir) = self.template_dir {
            return format!("{}/{}", dir.display(), template_name);
        }

        // No template found - will use JSON serialization in structured modes
        String::new()
    }

    /// Registers a command handler (closure) with a template.
    ///
    /// The handler will be invoked when the command path matches. The path uses
    /// dot notation for nested commands (e.g., "config.get" matches `app config get`).
    ///
    /// # Arguments
    ///
    /// * `path` - Command path using dot notation (e.g., "list" or "config.get")
    /// * `handler` - The handler closure
    /// * `template` - MiniJinja template for rendering output
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use outstanding::cli::{App, Output, HandlerResult};
    /// use serde::Serialize;
    ///
    /// #[derive(Serialize)]
    /// struct ListOutput { items: Vec<String> }
    ///
    /// App::builder()
    ///     .command("list", |_m, _ctx| -> HandlerResult<ListOutput> {
    ///         Ok(Output::Render(ListOutput { items: vec!["one".into()] }))
    ///     }, "{% for item in items %}{{ item }}\n{% endfor %}")
    ///     .parse(cmd);
    /// ```
    pub fn command<F, T>(self, path: &str, handler: F, template: &str) -> Self
    where
        F: Fn(&ArgMatches, &CommandContext) -> HandlerResult<T> + Send + Sync + 'static,
        T: Serialize + Send + Sync + 'static,
    {
        self.command_handler(path, FnHandler::new(handler), template)
    }

    /// Registers a struct handler with a template.
    ///
    /// Use this when your handler needs to carry state (like database connections).
    ///
    /// # Arguments
    ///
    /// * `path` - Command path using dot notation (e.g., "list" or "config.get")
    /// * `handler` - A struct implementing the `Handler` trait
    /// * `template` - MiniJinja template for rendering output
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use outstanding::cli::{App, Handler, HandlerResult, Output, CommandContext};
    /// use clap::ArgMatches;
    /// use serde::Serialize;
    ///
    /// struct ListHandler { db: Database }
    ///
    /// impl Handler for ListHandler {
    ///     type Output = Vec<Item>;
    ///     fn handle(&self, _m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<Self::Output> {
    ///         Ok(Output::Render(self.db.list()?))
    ///     }
    /// }
    ///
    /// App::builder()
    ///     .command_handler("list", ListHandler { db }, "{% for item in items %}...")
    ///     .parse(cmd);
    /// ```
    pub fn command_handler<H, T>(self, path: &str, handler: H, template: &str) -> Self
    where
        H: Handler<Output = T> + Send + Sync + 'static,
        T: Serialize + Send + Sync + 'static,
    {
        let template = template.to_string();

        // Create a recipe for deferred closure creation
        let recipe = StructRecipe::new(handler);

        // Store pending command - closure will be created at dispatch time
        self.pending_commands.borrow_mut().insert(
            path.to_string(),
            PendingCommand {
                recipe: Box::new(recipe),
                template,
            },
        );

        self
    }

    /// Registers hooks for a specific command path.
    ///
    /// Hooks are executed around the command handler:
    /// - Pre-dispatch hooks run before the handler
    /// - Post-dispatch hooks run after the handler, before rendering (receives raw data)
    /// - Post-output hooks run after rendering, can transform output
    ///
    /// Multiple hooks at the same phase are chained in registration order.
    /// Hooks abort on first error.
    ///
    /// # Arguments
    ///
    /// * `path` - Command path using dot notation (e.g., "list" or "config.get")
    /// * `hooks` - The hooks configuration
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use outstanding::cli::{App, Hooks, Output, HookError};
    /// use serde_json::json;
    ///
    /// App::builder()
    ///     .command("list", handler, template)
    ///     .hooks("list", Hooks::new()
    ///         .pre_dispatch(|_m, ctx| {
    ///             println!("Running: {:?}", ctx.command_path);
    ///             Ok(())
    ///         })
    ///         .post_dispatch(|_m, _ctx, mut data| {
    ///             // Modify raw data before rendering
    ///             if let Some(obj) = data.as_object_mut() {
    ///                 obj.insert("processed".into(), json!(true));
    ///             }
    ///             Ok(data)
    ///         })
    ///         .post_output(|_m, _ctx, output| {
    ///             if let RenderedOutput::Text(ref text) = output {
    ///                 // Copy to clipboard, log, etc.
    ///             }
    ///             Ok(output)
    ///         }))
    ///     .build()?
    ///     .run(cmd, args);
    /// ```
    pub fn hooks(mut self, path: &str, hooks: Hooks) -> Self {
        self.command_hooks.insert(path.to_string(), hooks);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::handler::Output as HandlerOutput;
    use crate::OutputMode;
    use clap::Command;

    #[test]
    fn test_command_registration() {
        use serde_json::json;

        let builder = AppBuilder::new().command(
            "list",
            |_m, _ctx| Ok(HandlerOutput::Render(json!({"items": ["a", "b"]}))),
            "Items: {{ items }}",
        );

        assert!(builder.has_command("list"));
    }

    #[test]
    fn test_hooks_registration() {
        use crate::cli::hooks::Hooks;

        let builder = AppBuilder::new().hooks("list", Hooks::new().pre_dispatch(|_, _| Ok(())));

        assert!(builder.command_hooks.contains_key("list"));
    }

    #[test]
    fn test_command_with_inline_config() {
        use serde_json::json;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let builder = AppBuilder::new().command_with(
            "list",
            |_m, _ctx| Ok(HandlerOutput::Render(json!({"items": ["a", "b"]}))),
            move |cfg| {
                cfg.template("Items: {{ items | length }}")
                    .pre_dispatch(move |_, _| {
                        counter_clone.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    })
            },
        );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Items: 2"));
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    // ============================================================================
    // Group Tests
    // ============================================================================

    #[test]
    fn test_group_basic() {
        use serde_json::json;

        let builder = AppBuilder::new().group("db", |g| {
            g.command("migrate", |_m, _ctx| {
                Ok(HandlerOutput::Render(json!({"status": "migrated"})))
            })
            .command("backup", |_m, _ctx| {
                Ok(HandlerOutput::Render(json!({"status": "backed_up"})))
            })
        });

        let cmd =
            Command::new("app").subcommand(Command::new("db").subcommand(Command::new("migrate")));

        let matches = cmd.try_get_matches_from(["app", "db", "migrate"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Json);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("migrated"));
    }

    #[test]
    fn test_group_nested() {
        use serde_json::json;

        let builder = AppBuilder::new().group("app", |g| {
            g.command("start", |_m, _ctx| {
                Ok(HandlerOutput::Render(json!({"action": "start"})))
            })
            .group("config", |g| {
                g.command("get", |_m, _ctx| {
                    Ok(HandlerOutput::Render(json!({"value": "test_value"})))
                })
                .command("set", |_m, _ctx| {
                    Ok(HandlerOutput::Render(json!({"ok": true})))
                })
            })
        });

        // Test nested command: app.config.get
        let cmd = Command::new("cli").subcommand(
            Command::new("app")
                .subcommand(Command::new("start"))
                .subcommand(
                    Command::new("config")
                        .subcommand(Command::new("get"))
                        .subcommand(Command::new("set")),
                ),
        );

        let matches = cmd
            .try_get_matches_from(["cli", "app", "config", "get"])
            .unwrap();
        let result = builder.dispatch(matches, OutputMode::Json);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("test_value"));
    }

    #[test]
    fn test_group_with_template() {
        use serde_json::json;

        let builder = AppBuilder::new().group("db", |g| {
            g.command_with(
                "migrate",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 5}))),
                |cfg| cfg.template("Migrated {{ count }} tables"),
            )
        });

        let cmd =
            Command::new("app").subcommand(Command::new("db").subcommand(Command::new("migrate")));

        let matches = cmd.try_get_matches_from(["app", "db", "migrate"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Migrated 5 tables"));
    }

    #[test]
    fn test_group_with_hooks() {
        use serde_json::json;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let hook_called = Arc::new(AtomicBool::new(false));
        let hook_called_clone = hook_called.clone();

        let builder = AppBuilder::new().group("db", |g| {
            g.command_with(
                "migrate",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"done": true}))),
                move |cfg| {
                    cfg.template("{{ done }}").pre_dispatch(move |_, _| {
                        hook_called_clone.store(true, Ordering::SeqCst);
                        Ok(())
                    })
                },
            )
        });

        let cmd =
            Command::new("app").subcommand(Command::new("db").subcommand(Command::new("migrate")));

        let matches = cmd.try_get_matches_from(["app", "db", "migrate"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert!(hook_called.load(Ordering::SeqCst));
    }

    #[test]
    fn test_multiple_groups() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .group("db", |g| {
                g.command("migrate", |_m, _ctx| {
                    Ok(HandlerOutput::Render(json!({"type": "db"})))
                })
            })
            .group("cache", |g| {
                g.command("clear", |_m, _ctx| {
                    Ok(HandlerOutput::Render(json!({"type": "cache"})))
                })
            });

        assert!(builder.has_command("db.migrate"));
        assert!(builder.has_command("cache.clear"));
    }

    #[test]
    fn test_group_mixed_with_regular_commands() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .command(
                "version",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"v": "1.0.0"}))),
                "{{ v }}",
            )
            .group("db", |g| {
                g.command("migrate", |_m, _ctx| {
                    Ok(HandlerOutput::Render(json!({"ok": true})))
                })
            });

        assert!(builder.has_command("version"));
        assert!(builder.has_command("db.migrate"));
    }
}
