use clap::Command;
use insta::assert_snapshot;
use serde_json::json;
use standout::cli::FnHandler;
use standout::cli::{App, Output};
use standout::ColorPolicy;
use standout::EmbeddedTemplates;
use standout::{
    AmbiguousWidth, ColorMode, IconMode, InputSources, Representation, TargetProperties,
};

const TEMPLATES: &[(&str, &str)] = &[("list", "Items: {{ items }}\nCount: {{ count }}")];

#[test]
fn test_snapshots_term_output() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| {
                Ok(Output::Render(json!({
                    "items": ["apple", "banana", "cherry"],
                    "count": 3
                })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("list"));
    let result = app.run_with_color(
        cmd,
        ["app", "list"],
        snapshot_target(),
        ColorPolicy::Always,
        InputSources::from_process(),
    );
    let output = result.output().unwrap();

    assert_snapshot!("term_list_output", output);
}

#[test]
fn test_snapshots_json_output() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| {
                Ok(Output::Render(json!({
                    "items": ["apple", "banana", "cherry"],
                    "count": 3
                })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("list"));
    let result = app.run_with(
        cmd,
        ["app", "list", "--output=json"],
        snapshot_target(),
        InputSources::from_process(),
    );
    let output = result.output().unwrap();

    let json_value: serde_json::Value = serde_json::from_str(output).unwrap();
    assert_snapshot!(
        "json_list_output",
        serde_json::to_string_pretty(&json_value).unwrap()
    );
}

#[test]
fn test_snapshots_error_handling() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "fail",
            FnHandler::new(|_m, _ctx| -> standout::cli::HandlerResult<()> {
                Err(anyhow::anyhow!("Critical failure in operation"))
            }),
            |config| config.silent(),
        )
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("fail"));
    let matches = cmd.try_get_matches_from(["app", "fail"]).unwrap();

    let result = app.dispatch(matches, Representation::Human);

    assert!(
        result.is_error(),
        "expected DispatchResult::Error, got {:?}",
        result
    );
    let output = result.error().unwrap();
    assert_snapshot!("error_output", output);
}

fn snapshot_target() -> TargetProperties {
    TargetProperties {
        width: None,
        stdout_is_terminal: false,
        stderr_is_terminal: false,
        stdout_color_capability: false,
        stderr_color_capability: false,
        color_scheme: ColorMode::Dark,
        icon_mode: IconMode::Classic,
        ambiguous_width: AmbiguousWidth::Narrow,
    }
}
