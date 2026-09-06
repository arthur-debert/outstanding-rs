use clap::{Arg, ArgAction, Command};
use console::Style;
use serde_json::json;
use serial_test::serial;
use standout::cli::FnHandler;
use standout::cli::{App, Output};
use standout::EmbeddedTemplates;
use standout::Theme;
use standout_test::TestHarness;

const TEMPLATES: &[(&str, &str)] = &[("say", "[shout]hello[/shout]"), ("list", "listed")];
const RED: &str = "\u{1b}[31m";
fn styled_app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(Theme::new().add("shout", Style::new().red()))
        .command_with(
            "say",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({})))),
            |cfg| cfg,
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
        .subcommand(Command::new("list").about("List the notes"))
}
#[test]
#[serial]
fn with_color_makes_a_styled_render_ansi_positive() {
    let result = TestHarness::new().color_capable_terminal().run(
        &styled_app(),
        styled_command(),
        ["app", "say"],
    );
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
    let result = TestHarness::new().color_capable_terminal().run(
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
    let term = TestHarness::new().color_capable_terminal().run(
        &styled_app(),
        styled_command(),
        ["app", "say"],
    );
    let styled = term.stdout().to_string();
    let stripped = term.stdout_plain();
    drop(term);
    let text = TestHarness::new().stdout_is_terminal(false).run(
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
        .color_capable_terminal()
        .terminal_width(80)
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
    assert!(result.stdout_plain().contains("--file <PATH>"), "{raw:?}");
}
#[test]
#[serial]
fn with_color_does_not_call_set_colors_enabled() {
    let before = console::colors_enabled();
    let result = TestHarness::new().color_capable_terminal().run(
        &styled_app(),
        styled_command(),
        ["app", "say"],
    );
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
