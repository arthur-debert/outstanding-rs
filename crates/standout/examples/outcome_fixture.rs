use clap::Command;
use serde_json::json;
use standout::cli::{App, Artifact, ExternalFailure, HandlerResult, HookError, Hooks, Output};

const ARTIFACT_PATH_ENV: &str = "STANDOUT_FIXTURE_ARTIFACT_PATH";

const ARTIFACT_TEMPLATE: &str = "wrote {{ report.entries }} entries to {{ receipt.destination }}";

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
        .subcommand(Command::new("artifact"))
        .subcommand(Command::new("artifact-stdout"))
        .subcommand(Command::new("artifact-no-destination"))
}

fn app() -> App {
    App::builder()
        .command(
            "ok",
            |_, _| Ok(Output::Render(json!({ "message": "ok" }))),
            "{{ message }}",
        )
        .unwrap()
        .command_with(
            "fail",
            |_, _| -> HandlerResult<serde_json::Value> {
                Err(anyhow::anyhow!("fixture handler failed"))
            },
            |config| config.structured_only(),
        )
        .unwrap()
        .command_with(
            "silent",
            |_, _| -> HandlerResult<()> { Ok(Output::Silent) },
            |config| config.silent(),
        )
        .unwrap()
        .command_with(
            "binary",
            |_, _| -> HandlerResult<()> {
                Ok(Output::Binary {
                    data: vec![0, 1, 2],
                    filename: "fixture.bin".into(),
                })
            },
            |config| config.binary(),
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
        .command_with(
            "binary-huge",
            |_, _| -> HandlerResult<()> {
                Ok(Output::Binary {
                    data: vec![7; 1024 * 1024],
                    filename: "fixture.bin".into(),
                })
            },
            |config| config.binary(),
        )
        .unwrap()
        .command(
            "warn-ok",
            |_, ctx| {
                use standout::cli::CommandContextInput;
                ctx.warn("fixture warning");
                Ok(Output::Render(json!({ "message": "ok" })))
            },
            "{{ message }}",
        )
        .unwrap()
        .command_with(
            "warn-fail",
            |_, ctx| -> HandlerResult<serde_json::Value> {
                use standout::cli::CommandContextInput;
                ctx.warn("fixture warning");
                Err(anyhow::anyhow!("fixture handler failed"))
            },
            |config| config.structured_only(),
        )
        .unwrap()
        .command_with(
            "external",
            |_, _| -> HandlerResult<serde_json::Value> {
                Err(ExternalFailure::new(128, "fatal: external fixture failed")
                    .unwrap()
                    .into())
            },
            |config| config.structured_only(),
        )
        .unwrap()
        .command(
            "external-pre",
            |_, _| Ok(Output::Render(json!({ "message": "unreachable" }))),
            "{{ message }}",
        )
        .unwrap()
        .command(
            "artifact",
            |_, _| {
                Ok(Output::Artifact(
                    Artifact::new(vec![0, 1, 2])
                        .suggest_destination(std::env::var(ARTIFACT_PATH_ENV).unwrap())
                        .with_report(json!({ "entries": 3 })),
                ))
            },
            ARTIFACT_TEMPLATE,
        )
        .unwrap()
        .command(
            "artifact-stdout",
            |_, _| {
                Ok(Output::Artifact(
                    Artifact::new(vec![0, 1, 2])
                        .allow_stdout()
                        .with_report(json!({ "entries": 3 })),
                ))
            },
            ARTIFACT_TEMPLATE,
        )
        .unwrap()
        .command(
            "artifact-no-destination",
            |_, _| {
                Ok(Output::Artifact(
                    Artifact::new(vec![0, 1, 2]).with_report(json!({ "entries": 3 })),
                ))
            },
            ARTIFACT_TEMPLATE,
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
