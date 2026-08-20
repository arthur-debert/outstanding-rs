//! Compound artifacts: destination policy, write-then-report ordering, and
//! the typed failure path.
//!
//! These tests pin the transaction Standout owns for an artifact command: the
//! application produces bytes and facts, the framework picks the destination,
//! writes, and only then renders a report that can name where the bytes landed.

use clap::{Arg, Command};
use serde::Serialize;
use serde_json::json;
use standout::cli::{
    App, ArtifactDestination, DispatchResult as RunResult, ExitStatus, HandlerResult, HookError,
    Hooks, Output, OutputKind, RunErrorKind,
};
use standout::cli::{Artifact, RenderedOutput};

const BYTES: &[u8] = b"id,title\n1,buy milk\n";

/// The report a handler owns: domain facts, including its own warnings.
#[derive(Serialize)]
struct ExportReport {
    entries: usize,
    warnings: Vec<String>,
}

fn report() -> ExportReport {
    ExportReport {
        entries: 1,
        warnings: vec!["1 entry had no due date".into()],
    }
}

fn command() -> Command {
    Command::new("app").subcommand(Command::new("export")).arg(
        Arg::new("_ignored")
            .long("ignored")
            .global(true)
            .required(false),
    )
}

/// The canonical report template: it can only say this because the write
/// already happened.
const TEMPLATE: &str =
    "Wrote {{ report.entries }} entries to {{ receipt.destination }}{% for w in report.warnings %}\nwarning: {{ w }}{% endfor %}";

fn app_with(artifact: impl Fn() -> Artifact<ExportReport> + 'static) -> App {
    App::builder()
        .output_file_flag(Some("output-file-path"))
        .command(
            "export",
            move |_matches, _ctx| Ok(Output::Artifact(artifact())),
            TEMPLATE,
        )
        .unwrap()
        .build()
        .unwrap()
}

// ---------------------------------------------------------------------------
// Destination policy
// ---------------------------------------------------------------------------

#[test]
fn suggested_destination_is_written_and_reported() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.csv");
    let suggested = path.clone();

    let result = app_with(move || {
        Artifact::new(BYTES.to_vec())
            .suggest_destination(&suggested)
            .with_report(report())
    })
    .run_to_string(command(), ["app", "export"]);

    // The write happened, byte-for-byte.
    assert_eq!(std::fs::read(&path).unwrap(), BYTES);

    let run = result.artifact().expect("artifact run");
    assert_eq!(run.bytes(), BYTES);
    assert_eq!(run.suggested_destination(), Some(path.as_path()));
    assert_eq!(run.destination(), &ArtifactDestination::File(path.clone()));
    assert_eq!(run.receipt().byte_count(), BYTES.len());
    assert_eq!(result.exit_status(), Some(ExitStatus::SUCCESS));

    // The report names the destination that actually completed, and the
    // application's warning survived as a typed fact.
    let report = run.report().unwrap();
    assert!(report.contains(&format!("Wrote 1 entries to {}", path.display())));
    assert!(report.contains("warning: 1 entry had no due date"));
}

#[test]
fn explicit_override_wins_over_the_suggested_destination() {
    let dir = tempfile::tempdir().unwrap();
    let suggested = dir.path().join("suggested.csv");
    let override_path = dir.path().join("override.csv");
    let suggestion = suggested.clone();

    let result = app_with(move || {
        Artifact::new(BYTES.to_vec())
            .suggest_destination(&suggestion)
            .with_report(report())
    })
    .run_to_string(
        command(),
        [
            "app",
            "export",
            "--output-file-path",
            override_path.to_str().unwrap(),
        ],
    );

    assert_eq!(std::fs::read(&override_path).unwrap(), BYTES);
    assert!(!suggested.exists(), "the suggestion must not be written");

    let run = result.artifact().expect("artifact run");
    // The suggestion is still observable — it just didn't win.
    assert_eq!(run.suggested_destination(), Some(suggested.as_path()));
    assert_eq!(
        run.destination(),
        &ArtifactDestination::File(override_path.clone())
    );
    assert!(run
        .report()
        .unwrap()
        .contains(&format!("Wrote 1 entries to {}", override_path.display())));
}

