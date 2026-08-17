//! What the harness can now observe: stream routing, styling, whole pages.
//!
//! These are the capabilities the existing suite could not express. Stream
//! routing had no oracle at all — the harness modeled the error channel as an
//! `error()` string, so "the report went to stderr because the bytes owned
//! stdout" was unassertable. Styled output had no strip helper, so a test
//! either hand-rolled a regex or asserted on escape sequences. And nothing
//! pinned a whole page: existential `contains` assertions pass happily while
//! an extra wrong line renders beside them.
//!
//! All tests are `#[serial]` because the harness mutates process-global state.

use clap::{Arg, ArgAction, Command};
use console::Style;
use serde_json::json;
use serial_test::serial;
use standout::cli::{App, Artifact, ExternalFailure, HandlerResult, Output};
use standout::Theme;
use standout_render::OutputMode;
use standout_test::{assert_page_snapshot, SnapshotCase, TestHarness};

// ---------------------------------------------------------------------------
// Stream routing: stdout and stderr are different channels
// ---------------------------------------------------------------------------

const ARTIFACT_BYTES: &[u8] = b"id,title\n1,buy milk\n";

/// An export app whose artifact claims stdout, so the framework has to route
/// the report somewhere that cannot corrupt the bytes.
fn stdout_artifact_app() -> App {
    App::builder()
        .command(
            "export",
            |_m, _ctx| {
                Ok(Output::Artifact(
                    Artifact::new(ARTIFACT_BYTES.to_vec())
                        .with_report(json!({ "entries": 1 }))
                        .allow_stdout(),
                ))
            },
            "Wrote {{ report.entries }} entries to {{ receipt.destination }}",
        )
        .unwrap()
        .build()
        .unwrap()
}

/// An export app that writes to a file, leaving stdout free for the report.
fn file_artifact_app(destination: std::path::PathBuf) -> App {
    App::builder()
        .command(
            "export",
            move |_m, _ctx| {
                Ok(Output::Artifact(
                    Artifact::new(ARTIFACT_BYTES.to_vec())
                        .with_report(json!({ "entries": 1 }))
                        .suggest_destination(&destination),
                ))
            },
            "Wrote {{ report.entries }} entries to {{ receipt.destination }}",
        )
        .unwrap()
        .build()
        .unwrap()
}

fn export_command() -> Command {
    Command::new("app").subcommand(Command::new("export"))
}

/// The case the harness could not previously express: two streams, different
/// content, in the same run. The artifact bytes own stdout, so the report —
/// which would corrupt them — is on stderr instead.
#[test]
#[serial]
fn stdout_and_stderr_carry_different_content() {
    let app = stdout_artifact_app();
    let result = TestHarness::new().run(&app, export_command(), ["app", "export"]);

    result.assert_success();
    result.assert_artifact_to_stdout();

    let stdout_payload = result.artifact_bytes().expect("artifact bytes");
    assert_eq!(stdout_payload, ARTIFACT_BYTES);

    result.assert_stderr_eq("Wrote 1 entries to -\n");
    assert_ne!(
        result.stderr().as_bytes(),
        stdout_payload,
        "the two streams must carry different content"
    );
    assert!(
        !String::from_utf8_lossy(stdout_payload).contains("Wrote 1 entries"),
        "the report must not ride the stdout byte stream"
    );
    assert_eq!(
        result.stdout(),
        "",
        "the artifact owns stdout, so nothing textual joins its bytes there"
    );
}

/// The mirror case: a file destination leaves stdout free, so the report goes
/// there and the error channel stays silent.
#[test]
#[serial]
fn a_file_artifact_reports_on_stdout_and_leaves_stderr_silent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.csv");
    let app = file_artifact_app(path.clone());

    let result = TestHarness::new().run(&app, export_command(), ["app", "export"]);

    result.assert_success();
    result.assert_artifact_written_to(&path);
    result.assert_artifact_report_contains("Wrote 1 entries to");
    result.assert_stdout_contains("Wrote 1 entries to");
    result.assert_stderr_empty();
}

fn failing_app() -> App {
    App::builder()
        .command(
            "run",
            |_m, _ctx| -> HandlerResult<serde_json::Value> {
                Err(std::io::Error::other("the handler refused").into())
            },
            "unused",
        )
        .unwrap()
        .build()
        .unwrap()
}

/// A handler that delegates to a tool and surrenders that tool's diagnostic
/// unchanged — deliberately without a trailing newline, so "verbatim" is
/// testable against the framework's own newline termination.
fn external_failure_app() -> App {
    App::builder()
        .command(
            "run",
            |_m, _ctx| -> HandlerResult<serde_json::Value> {
                Err(ExternalFailure::new(3, "fatal: not a git repository")
                    .unwrap()
                    .into())
            },
            "unused",
        )
        .unwrap()
        .build()
        .unwrap()
}

fn run_command() -> Command {
    Command::new("app").subcommand(Command::new("run"))
}

#[test]
#[serial]
fn an_error_diagnostic_lands_on_stderr_with_stdout_empty() {
    let app = failing_app();
    let result = TestHarness::new().run(&app, run_command(), ["app", "run"]);

    result.assert_error();
    assert_eq!(result.stdout(), "", "a failed run writes nothing to stdout");
    result.assert_stderr_contains("the handler refused");
    assert!(
        result.stderr().ends_with('\n'),
        "the framework newline-terminates a diagnostic: {:?}",
        result.stderr()
    );
}

