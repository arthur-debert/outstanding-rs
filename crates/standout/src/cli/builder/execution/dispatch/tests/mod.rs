mod defaults;
mod hooks;
mod state;

use crate::cli::builder::{test_support::EXECUTION_TEMPLATES as TEMPLATES, AppBuilder};
use crate::cli::handler::{FnHandler, Output as HandlerOutput};
use crate::cli::hooks::Hooks;
use crate::EmbeddedTemplates;
use crate::Representation;
use clap::Command;

#[test]
fn test_dispatch_to_handler() {
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 42})))),
            |cfg| cfg,
        )
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
fn test_dispatch_unhandled_fallthrough() {
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({})))),
            |config| config.structured_only(),
        )
        .unwrap();

    let cmd = Command::new("app")
        .subcommand(Command::new("list"))
        .subcommand(Command::new("other"));

    let matches = cmd.try_get_matches_from(["app", "other"]).unwrap();
    let result = builder
        .build()
        .unwrap()
        .dispatch(matches, Representation::Human);

    assert!(!result.is_handled());
    assert!(result.matches().is_some());
}

#[test]
fn test_dispatch_json_output() {
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| {
                Ok(HandlerOutput::Render(json!({"name": "test", "value": 123})))
            }),
            |cfg| cfg.template_name("list-2"),
        )
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("list"));

    let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
    let result = builder
        .build()
        .unwrap()
        .dispatch(matches, Representation::Json);

    assert!(result.is_handled());
    let output = result.output().unwrap();
    assert!(output.contains("\"name\": \"test\""));
    assert!(output.contains("\"value\": 123"));
}

#[test]
fn test_dispatch_nested_command() {
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "config.get",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"key": "value"})))),
            |cfg| cfg,
        )
        .unwrap();

    let cmd =
        Command::new("app").subcommand(Command::new("config").subcommand(Command::new("get")));

    let matches = cmd.try_get_matches_from(["app", "config", "get"]).unwrap();
    let result = builder
        .build()
        .unwrap()
        .dispatch(matches, Representation::Human);

    assert!(result.is_handled());
    assert_eq!(result.output(), Some("value"));
}

#[test]
fn test_dispatch_silent_result() {
    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "quiet",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::<()>::Silent)),
            |config| config.silent(),
        )
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("quiet"));

    let matches = cmd.try_get_matches_from(["app", "quiet"]).unwrap();
    let result = builder
        .build()
        .unwrap()
        .dispatch(matches, Representation::Human);

    assert!(result.is_handled());
    assert_eq!(result.output(), Some(""));
}

#[test]
fn test_dispatch_error_result() {
    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "fail",
            FnHandler::new(|_m, _ctx| {
                Err::<HandlerOutput<()>, _>(anyhow::anyhow!("something went wrong"))
            }),
            |config| config.silent(),
        )
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("fail"));

    let matches = cmd.try_get_matches_from(["app", "fail"]).unwrap();
    let result = builder
        .build()
        .unwrap()
        .dispatch(matches, Representation::Human);

    assert!(result.is_error(), "expected Error, got {:?}", result);
    let msg = result.error().unwrap();
    assert!(msg.contains("Error:"));
    assert!(msg.contains("something went wrong"));
}

#[test]
fn test_dispatch_from_basic() {
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"items": ["a", "b"]})))),
            |cfg| cfg.template_name("list-3"),
        )
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("list"));

    let result = builder.build().unwrap().run_with(
        cmd,
        ["app", "list"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );

    assert!(result.is_handled());
    assert_eq!(result.output(), Some("Items: [\"a\", \"b\"]"));
}

#[test]
fn test_dispatch_from_with_json_flag() {
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 5})))),
            |cfg| cfg,
        )
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("list"));

    let result = builder.build().unwrap().run_with(
        cmd,
        ["app", "--output=json", "list"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );

    assert!(result.is_handled());
    let output = result.output().unwrap();
    assert!(output.contains("\"count\": 5"));
}

#[test]
fn test_dispatch_from_unhandled() {
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({})))),
            |config| config.structured_only(),
        )
        .unwrap();

    let cmd = Command::new("app")
        .subcommand(Command::new("list"))
        .subcommand(Command::new("other"));

    let result = builder.build().unwrap().run_with(
        cmd,
        ["app", "other"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );

    assert!(!result.is_handled());
}

#[test]
fn test_dispatch_post_dispatch_chain() {
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"value": 1})))),
            |cfg| cfg.template_name("list-8"),
        )
        .unwrap()
        .hooks(
            "list",
            Hooks::new()
                .post_dispatch(|_, _ctx, mut data| {
                    if let Some(v) = data.get_mut("value") {
                        *v = json!(v.as_i64().unwrap_or(0) * 2).into();
                    }
                    Ok(data)
                })
                .post_dispatch(|_, _ctx, mut data| {
                    if let Some(v) = data.get_mut("value") {
                        *v = json!(v.as_i64().unwrap_or(0) + 10).into();
                    }
                    Ok(data)
                }),
        );

    let cmd = Command::new("app").subcommand(Command::new("list"));

    let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
    let result = builder
        .build()
        .unwrap()
        .dispatch(matches, Representation::Human);

    assert!(result.is_handled());
    assert_eq!(result.output(), Some("12"));
}

#[test]
fn a_framework_flag_spelled_like_a_generated_one_is_a_setup_error() {
    let generated = |builder: AppBuilder| {
        builder
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .build()
            .unwrap()
            .verify_command(&Command::new("app").subcommand(Command::new("list")))
            .unwrap_err()
            .to_string()
    };

    let help = generated(AppBuilder::new().output_flag(Some("help")));
    assert!(
        help.contains("output_flag installs `--help`") && help.contains("`app`"),
        "expected the generated help flag named, got: {help}"
    );

    let version = generated(
        AppBuilder::new()
            .version("1.0.0")
            .color_flag(Some("version")),
    );
    assert!(
        version.contains("color_flag installs `--version`") && version.contains("`app`"),
        "expected the generated version flag named, got: {version}"
    );
}