#[test]
fn stdout_fallback_requires_the_opt_in() {
    let result = app_with(|| Artifact::new(BYTES.to_vec()).with_report(report()))
        .run_to_string(command(), ["app", "export"]);

    // No override, no suggestion, no stdout opt-in: a typed failure rather
    // than an invented file or silently discarded bytes.
    assert_eq!(
        result.error_kind(),
        Some(RunErrorKind::FinalWrite(OutputKind::Artifact))
    );
    assert_eq!(result.exit_status(), Some(ExitStatus::FAILURE));
    assert!(result.error().unwrap().contains("no destination selected"));
    assert!(result.artifact().is_none(), "no success report is produced");
}

#[test]
fn opted_in_stdout_is_the_last_resort_destination() {
    let result = app_with(|| {
        Artifact::new(BYTES.to_vec())
            .allow_stdout()
            .with_report(report())
    })
    .run_to_string(command(), ["app", "export"]);

    let run = result.artifact().expect("artifact run");
    assert_eq!(run.destination(), &ArtifactDestination::Stdout);
    assert_eq!(run.suggested_destination(), None);
    assert_eq!(run.bytes(), BYTES);
    // `-` is the destination label a stdout artifact reports.
    assert!(run.report().unwrap().contains("Wrote 1 entries to -"));
}

#[test]
fn an_override_writes_an_artifact_that_only_allows_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.csv");

    let result = app_with(|| {
        Artifact::new(BYTES.to_vec())
            .allow_stdout()
            .with_report(report())
    })
    .run_to_string(
        command(),
        [
            "app",
            "export",
            "--output-file-path",
            path.to_str().unwrap(),
        ],
    );

    assert_eq!(std::fs::read(&path).unwrap(), BYTES);
    assert_eq!(
        result.artifact().unwrap().destination(),
        &ArtifactDestination::File(path)
    );
}

// ---------------------------------------------------------------------------
// Write-then-report ordering and the failure path
// ---------------------------------------------------------------------------

#[test]
fn a_failed_write_is_typed_and_emits_no_report() {
    let dir = tempfile::tempdir().unwrap();
    let unwritable = dir.path().join("missing-dir").join("export.csv");
    let target = unwritable.clone();

    let result = app_with(move || {
        Artifact::new(BYTES.to_vec())
            .suggest_destination(&target)
            .with_report(report())
    })
    .run_to_string(command(), ["app", "export"]);

    assert_eq!(
        result.error_kind(),
        Some(RunErrorKind::FinalWrite(OutputKind::Artifact))
    );
    assert_eq!(result.exit_status(), Some(ExitStatus::FAILURE));
    assert!(result.error().unwrap().contains("Error writing artifact"));
    // The whole point: no success survived a failed write.
    assert!(result.artifact().is_none());
    assert!(!unwritable.exists());
}

#[test]
fn an_overridden_write_failure_shares_the_artifact_failure_path() {
    let dir = tempfile::tempdir().unwrap();
    let unwritable = dir.path().join("missing-dir").join("export.csv");
    let suggested = dir.path().join("fine.csv");
    let suggestion = suggested.clone();

    let result = app_with(move || {
        Artifact::new(BYTES.to_vec())
            .suggest_destination(&suggestion)
            .with_report(report())
    })
    .run_to_string(
        command(),
        [
            "app",
            "export",
            "--output-file-path",
            unwritable.to_str().unwrap(),
        ],
    );

    assert_eq!(
        result.error_kind(),
        Some(RunErrorKind::FinalWrite(OutputKind::Artifact))
    );
    assert!(
        !suggested.exists(),
        "a failed override must not fall back to the suggestion"
    );
}

// ---------------------------------------------------------------------------
// Empty and report-free outcomes
// ---------------------------------------------------------------------------

