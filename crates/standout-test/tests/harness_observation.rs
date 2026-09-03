use clap::{Arg, ArgAction, Command};
use console::Style;
use serde_json::json;
use serial_test::serial;
use standout::cli::FnHandler;
use standout::cli::{App, Artifact, ExternalFailure, HandlerResult, Output};
use standout::ColorPolicy;
use standout::EmbeddedTemplates;
use standout::Theme;
use standout_render::Representation;
use standout_test::{assert_page_snapshot, SnapshotCase, TestHarness};

const TEMPLATES: &[(&str, &str)] = &[
    (
        "export",
        "Wrote {{ report.entries }} entries to {{ receipt.destination }}",
    ),
    ("say", "hello"),
    ("say-2", "[shout]hello[/shout]"),
    ("list", "listed"),
];
const ARTIFACT_BYTES: &[u8] = b"id,title\n1,buy milk\n";
fn stdout_artifact_app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "export",
            FnHandler::new(|_m, _ctx| {
                Ok(Output::Artifact(
                    Artifact::new(ARTIFACT_BYTES.to_vec())
                        .with_report(json!({ "entries": 1 }))
                        .allow_stdout(),
                ))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap()
}
fn file_artifact_app(destination: std::path::PathBuf) -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "export",
            FnHandler::new(move |_m, _ctx| {
                Ok(Output::Artifact(
                    Artifact::new(ARTIFACT_BYTES.to_vec())
                        .with_report(json!({ "entries": 1 }))
                        .suggest_destination(&destination),
                ))
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
        result.stdout().as_bytes(),
        stdout_payload,
        "the artifact owns stdout, so nothing textual joins its bytes there"
    );
}
#[test]
#[serial]
fn opaque_stdout_is_observable_byte_for_byte() {
    const RAW: &[u8] = &[0xff, 0x00, b'x', 0xfe];
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "export",
            FnHandler::new(|_m, _ctx| {
                Ok(Output::Artifact(
                    Artifact::<serde_json::Value>::new(RAW.to_vec()).allow_stdout(),
                ))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let result = TestHarness::new().run(&app, export_command(), ["app", "export"]);
    result.assert_success();
    assert_eq!(result.stdout_bytes(), RAW);
    assert_ne!(result.stdout().as_bytes(), RAW);
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "export",
            FnHandler::new(|_m, _ctx| -> HandlerResult<serde_json::Value> {
                Ok(Output::Binary {
                    data: RAW.to_vec(),
                    filename: "raw.bin".into(),
                })
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let result = TestHarness::new().run(&app, export_command(), ["app", "export"]);
    result.assert_success();
    assert_eq!(result.stdout_bytes(), RAW);
}
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
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "run",
            FnHandler::new(|_m, _ctx| -> HandlerResult<serde_json::Value> {
                Err(std::io::Error::other("the handler refused").into())
            }),
            |config| config.structured_only(),
        )
        .unwrap()
        .build()
        .unwrap()
}
fn external_failure_app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "run",
            FnHandler::new(|_m, _ctx| -> HandlerResult<serde_json::Value> {
                Err(ExternalFailure::new(3, "fatal: not a git repository")
                    .unwrap()
                    .into())
            }),
            |config| config.structured_only(),
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
#[test]
#[serial]
fn an_external_failure_reaches_stderr_verbatim() {
    let app = external_failure_app();
    let result = TestHarness::new().run(&app, run_command(), ["app", "run"]);
    result.assert_error();
    result.assert_stderr_eq("fatal: not a git repository");
}
fn warning_app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "say",
            FnHandler::new(|_m, ctx| {
                use standout::cli::CommandContextInput;
                ctx.warn("stylesheet fell back to the compiled copy");
                Ok(Output::Render(json!({})))
            }),
            |cfg| cfg,
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
fn warning_block_uses_the_app_theme_not_theme_default() {
    use standout::{AmbiguousWidth, ColorMode, IconMode, TargetProperties};
    use standout_render::warnings::{
        render_block_for_target, WARNING_BANNER_STYLE, WARNING_ITEM_STYLE,
    };
    let theme = Theme::default()
        .add(WARNING_BANNER_STYLE, Style::new().magenta().bold())
        .add(WARNING_ITEM_STYLE, Style::new().magenta());
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(theme.clone())
        .command_with(
            "say",
            FnHandler::new(|_m, ctx| {
                use standout::cli::CommandContextInput;
                ctx.warn("stylesheet fell back");
                Ok(Output::Render(json!({})))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app").subcommand(Command::new("say"));
    let result = TestHarness::new()
        .stdout_is_terminal(true)
        .run(&app, cmd, ["app", "say"]);
    result.assert_success();
    let target = TargetProperties {
        width: None,
        stdout_is_terminal: false,
        stderr_is_terminal: false,
        stdout_color_capability: true,
        stderr_color_capability: true,
        color_scheme: ColorMode::Dark,
        icon_mode: IconMode::Classic,
        ambiguous_width: AmbiguousWidth::Narrow,
    };
    let expected = render_block_for_target(&theme, ColorPolicy::Auto, target, result.warnings());
    assert_eq!(
        result.stderr(),
        expected,
        "harness stderr must match App::run's warning block for the app theme"
    );
    let default_block = render_block_for_target(
        &Theme::default(),
        ColorPolicy::Auto,
        target,
        result.warnings(),
    );
    assert_ne!(
        result.stderr(),
        default_block,
        "a custom warning theme must diverge from Theme::default()"
    );
}
#[test]
#[serial]
fn a_handled_run_leaves_stderr_silent() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "say",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({})))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app").subcommand(Command::new("say"));
    let result = TestHarness::new().run(&app, cmd, ["app", "say"]);
    result.assert_stdout_contains("hello");
    result.assert_stderr_empty();
}
fn styled_app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(Theme::new().add("shout", Style::new().red().force_styling(true)))
        .command_with(
            "say",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({})))),
            |cfg| cfg.template_name("say-2"),
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
        .stdout_is_terminal(true)
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
        .stdout_is_terminal(false)
        .run(&app, cmd, ["app", "say"]);
    assert_eq!(result.stdout_plain(), result.stdout());
}
fn help_app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .help_handling(true)
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({})))),
            |cfg| cfg,
        )
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
#[test]
#[serial]
fn the_help_page_is_pinned_by_snapshot() {
    let app = help_app();
    let result = TestHarness::new()
        .terminal_width(80)
        .stdout_is_terminal(false)
        .run(&app, help_command(), ["notes", "--help"]);
    result.assert_success();
    assert_page_snapshot!(
        result,
        SnapshotCase::new("help")
            .output_mode(Representation::Human)
            .tty(false)
            .theme("default")
            .entry_point("--help")
    );
}
