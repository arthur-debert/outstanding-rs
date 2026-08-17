//! The two gates between a styled template and ANSI bytes, and the worked
//! example that proves the harness opens both.
//!
//! Standout's own color decision (`OutputMode::should_use_color`) is only the
//! first gate. The second is `console`'s process-global color switch, which
//! `Style::apply_to` reads before emitting an escape and which is off in a
//! non-TTY process — and a test binary is never a TTY. Opening gate 1 alone
//! yields plain text, which is why in-repo tests that wanted real escapes
//! reached for `force_styling(true)` on the fixture theme or called
//! `set_colors_enabled(true)` by hand.
//!
//! `.with_color()` now opens both, so ANSI-positive assertions work
//! in-process against an *unmodified* theme — including the default help
//! theme, which sets `force_styling` on none of its styles and was the
//! specific reason to doubt this was reachable at all.

use clap::{Arg, ArgAction, Command};
use console::Style;
use serde_json::json;
use serial_test::serial;
use standout::cli::{App, Output};
use standout::Theme;
use standout_render::OutputMode;
use standout_test::TestHarness;

const RED: &str = "\u{1b}[31m";

/// A styled app whose theme is exactly what an application would write: a
/// style with no `force_styling` escape hatch. Whether its escapes reach
/// stdout is therefore entirely the two gates' decision.
fn styled_app() -> App {
    App::builder()
        .theme(Theme::new().add("shout", Style::new().red()))
        .command(
            "say",
            |_m, _ctx| Ok(Output::Render(json!({}))),
            "[shout]hello[/shout]",
        )
        .unwrap()
        .build()
        .unwrap()
}

fn styled_command() -> Command {
    Command::new("app").subcommand(Command::new("say"))
}

/// A help-bearing app with no theme of its own, so its help page renders
/// through `default_help_theme()` — the theme whose missing `force_styling`
/// was the reason to doubt an in-process ANSI assertion was possible.
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
        .subcommand(Command::new("list").about("List the notes"))
}

/// The worked example: an unmodified theme, a `Term` render, one builder
/// call, real escapes.
#[test]
#[serial]
fn with_color_makes_a_styled_render_ansi_positive() {
    let result = TestHarness::new()
        .with_color()
        .output_mode(OutputMode::Term)
        .run(&styled_app(), styled_command(), ["app", "say"]);

    let raw = result.stdout();
    assert!(
        raw.contains(RED),
        "with_color() must produce real escapes with no force_styling in the theme: {raw:?}"
    );
    assert_eq!(result.stdout_plain().trim_end(), "hello");
}

/// The other half of the same fact: without `with_color()`, gate 2 stays
/// shut and the identical run emits no escapes. This is the behavior that
/// made `strip_ansi(Term) == Text` a comparison of two no-ops.
#[test]
#[serial]
fn a_term_render_without_with_color_emits_no_escapes() {
    let result = TestHarness::new().output_mode(OutputMode::Term).run(
        &styled_app(),
        styled_command(),
        ["app", "say"],
    );

    assert!(
        !result.stdout().contains('\u{1b}'),
        "a test process is not a TTY, so console suppresses styling: {:?}",
        result.stdout()
    );
}

/// `strip_ansi(Term) == Text` with both sides non-trivial: the `Term` side
/// carries escapes, so the equality is an assertion about the styling being
/// purely additive rather than about two identical plain strings.
#[test]
#[serial]
fn stripping_a_colored_term_render_recovers_the_text_render() {
    let term = TestHarness::new()
        .with_color()
        .output_mode(OutputMode::Term)
        .run(&styled_app(), styled_command(), ["app", "say"]);
    let styled = term.stdout().to_string();
    let stripped = term.stdout_plain();
    drop(term);

    let text = TestHarness::new().output_mode(OutputMode::Text).run(
        &styled_app(),
        styled_command(),
        ["app", "say"],
    );

    assert!(
        styled.contains(RED),
        "the Term side must carry escapes or this proves nothing: {styled:?}"
    );
    assert_eq!(stripped, text.stdout());
}

/// The help path specifically: `default_help_theme()` sets `force_styling`
/// on none of its nine styles, so a colored help page was the case the
/// workstream was told to expect to fail.
#[test]
#[serial]
fn the_help_page_renders_ansi_through_the_default_help_theme() {
    let result = TestHarness::new()
        .with_color()
        .terminal_width(80)
        .output_mode(OutputMode::Term)
        .run(&help_app(), help_command(), ["notes", "--help"]);

    result.assert_success();
    let raw = result.stdout();
    assert!(
        raw.contains('\u{1b}'),
        "a colored help render must carry escapes: {raw:?}"
    );
    assert!(
        !raw.contains("?]"),
        "a colored help render must resolve every tag: {raw:?}"
    );
    assert!(result.stdout_plain().contains("--file <PATH>"));
}

/// The switch is process-global, so the harness has to hand it back. Without
/// this, one colored test would silently color every later test in the same
/// binary.
#[test]
#[serial]
fn the_color_switch_is_restored_when_the_run_is_dropped() {
    let before = console::colors_enabled();

    let result = TestHarness::new()
        .with_color()
        .output_mode(OutputMode::Term)
        .run(&styled_app(), styled_command(), ["app", "say"]);
    assert!(console::colors_enabled(), "the run must open gate 2");
    drop(result);

    assert_eq!(
        console::colors_enabled(),
        before,
        "with_color() must not leak into the next test in this binary"
    );
}
