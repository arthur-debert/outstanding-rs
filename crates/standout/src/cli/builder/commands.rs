//! Command registration methods for AppBuilder.
//!
//! This module contains methods for registering commands and handlers:
//! - Simple command registration with closures
//! - Struct-based handler registration
//! - Command groups for nested hierarchies
//! - Hook registration

use clap::ArgMatches;
use serde::Serialize;

use super::{inline_template_ref, AppBuilder, PendingCommand, TemplateAbsence, TemplateRef};
use crate::cli::group::{
    ClosureRecipe, CommandConfig, ErasedConfigRecipe, GroupBuilder, GroupEntry, PassthroughRecipe,
    StructRecipe,
};
use crate::cli::handler::{CommandContext, FnHandler, Handler, HandlerResult};
use crate::cli::hooks::Hooks;
use crate::setup::SetupError;

impl AppBuilder {
    /// Creates a command group for organizing related commands.
    ///
    /// Groups allow nested command hierarchies with a fluent API:
    ///
    /// ```rust,ignore
    /// App::builder()
    ///     .templates_dir("templates")?
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
    pub fn group<F>(mut self, name: &str, configure: F) -> Result<Self, SetupError>
    where
        F: FnOnce(GroupBuilder) -> GroupBuilder,
    {
        let builder = configure(GroupBuilder::new());
        self.register_group(name, builder)?;
        Ok(self)
    }

    /// Registers a command handler with inline configuration.
    ///
    /// Use this to set explicit template or hooks without using `.hooks()` separately:
    ///
    /// ```rust,ignore
    /// App::builder()
    ///     .command_with("list", handler, |cfg| cfg
    ///         .template_name("custom/list")
    ///         .pre_dispatch(validate_auth)
    ///         .post_output(copy_to_clipboard))
    ///     .build()
    /// ```
    pub fn command_with<F, T, C>(
        mut self,
        path: &str,
        handler: F,
        configure: C,
    ) -> Result<Self, SetupError>
    where
        F: FnMut(&ArgMatches, &CommandContext) -> HandlerResult<T> + 'static,
        T: Serialize + 'static,
        C: FnOnce(CommandConfig<FnHandler<F, T>>) -> CommandConfig<FnHandler<F, T>>,
    {
        let config = CommandConfig::new(FnHandler::new(handler));
        let mut config = configure(config);

        let template = if let Some(absence) = config.template_absence {
            TemplateRef::Absent(absence)
        } else if let Some(name) = config.template_name.clone() {
            TemplateRef::Named(name)
        } else if let Some(template) = config.template.clone() {
            inline_template_ref(template, "CommandConfig::template")?
        } else {
            TemplateRef::convention(path, &self.template_ext)
        };

        // Register hooks if present
        if let Some(hooks) = config.hooks.take() {
            self.command_hooks.insert(path.to_string(), hooks);
        }
        if let Some(questionnaire) = config.questionnaire.take() {
            self.questionnaire_commands
                .insert(path.to_string(), questionnaire);
        }

        // Create a recipe for deferred closure creation using the handler
        let mut recipe = ClosureRecipe::new(config.handler);
        if let Some(projection) = config.structured_output_projection {
            recipe = recipe.with_structured_output_projection(projection);
        }

        // Store pending command - check for duplicates
        if self.pending_commands.borrow().contains_key(path) {
            return Err(SetupError::DuplicateCommand(path.to_string()));
        }

        self.pending_commands.borrow_mut().insert(
            path.to_string(),
            PendingCommand {
                recipe: Box::new(recipe),
                template,
            },
        );

        Ok(self)
    }