#[test]
fn an_artifact_without_a_report_completes_silently() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.bin");
    let target = path.clone();

    let result = App::builder()
        .command(
            "export",
            move |_matches, _ctx| -> HandlerResult<ExportReport> {
                Ok(Output::Artifact(
                    Artifact::new(Vec::new()).suggest_destination(&target),
                ))
            },
            TEMPLATE,
        )
        .unwrap()
        .build()
        .unwrap()
        .run_to_string(command(), ["app", "export"]);

    let run = result.artifact().expect("artifact run");
    // An empty artifact is still a real, written file — not a fabrication.
    assert_eq!(std::fs::read(&path).unwrap(), Vec::<u8>::new());
    assert_eq!(run.receipt().byte_count(), 0);
    assert_eq!(run.report(), None);
    assert_eq!(result.exit_status(), Some(ExitStatus::SUCCESS));
}

#[test]
fn a_silent_handler_still_fabricates_no_file() {
    let result = App::builder()
        .output_file_flag(Some("output-file-path"))
        .command(
            "export",
            |_matches, _ctx| -> HandlerResult<ExportReport> { Ok(Output::Silent) },
            TEMPLATE,
        )
        .unwrap()
        .build()
        .unwrap()
        .run_to_string(command(), ["app", "export"]);

    assert!(result.artifact().is_none());
    assert_eq!(result.output(), Some(""));
}

// ---------------------------------------------------------------------------
// Structured modes
// ---------------------------------------------------------------------------

#[test]
fn structured_mode_serializes_the_report_and_receipt_envelope() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.csv");
    let target = path.clone();

    let result = app_with(move || {
        Artifact::new(BYTES.to_vec())
            .suggest_destination(&target)
            .with_report(report())
    })
    .run_to_string(command(), ["app", "export", "--output=json"]);

    let report: serde_json::Value =
        serde_json::from_str(result.artifact().unwrap().report().unwrap()).unwrap();

    assert_eq!(
        report,
        json!({
            "report": {
                "entries": 1,
                "warnings": ["1 entry had no due date"],
            },
            "receipt": {
                "destination": path.display().to_string(),
                "stdout": false,
                "byte_count": BYTES.len(),
            }
        })
    );
}

// ---------------------------------------------------------------------------
// Hook coherence
// ---------------------------------------------------------------------------

#[test]
fn post_dispatch_hooks_see_the_report_like_any_handler_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.csv");
    let target = path.clone();

    let result = App::builder()
        .command(
            "export",
            move |_matches, _ctx| {
                Ok(Output::Artifact(
                    Artifact::new(BYTES.to_vec())
                        .suggest_destination(&target)
                        .with_report(report()),
                ))
            },
            TEMPLATE,
        )
        .unwrap()
        .hooks(
            "export",
            Hooks::new().post_dispatch(|_matches, _ctx, mut data| {
                data["entries"] = json!(42);
                Ok(data)
            }),
        )
        .build()
        .unwrap()
        .run_to_string(command(), ["app", "export"]);

    assert!(result
        .artifact()
        .unwrap()
        .report()
        .unwrap()
        .contains("Wrote 42 entries"));
}

#[test]
fn post_output_hooks_transform_bytes_before_the_framework_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.csv");
    let target = path.clone();

    let result = App::builder()
        .command(
            "export",
            move |_matches, _ctx| {
                Ok(Output::Artifact(
                    Artifact::new(BYTES.to_vec())
                        .suggest_destination(&target)
                        .with_report(report()),
                ))
            },
            TEMPLATE,
        )
        .unwrap()
        .hooks(
            "export",
            Hooks::new().post_output(|_matches, _ctx, mut output| {
                let artifact = output.as_artifact_mut().expect("artifact output");
                artifact.bytes = b"replaced".to_vec();
                Ok(output)
            }),
        )
        .build()
        .unwrap()
        .run_to_string(command(), ["app", "export"]);

    // The hook changed the bytes; the framework still owns the write, and the
    // receipt counts what was actually written.
    assert_eq!(std::fs::read(&path).unwrap(), b"replaced");
    assert_eq!(result.artifact().unwrap().receipt().byte_count(), 8);
}

