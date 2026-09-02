//! The exit-code table of the machine contract: four outcomes under `text`,
//! `json` and `ndjson`, each row asserting the status and where the document,
//! the diagnostic and the prose land. `diagnostics.rs`, `exit_statuses.rs` and
//! `ndjson.rs` go deeper into each cell; this file is the contract read as one
//! table.

use clap::Command;
use serde_json::json;
use serial_test::serial;
use standout::cli::{
    App, DiagnosticKind, ExitStatus, FnHandler, HandlerResult, Output, RunErrorKind,
};
use standout::{EmbeddedTemplates, OutputMode};
use standout_test::{TestHarness, TestResult};

const TEMPLATES: &[(&str, &str)] = &[
    ("ok", "{{ message }}"),
    ("signal", "{{ message }}"),
    ("fail", "{{ message }}"),
];

fn command() -> Command {
    Command::new("app")
        .subcommand(Command::new("ok"))
        .subcommand(Command::new("signal"))
        .subcommand(Command::new("fail"))
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
            "signal",
            FnHandler::new(|_, _| {
                Ok(Output::Render(json!({ "message": "signal" }))
                    .with_exit_status(ExitStatus::from(3)))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .command_with(
            "fail",
            FnHandler::new(|_, _| -> HandlerResult<serde_json::Value> {
                Err(anyhow::anyhow!("the handler refused"))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap()
}

#[derive(Clone, Copy, Debug)]
enum Outcome {
    Success,
    DeclaredStatus,
    UsageError,
    Failure,
}

impl Outcome {
    fn args(self) -> &'static [&'static str] {
        match self {
            Self::Success => &["app", "ok"],
            Self::DeclaredStatus => &["app", "signal"],
            Self::UsageError => &["app", "ok", "--bogus"],
            Self::Failure => &["app", "fail"],
        }
    }

    fn error_kind(self) -> Option<RunErrorKind> {
        match self {
            Self::Success | Self::DeclaredStatus => None,
            Self::UsageError => Some(RunErrorKind::ClapUsage),
            Self::Failure => Some(RunErrorKind::Handler),
        }
    }
}

#[derive(Debug)]
enum Stdout {
    Nothing,
    Text(&'static str),
    Document(&'static str),
    ResultEntry(&'static str),
    Diagnostic(DiagnosticKind),
}

#[derive(Debug)]
enum Stderr {
    Silent,
    Prose(&'static str),
}

struct Row {
    outcome: Outcome,
    mode: OutputMode,
    status: u8,
    stdout: Stdout,
    stderr: Stderr,
}

fn table() -> Vec<Row> {
    use DiagnosticKind::{ClapUsage, Handler};
    use Outcome::{DeclaredStatus, Failure, Success, UsageError};
    use OutputMode::{Json, Ndjson, Text};
    use Stderr::{Prose, Silent};
    use Stdout::{Diagnostic, Document, Nothing, ResultEntry};

    let usage = "error: unexpected argument '--bogus'";
    let refused = "Error: the handler refused";
    [
        (Success, Text, 0, Stdout::Text("ok"), Silent),
        (DeclaredStatus, Text, 3, Stdout::Text("signal"), Silent),
        (UsageError, Text, 2, Nothing, Prose(usage)),
        (Failure, Text, 1, Nothing, Prose(refused)),
        (Success, Json, 0, Document("ok"), Silent),
        (DeclaredStatus, Json, 3, Document("signal"), Silent),
        (UsageError, Json, 2, Diagnostic(ClapUsage), Silent),
        (Failure, Json, 1, Diagnostic(Handler), Silent),
        (Success, Ndjson, 0, ResultEntry("ok"), Silent),
        (DeclaredStatus, Ndjson, 3, ResultEntry("signal"), Silent),
        (UsageError, Ndjson, 2, Diagnostic(ClapUsage), Silent),
        (Failure, Ndjson, 1, Diagnostic(Handler), Silent),
    ]
    .into_iter()
    .map(|(outcome, mode, status, stdout, stderr)| Row {
        outcome,
        mode,
        status,
        stdout,
        stderr,
    })
    .collect()
}

fn assert_stdout(row: &Row, result: &TestResult) {
    let cell = format!("{:?} under {:?}", row.outcome, row.mode);
    match row.stdout {
        Stdout::Nothing => assert_eq!(result.stdout(), "", "{cell}"),
        Stdout::Text(text) => {
            assert_eq!(result.stdout(), text, "{cell}");
            assert_eq!(result.diagnostic(), None, "{cell}");
        }
        Stdout::Document(message) => {
            let document: serde_json::Value =
                serde_json::from_str(result.stdout()).unwrap_or_else(|e| {
                    panic!("{cell}: stdout is not JSON ({e}):\n{}", result.stdout())
                });
            assert_eq!(document, json!({ "message": message }), "{cell}");
            assert_eq!(result.diagnostic(), None, "{cell}");
        }
        Stdout::ResultEntry(message) => {
            let expected =
                format!("{{\"type\":\"result\",\"data\":{{\"message\":\"{message}\"}}}}\n");
            assert_eq!(result.stdout(), expected, "{cell}");
            assert_eq!(result.diagnostic(), None, "{cell}");
        }
        Stdout::Diagnostic(kind) => {
            let lines = result.stdout().lines().count();
            if row.mode == OutputMode::Ndjson {
                assert_eq!(
                    lines,
                    1,
                    "{cell}: one diagnostic entry:\n{}",
                    result.stdout()
                );
            } else {
                let document: serde_json::Value = serde_json::from_str(result.stdout())
                    .unwrap_or_else(|e| {
                        panic!("{cell}: stdout is not JSON ({e}):\n{}", result.stdout())
                    });
                assert_eq!(document["type"], "diagnostic", "{cell}");
            }
            assert_eq!(result.expect_diagnostic().kind, kind, "{cell}");
        }
    }
}

fn assert_stderr(row: &Row, result: &TestResult) {
    let cell = format!("{:?} under {:?}", row.outcome, row.mode);
    match row.stderr {
        Stderr::Silent => assert_eq!(result.stderr(), "", "{cell}"),
        Stderr::Prose(prefix) => assert!(
            result.stderr().starts_with(prefix),
            "{cell}: stderr should start with {prefix:?}:\n{}",
            result.stderr()
        ),
    }
}

#[test]
#[serial]
fn every_cell_of_the_exit_code_table_holds() {
    for row in &table() {
        let result =
            TestHarness::new()
                .output_mode(row.mode)
                .run(&app(), command(), row.outcome.args());
        let cell = format!("{:?} under {:?}", row.outcome, row.mode);
        assert_eq!(
            result.exit_status().map(|status| status.code()),
            Some(row.status),
            "{cell}"
        );
        assert_eq!(result.error_kind(), row.outcome.error_kind(), "{cell}");
        assert_stdout(row, &result);
        assert_stderr(row, &result);
    }
}
