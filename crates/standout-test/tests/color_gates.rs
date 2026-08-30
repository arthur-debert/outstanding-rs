use clap::{Arg, ArgAction, Command};
use console::Style;
use serde_json::json;
use serial_test::serial;
use standout::cli::{App, Output};
use standout::Theme;
use standout_render::OutputMode;
use standout_test::TestHarness;
const RED: &str = "\u{1b}[31m";
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
#[test]
#[serial]
fn a_term_render_without_with_color_emits_escapes() {
    let result = TestHarness::new().output_mode(OutputMode::Term).run(
        &styled_app(),
        styled_command(),
        ["app", "say"],
    );
    assert!(
        result.stdout().contains('\u{1b}'),
        "Term force_styling is a function of the request, not console's switch: {:?}",
        result.stdout()
    );
}
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
#[test]
#[serial]
fn with_color_does_not_call_set_colors_enabled() {
    let before = console::colors_enabled();
    let result = TestHarness::new()
        .with_color()
        .output_mode(OutputMode::Term)
        .run(&styled_app(), styled_command(), ["app", "say"]);
    assert_eq!(
        console::colors_enabled(),
        before,
        "with_color() must not write console's process-global switch"
    );
    drop(result);
    assert_eq!(
        console::colors_enabled(),
        before,
        "with_color() must not leak a console switch change"
    );
}
