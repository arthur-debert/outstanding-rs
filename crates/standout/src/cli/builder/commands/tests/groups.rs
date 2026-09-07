use super::*;

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
