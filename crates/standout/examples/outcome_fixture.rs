use clap::Command;
use serde_json::json;
use standout::cli::FnHandler;
use standout::cli::{
    App, Artifact, Diagnostic, ExternalFailure, HandlerResult, HookError, Hooks, Output,
};
use standout::EmbeddedTemplates;

const TEMPLATES: &[(&str, &str)] = &[
    ("external-pre", "{{ message }}"),
    ("hook-fail", "{{ message }}"),
    ("render-fail", "{{ message }}"),
    ("ok", "{{ message }}"),
    ("warn-ok", "{{ message }}"),
    ("huge", "{{ message }}"),
    ("artifact", ARTIFACT_TEMPLATE),
    ("artifact-stdout", ARTIFACT_TEMPLATE),
    ("artifact-no-destination", ARTIFACT_TEMPLATE),
];

const ARTIFACT_PATH_ENV: &str = "STANDOUT_FIXTURE_ARTIFACT_PATH";
const EDGE_ENV: &str = "STANDOUT_FIXTURE_EDGE";
const OUTCOME_PATH_ENV: &str = "STANDOUT_FIXTURE_OUTCOME_PATH";

const ARTIFACT_TEMPLATE: &str = "wrote {{ report.entries }} entries to {{ receipt.destination }}";

#[derive(serde::Serialize)]
struct Unserializable {
    map: std::collections::HashMap<(u8, u8), u8>,
}

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
        .subcommand(Command::new("hook-fail"))
        .subcommand(Command::new("render-fail"))
        .subcommand(Command::new("ranged"))
        .subcommand(Command::new("artifact"))
        .subcommand(Command::new("artifact-stdout"))
        .subcommand(Command::new("artifact-no-destination"))
}

fn app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "ok",
            FnHandler::new(|_, _| Ok(Output::Render(json!({ "message": "ok" })))),
            |cfg| cfg,
        )
        .unwrap()
        .command_with(
            "fail",
            FnHandler::new(|_, _| -> HandlerResult<serde_json::Value> {
                Err(anyhow::anyhow!("fixture handler failed"))
            }),
            |config| config.structured_only(),
        )
        .unwrap()
        .command_with(
            "silent",
            FnHandler::new(|_, _| -> HandlerResult<()> { Ok(Output::Silent) }),
            |config| config.silent(),
        )
        .unwrap()
        .command_with(
            "binary",
            FnHandler::new(|_, _| -> HandlerResult<()> {
                Ok(Output::Binary {
                    data: vec![0, 1, 2],
                    filename: "fixture.bin".into(),
                })
            }),
            |config| config.binary(),
        )
        .unwrap()
        .command_with(
            "huge",
            FnHandler::new(|_, _| {
                Ok(Output::Render(
                    json!({ "message": "x".repeat(1024 * 1024) }),
                ))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .command_with(
            "binary-huge",
            FnHandler::new(|_, _| -> HandlerResult<()> {
                Ok(Output::Binary {
                    data: vec![7; 1024 * 1024],
                    filename: "fixture.bin".into(),
                })
            }),
            |config| config.binary(),
        )
        .unwrap()
        .command_with(
            "warn-ok",
            FnHandler::new(|_, ctx| {
                use standout::cli::CommandContextInput;
                ctx.warn("fixture warning");
                Ok(Output::Render(json!({ "message": "ok" })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .command_with(
            "warn-fail",
            FnHandler::new(|_, ctx| -> HandlerResult<serde_json::Value> {
                use standout::cli::CommandContextInput;
                ctx.warn("fixture warning");
                Err(anyhow::anyhow!("fixture handler failed"))
            }),
            |config| config.structured_only(),
        )
        .unwrap()
        .command_with(
            "external",
            FnHandler::new(|_, _| -> HandlerResult<serde_json::Value> {
                Err(ExternalFailure::new(128, "fatal: external fixture failed")
                    .unwrap()
                    .into())
            }),
            |config| config.structured_only(),
        )
        .unwrap()
        .command_with(
            "external-pre",
            FnHandler::new(|_, _| Ok(Output::Render(json!({ "message": "unreachable" })))),
            |cfg| cfg,
        )
        .unwrap()
        .command_with(
            "artifact",
            FnHandler::new(|_, _| {
                Ok(Output::Artifact(
                    Artifact::new(vec![0, 1, 2])
                        .suggest_destination(std::env::var(ARTIFACT_PATH_ENV).unwrap())
                        .with_report(json!({ "entries": 3 })),
                ))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .command_with(
            "artifact-stdout",
            FnHandler::new(|_, _| {
                Ok(Output::Artifact(
                    Artifact::new(vec![0, 1, 2])
                        .allow_stdout()
                        .with_report(json!({ "entries": 3 })),
                ))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .command_with(
            "artifact-no-destination",
            FnHandler::new(|_, _| {
                Ok(Output::Artifact(
                    Artifact::new(vec![0, 1, 2]).with_report(json!({ "entries": 3 })),
                ))
            }),
            |cfg| cfg,
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
        .command_with(
            "hook-fail",
            FnHandler::new(|_, _| Ok(Output::Render(json!({ "message": "unreachable" })))),
            |cfg| cfg,
        )
        .unwrap()
        .hooks(
            "hook-fail",
            Hooks::new().pre_dispatch(|_, _| Err(HookError::pre_dispatch("fixture hook failed"))),
        )
        .command_with(
            "render-fail",
            FnHandler::new(|_, _| {
                let mut map = std::collections::HashMap::new();
                map.insert((1u8, 2u8), 3u8);
                Ok(Output::Render(Unserializable { map }))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .command_with(
            "ranged",
            FnHandler::new(|_, _| -> HandlerResult<serde_json::Value> {
                Err(Diagnostic::error("config line 2 does not parse")
                    .detail("expected `resource <name> <state>`")
                    .range("main.tfl", 2, 1)
                    .into())
            }),
            |config| config.structured_only(),
        )
        .unwrap()
        .build()
        .unwrap()
}

fn main() {
    if std::env::var_os(EDGE_ENV).is_some_and(|edge| edge == "emitted") {
        let outcome = app().run_emitted(command(), std::env::args());
        std::fs::write(
            std::env::var_os(OUTCOME_PATH_ENV).unwrap(),
            format!(
                "handled={} status={}",
                outcome.handled,
                outcome.status.code()
            ),
        )
        .unwrap();
        std::process::exit(outcome.status.code().into());
    }
    app().run(command(), std::env::args());
}
