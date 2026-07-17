use clap::Command;
use serde_json::json;
use standout::cli::{App, ExternalFailure, HandlerResult, HookError, Hooks, Output};

fn command() -> Command {
    Command::new("outcome-fixture")
        .version("1.2.3")
        .subcommand(Command::new("ok"))
        .subcommand(Command::new("fail"))
        .subcommand(Command::new("silent"))
        .subcommand(Command::new("binary"))
        .subcommand(Command::new("huge"))
        .subcommand(Command::new("binary-huge"))
        .subcommand(Command::new("warn-ok"))
        .subcommand(Command::new("warn-fail"))
        .subcommand(Command::new("external"))
        .subcommand(Command::new("external-pre"))
}

fn app() -> App {
    App::builder()
        .command(
            "ok",
            |_, _| Ok(Output::Render(json!({ "message": "ok" }))),
            "{{ message }}",
        )
        .unwrap()
        .command(
            "fail",
            |_, _| -> HandlerResult<serde_json::Value> {
                Err(anyhow::anyhow!("fixture handler failed"))
            },
            "",
        )
        .unwrap()
        .command(
            "silent",
            |_, _| -> HandlerResult<()> { Ok(Output::Silent) },
            "",
        )
        .unwrap()
        .command(
            "binary",
            |_, _| -> HandlerResult<()> {
                Ok(Output::Binary {
                    data: vec![0, 1, 2],
                    filename: "fixture.bin".into(),
                })
            },
            "",
        )
        .unwrap()
        .command(
            "huge",
            |_, _| {
                Ok(Output::Render(
                    json!({ "message": "x".repeat(1024 * 1024) }),
                ))
            },
            "{{ message }}",
        )
        .unwrap()
        .command(
            "binary-huge",
            |_, _| -> HandlerResult<()> {
                Ok(Output::Binary {
                    data: vec![7; 1024 * 1024],
                    filename: "fixture.bin".into(),
                })
            },
            "",
        )
        .unwrap()
        .command(
            "warn-ok",
            |_, _| {
                standout::warnings::push_warning("fixture warning");
                Ok(Output::Render(json!({ "message": "ok" })))
            },
            "{{ message }}",
        )
        .unwrap()
        .command(
            "warn-fail",
            |_, _| -> HandlerResult<serde_json::Value> {
                standout::warnings::push_warning("fixture warning");
                Err(anyhow::anyhow!("fixture handler failed"))
            },
            "",
        )
        .unwrap()
        .command(
            "external",
            |_, _| -> HandlerResult<serde_json::Value> {
                Err(ExternalFailure::new(128, "fatal: external fixture failed")
                    .unwrap()
                    .into())
            },
            "",
        )
        .unwrap()
        .command(
            "external-pre",
            |_, _| Ok(Output::Render(json!({ "message": "unreachable" }))),
            "{{ message }}",
        )
        .unwrap()
        .hooks(
            "external-pre",
            Hooks::new().pre_dispatch(|_, _| {
                Err(HookError::pre_dispatch_external(
                    ExternalFailure::new(128, "fatal: pre-dispatch fixture failed").unwrap(),
                ))
            }),
        )
        .build()
        .unwrap()
}

fn main() {
    app().run(command(), std::env::args());
}
