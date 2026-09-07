use crate::cli::builder::{test_support::EXECUTION_TEMPLATES as TEMPLATES, AppBuilder};
use crate::cli::handler::{FnHandler, Output as HandlerOutput};
use crate::EmbeddedTemplates;
use clap::Command;

#[test]
fn test_dispatch_with_output_file_flag() {
    use serde_json::json;
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("output.txt");
    let path_str = file_path.to_str().unwrap();

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 42})))),
            |cfg| cfg,
        )
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("list"));

    let result = builder.build().unwrap().run_with(
        cmd,
        ["app", "--output-file-path", path_str, "list"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );

    assert!(result.is_handled());
    assert_eq!(result.output(), Some(""));

    let content = std::fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "Count: 42");
}

#[test]
fn test_dispatch_with_custom_output_file_flag() {
    use serde_json::json;
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("out.txt");
    let path_str = file_path.to_str().unwrap();

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .output_file_flag(Some("save-to"))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 99})))),
            |cfg| cfg.template_name("list-4"),
        )
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("list"));

    let result = builder.build().unwrap().run_with(
        cmd,
        ["app", "--save-to", path_str, "list"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );

    assert!(result.is_handled());
    assert_eq!(result.output(), Some(""));

    let content = std::fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "99");
}

#[test]
fn test_dispatch_with_output_file_json_mode() {
    use serde_json::json;
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("output.json");
    let path_str = file_path.to_str().unwrap();

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "show",
            FnHandler::new(|_m, _ctx| {
                Ok(HandlerOutput::Render(json!({"name": "test", "count": 42})))
            }),
            |cfg| cfg,
        )
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("show"));

    let result = builder.build().unwrap().run_with(
        cmd,
        [
            "app",
            "--output",
            "json",
            "--output-file-path",
            path_str,
            "show",
        ],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );

    assert!(result.is_handled());
    assert_eq!(result.output(), Some(""));

    let content = std::fs::read_to_string(file_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["name"], "test");
    assert_eq!(parsed["count"], 42);
}

#[test]
fn test_dispatch_with_output_file_human_representation() {
    use serde_json::json;
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("output.txt");
    let path_str = file_path.to_str().unwrap();

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "show",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"name": "Alice"})))),
            |cfg| cfg.template_name("show-2"),
        )
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("show"));

    let result = builder.build().unwrap().run_with(
        cmd,
        ["app", "--output-file-path", path_str, "show"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );

    assert!(result.is_handled());
    assert_eq!(result.output(), Some(""));

    let content = std::fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "Hello Alice");
}

#[test]
fn test_dispatch_without_output_file_flag() {
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .no_output_file_flag()
        .command_with(
            "show",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 42})))),
            |cfg| cfg.template_name("show-3"),
        )
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("show"));

    let result = builder.build().unwrap().run_with(
        cmd,
        ["app", "show"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );

    assert!(result.is_handled());
    assert!(result.output().unwrap().contains("Count: 42"));
}
