use clap::Command;
use serde::Serialize;
use serde_json::json;
use standout::cli::{
    App, Artifact, CommandContextInput, Diagnostic, DiagnosticKind, ExitStatus, FnHandler,
    HandlerResult, Output, RunErrorKind, Severity,
};
use standout::ColorPolicy;
use standout::{EmbeddedTemplates, Representation};
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
    let result = TestHarness::new().output_mode(Representation::Ndjson).run(
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
    let result = TestHarness::new().output_mode(Representation::Ndjson).run(
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
    let result = TestHarness::new().output_mode(Representation::Ndjson).run(
        &app(),
        command(),
        ["app", "warn"],
    );
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

    let json = TestHarness::new().output_mode(Representation::Json).run(
        &app(),
        command(),
        ["app", "warn"],
    );
    json.assert_success();
    json.assert_stderr_contains("a soft warning");
    assert!(!json.stdout().contains("soft warning"), "{}", json.stdout());
}

#[test]
fn a_warning_entry_reads_back_as_a_warning_severity_diagnostic() {
    let result = TestHarness::new().output_mode(Representation::Ndjson).run(
        &app(),
        command(),
        ["app", "warn"],
    );
    let warning: Diagnostic =
        serde_json::from_str(result.stdout().lines().nth(1).unwrap()).unwrap();
    assert_eq!(warning.severity, Severity::Warning);
    assert_eq!(warning.kind, DiagnosticKind::Framework);
}

#[test]
fn the_stream_discards_under_json_and_text() {
    let json = TestHarness::new().output_mode(Representation::Json).run(
        &app(),
        command(),
        ["app", "stream"],
    );
    json.assert_success();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(json.stdout()).unwrap(),
        json!({ "applied": 1 })
    );

    let text =
        TestHarness::new()
            .color(ColorPolicy::Never)
            .run(&app(), command(), ["app", "stream"]);
    text.assert_success();
    text.assert_stdout_eq("1 applied");

    let silent = TestHarness::new().color(ColorPolicy::Never).run(
        &app(),
        command(),
        ["app", "silent-stream"],
    );
    silent.assert_success();
    assert!(silent.stdout_bytes().is_empty());
}

#[test]
fn a_silent_streaming_handler_leaves_only_its_entries() {
    let result = TestHarness::new().output_mode(Representation::Ndjson).run(
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
    let result = TestHarness::new().output_mode(Representation::Ndjson).run(
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

fn run_to_file(subcommand: &str) -> (standout_test::TestResult, String) {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("out.ndjson");
    let result = TestHarness::new().output_mode(Representation::Ndjson).run(
        &app(),
        command(),
        [
            "app".to_string(),
            subcommand.to_string(),
            format!("--output-file-path={}", path.display()),
        ],
    );
    let file = std::fs::read_to_string(&path).unwrap();
    (result, file)
}

#[test]
fn an_output_file_under_ndjson_takes_the_entries_and_the_result_and_stdout_stays_empty() {
    let (result, file) = run_to_file("stream");
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
    let (result, file) = run_to_file("fail-mid-stream");
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
    let (result, file) = run_to_file("warn");
    result.assert_success();
    result.assert_stderr_empty();
    assert_eq!(result.stdout_bytes(), b"", "{}", result.stdout());
    let entries = lines(&file);
    assert_eq!(entries.len(), 2, "{file}");
    assert_eq!(entries[0]["type"], "result");
    assert_eq!(entries[1]["severity"], "warning");
    assert_eq!(entries[1]["summary"], "a soft warning");
}

fn assert_payload_is_a_render_error(result: &standout_test::TestResult) {
    result.assert_error_kind(RunErrorKind::Render);
    result.assert_exit_status(ExitStatus::FAILURE);
    result.assert_stderr_empty();
}

fn assert_entries_are_the_version_then_the_render_error(
    entries: &[serde_json::Value],
    payload: &str,
) {
    assert_eq!(entries.len(), 2, "{payload}: {entries:?}");
    assert_eq!(entries[0]["type"], "version");
    assert_eq!(entries[1]["type"], "diagnostic");
    assert_eq!(entries[1]["kind"], "render");
    let summary = entries[1]["summary"].as_str().unwrap_or_default();
    assert!(
        summary.contains(&format!("{payload} output was produced under ndjson")),
        "{payload}: {summary}"
    );
}

#[test]
fn binary_and_artifact_output_under_ndjson_are_render_errors_after_the_entries() {
    for payload in ["binary", "artifact"] {
        let result = TestHarness::new().output_mode(Representation::Ndjson).run(
            &app(),
            command(),
            ["app", payload],
        );
        assert_payload_is_a_render_error(&result);
        assert_eq!(result.expect_diagnostic().kind, DiagnosticKind::Render);
        assert_entries_are_the_version_then_the_render_error(&lines(result.stdout()), payload);
    }
}

#[test]
fn binary_and_artifact_output_under_ndjson_with_an_output_file_leave_only_the_stream_in_it() {
    for payload in ["binary", "artifact"] {
        let (result, file) = run_to_file(payload);
        assert_payload_is_a_render_error(&result);
        assert_eq!(result.stdout_bytes(), b"", "{payload}: {}", result.stdout());
        assert_entries_are_the_version_then_the_render_error(&lines(&file), payload);
    }
}

#[test]
fn binary_and_artifact_output_under_the_single_document_modes_are_untouched() {
    let binary = TestHarness::new().output_mode(Representation::Json).run(
        &app(),
        command(),
        ["app", "binary"],
    );
    binary.assert_success();
    assert_eq!(binary.stdout_bytes(), [0, 1, 2]);
}
