use clap::Command;
use serde::Serialize;
use serde_json::json;
use standout::cli::{
    App, Artifact, CommandContextInput, Diagnostic, DiagnosticKind, ExitStatus, FnHandler,
    HandlerResult, Output, RunErrorKind, Severity,
};
use standout::{EmbeddedTemplates, OutputMode};
use standout_test::TestHarness;

const TEMPLATES: &[(&str, &str)] = &[
    ("stream", "{{ applied }} applied"),
    ("warn", "{{ ok }}"),
    ("artifact", "wrote {{ report.entries }} entries"),
];

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Entry<'a> {
    Version { format_version: u32 },
    ApplyStart { resource: &'a str },
    ApplyComplete { resource: &'a str },
}

fn command() -> Command {
    Command::new("app")
        .subcommand(Command::new("stream"))
        .subcommand(Command::new("fail-mid-stream"))
        .subcommand(Command::new("warn"))
        .subcommand(Command::new("silent-stream"))
        .subcommand(Command::new("binary"))
        .subcommand(Command::new("artifact"))
}

fn app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "stream",
            FnHandler::new(|_, ctx| -> HandlerResult<serde_json::Value> {
                let stream = ctx.stream();
                stream.emit(&Entry::Version { format_version: 1 })?;
                stream.emit(&Entry::ApplyStart { resource: "web" })?;
                stream.emit(&Entry::ApplyComplete { resource: "web" })?;
                Ok(Output::Render(json!({ "applied": 1 })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .command_with(
            "fail-mid-stream",
            FnHandler::new(|_, ctx| -> HandlerResult<serde_json::Value> {
                let stream = ctx.stream();
                stream.emit(&Entry::Version { format_version: 1 })?;
                stream.emit(&Entry::ApplyStart { resource: "web" })?;
                Err(Diagnostic::error("web: refused")
                    .detail("the resource refuses every apply")
                    .into())
            }),
            |cfg| cfg.structured_only(),
        )
        .unwrap()
        .command_with(
            "warn",
            FnHandler::new(|_, ctx| -> HandlerResult<serde_json::Value> {
                ctx.warn("a soft warning");
                Ok(Output::Render(json!({ "ok": true })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .command_with(
            "silent-stream",
            FnHandler::new(|_, ctx| -> HandlerResult<()> {
                ctx.stream().emit(&Entry::Version { format_version: 1 })?;
                Ok(Output::Silent)
            }),
            |cfg| cfg.silent(),
        )
        .unwrap()
        .command_with(
            "binary",
            FnHandler::new(|_, ctx| -> HandlerResult<()> {
                ctx.stream().emit(&Entry::Version { format_version: 1 })?;
                ctx.warn("a soft warning");
                Ok(Output::Binary {
                    data: vec![0, 1, 2],
                    filename: "out.bin".into(),
                })
            }),
            |cfg| cfg.binary(),
        )
        .unwrap()
        .command_with(
            "artifact",
            FnHandler::new(|_, ctx| {
                ctx.stream().emit(&Entry::Version { format_version: 1 })?;
                ctx.warn("a soft warning");
                Ok(Output::Artifact(
                    Artifact::new(vec![0, 1, 2]).with_report(json!({ "entries": 3 })),
                ))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap()
}

fn lines(stdout: &str) -> Vec<serde_json::Value> {
    stdout
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("{line:?}: {e}")))
        .collect()
}

#[test]
fn a_handler_streams_three_entries_then_its_result_as_a_result_entry() {
    let result = TestHarness::new().output_mode(OutputMode::Ndjson).run(
        &app(),
        command(),
        ["app", "stream"],
    );
    result.assert_success();
    result.assert_stderr_empty();
    assert_eq!(
        result.stdout_bytes(),
        b"{\"type\":\"version\",\"format_version\":1}\n\
          {\"type\":\"apply_start\",\"resource\":\"web\"}\n\
          {\"type\":\"apply_complete\",\"resource\":\"web\"}\n\
          {\"type\":\"result\",\"data\":{\"applied\":1}}\n"
    );
}

#[test]
fn a_failure_mid_stream_is_a_diagnostic_entry_after_the_emitted_lines() {
    let result = TestHarness::new().output_mode(OutputMode::Ndjson).run(
        &app(),
        command(),
        ["app", "fail-mid-stream"],
    );
    result.assert_error_kind(RunErrorKind::Handler);
    result.assert_exit_status(ExitStatus::FAILURE);
    result.assert_stderr_empty();
    let entries = lines(result.stdout());
    assert_eq!(entries.len(), 3, "{}", result.stdout());
    assert_eq!(entries[0]["type"], "version");
    assert_eq!(entries[1]["type"], "apply_start");
    assert_eq!(
        entries[2],
        json!({
            "type": "diagnostic",
            "schema_version": 1,
            "severity": "error",
            "kind": "handler",
            "summary": "web: refused",
            "detail": "the resource refuses every apply",
        })
    );
    let diagnostic = result.expect_diagnostic();
    assert_eq!(diagnostic.kind, DiagnosticKind::Handler);
    assert_eq!(diagnostic.summary, "web: refused");
}

#[test]
fn a_warning_is_a_warning_entry_on_stdout_after_the_result() {
    let result =
        TestHarness::new()
            .output_mode(OutputMode::Ndjson)
            .run(&app(), command(), ["app", "warn"]);
    result.assert_success();
    result.assert_stderr_empty();
    let entries = lines(result.stdout());
    assert_eq!(entries.len(), 2, "{}", result.stdout());
    assert_eq!(
        entries[0],
        json!({ "type": "result", "data": { "ok": true } })
    );
    assert_eq!(
        entries[1],
        json!({
            "type": "diagnostic",
            "schema_version": 1,
            "severity": "warning",
            "kind": "framework",
            "summary": "a soft warning",
            "detail": "",
        })
    );
    assert_eq!(result.warnings(), ["a soft warning"]);
    assert!(
        result.diagnostic().is_none(),
        "a warning is not the run's failure"
    );

    let json =
        TestHarness::new()
            .output_mode(OutputMode::Json)
            .run(&app(), command(), ["app", "warn"]);
    json.assert_success();
    json.assert_stderr_contains("a soft warning");
    assert!(!json.stdout().contains("soft warning"), "{}", json.stdout());
}

#[test]
fn a_warning_entry_reads_back_as_a_warning_severity_diagnostic() {
    let result =
        TestHarness::new()
            .output_mode(OutputMode::Ndjson)
            .run(&app(), command(), ["app", "warn"]);
    let warning: Diagnostic =
        serde_json::from_str(result.stdout().lines().nth(1).unwrap()).unwrap();
    assert_eq!(warning.severity, Severity::Warning);
    assert_eq!(warning.kind, DiagnosticKind::Framework);
}

#[test]
fn the_stream_discards_under_json_and_text() {
    let json =
        TestHarness::new()
            .output_mode(OutputMode::Json)
            .run(&app(), command(), ["app", "stream"]);
    json.assert_success();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(json.stdout()).unwrap(),
        json!({ "applied": 1 })
    );

    let text =
        TestHarness::new()
            .output_mode(OutputMode::Text)
            .run(&app(), command(), ["app", "stream"]);
    text.assert_success();
    text.assert_stdout_eq("1 applied");

    let silent = TestHarness::new().output_mode(OutputMode::Text).run(
        &app(),
        command(),
        ["app", "silent-stream"],
    );
    silent.assert_success();
    assert!(silent.stdout_bytes().is_empty());
}

#[test]
fn a_silent_streaming_handler_leaves_only_its_entries() {
    let result = TestHarness::new().output_mode(OutputMode::Ndjson).run(
        &app(),
        command(),
        ["app", "silent-stream"],
    );
    result.assert_success();
    assert_eq!(
        result.stdout_bytes(),
        b"{\"type\":\"version\",\"format_version\":1}\n"
    );
}

#[test]
fn a_usage_error_under_ndjson_is_a_diagnostic_line_exiting_two() {
    let result = TestHarness::new().output_mode(OutputMode::Ndjson).run(
        &app(),
        command(),
        ["app", "stream", "--bogus"],
    );
    result.assert_exit_status(ExitStatus::USAGE_ERROR);
    result.assert_stderr_empty();
    let entries = lines(result.stdout());
    assert_eq!(entries.len(), 1, "{}", result.stdout());
    assert_eq!(result.expect_diagnostic().kind, DiagnosticKind::ClapUsage);
}

fn run_to_file(subcommand: &str) -> (standout_test::TestResult, Vec<u8>) {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("out.ndjson");
    let result = TestHarness::new().output_mode(OutputMode::Ndjson).run(
        &app(),
        command(),
        [
            "app".to_string(),
            subcommand.to_string(),
            format!("--output-file-path={}", path.display()),
        ],
    );
    let file = std::fs::read(&path).unwrap();
    (result, file)
}

fn stream_file(subcommand: &str) -> (standout_test::TestResult, String) {
    let (result, file) = run_to_file(subcommand);
    (result, String::from_utf8(file).unwrap())
}

#[test]
fn an_output_file_under_ndjson_takes_the_entries_and_the_result_and_stdout_stays_empty() {
    let (result, file) = stream_file("stream");
    result.assert_success();
    result.assert_stderr_empty();
    assert_eq!(result.stdout_bytes(), b"", "{}", result.stdout());
    assert_eq!(
        file,
        "{\"type\":\"version\",\"format_version\":1}\n\
         {\"type\":\"apply_start\",\"resource\":\"web\"}\n\
         {\"type\":\"apply_complete\",\"resource\":\"web\"}\n\
         {\"type\":\"result\",\"data\":{\"applied\":1}}\n"
    );
}

#[test]
fn an_output_file_under_ndjson_takes_the_diagnostic_after_the_entries() {
    let (result, file) = stream_file("fail-mid-stream");
    result.assert_error_kind(RunErrorKind::Handler);
    result.assert_stderr_empty();
    assert_eq!(result.stdout_bytes(), b"", "{}", result.stdout());
    let entries = lines(&file);
    assert_eq!(entries.len(), 3, "{file}");
    assert_eq!(entries[1]["type"], "apply_start");
    assert_eq!(entries[2]["type"], "diagnostic");
    assert_eq!(entries[2]["severity"], "error");
}

#[test]
fn an_output_file_under_ndjson_takes_the_warning_entries_too() {
    let (result, file) = stream_file("warn");
    result.assert_success();
    result.assert_stderr_empty();
    assert_eq!(result.stdout_bytes(), b"", "{}", result.stdout());
    let entries = lines(&file);
    assert_eq!(entries.len(), 2, "{file}");
    assert_eq!(entries[0]["type"], "result");
    assert_eq!(entries[1]["severity"], "warning");
    assert_eq!(entries[1]["summary"], "a soft warning");
}

#[test]
fn a_binary_payload_takes_the_output_file_and_the_stream_stays_on_stdout() {
    let (result, file) = run_to_file("binary");
    result.assert_success();
    result.assert_stderr_empty();
    assert_eq!(file, [0, 1, 2]);
    let entries = lines(result.stdout());
    assert_eq!(entries.len(), 2, "{}", result.stdout());
    assert_eq!(entries[0]["type"], "version");
    assert_eq!(entries[1]["severity"], "warning");
    assert_eq!(entries[1]["summary"], "a soft warning");
}

#[test]
fn an_artifact_payload_takes_the_output_file_and_its_report_follows_the_entries_on_stdout() {
    let (result, file) = run_to_file("artifact");
    result.assert_success();
    result.assert_stderr_empty();
    assert_eq!(file, [0, 1, 2]);
    let entries = lines(result.stdout());
    assert_eq!(entries.len(), 3, "{}", result.stdout());
    assert_eq!(entries[0]["type"], "version");
    assert_eq!(entries[1]["type"], "result");
    assert_eq!(entries[1]["data"]["report"]["entries"], 3);
    assert_eq!(entries[1]["data"]["receipt"]["byte_count"], 3);
    assert_eq!(entries[2]["severity"], "warning");
    assert_eq!(entries[2]["summary"], "a soft warning");
}
