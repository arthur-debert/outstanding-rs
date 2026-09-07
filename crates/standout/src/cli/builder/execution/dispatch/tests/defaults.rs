use crate::cli::builder::{test_support::EXECUTION_TEMPLATES as TEMPLATES, AppBuilder};
use crate::cli::handler::{FnHandler, Output as HandlerOutput};
use crate::EmbeddedTemplates;
use clap::Command;

#[test]
fn test_default_command_builder() {
    let builder = AppBuilder::new().default_command("list");

    assert_eq!(builder.default_command, Some("list".to_string()));
}

#[test]
fn test_default_command_naked_invocation() {
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .default_command("list")
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"items": ["a", "b"]})))),
            |cfg| cfg.template_name("list-3"),
        )
        .unwrap()
        .command_with(
            "add",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"added": true})))),
            |cfg| cfg,
        )
        .unwrap();

    let cmd = Command::new("app")
        .subcommand(Command::new("list"))
        .subcommand(Command::new("add"));

    let result = builder.build().unwrap().run_with(
        cmd,
        ["app"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );
    assert!(result.is_handled());
    assert_eq!(result.output(), Some("Items: [\"a\", \"b\"]"));
}

#[test]
fn test_default_command_with_options() {
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .default_command("list")
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 42})))),
            |cfg| cfg,
        )
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("list"));

    let result = builder.build().unwrap().run_with(
        cmd,
        ["app", "--output=json"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );
    assert!(result.is_handled());
    let output = result.output().unwrap();
    assert!(output.contains("\"count\": 42"));
}

#[test]
fn test_default_command_explicit_command_overrides() {
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .default_command("list")
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"cmd": "list"})))),
            |cfg| cfg.template_name("list-9"),
        )
        .unwrap()
        .command_with(
            "add",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"cmd": "add"})))),
            |cfg| cfg.template_name("add-2"),
        )
        .unwrap();

    let cmd = Command::new("app")
        .subcommand(Command::new("list"))
        .subcommand(Command::new("add"));

    let result = builder.build().unwrap().run_with(
        cmd,
        ["app", "add"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );
    assert!(result.is_handled());
    assert_eq!(result.output(), Some("add"));
}

#[test]
fn test_default_command_no_default_set() {
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"items": []})))),
            |cfg| cfg.template_name("list-3"),
        )
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("list"));

    let result = builder.build().unwrap().run_with(
        cmd,
        ["app"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );
    assert!(!result.is_handled());
}