    /// Helper to register a group's commands recursively.
    pub(crate) fn register_group(
        &mut self,
        prefix: &str,
        builder: GroupBuilder,
    ) -> Result<(), SetupError> {
        for (name, entry) in builder.entries {
            let path = format!("{}.{}", prefix, name);

            match entry {
                GroupEntry::Command { mut handler } => {
                    let template = if let Some(absence) = handler.template_absence() {
                        TemplateRef::Absent(absence)
                    } else if let Some(name) = handler.template_name() {
                        TemplateRef::Named(name.to_string())
                    } else if let Some(template) = handler.template() {
                        inline_template_ref(template, "CommandConfig::template")?
                    } else {
                        TemplateRef::convention(&path, &self.template_ext)
                    };

                    // Extract and register hooks
                    if let Some(hooks) = handler.take_hooks() {
                        self.command_hooks.insert(path.clone(), hooks);
                    }
                    if let Some(questionnaire) = handler.take_questionnaire() {
                        self.questionnaire_commands
                            .insert(path.clone(), questionnaire);
                    }

                    // Create a recipe for deferred closure creation
                    let recipe = ErasedConfigRecipe::from_handler(handler);

                    // Check for duplicates
                    if self.pending_commands.borrow().contains_key(&path) {
                        return Err(SetupError::DuplicateCommand(path.clone()));
                    }

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
                    self.register_group(&path, nested)?;
                }
            }
        }
        Ok(())
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
    /// use standout::cli::{App, Output, HandlerResult};
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
    pub fn command<F, T>(self, path: &str, handler: F, template: &str) -> Result<Self, SetupError>
    where
        F: FnMut(&ArgMatches, &CommandContext) -> HandlerResult<T> + 'static,
        T: Serialize + 'static,
    {
        self.register_struct_config(
            path,
            CommandConfig::new(FnHandler::new(handler)).template(template),
            "AppBuilder::command",
        )
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
    /// use standout::cli::{App, Handler, HandlerResult, Output, CommandContext};
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
    pub fn command_handler<H, T>(
        self,
        path: &str,
        handler: H,
        template: &str,
    ) -> Result<Self, SetupError>
    where
        H: Handler<Output = T> + 'static,
        T: Serialize + 'static,
    {
        self.register_struct_config(
            path,
            CommandConfig::new(handler).template(template),
            "AppBuilder::command_handler",
        )
    }

    /// Registers a struct handler with command configuration.
    ///
    /// This is the struct-handler counterpart to [`command_with`](Self::command_with).
    /// Use it to select a named template, declare template absence, or attach
    /// hooks and structured-output projection without a placeholder template.
    pub fn command_handler_with<H, T, C>(
        self,
        path: &str,
        handler: H,
        configure: C,
    ) -> Result<Self, SetupError>
    where
        H: Handler<Output = T> + 'static,
        T: Serialize + 'static,
        C: FnOnce(CommandConfig<H>) -> CommandConfig<H>,
    {
        self.register_struct_config(
            path,
            configure(CommandConfig::new(handler)),
            "CommandConfig::template",
        )
    }

    fn register_struct_config<H, T>(
        mut self,
        path: &str,
        mut config: CommandConfig<H>,
        inline_api: &str,
    ) -> Result<Self, SetupError>
    where
        H: Handler<Output = T> + 'static,
        T: Serialize + 'static,
    {
        let template = if let Some(absence) = config.template_absence {
            TemplateRef::Absent(absence)
        } else if let Some(name) = config.template_name.take() {
            TemplateRef::Named(name)
        } else if let Some(template) = config.template.take() {
            inline_template_ref(template, inline_api)?
        } else {
            TemplateRef::convention(path, &self.template_ext)
        };

        if let Some(hooks) = config.hooks.take() {
            self.command_hooks.insert(path.to_string(), hooks);
        }
        if let Some(questionnaire) = config.questionnaire.take() {
            self.questionnaire_commands
                .insert(path.to_string(), questionnaire);
        }

        let mut recipe = StructRecipe::new(config.handler);
        if let Some(projection) = config.structured_output_projection {
            recipe = recipe.with_structured_output_projection(projection);
        }

        // Check for duplicates
        if self.pending_commands.borrow().contains_key(path) {
            return Err(SetupError::DuplicateCommand(path.to_string()));
        }

        // Store pending command - closure will be created at dispatch time
        self.pending_commands.borrow_mut().insert(
            path.to_string(),
            PendingCommand {
                recipe: Box::new(recipe),
                template,
            },
        );

        Ok(self)
    }

    /// Registers a passthrough command that bypasses the rendering pipeline.
    ///
    /// The handler receives `&ArgMatches` and `&CommandContext`, writes directly to
    /// stdout (or does whatever it needs), and the framework marks the command as
    /// handled with no rendered output. The command still participates in
    /// help/completions and dispatch.
    ///
    /// Use this for commands that manage their own output (e.g., shell init scripts
    /// that output `eval`-able code, or commands that delegate to another tool).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use standout::cli::App;
    ///
    /// App::builder()
    ///     .command_passthrough("init-sh", |_m, _ctx| {
    ///         print!("export PATH=\"$HOME/.myapp/bin:$PATH\"");
    ///         Ok(())
    ///     })
    ///     .build()?
    ///     .run(cmd, args);
    /// ```
    pub fn command_passthrough<F>(self, path: &str, handler: F) -> Result<Self, SetupError>
    where
        F: FnMut(&ArgMatches, &CommandContext) -> Result<(), anyhow::Error> + 'static,
    {
        let recipe = PassthroughRecipe::new(handler);

        if self.pending_commands.borrow().contains_key(path) {
            return Err(SetupError::DuplicateCommand(path.to_string()));
        }

        self.pending_commands.borrow_mut().insert(
            path.to_string(),
            PendingCommand {
                recipe: Box::new(recipe),
                template: TemplateRef::Absent(TemplateAbsence::Silent),
            },
        );

        Ok(self)
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
    /// use standout::cli::{App, Hooks, Output, HookError};
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

        let builder = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"items": ["a", "b"]}))),
                "Items: {{ items }}",
            )
            .unwrap();

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

        let builder = AppBuilder::new()
            .command_with(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"items": ["a", "b"]}))),
                move |cfg| {
                    cfg.template("Items: {{ items | length }}")
                        .pre_dispatch(move |_, _| {
                            counter_clone.fetch_add(1, Ordering::SeqCst);
                            Ok(())
                        })
                },
            )
            .unwrap();
        let app = builder.build().unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = app.dispatch(matches, OutputMode::Text);

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

        let builder = AppBuilder::new()
            .group("db", |g| {
                g.command("migrate", |_m, _ctx| {
                    Ok(HandlerOutput::Render(json!({"status": "migrated"})))
                })
                .command("backup", |_m, _ctx| {
                    Ok(HandlerOutput::Render(json!({"status": "backed_up"})))
                })
            })
            .unwrap();
        let app = builder.build().unwrap();

        let cmd =
            Command::new("app").subcommand(Command::new("db").subcommand(Command::new("migrate")));

        let matches = cmd.try_get_matches_from(["app", "db", "migrate"]).unwrap();
        let result = app.dispatch(matches, OutputMode::Json);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("migrated"));
    }

    #[test]
    fn test_group_nested() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .group("app", |g| {
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
            })
            .unwrap();
        let app = builder.build().unwrap();

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
        let result = app.dispatch(matches, OutputMode::Json);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("test_value"));
    }

    #[test]
    fn test_group_with_template() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .group("db", |g| {
                g.command_with(
                    "migrate",
                    |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 5}))),
                    |cfg| cfg.template("Migrated {{ count }} tables"),
                )
            })
            .unwrap();
        let app = builder.build().unwrap();

        let cmd =
            Command::new("app").subcommand(Command::new("db").subcommand(Command::new("migrate")));

        let matches = cmd.try_get_matches_from(["app", "db", "migrate"]).unwrap();
        let result = app.dispatch(matches, OutputMode::Text);

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

        let builder = AppBuilder::new()
            .group("db", |g| {
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
            })
            .unwrap();
        let app = builder.build().unwrap();

        let cmd =
            Command::new("app").subcommand(Command::new("db").subcommand(Command::new("migrate")));

        let matches = cmd.try_get_matches_from(["app", "db", "migrate"]).unwrap();
        let result = app.dispatch(matches, OutputMode::Text);

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
            .unwrap()
            .group("cache", |g| {
                g.command("clear", |_m, _ctx| {
                    Ok(HandlerOutput::Render(json!({"type": "cache"})))
                })
            })
            .unwrap();

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
            .unwrap()
            .group("db", |g| {
                g.command("migrate", |_m, _ctx| {
                    Ok(HandlerOutput::Render(json!({"ok": true})))
                })
            })
            .unwrap();

        assert!(builder.has_command("version"));
        assert!(builder.has_command("db.migrate"));
    }

    #[test]
    fn test_command_passthrough() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        let builder = AppBuilder::new()
            .command_passthrough("init-sh", move |_m, _ctx| {
                called_clone.store(true, Ordering::SeqCst);
                Ok(())
            })
            .unwrap();

        assert!(builder.has_command("init-sh"));

        let cmd = Command::new("app").subcommand(Command::new("init-sh"));
        let matches = cmd.try_get_matches_from(["app", "init-sh"]).unwrap();
        let app = builder.build().unwrap();
        let result = app.dispatch(matches, OutputMode::Text);

        assert!(called.load(Ordering::SeqCst));
        // Passthrough commands produce empty handled output (silent)
        assert!(result.is_handled());
        assert_eq!(result.output(), Some(""));
    }

    #[test]
    fn test_group_passthrough() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        let builder = AppBuilder::new()
            .group("shell", |g| {
                g.passthrough("init", move |_m, _ctx| {
                    called_clone.store(true, Ordering::SeqCst);
                    Ok(())
                })
            })
            .unwrap();

        assert!(builder.has_command("shell.init"));

        let cmd =
            Command::new("app").subcommand(Command::new("shell").subcommand(Command::new("init")));
        let matches = cmd.try_get_matches_from(["app", "shell", "init"]).unwrap();
        let app = builder.build().unwrap();
        let result = app.dispatch(matches, OutputMode::Text);

        assert!(called.load(Ordering::SeqCst));
        assert!(result.is_handled());
    }
}
