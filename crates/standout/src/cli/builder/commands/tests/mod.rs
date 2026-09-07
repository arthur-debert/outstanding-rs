mod groups;
mod hooks;
mod macros;

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
