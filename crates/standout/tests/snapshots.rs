use clap::Command;
use insta::{assert_json_snapshot, assert_snapshot};
use serde_json::json;
use standout::cli::{App, Output};
use standout::{AmbiguousWidth, ColorMode, IconMode, InputSources, OutputMode, TargetProperties};

#[test]
fn test_snapshots_term_output() {
    let app = App::builder()
        .command(
            "list",
            |_m, _ctx| {
                Ok(Output::Render(json!({
                    "items": ["apple", "banana", "cherry"],
                    "count": 3
                })))
            },
            "Items: {{ items }}\nCount: {{ count }}",
        )
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("list"));
    let result = app.run_with(
        cmd,
        ["app", "list", "--output=term"],
        snapshot_target(),
        InputSources::from_process(),
    );
    let output = result.output().unwrap();

    assert_snapshot!("term_list_output", output);
}

#[test]
fn test_snapshots_json_output() {
    let app = App::builder()
        .command(
            "list",
            |_m, _ctx| {
                Ok(Output::Render(json!({
                    "items": ["apple", "banana", "cherry"],
                    "count": 3
                })))
            },
            "Items: {{ items }}\nCount: {{ count }}",
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

    // Use assert_json_snapshot for semantic comparison
    // This normalizes key ordering, preventing spurious failures across platforms
    let json_value: serde_json::Value = serde_json::from_str(output).unwrap();
    assert_json_snapshot!("json_list_output", json_value);
}

#[test]
fn test_snapshots_error_handling() {
    let app = App::builder()
        .command_with(
            "fail",
            |_m, _ctx| -> standout::cli::HandlerResult<()> {
                Err(anyhow::anyhow!("Critical failure in operation"))
            },
            |config| config.silent(),
        )
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("fail"));
    let matches = cmd.try_get_matches_from(["app", "fail"]).unwrap();

    let result = app.dispatch(matches, OutputMode::Term);

    // Handler errors surface as RunResult::Error("Error: {message}").
    // Consumers should write this to stderr and exit non-zero.
    assert!(
        result.is_error(),
        "expected RunResult::Error, got {:?}",
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