/// An external failure is passed through verbatim — no framework newline —
/// so the wrapped tool's own diagnostic reaches the user unreshaped.
#[test]
#[serial]
fn an_external_failure_reaches_stderr_verbatim() {
    let app = external_failure_app();
    let result = TestHarness::new().run(&app, run_command(), ["app", "run"]);

    result.assert_error();
    result.assert_stderr_eq("fatal: not a git repository");
}

/// A handler that succeeds and still leaves the framework something to say.
/// `App::run` flushes that warning block to stderr after the primary output,
/// so a "clean run" oracle that ignored it would pass a run the user sees
/// warnings from.
fn warning_app() -> App {
    App::builder()
        .command(
            "say",
            |_m, _ctx| {
                standout::warnings::push_warning("stylesheet fell back to the compiled copy");
                Ok(Output::Render(json!({})))
            },
            "hello",
        )
        .unwrap()
        .build()
        .unwrap()
}

#[test]
#[serial]
fn framework_warnings_land_on_stderr_and_defeat_the_silent_assertion() {
    let app = warning_app();
    let cmd = Command::new("app").subcommand(Command::new("say"));

    let result = TestHarness::new().run(&app, cmd, ["app", "say"]);

    result.assert_success();
    result.assert_stdout_contains("hello");
    result.assert_stderr_contains("stylesheet fell back to the compiled copy");
    result.assert_stderr_contains("Standout :: Warnings");
    assert!(
        result.stderr().starts_with('\n'),
        "the block opens with the blank line `run` writes: {:?}",
        result.stderr()
    );
    assert_eq!(
        result.warnings(),
        ["stylesheet fell back to the compiled copy"],
        "each warning stays individually addressable"
    );

    let empty = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        result.assert_stderr_empty()
    }));
    assert!(
        empty.is_err(),
        "a warning-producing run must not read as a silent error channel"
    );
}

#[test]
#[serial]
fn a_handled_run_leaves_stderr_silent() {
    let app = App::builder()
        .command("say", |_m, _ctx| Ok(Output::Render(json!({}))), "hello")
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app").subcommand(Command::new("say"));

    let result = TestHarness::new().run(&app, cmd, ["app", "say"]);

    result.assert_stdout_contains("hello");
    result.assert_stderr_empty();
}

// ---------------------------------------------------------------------------
// Styling: stripped and raw are both available
// ---------------------------------------------------------------------------

/// A styled app in the one mode that emits escapes. `force_styling` is what
/// makes ANSI appear with no TTY under the test binary.
fn styled_app() -> App {
    App::builder()
        .theme(Theme::new().add("shout", Style::new().red().force_styling(true)))
        .command(
            "say",
            |_m, _ctx| Ok(Output::Render(json!({}))),
            "[shout]hello[/shout]",
        )
        .unwrap()
        .build()
        .unwrap()
}

#[test]
#[serial]
fn stdout_plain_strips_the_styling_the_raw_accessor_keeps() {
    let app = styled_app();
    let cmd = Command::new("app").subcommand(Command::new("say"));

    let result = TestHarness::new()
        .output_mode(OutputMode::Term)
        .run(&app, cmd, ["app", "say"]);

    let raw = result.stdout();
    assert!(
        raw.contains("\x1b[31m"),
        "the fixture must actually emit ANSI, got {:?}",
        raw
    );

    let plain = result.stdout_plain();
    assert_eq!(plain.trim_end(), "hello");
    assert!(
        !plain.contains('\x1b'),
        "stdout_plain must carry no escapes: {:?}",
        plain
    );
    assert_eq!(
        result.stdout(),
        raw,
        "stripping must not disturb the raw accessor"
    );
}

#[test]
#[serial]
fn stdout_plain_is_a_no_op_on_unstyled_output() {
    let app = styled_app();
    let cmd = Command::new("app").subcommand(Command::new("say"));

    let result = TestHarness::new()
        .text_output()
        .run(&app, cmd, ["app", "say"]);

    assert_eq!(result.stdout_plain(), result.stdout());
}

// ---------------------------------------------------------------------------
// Whole-page snapshots
// ---------------------------------------------------------------------------

/// A help-bearing app: a valued option, a `SetTrue` flag, and a subcommand —
/// enough surface that a dropped or bogus row shows up in the snapshot.
fn help_app() -> App {
    App::builder()
        .help_handling(true)
        .command("list", |_m, _ctx| Ok(Output::Render(json!({}))), "listed")
        .unwrap()
        .build()
        .unwrap()
}

fn help_command() -> Command {
    Command::new("notes")
        .about("Keep short notes")
        .arg(
            Arg::new("file")
                .short('f')
                .long("file")
                .value_name("PATH")
                .action(ArgAction::Set)
                .help("Notes file to read"),
        )
        .arg(
            Arg::new("all")
                .long("all")
                .action(ArgAction::SetTrue)
                .help("Include archived notes"),
        )
        .subcommand(Command::new("list").about("List the notes"))
}

/// The proof the whole slice exists for: one help page pinned end to end
/// through the harness, keyed by its case rather than a hand-written label.
/// Later workstreams multiply this across the matrix; the keying is what makes
/// that multiplication reviewable.
#[test]
#[serial]
fn the_help_page_is_pinned_by_snapshot() {
    let app = help_app();
    let result = TestHarness::new()
        .terminal_width(80)
        .no_color()
        .text_output()
        .run(&app, help_command(), ["notes", "--help"]);

    result.assert_success();
    assert_page_snapshot!(
        result,
        SnapshotCase::new("help")
            .output_mode(OutputMode::Text)
            .tty(false)
            .theme("default")
            .entry_point("--help")
    );
}
