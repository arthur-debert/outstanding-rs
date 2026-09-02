//! The one place a completed run becomes bytes on stdout and stderr.
//!
//! `run_emitted` and `standout-test`'s harness both call [`emit_run_result`],
//! so what a test observes is what a process writes. The output mode decides
//! where a failure goes (spec `parity-machine-contract.md`, D2): under `json`,
//! `yaml`, `csv` and `ndjson` the failure is the stdout document, serialized
//! from [`RunError::diagnostic`], and stderr carries nothing the framework
//! wrote for it; under every human mode the failure is prose on stderr. An
//! `App` or `External` failure writes its verbatim bytes to stderr in every
//! mode and adds the stdout document in the structured ones. `xml` keeps the
//! prose path: the mode is scheduled for deletion, not for a document shape.
//!
//! Under `ndjson` the document is one compact line, and it is the only mode
//! whose warnings are stdout entries too: [`emit_warning_entries`] writes each
//! as a `severity: warning` diagnostic of kind `framework` after the result or
//! the failure (D2, D3). The single-document modes keep warnings as stderr
//! prose, which `standout_render::warnings::flush_to_stderr` owns.
//!
//! The CSV form of the diagnostic is one row whose `range` is three columns,
//! `range_filename`, `range_line` and `range_column`, empty when unset (D8).

use std::io::Write;

use serde::{Deserialize, Serialize};

use crate::cli::handler::{
    ArtifactRun, Diagnostic, DiagnosticKind, DispatchResult, OutputKind, RunError, RunErrorKind,
    Severity,
};
use crate::OutputMode;

