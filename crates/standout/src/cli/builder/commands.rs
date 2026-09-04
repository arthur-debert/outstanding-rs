use clap::ArgMatches;
use serde::Serialize;

use super::{AppBuilder, HookRegistrationSource, PendingCommand, TemplateAbsence, TemplateRef};
use crate::cli::group::{
    CommandConfig, ErasedConfigRecipe, GroupBuilder, GroupEntry, PassthroughRecipe, StructRecipe,
};
use crate::cli::handler::{CommandContext, Handler};
use crate::cli::hooks::Hooks;
use crate::setup::SetupError;

impl AppBuilder {
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
                    } else {
                        TemplateRef::convention(&path)
                    };

                    if let Some(hooks) = handler.take_hooks() {
                        self.register_command_hooks(
                            &path,
                            hooks,
                            HookRegistrationSource::CommandConfig,
                        )?;
                    }
                    if let Some(questionnaire) = handler.take_questionnaire() {
                        self.questionnaire_commands
                            .insert(path.clone(), questionnaire);
                    }

                    let recipe = ErasedConfigRecipe::from_handler(handler);

                    if self.pending_commands.borrow().contains_key(&path) {
                        return Err(SetupError::DuplicateCommand(path.clone()));
                    }

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

    pub fn command_with<H, T, C>(
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
        self.register_struct_config(path, configure(CommandConfig::new(handler)))
    }

    fn register_struct_config<H, T>(
        mut self,
        path: &str,
        mut config: CommandConfig<H>,
    ) -> Result<Self, SetupError>
    where
        H: Handler<Output = T> + 'static,
        T: Serialize + 'static,
    {
        let template = if let Some(absence) = config.template_absence {
            TemplateRef::Absent(absence)
        } else if let Some(name) = config.template_name.take() {
            TemplateRef::Named(name)
        } else {
            TemplateRef::convention(path)
        };

        if let Some(hooks) = config.hooks.take() {
            self.register_command_hooks(path, hooks, HookRegistrationSource::CommandConfig)?;
        }
        if let Some(questionnaire) = config.questionnaire.take() {
            self.questionnaire_commands
                .insert(path.to_string(), questionnaire);
        }

        let mut recipe = StructRecipe::new(config.handler);
        if let Some(projection) = config.structured_output_projection {
            recipe = recipe.with_structured_output_projection(projection);
        }
        if config.pageable {
            recipe = recipe.pageable();
        }

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

    pub fn hooks(mut self, path: &str, hooks: Hooks) -> Self {
        if let Err(error) =
            self.register_command_hooks(path, hooks, HookRegistrationSource::AppBuilderHooks)
        {
            self.setup_errors.push(error);
        }
        self
    }

    pub(super) fn register_command_hooks(
        &mut self,
        path: &str,
        hooks: Hooks,
        source: HookRegistrationSource,
    ) -> Result<(), SetupError> {
        let phases: Vec<_> = hooks.phases().collect();
        if phases.is_empty() {
            return Ok(());
        }

        for phase in &phases {
            let key = (path.to_string(), *phase);
            if let Some(existing_source) = self.hook_phase_sources.get(&key) {
                if *existing_source != source {
                    return Err(SetupError::Config(format!(
                        "command `{path}` registers {phase} hooks through both CommandConfig and AppBuilder::hooks; keep each (path, phase) in one registration path"
                    )));
                }
            }
        }

        for phase in phases {
            self.hook_phase_sources
                .insert((path.to_string(), phase), source);
        }

        let (key, hooks) = match self.command_hooks.remove_entry(path) {
            Some((key, existing)) => (key, existing.append(hooks)),
            None => (path.to_string(), hooks),
        };
        self.command_hooks.insert(key, hooks);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EmbeddedTemplates;

    const TEMPLATES: &[(&str, &str)] = &[
        ("migrate-2", "{{ done }}"),
        ("db/migrate", "Migrated {{ count }} tables"),
        ("list-2", "{{ ok }}"),
        ("list", "Items: {{ items }}"),
        ("version", "{{ v }}"),
        ("list-3", "Items: {{ items | length }}"),
    ];

    use crate::cli::handler::FnHandler;
    use crate::cli::handler::Output as HandlerOutput;
    use crate::Representation;
    use clap::Command;

    #[test]
    fn test_command_registration() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"items": ["a", "b"]})))),
                |cfg| cfg,
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
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"items": ["a", "b"]})))),
                move |cfg| {
                    cfg.template_name("list-3").pre_dispatch(move |_, _| {
                        counter_clone.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    })
                },
            )
            .unwrap();
        let app = builder.build().unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = app.dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Items: 2"));
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_command_config_and_builder_hooks_same_phase_errors() {
        use crate::cli::hooks::Hooks;
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"ok": true})))),
                |cfg| cfg.template_name("list-2").pre_dispatch(|_, _| Ok(())),
            )
            .unwrap()
            .hooks("list", Hooks::new().pre_dispatch(|_, _| Ok(())));

        let error = match builder.build() {
            Ok(_) => panic!("expected duplicate hook registration to fail"),
            Err(error) => error.to_string(),
        };

        assert!(error.contains("command `list`"));
        assert!(error.contains("pre-dispatch"));
        assert!(error.contains("CommandConfig"));
        assert!(error.contains("AppBuilder::hooks"));
    }

    #[test]
    fn test_builder_and_command_config_hooks_same_phase_errors_in_either_order() {
        use crate::cli::hooks::{Hooks, RenderedOutput};
        use serde_json::json;

        let error = match AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .hooks("list", Hooks::new().post_output(|_, _, output| Ok(output)))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"ok": true})))),
                |cfg| {
                    cfg.template_name("list-2")
                        .post_output(|_, _, output: RenderedOutput| Ok(output))
                },
            ) {
            Ok(_) => panic!("expected duplicate hook registration to fail"),
            Err(error) => error.to_string(),
        };

        assert!(error.contains("command `list`"));
        assert!(error.contains("post-output"));
    }

    #[test]
    fn test_builder_and_command_config_hooks_different_phases_are_combined() {
        use crate::cli::hooks::{Hooks, RenderedOutput};
        use serde_json::json;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let calls = Arc::new(AtomicUsize::new(0));
        let pre_calls = calls.clone();
        let post_calls = calls.clone();

        let app = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .hooks(
                "list",
                Hooks::new().post_output(move |_, _, output: RenderedOutput| {
                    post_calls.fetch_add(1, Ordering::SeqCst);
                    Ok(output)
                }),
            )
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"ok": true})))),
                move |cfg| {
                    cfg.template_name("list-2").pre_dispatch(move |_, _| {
                        pre_calls.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    })
                },
            )
            .unwrap()
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = app.dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("true"));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_commands_and_builder_hooks_same_phase_errors_in_either_order() {
        use crate::cli::hooks::{Hooks, RenderedOutput};
        use serde_json::json;

        let error = match AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .hooks("list", Hooks::new().pre_dispatch(|_, _| Ok(())))
            .commands(|g| {
                g.command_with(
                    "list",
                    |_m, _ctx| Ok(HandlerOutput::Render(json!({"ok": true}))),
                    |cfg| cfg.template_name("list-2").pre_dispatch(|_, _| Ok(())),
                )
            }) {
            Ok(_) => panic!("expected duplicate hook registration to fail"),
            Err(error) => error.to_string(),
        };

        assert!(error.contains("command `list`"));
        assert!(error.contains("pre-dispatch"));
        assert!(error.contains("CommandConfig"));
        assert!(error.contains("AppBuilder::hooks"));

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .commands(|g| {
                g.command_with(
                    "list",
                    |_m, _ctx| Ok(HandlerOutput::Render(json!({"ok": true}))),
                    |cfg| {
                        cfg.template_name("list-2")
                            .post_output(|_, _, output: RenderedOutput| Ok(output))
                    },
                )
            })
            .unwrap()
            .hooks("list", Hooks::new().post_output(|_, _, output| Ok(output)));

        let error = match builder.build() {
            Ok(_) => panic!("expected duplicate hook registration to fail"),
            Err(error) => error.to_string(),
        };

        assert!(error.contains("command `list`"));
        assert!(error.contains("post-output"));
    }

    #[test]
    fn test_commands_and_builder_hooks_different_phases_are_combined() {
        use crate::cli::hooks::{Hooks, RenderedOutput};
        use serde_json::json;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let calls = Arc::new(AtomicUsize::new(0));
        let pre_calls = calls.clone();
        let post_calls = calls.clone();

        let app = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .hooks(
                "list",
                Hooks::new().post_output(move |_, _, output: RenderedOutput| {
                    post_calls.fetch_add(1, Ordering::SeqCst);
                    Ok(output)
                }),
            )
            .commands(|g| {
                g.command_with(
                    "list",
                    |_m, _ctx| Ok(HandlerOutput::Render(json!({"ok": true}))),
                    move |cfg| {
                        cfg.template_name("list-2").pre_dispatch(move |_, _| {
                            pre_calls.fetch_add(1, Ordering::SeqCst);
                            Ok(())
                        })
                    },
                )
            })
            .unwrap()
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = app.dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("true"));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_group_basic() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .commands(|__g| {
                __g.group("db", |g| {
                    g.command_with(
                        "migrate",
                        |_m, _ctx| Ok(HandlerOutput::Render(json!({"status": "migrated"}))),
                        |cfg| cfg.structured_only(),
                    )
                    .command_with(
                        "backup",
                        |_m, _ctx| Ok(HandlerOutput::Render(json!({"status": "backed_up"}))),
                        |cfg| cfg.structured_only(),
                    )
                })
            })
            .unwrap();
        let app = builder.build().unwrap();

        let cmd =
            Command::new("app").subcommand(Command::new("db").subcommand(Command::new("migrate")));

        let matches = cmd.try_get_matches_from(["app", "db", "migrate"]).unwrap();
        let result = app.dispatch(matches, Representation::Json);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("migrated"));
    }

    #[test]
    fn test_group_nested() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .commands(|__g| {
                __g.group("app", |g| {
                    g.command_with(
                        "start",
                        |_m, _ctx| Ok(HandlerOutput::Render(json!({"action": "start"}))),
                        |cfg| cfg.structured_only(),
                    )
                    .group("config", |g| {
                        g.command_with(
                            "get",
                            |_m, _ctx| Ok(HandlerOutput::Render(json!({"value": "test_value"}))),
                            |cfg| cfg.structured_only(),
                        )
                        .command_with(
                            "set",
                            |_m, _ctx| Ok(HandlerOutput::Render(json!({"ok": true}))),
                            |cfg| cfg.structured_only(),
                        )
                    })
                })
            })
            .unwrap();
        let app = builder.build().unwrap();

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
        let result = app.dispatch(matches, Representation::Json);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("test_value"));
    }

    #[test]
    fn test_group_with_template() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .commands(|__g| {
                __g.group("db", |g| {
                    g.command_with(
                        "migrate",
                        |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 5}))),
                        |cfg| cfg,
                    )
                })
            })
            .unwrap();
        let app = builder.build().unwrap();

        let cmd =
            Command::new("app").subcommand(Command::new("db").subcommand(Command::new("migrate")));

        let matches = cmd.try_get_matches_from(["app", "db", "migrate"]).unwrap();
        let result = app.dispatch(matches, Representation::Human);

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
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .commands(|__g| {
                __g.group("db", |g| {
                    g.command_with(
                        "migrate",
                        |_m, _ctx| Ok(HandlerOutput::Render(json!({"done": true}))),
                        move |cfg| {
                            cfg.template_name("migrate-2").pre_dispatch(move |_, _| {
                                hook_called_clone.store(true, Ordering::SeqCst);
                                Ok(())
                            })
                        },
                    )
                })
            })
            .unwrap();
        let app = builder.build().unwrap();

        let cmd =
            Command::new("app").subcommand(Command::new("db").subcommand(Command::new("migrate")));

        let matches = cmd.try_get_matches_from(["app", "db", "migrate"]).unwrap();
        let result = app.dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert!(hook_called.load(Ordering::SeqCst));
    }

    #[test]
    fn test_multiple_groups() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .commands(|__g| {
                __g.group("db", |g| {
                    g.command("migrate", |_m, _ctx| {
                        Ok(HandlerOutput::Render(json!({"type": "db"})))
                    })
                })
            })
            .unwrap()
            .commands(|__g| {
                __g.group("cache", |g| {
                    g.command("clear", |_m, _ctx| {
                        Ok(HandlerOutput::Render(json!({"type": "cache"})))
                    })
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
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "version",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"v": "1.0.0"})))),
                |cfg| cfg,
            )
            .unwrap()
            .commands(|__g| {
                __g.group("db", |g| {
                    g.command("migrate", |_m, _ctx| {
                        Ok(HandlerOutput::Render(json!({"ok": true})))
                    })
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
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_passthrough("init-sh", move |_m, _ctx| {
                called_clone.store(true, Ordering::SeqCst);
                Ok(())
            })
            .unwrap();

        assert!(builder.has_command("init-sh"));

        let cmd = Command::new("app").subcommand(Command::new("init-sh"));
        let matches = cmd.try_get_matches_from(["app", "init-sh"]).unwrap();
        let app = builder.build().unwrap();
        let result = app.dispatch(matches, Representation::Human);

        assert!(called.load(Ordering::SeqCst));
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
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .commands(|__g| {
                __g.group("shell", |g| {
                    g.passthrough("init", move |_m, _ctx| {
                        called_clone.store(true, Ordering::SeqCst);
                        Ok(())
                    })
                })
            })
            .unwrap();

        assert!(builder.has_command("shell.init"));

        let cmd =
            Command::new("app").subcommand(Command::new("shell").subcommand(Command::new("init")));
        let matches = cmd.try_get_matches_from(["app", "shell", "init"]).unwrap();
        let app = builder.build().unwrap();
        let result = app.dispatch(matches, Representation::Human);

        assert!(called.load(Ordering::SeqCst));
        assert!(result.is_handled());
    }
}
