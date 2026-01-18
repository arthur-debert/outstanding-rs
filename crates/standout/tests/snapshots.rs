#![cfg(feature = "clap")]

use clap::Command;
use insta::{assert_json_snapshot, assert_snapshot};
use serde_json::json;
use standout::cli::{App, Output};
use standout::OutputMode;

#[test]
fn test_snapshots_term_output() {
    let app = App::<standout::cli::ThreadSafe>::builder()
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
    let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();

    let result = app.dispatch(matches, OutputMode::Term);
    let output = result.output().unwrap();

    assert_snapshot!("term_list_output", output);
}

#[test]
fn test_snapshots_json_output() {
    let app = App::<standout::cli::ThreadSafe>::builder()
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
    let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();

    let result = app.dispatch(matches, OutputMode::Json);
    let output = result.output().unwrap();

    // Use assert_json_snapshot for semantic comparison
    // This normalizes key ordering, preventing spurious failures across platforms
    let json_value: serde_json::Value = serde_json::from_str(output).unwrap();
    assert_json_snapshot!("json_list_output", json_value);
}

#[test]
fn test_snapshots_error_handling() {
    let app = App::<standout::cli::ThreadSafe>::builder()
        .command(
            "fail",
            |_m, _ctx| -> standout::cli::HandlerResult<()> {
                Err(anyhow::anyhow!("Critical failure in operation"))
            },
            "",
        )
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("fail"));
    let matches = cmd.try_get_matches_from(["app", "fail"]).unwrap();

    let result = app.dispatch(matches, OutputMode::Term);

    // Handler errors are converted to "Error: {message}" in RunResult::Handled
    let output = result.output().unwrap();
    assert_snapshot!("error_output", output);
}