/// Write `result` to the two streams the way `run` does, honoring
/// `output_mode` for failures. `Ok(handled)` reports whether a registered
/// command owned the invocation (`false` only for a `NoMatch` handoff);
/// `Err` is a final-write failure, already reported on stderr, whose status
/// replaces the run's own.
pub fn emit_run_result<W: Write, E: Write>(
    result: &DispatchResult,
    output_mode: OutputMode,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<bool, RunError> {
    let failure = match result {
        DispatchResult::Handled(output) if output.is_empty() => None,
        DispatchResult::Handled(output) => writeln!(stdout, "{}", output)
            .and_then(|()| stdout.flush())
            .err()
            .and_then(|error| final_write_error_unless_broken_pipe(error, OutputKind::Text)),
        DispatchResult::Binary(bytes, _) => stdout
            .write_all(bytes)
            .and_then(|()| stdout.flush())
            .err()
            .map(|error| {
                RunError::new(
                    format!("Error writing binary stdout: {}", error),
                    RunErrorKind::FinalWrite(OutputKind::Binary),
                )
            }),
        DispatchResult::Artifact(run) => emit_artifact(run, stdout, stderr),
        DispatchResult::Silent => None,
        DispatchResult::Error(error) => emit_failure(error, output_mode, stdout, stderr),
        DispatchResult::NoMatch(_) => return Ok(false),
        _ => return Ok(false),
    };

    match failure {
        Some(error) => {
            let _ = writeln!(stderr, "{}", error).and_then(|()| stderr.flush());
            Err(error)
        }
        None => Ok(true),
    }
}

fn emit_failure<W: Write, E: Write>(
    error: &RunError,
    output_mode: OutputMode,
    stdout: &mut W,
    stderr: &mut E,
) -> Option<RunError> {
    let stderr_prose = if error.writes_diagnostic_verbatim() {
        stderr.write_all(error.as_str().as_bytes())
    } else if carries_diagnostic_document(output_mode) {
        Ok(())
    } else {
        writeln!(stderr, "{}", error)
    };
    if let Err(write_error) = stderr_prose.and_then(|()| stderr.flush()) {
        return Some(RunError::new(
            format!("Error writing stderr: {}", write_error),
            RunErrorKind::FinalWrite(OutputKind::Text),
        ));
    }
    if !carries_diagnostic_document(output_mode) {
        return None;
    }
    let document = render_diagnostic(&error.diagnostic(), output_mode);
    stdout
        .write_all(document.as_bytes())
        .and_then(|()| stdout.flush())
        .err()
        .and_then(|write_error| final_write_error_unless_broken_pipe(write_error, OutputKind::Text))
}

/// The modes whose failure is a stdout document rather than stderr prose.
pub fn carries_diagnostic_document(output_mode: OutputMode) -> bool {
    matches!(
        output_mode,
        OutputMode::Json | OutputMode::Yaml | OutputMode::Csv | OutputMode::Ndjson
    )
}

/// The modes whose warnings are stdout entries rather than stderr prose:
/// `ndjson` alone, whose stream has room for more than one root.
pub fn carries_warning_entries(output_mode: OutputMode) -> bool {
    output_mode.is_stream()
}

/// Write each of `warnings` as a `severity: warning` diagnostic entry of kind
/// `framework`, one line each, when `carries_warning_entries(output_mode)`;
/// do nothing otherwise. `Err` is a final-write failure on stdout.
pub fn emit_warning_entries<W: Write>(
    warnings: &[String],
    output_mode: OutputMode,
    stdout: &mut W,
) -> Result<(), RunError> {
    if !carries_warning_entries(output_mode) || warnings.is_empty() {
        return Ok(());
    }
    let written = warnings.iter().try_for_each(|warning| {
        let mut entry = Diagnostic::warning(warning.clone());
        entry.kind = DiagnosticKind::Framework;
        stdout.write_all(render_diagnostic(&entry, output_mode).as_bytes())
    });
    match written
        .and_then(|()| stdout.flush())
        .err()
        .and_then(|error| final_write_error_unless_broken_pipe(error, OutputKind::Text))
    {
        Some(failure) => Err(failure),
        None => Ok(()),
    }
}

/// The diagnostic document in `output_mode`, newline-terminated. Panics on a
/// mode `carries_diagnostic_document` rejects: there is no document shape for
/// it to render.
pub fn render_diagnostic(diagnostic: &Diagnostic, output_mode: OutputMode) -> String {
    let rendered = if output_mode == OutputMode::Csv {
        standout_render::serialize_document(&DiagnosticRow::from(diagnostic), output_mode)
    } else {
        standout_render::serialize_document(diagnostic, output_mode)
    };
    rendered.unwrap_or_else(|error| panic!("{output_mode:?} has no diagnostic document: {error}"))
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct DiagnosticDocumentError(String);

/// Read a diagnostic document back from `text`, the inverse of
/// [`render_diagnostic`] for the same `output_mode`. Under `ndjson`, `text`
/// is the whole stream and the document is its first error-severity
/// diagnostic entry; the handler's own entries and warning entries are
/// skipped.
pub fn parse_diagnostic(
    output_mode: OutputMode,
    text: &str,
) -> Result<Diagnostic, DiagnosticDocumentError> {
    let malformed =
        |error: standout_render::RenderError| DiagnosticDocumentError(error.to_string());
    if output_mode.is_stream() {
        text.lines()
            .filter_map(|line| {
                standout_render::deserialize_document::<Diagnostic>(output_mode, line).ok()
            })
            .find(|entry| entry.severity == Severity::Error)
            .ok_or_else(|| {
                DiagnosticDocumentError(
                    "the stream carries no error-severity diagnostic entry".into(),
                )
            })
    } else if output_mode == OutputMode::Csv {
        let row: DiagnosticRow =
            standout_render::deserialize_document(output_mode, text).map_err(malformed)?;
        row.into_diagnostic()
    } else {
        standout_render::deserialize_document(output_mode, text).map_err(malformed)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct DiagnosticRow {
    #[serde(rename = "type")]
    document_type: String,
    schema_version: u32,
    severity: Severity,
    kind: DiagnosticKind,
    summary: String,
    detail: String,
    range_filename: Option<String>,
    range_line: Option<u64>,
    range_column: Option<u64>,
}

impl From<&Diagnostic> for DiagnosticRow {
    fn from(diagnostic: &Diagnostic) -> Self {
        Self {
            document_type: "diagnostic".into(),
            schema_version: diagnostic.schema_version(),
            severity: diagnostic.severity,
            kind: diagnostic.kind,
            summary: diagnostic.summary.clone(),
            detail: diagnostic.detail.clone(),
            range_filename: diagnostic.range.as_ref().map(|r| r.filename.clone()),
            range_line: diagnostic.range.as_ref().map(|r| r.start.line),
            range_column: diagnostic.range.as_ref().map(|r| r.start.column),
        }
    }
}

impl DiagnosticRow {
    fn into_diagnostic(self) -> Result<Diagnostic, DiagnosticDocumentError> {
        let range = match (self.range_filename, self.range_line, self.range_column) {
            (None, None, None) => None,
            (Some(filename), Some(line), Some(column)) => Some(serde_json::json!({
                "filename": filename, "start": { "line": line, "column": column },
            })),
            _ => {
                return Err(DiagnosticDocumentError(
                    "the range columns must be all set or all empty".into(),
                ))
            }
        };
        let mut document = serde_json::json!({
            "type": self.document_type,
            "schema_version": self.schema_version,
            "severity": self.severity,
            "kind": self.kind,
            "summary": self.summary,
            "detail": self.detail,
        });
        if let Some(range) = range {
            document["range"] = range;
        }
        serde_json::from_value(document).map_err(|e| DiagnosticDocumentError(e.to_string()))
    }
}

fn emit_artifact<W: Write, E: Write>(
    run: &ArtifactRun,
    stdout: &mut W,
    stderr: &mut E,
) -> Option<RunError> {
    let to_stdout = run.destination().is_stdout();

    if to_stdout {
        if let Err(error) = stdout.write_all(run.bytes()).and_then(|()| stdout.flush()) {
            return Some(RunError::new(
                format!("Error writing artifact stdout: {}", error),
                RunErrorKind::FinalWrite(OutputKind::Artifact),
            ));
        }
    }

    let report = run.report().filter(|report| !report.is_empty())?;

    let written = if to_stdout {
        writeln!(stderr, "{}", report).and_then(|()| stderr.flush())
    } else {
        writeln!(stdout, "{}", report).and_then(|()| stdout.flush())
    };

    written.err().map(|error| {
        RunError::new(
            format!("Error writing artifact report: {}", error),
            RunErrorKind::FinalWrite(OutputKind::Artifact),
        )
    })
}

fn final_write_error_unless_broken_pipe(
    error: std::io::Error,
    kind: OutputKind,
) -> Option<RunError> {
    if kind == OutputKind::Text && error.kind() == std::io::ErrorKind::BrokenPipe {
        None
    } else {
        Some(RunError::new(
            format!("Error writing stdout: {}", error),
            RunErrorKind::FinalWrite(kind),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::handler::{AppFailure, ArtifactDestination, ArtifactReceipt, RunOutput};
    use crate::cli::HookPhase;

    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "closed",
            ))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct ClosedWriter;

    impl Write for ClosedWriter {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("disk full"))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct FlushFailingWriter {
        bytes: Vec<u8>,
    }

    impl Write for FlushFailingWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.bytes.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "flush closed",
            ))
        }
    }

    fn emit(result: &DispatchResult, mode: OutputMode) -> (Result<bool, RunError>, String, String) {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let emitted = emit_run_result(result, mode, &mut stdout, &mut stderr);
        (
            emitted,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    }

    #[test]
    fn final_emission_routes_success_and_diagnostics_to_distinct_streams() {
        let (handled, stdout, stderr) = emit(
            &DispatchResult::Handled(RunOutput::command("hello")),
            OutputMode::Text,
        );
        assert!(handled.unwrap());
        assert_eq!(stdout, "hello\n");
        assert!(stderr.is_empty());

        let (handled, stdout, stderr) = emit(
            &DispatchResult::Error(RunError::new("bad argv", RunErrorKind::ClapUsage)),
            OutputMode::Text,
        );
        assert!(handled.unwrap());
        assert!(stdout.is_empty());
        assert_eq!(stderr, "bad argv\n");
    }

    #[test]
    fn every_human_mode_keeps_prose_on_stderr() {
        for mode in [
            OutputMode::Auto,
            OutputMode::Term,
            OutputMode::Text,
            OutputMode::TermDebug,
            OutputMode::Xml,
        ] {
            let (handled, stdout, stderr) = emit(
                &DispatchResult::Error(RunError::new("Error: boom", RunErrorKind::Handler)),
                mode,
            );
            assert!(handled.unwrap(), "{mode:?}");
            assert!(stdout.is_empty(), "{mode:?}: {stdout}");
            assert_eq!(stderr, "Error: boom\n", "{mode:?}");
        }
    }

    #[test]
    fn a_structured_mode_puts_the_diagnostic_on_stdout_and_nothing_on_stderr() {
        let error = RunError::new("Error: boom", RunErrorKind::Hook(HookPhase::PreDispatch));
        let (handled, stdout, stderr) = emit(&DispatchResult::Error(error), OutputMode::Json);
        assert!(handled.unwrap());
        assert!(stderr.is_empty(), "{stderr}");
        let document: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(document["type"], "diagnostic");
        assert_eq!(document["schema_version"], 1);
        assert_eq!(document["severity"], "error");
        assert_eq!(document["kind"], "hook-pre-dispatch");
        assert_eq!(document["summary"], "boom");
        assert_eq!(document["detail"], "");
        assert!(document.get("range").is_none());
        assert!(stdout.ends_with('\n'));
    }

    #[test]
    fn an_owner_declared_failure_keeps_its_stderr_bytes_and_adds_the_document() {
        let error = RunError::from(AppFailure::new(3, "app: refused\n").unwrap());
        let (handled, stdout, stderr) = emit(&DispatchResult::Error(error), OutputMode::Yaml);
        assert!(handled.unwrap());
        assert_eq!(stderr, "app: refused\n");
        let document = parse_diagnostic(OutputMode::Yaml, &stdout).unwrap();
        assert_eq!(document.kind, DiagnosticKind::App);
        assert_eq!(document.summary, "app: refused");
        assert_eq!(document.detail, "app: refused\n");
    }

    #[test]
    fn each_document_shape_round_trips_through_its_parser() {
        let ranged = Diagnostic::error("line 2 does not parse")
            .detail("expected `resource <name> <state>`")
            .range("main.tfl", 2, 1);
        let plain = Diagnostic::error("boom, with a \"quote\" and a, comma");
        for diagnostic in [ranged, plain] {
            for mode in [
                OutputMode::Json,
                OutputMode::Yaml,
                OutputMode::Csv,
                OutputMode::Ndjson,
            ] {
                let text = render_diagnostic(&diagnostic, mode);
                assert!(text.ends_with('\n'), "{mode:?}: {text:?}");
                assert_eq!(
                    parse_diagnostic(mode, &text).unwrap(),
                    diagnostic,
                    "{mode:?}: {text}"
                );
            }
        }
    }

    #[test]
    fn the_csv_document_is_one_row_with_three_range_columns() {
        let ranged = Diagnostic::error("bad line")
            .detail("why")
            .range("main.tfl", 2, 1);
        assert_eq!(
            render_diagnostic(&ranged, OutputMode::Csv),
            "type,schema_version,severity,kind,summary,detail,range_filename,range_line,range_column\n\
             diagnostic,1,error,handler,bad line,why,main.tfl,2,1\n"
        );
        assert_eq!(
            render_diagnostic(&Diagnostic::error("bad"), OutputMode::Csv),
            "type,schema_version,severity,kind,summary,detail,range_filename,range_line,range_column\n\
             diagnostic,1,error,handler,bad,,,,\n"
        );
    }

    #[test]
    fn a_partial_csv_range_is_refused() {
        let text = "type,schema_version,severity,kind,summary,detail,range_filename,range_line,range_column\n\
                    diagnostic,1,error,handler,bad,,main.tfl,,\n";
        let error = parse_diagnostic(OutputMode::Csv, text).unwrap_err();
        assert!(
            error.to_string().contains("all set or all empty"),
            "{error}"
        );
    }

    #[test]
    fn under_ndjson_the_failure_is_one_compact_line_on_stdout() {
        let error = RunError::new("Error: boom", RunErrorKind::Handler).with_diagnostic(
            Diagnostic::error("line 2 does not parse")
                .detail("expected `resource <name> <state>`")
                .range("main.tfl", 2, 1),
        );
        let (handled, stdout, stderr) = emit(&DispatchResult::Error(error), OutputMode::Ndjson);
        assert!(handled.unwrap());
        assert!(stderr.is_empty(), "{stderr}");
        assert_eq!(
            stdout,
            "{\"type\":\"diagnostic\",\"schema_version\":1,\"severity\":\"error\",\"kind\":\"handler\",\"summary\":\"line 2 does not parse\",\"detail\":\"expected `resource <name> <state>`\",\"range\":{\"filename\":\"main.tfl\",\"start\":{\"line\":2,\"column\":1}}}\n"
        );
        assert!(carries_diagnostic_document(OutputMode::Ndjson));
    }

    #[test]
    fn the_stream_parser_finds_the_error_entry_among_handler_and_warning_lines() {
        let stream = "{\"type\":\"version\",\"format_version\":1}\n\
                      {\"type\":\"diagnostic\",\"schema_version\":1,\"severity\":\"warning\",\"kind\":\"framework\",\"summary\":\"soft\",\"detail\":\"\"}\n\
                      {\"type\":\"diagnostic\",\"schema_version\":1,\"severity\":\"error\",\"kind\":\"handler\",\"summary\":\"boom\",\"detail\":\"\"}\n";
        let document = parse_diagnostic(OutputMode::Ndjson, stream).unwrap();
        assert_eq!(document.summary, "boom");
        assert_eq!(document.kind, DiagnosticKind::Handler);
        let error = parse_diagnostic(
            OutputMode::Ndjson,
            "{\"type\":\"version\",\"format_version\":1}\n",
        )
        .unwrap_err();
        assert!(error.to_string().contains("no error-severity"), "{error}");
    }

    #[test]
    fn warnings_are_stdout_entries_under_ndjson_and_nothing_elsewhere() {
        let warnings = vec!["first".to_string(), "second".to_string()];
        let mut stdout = Vec::new();
        emit_warning_entries(&warnings, OutputMode::Ndjson, &mut stdout).unwrap();
        let stdout = String::from_utf8(stdout).unwrap();
        assert_eq!(
            stdout,
            "{\"type\":\"diagnostic\",\"schema_version\":1,\"severity\":\"warning\",\"kind\":\"framework\",\"summary\":\"first\",\"detail\":\"\"}\n\
             {\"type\":\"diagnostic\",\"schema_version\":1,\"severity\":\"warning\",\"kind\":\"framework\",\"summary\":\"second\",\"detail\":\"\"}\n"
        );
        assert!(carries_warning_entries(OutputMode::Ndjson));
        for mode in [
            OutputMode::Text,
            OutputMode::Json,
            OutputMode::Yaml,
            OutputMode::Csv,
        ] {
            let mut stdout = Vec::new();
            emit_warning_entries(&warnings, mode, &mut stdout).unwrap();
            assert!(stdout.is_empty(), "{mode:?}");
            assert!(!carries_warning_entries(mode), "{mode:?}");
        }
        let failure =
            emit_warning_entries(&warnings, OutputMode::Ndjson, &mut ClosedWriter).unwrap_err();
        assert_eq!(failure.kind(), RunErrorKind::FinalWrite(OutputKind::Text));
        emit_warning_entries(&warnings, OutputMode::Ndjson, &mut FailingWriter).unwrap();
    }

    #[test]
    fn a_human_mode_has_no_document_to_parse() {
        assert!(parse_diagnostic(OutputMode::Text, "Error: boom\n").is_err());
        assert!(!carries_diagnostic_document(OutputMode::Text));
        assert!(carries_diagnostic_document(OutputMode::Csv));
    }

    #[test]
    fn a_failed_document_write_is_a_final_write_failure() {
        let mut stderr = Vec::new();
        let failure = emit_run_result(
            &DispatchResult::Error(RunError::new("Error: boom", RunErrorKind::Handler)),
            OutputMode::Json,
            &mut ClosedWriter,
            &mut stderr,
        )
        .unwrap_err();
        assert_eq!(failure.kind(), RunErrorKind::FinalWrite(OutputKind::Text));
        assert!(String::from_utf8(stderr)
            .unwrap()
            .contains("Error writing stdout"));
    }

    #[test]
    fn a_document_broken_pipe_keeps_the_failure_it_was_reporting() {
        let mut stderr = Vec::new();
        let handled = emit_run_result(
            &DispatchResult::Error(RunError::new("bad argv", RunErrorKind::ClapUsage)),
            OutputMode::Json,
            &mut FailingWriter,
            &mut stderr,
        );
        assert!(handled.unwrap());
        assert!(stderr.is_empty());
    }

    #[test]
    fn final_text_broken_pipe_is_successful_early_termination() {
        let mut stderr = Vec::new();
        let handled = emit_run_result(
            &DispatchResult::Handled(RunOutput::command("hello")),
            OutputMode::Text,
            &mut FailingWriter,
            &mut stderr,
        );
        assert!(handled.unwrap());
        assert!(stderr.is_empty());
    }

    #[test]
    fn final_binary_write_failures_keep_payload_kind() {
        let mut stderr = Vec::new();
        let binary_failure = emit_run_result(
            &DispatchResult::Binary(vec![0, 1], "data.bin".into()),
            OutputMode::Text,
            &mut FailingWriter,
            &mut stderr,
        )
        .unwrap_err();
        assert_eq!(
            binary_failure.kind(),
            RunErrorKind::FinalWrite(OutputKind::Binary)
        );
        assert_eq!(
            binary_failure.exit_status(),
            crate::cli::ExitStatus::FAILURE
        );
    }

    #[test]
    fn final_text_broken_pipe_flush_is_successful_early_termination() {
        let mut text_stdout = FlushFailingWriter::default();
        let handled = emit_run_result(
            &DispatchResult::Handled(RunOutput::command("hello")),
            OutputMode::Text,
            &mut text_stdout,
            &mut Vec::new(),
        );
        assert_eq!(text_stdout.bytes, b"hello\n");
        assert!(handled.unwrap());
    }

    #[test]
    fn final_binary_flush_failures_keep_payload_kind() {
        let mut binary_stdout = FlushFailingWriter::default();
        let binary_failure = emit_run_result(
            &DispatchResult::Binary(vec![0, 1], "data.bin".into()),
            OutputMode::Text,
            &mut binary_stdout,
            &mut Vec::new(),
        )
        .unwrap_err();
        assert_eq!(binary_stdout.bytes, [0, 1]);
        assert_eq!(
            binary_failure.kind(),
            RunErrorKind::FinalWrite(OutputKind::Binary)
        );
    }

    #[test]
    fn artifact_report_write_failures_keep_artifact_kind_on_both_channels() {
        let file_run = ArtifactRun::new(
            vec![0, 1],
            None,
            ArtifactReceipt::new(ArtifactDestination::File("out.bin".into()), 2),
            Some("wrote out.bin".into()),
        );
        let file_report_failure = emit_run_result(
            &DispatchResult::Artifact(file_run),
            OutputMode::Text,
            &mut FailingWriter,
            &mut Vec::new(),
        )
        .unwrap_err();
        assert_eq!(
            file_report_failure.kind(),
            RunErrorKind::FinalWrite(OutputKind::Artifact)
        );

        let stdout_run = ArtifactRun::new(
            vec![0, 1],
            None,
            ArtifactReceipt::new(ArtifactDestination::Stdout, 2),
            Some("wrote stdout".into()),
        );
        let mut stdout = Vec::new();
        let stdout_report_failure = emit_run_result(
            &DispatchResult::Artifact(stdout_run),
            OutputMode::Text,
            &mut stdout,
            &mut FailingWriter,
        )
        .unwrap_err();
        assert_eq!(stdout, [0, 1]);
        assert_eq!(
            stdout_report_failure.kind(),
            RunErrorKind::FinalWrite(OutputKind::Artifact)
        );
    }
}