#[test]
fn a_failing_post_output_hook_prevents_the_write() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.csv");
    let target = path.clone();

    let result = App::builder()
        .command(
            "export",
            move |_matches, _ctx| {
                Ok(Output::Artifact(
                    Artifact::new(BYTES.to_vec())
                        .suggest_destination(&target)
                        .with_report(report()),
                ))
            },
            TEMPLATE,
        )
        .unwrap()
        .hooks(
            "export",
            Hooks::new().post_output(|_matches, _ctx, _output| Err(HookError::post_output("no"))),
        )
        .build()
        .unwrap()
        .run_to_string(command(), ["app", "export"]);

    assert!(result.is_error());
    assert!(!path.exists(), "an aborted run must not write the artifact");
}

// ---------------------------------------------------------------------------
// Binary compatibility
// ---------------------------------------------------------------------------

#[test]
fn a_binary_filename_still_authorizes_no_write() {
    let dir = tempfile::tempdir().unwrap();
    let cwd_guard = dir.path().join("data.bin");

    let result = App::builder()
        .output_file_flag(Some("output-file-path"))
        .command_with(
            "export",
            |_matches, _ctx| -> HandlerResult<ExportReport> {
                Ok(Output::Binary {
                    data: BYTES.to_vec(),
                    filename: "data.bin".into(),
                })
            },
            |config| config.binary(),
        )
        .unwrap()
        .build()
        .unwrap()
        .run_to_string(command(), ["app", "export"]);

    // Unchanged 7.x behavior: bytes come back for the caller to place, and the
    // suggested filename touched nothing on disk.
    assert_eq!(result.binary(), Some((BYTES, "data.bin")));
    assert!(!cwd_guard.exists());
    assert!(!std::path::Path::new("data.bin").exists());
}

#[test]
fn binary_output_still_honors_the_explicit_override() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.bin");

    let result = App::builder()
        .output_file_flag(Some("output-file-path"))
        .command_with(
            "export",
            |_matches, _ctx| -> HandlerResult<ExportReport> {
                Ok(Output::Binary {
                    data: BYTES.to_vec(),
                    filename: "data.bin".into(),
                })
            },
            |config| config.binary(),
        )
        .unwrap()
        .build()
        .unwrap()
        .run_to_string(
            command(),
            [
                "app",
                "export",
                "--output-file-path",
                path.to_str().unwrap(),
            ],
        );

    assert_eq!(std::fs::read(&path).unwrap(), BYTES);
    assert_eq!(result.output(), Some(""));
    assert!(result.artifact().is_none());
}

// ---------------------------------------------------------------------------
// Partial adoption
// ---------------------------------------------------------------------------

#[test]
fn run_command_hands_back_the_pending_artifact_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.csv");
    let target = path.clone();

    let app = App::builder().build().unwrap();
    let matches = command().try_get_matches_from(["app", "export"]).unwrap();

    let output = app
        .run_command(
            "export",
            &matches,
            |_matches, _ctx| {
                Ok(Output::Artifact(
                    Artifact::new(BYTES.to_vec())
                        .suggest_destination(&target)
                        .with_report(report()),
                ))
            },
            TEMPLATE,
        )
        .unwrap();

    let artifact = output.as_artifact().expect("artifact output");
    assert_eq!(artifact.bytes, BYTES);
    assert_eq!(artifact.suggested_destination, Some(path.clone()));
    assert_eq!(artifact.report.as_ref().unwrap()["entries"], json!(1));
    assert!(
        !path.exists(),
        "manual dispatch performs no framework-owned write"
    );
    assert!(matches!(output, RenderedOutput::Artifact(_)));
}

#[test]
fn no_match_still_falls_through_for_manual_dispatch() {
    let result = app_with(|| Artifact::new(BYTES.to_vec()).allow_stdout()).run_to_string(
        Command::new("app").subcommand(Command::new("other")),
        ["app"],
    );
    assert!(matches!(result.outcome(), RunResult::NoMatch(_)));
}
