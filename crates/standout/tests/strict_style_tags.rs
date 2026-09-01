//! Integration coverage for `AppBuilder::strict_style_tags`: the opt-in gate
//! that turns an unresolved style tag from a graceful degradation into a hard
//! failure. The default path (off) must stay exactly as before — degrade to
//! unstyled text plus a stderr warning.

use clap::Command;
use serde_json::json;
use standout::cli::handler::{ExitStatus, RunErrorKind};
use standout::cli::{App, FnHandler, Output};
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

#[test]
fn strict_on_fails_and_names_a_balanced_unresolved_tag() {
    let result = run(true, "balanced-unknown");
    result.assert_exit_status(ExitStatus::FAILURE);
    result.assert_error_kind(RunErrorKind::Render);
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
