use clap::Command;
use serde::Serialize;
use serde_json::json;
use serial_test::serial;
use standout::cli::{
    App, AppFailure, CommandContextInput, Diagnostic, ExitStatus, ExternalFailure, FnHandler,
    HandlerResult, HookError, HookPhase, Hooks, Output, RunErrorKind, Severity,
};
use standout::{EmbeddedTemplates, OutputMode};
use standout_test::TestHarness;
use std::collections::HashMap;

const TEMPLATES: &[(&str, &str)] = &[
    ("ok", "{{ message }}"),
    ("hook-fail", "{{ message }}"),
    ("render-fail", "{{ message }}"),
];

#[derive(Serialize)]
struct Unserializable {
    map: HashMap<(u8, u8), u8>,
}

fn command() -> Command {
    Command::new("app")
        .subcommand(Command::new("ok"))
        .subcommand(Command::new("fail"))
        .subcommand(Command::new("warn-fail"))
        .subcommand(Command::new("ranged"))
        .subcommand(Command::new("hook-fail"))
        .subcommand(Command::new("render-fail"))
        .subcommand(Command::new("app-fail"))
        .subcommand(Command::new("external"))
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
                Err(anyhow::anyhow!("the handler refused"))
            }),
            |cfg| cfg.structured_only(),
        )
        .unwrap()
        .command_with(
            "warn-fail",
            FnHandler::new(|_, ctx| -> HandlerResult<serde_json::Value> {
                ctx.warn("a soft warning");
                Err(anyhow::anyhow!("the handler refused"))
            }),
            |cfg| cfg.structured_only(),
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
            |cfg| cfg.structured_only(),
        )
        .unwrap()
        .command_with(
            "hook-fail",
            FnHandler::new(|_, _| Ok(Output::Render(json!({ "message": "unreachable" })))),
            |cfg| cfg,
        )
        .unwrap()
        .hooks(
            "hook-fail",
            Hooks::new().pre_dispatch(|_, _| {
                Err(HookError::pre_dispatch(
                    "Configuration validation failed: reviewers: at least one reviewer must be enabled",
                ))
            }),
        )
        .command_with(
            "render-fail",
            FnHandler::new(|_, _| {
                let mut map = HashMap::new();
                map.insert((1, 2), 3);
                Ok(Output::Render(Unserializable { map }))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .command_with(
            "app-fail",
            FnHandler::new(|_, _| -> HandlerResult<serde_json::Value> {
                Err(AppFailure::new(3, "app: repository not found: demo/gamma\n")
                    .unwrap()
                    .into())
            }),
            |cfg| cfg.structured_only(),
        )
        .unwrap()
        .command_with(
            "external",
            FnHandler::new(|_, _| -> HandlerResult<serde_json::Value> {
                Err(ExternalFailure::new(128, "fatal: not a git repository\n")
                    .unwrap()
                    .into())
            }),
            |cfg| cfg.structured_only(),
        )
        .unwrap()
        .build()
        .unwrap()
}

fn run(args: &[&str]) -> standout_test::TestResult {
    TestHarness::new().run(&app(), command(), args)
}

#[test]
#[serial]
fn each_failure_class_is_a_json_document_on_stdout_with_stderr_empty() {
    let cases: [(&[&str], RunErrorKind, ExitStatus, &str); 5] = [
        (
            &["app", "--bogus", "--output", "json"],
            RunErrorKind::ClapUsage,
            ExitStatus::USAGE_ERROR,
            "unexpected argument '--bogus' found",
        ),
        (
            &["app", "fail", "--output", "json"],
            RunErrorKind::Handler,
            ExitStatus::FAILURE,
            "the handler refused",
        ),
        (
            &["app", "hook-fail", "--output", "json"],
            RunErrorKind::Hook(HookPhase::PreDispatch),
            ExitStatus::FAILURE,
            "Configuration validation failed: reviewers: at least one reviewer must be enabled",
        ),
        (
            &["app", "render-fail", "--output", "json"],
            RunErrorKind::Render,
            ExitStatus::FAILURE,
            "key must be a string",
        ),
        (
            &["app", "ranged", "--output", "json"],
            RunErrorKind::Handler,
            ExitStatus::FAILURE,
            "config line 2 does not parse",
        ),
    ];
    for (args, kind, status, summary) in cases {
        let result = run(args);
        result.assert_error_kind(kind);
        result.assert_exit_status(status);
        result.assert_stderr_empty();
        let document: serde_json::Value = serde_json::from_str(result.stdout())
            .unwrap_or_else(|e| panic!("{args:?}: stdout is not JSON ({e}):\n{}", result.stdout()));
        assert_eq!(document["type"], "diagnostic", "{args:?}");
        assert_eq!(document["schema_version"], 1, "{args:?}");
        assert_eq!(document["severity"], "error", "{args:?}");
        assert_eq!(document["kind"], kind.name(), "{args:?}");
        assert!(
            document["summary"].as_str().unwrap().contains(summary),
            "{args:?}: {document}"
        );
        let diagnostic = result.expect_diagnostic();
        assert_eq!(diagnostic.kind, kind);
        assert_eq!(diagnostic.severity, Severity::Error);
    }
}

#[test]
#[serial]
fn the_summary_is_the_error_text_without_the_prose_framing() {
    let handler = run(&["app", "fail", "--output", "json"]).expect_diagnostic();
    assert_eq!(handler.summary, "the handler refused");
    assert_eq!(handler.detail, "");
    assert_eq!(handler.range, None);

    let hook = run(&["app", "hook-fail", "--output", "json"]).expect_diagnostic();
    assert_eq!(
        hook.summary,
        "Configuration validation failed: reviewers: at least one reviewer must be enabled"
    );

    let usage = run(&["app", "--bogus", "--output", "json"]).expect_diagnostic();
    assert_eq!(usage.summary, "unexpected argument '--bogus' found");
    assert!(usage.detail.contains("Usage:"), "{}", usage.detail);
}

#[test]
#[serial]
fn a_handler_returned_diagnostic_keeps_its_detail_and_range() {
    for mode in [OutputMode::Json, OutputMode::Yaml, OutputMode::Csv] {
        let result = TestHarness::new()
            .output_mode(mode)
            .run(&app(), command(), ["app", "ranged"]);
        result.assert_error_kind(RunErrorKind::Handler);
        result.assert_stderr_empty();
        let diagnostic = result.expect_diagnostic();
        assert_eq!(diagnostic.summary, "config line 2 does not parse");
        assert_eq!(diagnostic.detail, "expected `resource <name> <state>`");
        let range = diagnostic.range.expect("the range survives");
        assert_eq!(range.filename, "main.tfl");
        assert_eq!(range.start.line, 2);
        assert_eq!(range.start.column, 1);
    }
}

#[test]
#[serial]
fn yaml_and_csv_carry_the_handler_error_as_their_own_document() {
    let yaml =
        TestHarness::new()
            .output_mode(OutputMode::Yaml)
            .run(&app(), command(), ["app", "fail"]);
    yaml.assert_stderr_empty();
    assert!(
        yaml.stdout().starts_with("type: diagnostic\n"),
        "{}",
        yaml.stdout()
    );
    assert!(
        yaml.stdout().contains("kind: handler\n"),
        "{}",
        yaml.stdout()
    );
    assert_eq!(yaml.expect_diagnostic().summary, "the handler refused");

    let csv =
        TestHarness::new()
            .output_mode(OutputMode::Csv)
            .run(&app(), command(), ["app", "fail"]);
    csv.assert_stderr_empty();
    assert_eq!(
        csv.stdout(),
        "type,schema_version,severity,kind,summary,detail,range_filename,range_line,range_column\n\
         diagnostic,1,error,handler,the handler refused,,,,\n"
    );
    assert_eq!(csv.expect_diagnostic().kind, RunErrorKind::Handler);

    let ranged =
        TestHarness::new()
            .output_mode(OutputMode::Csv)
            .run(&app(), command(), ["app", "ranged"]);
    assert!(
        ranged.stdout().ends_with(",main.tfl,2,1\n"),
        "{}",
        ranged.stdout()
    );
}

#[test]
#[serial]
fn the_argv_scan_finds_the_mode_on_either_side_of_a_usage_error() {
    for args in [
        &["app", "--output", "json", "--bogus"][..],
        &["app", "--bogus", "--output", "json"][..],
        &["app", "--bogus", "--output=json"][..],
        &["app", "--output", "text", "--bogus", "--output", "json"][..],
        &["app", "fail", "--bogus", "--output", "json"][..],
    ] {
        let result = run(args);
        result.assert_error_kind(RunErrorKind::ClapUsage);
        result.assert_exit_status(ExitStatus::USAGE_ERROR);
        assert!(result.stderr().is_empty(), "{args:?}: {}", result.stderr());
        assert_eq!(
            result.expect_diagnostic().kind,
            RunErrorKind::ClapUsage,
            "{args:?}"
        );
    }
}

#[test]
#[serial]
fn a_malformed_output_value_stays_a_prose_usage_error() {
    for args in [
        &["app", "--output", "jsn", "--bogus"][..],
        &["app", "--bogus", "--output", "jsn"][..],
        &["app", "fail", "--output", "jsn"][..],
    ] {
        let result = run(args);
        result.assert_error_kind(RunErrorKind::ClapUsage);
        result.assert_exit_status(ExitStatus::USAGE_ERROR);
        assert_eq!(result.stdout(), "", "{args:?}");
        assert!(
            result.stderr().starts_with("error:"),
            "{args:?}: {}",
            result.stderr()
        );
        assert_eq!(result.diagnostic(), None, "{args:?}");
    }
}

#[test]
#[serial]
fn human_modes_keep_prose_on_stderr_and_stdout_empty() {
    for mode in [
        OutputMode::Auto,
        OutputMode::Term,
        OutputMode::Text,
        OutputMode::TermDebug,
    ] {
        let result =
            TestHarness::new()
                .output_mode(mode)
                .run(&app(), command(), ["app", "hook-fail"]);
        result.assert_exit_status(ExitStatus::FAILURE);
        assert_eq!(result.stdout(), "", "{mode:?}");
        assert_eq!(
            result.stderr_plain(),
            "Error: hook error (pre-dispatch): Configuration validation failed: reviewers: at \
             least one reviewer must be enabled\n",
            "{mode:?}"
        );
        assert_eq!(result.diagnostic(), None, "{mode:?}");
    }
    let usage = run(&["app", "--bogus"]);
    assert_eq!(usage.stdout(), "");
    assert!(
        usage.stderr().starts_with("error: unexpected argument"),
        "{}",
        usage.stderr()
    );
}

#[test]
#[serial]
fn warnings_stay_prose_on_stderr_beside_the_stdout_document() {
    let result = run(&["app", "warn-fail", "--output", "json"]);
    result.assert_exit_status(ExitStatus::FAILURE);
    assert_eq!(result.expect_diagnostic().summary, "the handler refused");
    result.assert_stderr_contains("a soft warning");
    assert!(
        !result.stderr().contains("Error:"),
        "the failure itself must not reach stderr:\n{}",
        result.stderr()
    );
}

#[test]
#[serial]
fn owner_declared_failures_keep_their_bytes_and_add_the_document() {
    let app_failure = run(&["app", "app-fail", "--output", "json"]);
    assert_eq!(
        app_failure.exit_status(),
        Some(AppFailure::new(3, "").unwrap().exit_status())
    );
    assert_eq!(
        app_failure.stderr(),
        "app: repository not found: demo/gamma\n"
    );
    let diagnostic = app_failure.expect_diagnostic();
    assert_eq!(diagnostic.kind, RunErrorKind::App);
    assert_eq!(diagnostic.summary, "app: repository not found: demo/gamma");
    assert_eq!(diagnostic.detail, "app: repository not found: demo/gamma\n");

    let external = run(&["app", "external", "--output", "yaml"]);
    assert_eq!(external.stderr(), "fatal: not a git repository\n");
    let diagnostic = external.expect_diagnostic();
    assert_eq!(diagnostic.kind, RunErrorKind::External);
    assert_eq!(diagnostic.detail, "fatal: not a git repository\n");

    let human = run(&["app", "app-fail"]);
    assert_eq!(human.stdout(), "");
    assert_eq!(human.stderr(), "app: repository not found: demo/gamma\n");
}

#[test]
#[serial]
fn a_success_is_never_a_diagnostic() {
    let result = run(&["app", "ok", "--output", "json"]);
    result.assert_success();
    assert_eq!(result.diagnostic(), None);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(result.stdout()).unwrap()["message"],
        "ok"
    );
}
