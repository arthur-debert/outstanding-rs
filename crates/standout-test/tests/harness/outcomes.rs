use super::*;

#[test]
#[serial]
fn harness_exposes_typed_clap_and_handler_outcomes() {
    let app = build_echo_app("echo");
    let help = TestHarness::new().run(&app, echo_command(), ["app", "--help"]);
    help.assert_success();
    help.assert_exit_status(ExitStatus::SUCCESS);
    assert_eq!(help.success_kind(), Some(SuccessKind::ClapHelp));
    let usage = TestHarness::new().run(&app, echo_command(), ["app", "--unknown"]);
    usage.assert_error();
    usage.assert_exit_status(ExitStatus::USAGE_ERROR);
    usage.assert_error_kind(RunErrorKind::ClapUsage);
    let failing = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "fail",
            FnHandler::new(|_matches, _ctx| -> HandlerResult<serde_json::Value> {
                Err(std::io::Error::other("boom").into())
            }),
            |config| config.structured_only(),
        )
        .unwrap()
        .build()
        .unwrap();
    let failing_command = Command::new("app").subcommand(Command::new("fail"));
    let failure = TestHarness::new().run(&failing, failing_command, ["app", "fail"]);
    failure.assert_error();
    failure.assert_exit_status(ExitStatus::FAILURE);
    failure.assert_error_kind(RunErrorKind::Handler);
}
#[test]
#[serial]
fn harness_answers_a_version_declared_on_the_builder() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .version("4.5.6")
        .command_with(
            "echo",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({ "msg": "hi" })))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let result = TestHarness::new().run(&app, echo_command(), ["app", "--version"]);
    result.assert_success();
    result.assert_exit_status(ExitStatus::SUCCESS);
    assert_eq!(result.success_kind(), Some(SuccessKind::ClapVersion));
    result.assert_stdout_contains("4.5.6");
}
#[test]
#[serial]
fn harness_exposes_external_failure_payload_status_and_origin() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "external",
            FnHandler::new(|_matches, _ctx| -> HandlerResult<serde_json::Value> {
                Err(
                    ExternalFailure::new(128, "fatal: delegated command failed\n")
                        .unwrap()
                        .into(),
                )
            }),
            |config| config.structured_only(),
        )
        .unwrap()
        .command_with(
            "external-pre",
            FnHandler::new(|_matches, _ctx| {
                Ok(Output::Render(json!({ "message": "unreachable" })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .hooks(
            "external-pre",
            Hooks::new().pre_dispatch(|_, _| {
                Err(HookError::pre_dispatch_external(
                    ExternalFailure::new(128, "fatal: pre-dispatch failed\n").unwrap(),
                ))
            }),
        )
        .build()
        .unwrap();
    let command = Command::new("app")
        .subcommand(Command::new("external"))
        .subcommand(Command::new("external-pre"));
    let handler = TestHarness::new().run(&app, command.clone(), ["app", "external"]);
    handler.assert_error();
    handler.assert_exit_status(ExternalFailure::new(128, "").unwrap().exit_status());
    handler.assert_error_kind(RunErrorKind::External);
    assert_eq!(handler.error(), Some("fatal: delegated command failed\n"));
    handler.assert_stdout_eq("");
    drop(handler);
    let pre_dispatch = TestHarness::new().run(&app, command, ["app", "external-pre"]);
    pre_dispatch.assert_error();
    pre_dispatch.assert_exit_status(ExternalFailure::new(128, "").unwrap().exit_status());
    pre_dispatch.assert_error_kind(RunErrorKind::External);
    assert_eq!(pre_dispatch.error(), Some("fatal: pre-dispatch failed\n"));
    pre_dispatch.assert_stdout_eq("");
}
#[test]
#[serial]
fn no_match_reports_cleanly() {
    let app = build_echo_app("echo");
    let result = TestHarness::new().run(&app, echo_command(), vec!["app", "unknown"]);
    assert!(
        result.is_error() || result.is_no_match(),
        "expected Error or NoMatch, got: {:?}",
        result.outcome()
    );
}
const ARTIFACT_BYTES: &[u8] = b"id,title\n1,buy milk\n";
fn build_export_app(destination: Option<std::path::PathBuf>) -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .output_file_flag(Some("output-file-path"))
        .command_with(
            "export",
            FnHandler::new(move |_m, _ctx| {
                let mut artifact = Artifact::new(ARTIFACT_BYTES.to_vec())
                    .with_report(json!({ "entries": 1, "warnings": ["no due date"] }));
                artifact = match &destination {
                    Some(path) => artifact.suggest_destination(path),
                    None => artifact.allow_stdout(),
                };
                Ok(Output::Artifact(artifact))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap()
}
fn export_command() -> Command {
    Command::new("app").subcommand(Command::new("export"))
}
#[test]
#[serial]
fn harness_asserts_bytes_destinations_receipt_and_report() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.csv");
    let app = build_export_app(Some(path.clone()));
    let result = TestHarness::new().run(&app, export_command(), ["app", "export"]);
    result.assert_success();
    result.assert_exit_status(ExitStatus::SUCCESS);
    result.assert_artifact_bytes(ARTIFACT_BYTES);
    result.assert_artifact_suggested_destination(&path);
    result.assert_artifact_written_to(&path);
    result.assert_artifact_report_contains("Wrote 1 entries to");
    result.assert_artifact_report_contains(&path.display().to_string());
    assert_eq!(result.artifact_bytes(), Some(ARTIFACT_BYTES));
    assert_eq!(
        result.artifact().unwrap().receipt().byte_count(),
        ARTIFACT_BYTES.len()
    );
    assert_eq!(std::fs::read(&path).unwrap(), ARTIFACT_BYTES);
}
#[test]
#[serial]
fn harness_asserts_the_stdout_artifact_destination() {
    let app = build_export_app(None);
    let result = TestHarness::new().run(&app, export_command(), ["app", "export"]);
    result.assert_success();
    result.assert_artifact_to_stdout();
    result.assert_artifact_report_contains("Wrote 1 entries to -");
    assert!(result.artifact_destination().unwrap().is_stdout());
}
#[test]
#[serial]
fn harness_asserts_the_report_data_in_structured_mode() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.csv");
    let app = build_export_app(Some(path.clone()));
    let result = TestHarness::new().output_mode(Representation::Json).run(
        &app,
        export_command(),
        ["app", "export"],
    );
    let report: serde_json::Value =
        serde_json::from_str(result.artifact_report().unwrap()).unwrap();
    assert_eq!(report["report"]["entries"], json!(1));
    assert_eq!(report["report"]["warnings"][0], json!("no due date"));
    assert_eq!(
        report["receipt"]["destination"],
        json!(path.display().to_string())
    );
    assert_eq!(report["receipt"]["stdout"], json!(false));
}
#[test]
#[serial]
fn harness_asserts_a_typed_artifact_write_failure() {
    let dir = tempfile::tempdir().unwrap();
    let unwritable = dir.path().join("missing").join("export.csv");
    let app = build_export_app(Some(unwritable));
    let result = TestHarness::new().run(&app, export_command(), ["app", "export"]);
    result.assert_error();
    result.assert_error_kind(RunErrorKind::FinalWrite(OutputKind::Artifact));
    result.assert_exit_status(ExitStatus::FAILURE);
    result.assert_error_contains("Error writing artifact");
    assert!(
        result.artifact().is_none(),
        "a failed write produces no report"
    );
}
