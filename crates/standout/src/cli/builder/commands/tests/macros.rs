use crate::cli::builder::{test_support::EXECUTION_TEMPLATES as TEMPLATES, AppBuilder};
use crate::cli::handler::Output as HandlerOutput;
use crate::EmbeddedTemplates;
use crate::Representation;
use clap::Command;

#[test]
fn test_dispatch_macro_simple() {
    use crate::dispatch;
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .commands(dispatch! {
            list => {
                handler: |_m, _ctx| Ok(HandlerOutput::Render(json!({"items": ["a", "b"]}))),
                structured_only: true,
            }
        })
        .unwrap();

    assert!(builder.has_command("list"));

    let cmd = Command::new("app").subcommand(Command::new("list"));
    let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
    let result = builder
        .build()
        .unwrap()
        .dispatch(matches, Representation::Json);

    assert!(result.is_handled());
    let output = result.output().unwrap();
    assert!(output.contains("items"));
}

#[test]
fn test_dispatch_macro_with_groups() {
    use crate::dispatch;
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .commands(dispatch! {
            db: {
                migrate => {
                    handler: |_m, _ctx| Ok(HandlerOutput::Render(json!({"migrated": true}))),
                    structured_only: true,
                },
                backup => {
                    handler: |_m, _ctx| Ok(HandlerOutput::Render(json!({"backed_up": true}))),
                    structured_only: true,
                },
            },
            version => {
                handler: |_m, _ctx| Ok(HandlerOutput::Render(json!({"v": "1.0"}))),
                structured_only: true,
            },
        })
        .unwrap();

    assert!(builder.has_command("db.migrate"));
    assert!(builder.has_command("db.backup"));
    assert!(builder.has_command("version"));

    let cmd = Command::new("app")
        .subcommand(
            Command::new("db")
                .subcommand(Command::new("migrate"))
                .subcommand(Command::new("backup")),
        )
        .subcommand(Command::new("version"));

    let matches = cmd
        .clone()
        .try_get_matches_from(["app", "db", "migrate"])
        .unwrap();
    let result = builder
        .build()
        .unwrap()
        .dispatch(matches, Representation::Json);
    assert!(result.is_handled());
    assert!(result.output().unwrap().contains("migrated"));
}

#[test]
fn test_dispatch_macro_with_template() {
    use crate::dispatch;
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(crate::EmbeddedTemplates::new(
            &[("list", "Count: {{ count }}")],
            "",
        ))
        .commands(dispatch! {
            list => {
                handler: |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 42}))),
                template_name: "list",
            }
        })
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("list"));
    let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
    let result = builder
        .build()
        .unwrap()
        .dispatch(matches, Representation::Human);

    assert!(result.is_handled());
    assert_eq!(result.output(), Some("Count: 42"));
}

#[test]
fn test_dispatch_macro_with_hooks() {
    use crate::dispatch;
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let hook_called = Arc::new(AtomicBool::new(false));
    let hook_called_clone = hook_called.clone();

    let builder = AppBuilder::new()
        .templates(crate::EmbeddedTemplates::new(&[("list", "{{ ok }}")], ""))
        .commands(dispatch! {
            list => {
                handler: |_m, _ctx| Ok(HandlerOutput::Render(json!({"ok": true}))),
                template_name: "list",
                pre_dispatch: move |_, _| {
                    hook_called_clone.store(true, Ordering::SeqCst);
                    Ok(())
                },
            }
        })
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("list"));
    let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
    let result = builder
        .build()
        .unwrap()
        .dispatch(matches, Representation::Human);

    assert!(result.is_handled());
    assert!(hook_called.load(Ordering::SeqCst));
}

#[test]
fn test_dispatch_macro_deeply_nested() {
    use crate::dispatch;
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .commands(dispatch! {
            app: {
                config: {
                    get => |_m, _ctx| Ok(HandlerOutput::Render(json!({"key": "value"}))),
                    set => |_m, _ctx| Ok(HandlerOutput::Render(json!({"ok": true}))),
                },
                start => |_m, _ctx| Ok(HandlerOutput::Render(json!({"started": true}))),
            },
        })
        .unwrap();

    assert!(builder.has_command("app.config.get"));
    assert!(builder.has_command("app.config.set"));
    assert!(builder.has_command("app.start"));
}
