//! Integration coverage for `AppBuilder::strict_style_tags`: the opt-in gate
//! that turns an unresolved style tag from a graceful degradation into a hard
//! failure. The default path (off) must stay exactly as before — degrade to
//! unstyled text plus a stderr warning.

use clap::Command;
use serde_json::json;
use standout::cli::handler::{DispatchResult, ExitStatus, RunErrorKind};
use standout::cli::{App, Artifact, FnHandler, Output};
use standout::{embed_styles, EmbeddedTemplates};
use standout_test::TestHarness;

const COMMANDS: [&str; 4] = [
    "clean",
    "balanced-unknown",
    "unbalanced-unknown",
    "malformed-known",
];

// `header` is styled by the `default` theme fixture; `bogus` is not defined in
// any theme.
const TEMPLATES: &[(&str, &str)] = &[
    ("clean", "[header]{{ msg }}[/header]"),
    ("balanced-unknown", "[bogus]{{ msg }}[/bogus]"),
    ("unbalanced-unknown", "[bogus]{{ msg }}"),
    ("malformed-known", "[header]{{ msg }}"),
];

fn app(strict: bool) -> App {
    let mut builder = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .styles(embed_styles!("tests/fixtures/styles"))
        .default_theme("default")
        .strict_style_tags(strict);
    for name in COMMANDS {
        builder = builder
            .command_with(
                name,
                FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"msg": "hi"})))),
                move |cfg| cfg.template_name(name),
            )
            .unwrap();
    }
    builder.build().unwrap()
}

fn command() -> Command {
    COMMANDS.into_iter().fold(Command::new("app"), |cmd, name| {
        cmd.subcommand(Command::new(name))
    })
}

fn run(strict: bool, subcommand: &str) -> standout_test::TestResult {
    TestHarness::new()
        .text_output()
        .run(&app(strict), command(), ["app", subcommand])
}

// Framework help renders through the app theme, so a style tag the theme does
// not define — here `bogus`, carried in the root command's `about` text —
// leaves the help page with an unresolved tag. Help is rendered on a path
// (`intercept_display_help` / `intercept_help_word`) that returns before
// command dispatch, so it exercises the run-completion strict sweep rather than
// the per-command pre-write gate.
fn help_app(strict: bool) -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .styles(embed_styles!("tests/fixtures/styles"))
        .default_theme("default")
        .help_word(true)
        .strict_style_tags(strict)
        .command_with(
            "clean",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"msg": "hi"})))),
            |cfg| cfg.template_name("clean"),
        )
        .unwrap()
        .build()
        .unwrap()
}

fn help_command() -> Command {
    Command::new("app")
        .about("[bogus]a summary the theme cannot style[/bogus]")
        .subcommand(Command::new("clean"))
}

const STRICT_ARTIFACT_TEMPLATE: &str = "[bogus]exported {{ report.entries }} entries[/bogus]";

fn strict_artifact_app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(
            &[("export", STRICT_ARTIFACT_TEMPLATE)],
            "",
        ))
        .output_file_flag(Some("output-file-path"))
        .strict_style_tags(true)
        .command_with(
            "export",
            FnHandler::new(|_m, _ctx| {
                Ok(Output::Artifact(
                    Artifact::new(b"id,title\n1,x\n".to_vec()).with_report(json!({"entries": 1})),
                ))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap()
}

fn artifact_command() -> Command {
    Command::new("app").subcommand(Command::new("export"))
}

#[test]
fn strict_on_fails_and_names_a_balanced_unresolved_tag() {
    let result = run(true, "balanced-unknown");
    result.assert_exit_status(ExitStatus::FAILURE);
    result.assert_error_kind(RunErrorKind::Render);
    result.assert_stdout_eq("");
    let error = result.error().expect("strict mode should produce an error");
    assert!(
        error.contains("bogus"),
        "error should name the tag: {error}"
    );
    assert!(
        error.contains("strict_style_tags"),
        "error should name the lever: {error}"
    );
}

#[test]
fn strict_on_fails_and_names_an_unbalanced_unresolved_tag() {
    // An unbalanced unknown tag travels a different parse path than a balanced
    // one, but is still recorded as unresolved — strict must catch both.
    let result = run(true, "unbalanced-unknown");
    result.assert_exit_status(ExitStatus::FAILURE);
    result.assert_error_kind(RunErrorKind::Render);
    result.assert_stdout_eq("");
    assert!(
        result.error().unwrap().contains("bogus"),
        "error should name the tag: {:?}",
        result.error()
    );
}

#[test]
fn strict_off_is_the_unchanged_graceful_default() {
    let result = run(false, "balanced-unknown");
    result.assert_success();
    assert_eq!(
        result.stdout_plain(),
        "hi",
        "the unknown tag degrades to unstyled text"
    );
    assert_eq!(result.unresolved_tag_names(), ["bogus"]);
    assert!(
        result
            .warnings()
            .iter()
            .any(|w| w.contains("degraded to unstyled text") && w.contains("bogus")),
        "the stderr warning must still fire, got {:?}",
        result.warnings()
    );
}

#[test]
fn strict_on_succeeds_on_a_clean_render() {
    let result = run(true, "clean");
    result.assert_success();
    assert_eq!(result.stdout_plain(), "hi");
}

#[test]
fn strict_on_ignores_a_malformed_but_defined_tag() {
    // `[header]hi` is unbalanced, but `header` is a defined tag, so it is
    // malformed markup, not an unresolved tag. Strict keys on unresolved tags
    // only, so this must still succeed.
    let result = run(true, "malformed-known");
    result.assert_success();
    assert!(
        result.unresolved_tag_names().is_empty(),
        "a malformed defined tag is not unresolved: {:?}",
        result.unresolved_tag_names()
    );
}

#[test]
fn strict_failure_leaves_a_preexisting_output_file_untouched() {
    // The "no output is emitted" contract covers `--output-file-path` too: the
    // gate runs before the file write, so a strict failure must not create or
    // overwrite the requested file.
    let tempdir = tempfile::tempdir().unwrap();
    let output_file = tempdir.path().join("out.txt");
    std::fs::write(&output_file, "original contents").unwrap();

    let result = TestHarness::new().text_output().run(
        &app(true),
        command(),
        [
            "app",
            "--output-file-path",
            output_file.to_str().unwrap(),
            "balanced-unknown",
        ],
    );

    result.assert_exit_status(ExitStatus::FAILURE);
    result.assert_error_kind(RunErrorKind::Render);
    assert_eq!(
        std::fs::read_to_string(&output_file).unwrap(),
        "original contents",
        "a strict failure must not overwrite the requested output file"
    );
}

#[test]
fn strict_on_fails_on_an_unresolved_tag_in_the_help_page() {
    // `--help` renders framework help and returns before command dispatch. The
    // run-completion sweep must still catch the unresolved tag and emit nothing.
    let result =
        TestHarness::new()
            .text_output()
            .run(&help_app(true), help_command(), ["app", "--help"]);
    result.assert_exit_status(ExitStatus::FAILURE);
    result.assert_error_kind(RunErrorKind::Render);
    result.assert_stdout_eq("");
    assert!(
        result.error().unwrap().contains("bogus"),
        "error should name the unresolved help tag: {:?}",
        result.error()
    );
}

#[test]
fn strict_on_fails_on_an_unresolved_tag_in_the_help_word_page() {
    // The `help` word takes a different interception path than `--help`; strict
    // mode must catch an unresolved tag on both.
    let result =
        TestHarness::new()
            .text_output()
            .run(&help_app(true), help_command(), ["app", "help"]);
    result.assert_exit_status(ExitStatus::FAILURE);
    result.assert_error_kind(RunErrorKind::Render);
    result.assert_stdout_eq("");
    assert!(
        result.error().unwrap().contains("bogus"),
        "error should name the unresolved help tag: {:?}",
        result.error()
    );
}

#[test]
fn strict_off_still_renders_the_help_page() {
    // With strict off, the same unresolved help tag degrades gracefully and the
    // help page is still produced — the sweep only fires when strict is on.
    let result =
        TestHarness::new()
            .text_output()
            .run(&help_app(false), help_command(), ["app", "--help"]);
    result.assert_success();
    assert!(
        result.stdout().contains("summary"),
        "the help page should still render its about text: {:?}",
        result.stdout()
    );
}

#[test]
fn strict_failure_leaves_no_artifact_file_behind() {
    // The artifact path renders its report and writes bytes inside
    // `complete_artifact`; the gate runs after the report render and before the
    // byte write, so a strict failure must leave a pre-existing artifact file
    // untouched.
    let tempdir = tempfile::tempdir().unwrap();
    let output_file = tempdir.path().join("export.csv");
    std::fs::write(&output_file, "original artifact contents").unwrap();

    let result = strict_artifact_app().run_with(
        artifact_command(),
        [
            "app",
            "export",
            "--output-file-path",
            output_file.to_str().unwrap(),
        ],
        standout::TargetProperties::detect(),
        standout::InputSources::from_process(),
    );

    match result.outcome() {
        DispatchResult::Error(error) => {
            assert_eq!(error.kind(), RunErrorKind::Render);
            assert!(
                error.as_str().contains("bogus"),
                "error should name the tag: {}",
                error.as_str()
            );
        }
        other => panic!("expected a strict Render error, got {other:?}"),
    }
    assert_eq!(
        std::fs::read_to_string(&output_file).unwrap(),
        "original artifact contents",
        "a strict failure must not overwrite the artifact file"
    );
}

#[test]
fn strict_gate_fires_through_a_direct_run_with_call() {
    // `run_with` is a public entry point that a caller may drive without opening
    // a capture window of its own. The shared `collect_run_warnings` boundary now
    // opens one, so strict mode enforces here just as it does through `run`,
    // rather than silently passing for want of a window.
    let result = app(true).run_with(
        command(),
        ["app", "balanced-unknown"],
        standout::TargetProperties::detect(),
        standout::InputSources::from_process(),
    );

    match result.outcome() {
        DispatchResult::Error(error) => {
            assert_eq!(error.kind(), RunErrorKind::Render);
            assert!(
                error.as_str().contains("bogus"),
                "error should name the tag: {}",
                error.as_str()
            );
        }
        other => panic!("expected a strict Render error, got {other:?}"),
    }
}

#[test]
fn strict_on_reports_the_failure_once_by_dropping_the_degrade_warning() {
    let result = run(true, "balanced-unknown");
    assert!(
        !result
            .warnings()
            .iter()
            .any(|w| w.contains("degraded to unstyled text")),
        "the superseded degrade warning must be dropped once strict escalates, got {:?}",
        result.warnings()
    );
}
