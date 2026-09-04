use clap::Command;
use clapfig::{Clapfig, SearchPath};
use console::Style;
use serde::{Deserialize, Serialize};
use serde_json::json;
use standout::cli::{
    App, AppBuilder, EventsFnHandler, FnHandler, HandlerResult, Output, Results, RunErrorKind,
    TermSettings,
};
use standout::{ColorPolicy, EmbeddedTemplates, Representation, Theme};
use standout_test::{serial, TestHarness};

const ESC: char = '\u{1b}';

const TEMPLATES: &[(&str, &str)] = &[("list", "[title]{{ name }}[/title]")];

fn themed() -> Theme {
    Theme::new().add("title", Style::new().cyan())
}

fn list_command(builder: AppBuilder) -> AppBuilder {
    builder
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(themed())
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({ "name": "milk" })))),
            |cfg| cfg.template_name("list"),
        )
        .unwrap()
}

fn app() -> App {
    list_command(App::builder()).build().unwrap()
}

fn cmd() -> Command {
    Command::new("colorapp").subcommand(Command::new("list"))
}

#[derive(Debug, Clone, Serialize, Deserialize, clapfig::Schema)]
struct FixtureConfig {
    term: TermSettings,
}

fn configured_app() -> App {
    list_command(App::builder())
        .config(
            Clapfig::typed::<FixtureConfig>()
                .app_name("colorapp")
                .search_paths(vec![SearchPath::Cwd]),
        )
        .term_settings(|config: &FixtureConfig| &config.term)
        .build()
        .unwrap()
}

/// The harness models a pipe by default: nothing is a terminal and stdout has
/// no color capability.
fn piped() -> TestHarness {
    TestHarness::new()
}

#[test]
fn the_flag_takes_three_values_and_defaults_to_auto() {
    let help = piped().run(&app(), cmd(), ["colorapp", "--help"]);
    help.assert_success();
    help.assert_stdout_contains("--color <WHEN>");
    help.assert_stdout_contains("possible values: auto, always, never");
    help.assert_stdout_contains("default: auto");

    let refused = piped().run(&app(), cmd(), ["colorapp", "list", "--color", "sometimes"]);
    refused.assert_error_kind(RunErrorKind::ClapUsage);
}

#[test]
fn the_flag_is_on_every_command_the_way_output_is() {
    for args in [
        &["colorapp", "--color", "always", "list"][..],
        &["colorapp", "list", "--color", "always"][..],
    ] {
        let result = piped().run(&app(), cmd(), args);
        result.assert_success();
        assert!(
            result.stdout().contains(ESC),
            "`{args:?}` should have emitted ANSI, got: {:?}",
            result.stdout()
        );
    }
}

#[test]
fn always_emits_ansi_into_a_pipe_and_never_suppresses_it_on_a_terminal() {
    let piped_always = piped().run(&app(), cmd(), ["colorapp", "list", "--color", "always"]);
    piped_always.assert_success();
    assert!(
        piped_always.stdout().contains(ESC),
        "got: {:?}",
        piped_always.stdout()
    );

    let terminal_never = TestHarness::new().color_capable_terminal().run(
        &app(),
        cmd(),
        ["colorapp", "list", "--color", "never"],
    );
    terminal_never.assert_success();
    assert!(
        !terminal_never.stdout().contains(ESC),
        "got: {:?}",
        terminal_never.stdout()
    );
}

#[test]
fn auto_reads_the_destination() {
    let terminal = TestHarness::new().color_capable_terminal().run(
        &app(),
        cmd(),
        ["colorapp", "list", "--color", "auto"],
    );
    terminal.assert_success();
    assert!(
        terminal.stdout().contains(ESC),
        "got: {:?}",
        terminal.stdout()
    );

    let pipe = piped().run(&app(), cmd(), ["colorapp", "list", "--color", "auto"]);
    pipe.assert_success();
    assert!(!pipe.stdout().contains(ESC), "got: {:?}", pipe.stdout());
}

#[test]
fn one_template_renders_colored_and_plain_without_a_second_layout() {
    let colored = piped().run(&app(), cmd(), ["colorapp", "list", "--color", "always"]);
    let plain = piped().run(&app(), cmd(), ["colorapp", "list", "--color", "never"]);

    assert_eq!(
        console::strip_ansi_codes(colored.stdout()),
        plain.stdout(),
        "the same template must produce the same text under either policy"
    );
    assert_eq!(plain.stdout(), "milk");
}

#[test]
fn no_structured_encoding_carries_ansi_under_any_color_setting() {
    for encoding in ["json", "yaml", "csv", "ndjson"] {
        for when in ["auto", "always", "never"] {
            let result = TestHarness::new().color_capable_terminal().run(
                &app(),
                cmd(),
                ["colorapp", "list", "--output", encoding, "--color", when],
            );
            result.assert_success();
            assert!(
                !result.stdout().contains(ESC),
                "--output {encoding} --color {when} carried ANSI: {:?}",
                result.stdout()
            );
        }
    }
}

#[test]
fn term_debug_is_not_a_color_selection_alias() {
    for when in ["auto", "always", "never"] {
        let result = piped().run(
            &app(),
            cmd(),
            [
                "colorapp",
                "list",
                "--output",
                "term-debug",
                "--color",
                when,
            ],
        );
        result.assert_success();
        assert_eq!(result.output_mode(), Representation::TermDebug);
        assert_eq!(
            result.stdout(),
            "[title]milk[/title]",
            "term-debug renders the style tags whatever --color says"
        );
    }
}

/// A run that names an output file writes its events into it as they happen,
/// so the file's bytes are where the destination's answer shows.
fn emitting_app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(
            &[
                ("apply", "[title]{{ done }} done[/title]"),
                ("apply.event", "[title]{{ event.step }}[/title]"),
            ],
            "",
        ))
        .theme(themed())
        .command_with(
            "apply",
            EventsFnHandler::new(
                |_m,
                 _ctx,
                 results: &mut Results<serde_json::Value>|
                 -> HandlerResult<serde_json::Value> {
                    results.emit(json!({ "step": "one" }))?;
                    Ok(Output::Render(json!({ "done": 1 })))
                },
            ),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap()
}

fn emitting_command() -> Command {
    Command::new("colorapp").subcommand(Command::new("apply"))
}

#[test]
fn a_named_output_file_is_never_a_terminal() {
    let dir = tempfile::tempdir().unwrap();
    let written = |name: &str, args: &[&str]| {
        let path = dir.path().join(name);
        let mut argv: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        argv.push("--output-file-path".to_string());
        argv.push(path.display().to_string());
        TestHarness::new()
            .color_capable_terminal()
            .run(&emitting_app(), emitting_command(), argv)
            .assert_success();
        std::fs::read_to_string(path).unwrap()
    };

    let auto = written("auto.txt", &["colorapp", "apply"]);
    assert!(
        !auto.contains(ESC),
        "a file is not a terminal, so auto resolves to no escapes, got: {auto:?}"
    );

    let always = written("always.txt", &["colorapp", "apply", "--color", "always"]);
    assert!(
        always.contains(ESC),
        "an explicit --color always outranks the destination, got: {always:?}"
    );
}

#[test]
fn the_flag_renames_and_removes_through_the_output_seam() {
    let renamed = list_command(App::builder())
        .color_flag(Some("colour"))
        .build()
        .unwrap();
    let result = piped().run(&renamed, cmd(), ["colorapp", "list", "--colour", "always"]);
    result.assert_success();
    assert!(result.stdout().contains(ESC), "got: {:?}", result.stdout());

    let refused = piped().run(&renamed, cmd(), ["colorapp", "list", "--color", "always"]);
    refused.assert_error_kind(RunErrorKind::ClapUsage);

    let removed = list_command(App::builder())
        .no_color_flag()
        .build()
        .unwrap();
    let gone = piped().run(&removed, cmd(), ["colorapp", "list", "--color", "always"]);
    gone.assert_error_kind(RunErrorKind::ClapUsage);
}

const COLOR_ALWAYS: &str = "[term]\ncolor = \"always\"\n";
const COLOR_NEVER: &str = "[term]\ncolor = \"never\"\n";

#[test]
#[serial]
fn the_term_color_key_decides_when_the_flag_is_absent() {
    let configured = TestHarness::new()
        .env_remove("NO_COLOR")
        .fixture("colorapp.toml", COLOR_ALWAYS)
        .run(&configured_app(), cmd(), ["colorapp", "list"]);
    configured.assert_success();
    assert!(
        configured.stdout().contains(ESC),
        "got: {:?}",
        configured.stdout()
    );

    let suppressed = TestHarness::new()
        .env_remove("NO_COLOR")
        .color_capable_terminal()
        .fixture("colorapp.toml", COLOR_NEVER)
        .run(&configured_app(), cmd(), ["colorapp", "list"]);
    suppressed.assert_success();
    assert!(
        !suppressed.stdout().contains(ESC),
        "got: {:?}",
        suppressed.stdout()
    );
}

#[test]
#[serial]
fn an_explicit_flag_outranks_the_term_color_key() {
    let result = TestHarness::new()
        .env_remove("NO_COLOR")
        .fixture("colorapp.toml", COLOR_ALWAYS)
        .run(
            &configured_app(),
            cmd(),
            ["colorapp", "list", "--color", "never"],
        );
    result.assert_success();
    assert!(!result.stdout().contains(ESC), "got: {:?}", result.stdout());
}

#[test]
#[serial]
fn no_color_outranks_the_term_color_key() {
    let vetoed = TestHarness::new()
        .env("NO_COLOR", "1")
        .fixture("colorapp.toml", COLOR_ALWAYS)
        .run(&configured_app(), cmd(), ["colorapp", "list"]);
    vetoed.assert_success();
    assert!(!vetoed.stdout().contains(ESC), "got: {:?}", vetoed.stdout());

    let asked_for = TestHarness::new()
        .env("NO_COLOR", "1")
        .fixture("colorapp.toml", COLOR_ALWAYS)
        .run(
            &configured_app(),
            cmd(),
            ["colorapp", "list", "--color", "always"],
        );
    asked_for.assert_success();
    assert!(
        asked_for.stdout().contains(ESC),
        "an explicit --color always outranks NO_COLOR, got: {:?}",
        asked_for.stdout()
    );

    let empty = TestHarness::new()
        .env("NO_COLOR", "")
        .fixture("colorapp.toml", COLOR_ALWAYS)
        .run(&configured_app(), cmd(), ["colorapp", "list"]);
    empty.assert_success();
    assert!(
        empty.stdout().contains(ESC),
        "an empty NO_COLOR is not set, got: {:?}",
        empty.stdout()
    );
}

#[test]
fn the_harness_names_the_policy_and_the_destination_separately() {
    let forced =
        TestHarness::new()
            .color(ColorPolicy::Always)
            .run(&app(), cmd(), ["colorapp", "list"]);
    forced.assert_success();
    assert!(forced.stdout().contains(ESC), "got: {:?}", forced.stdout());

    let refused = TestHarness::new()
        .color(ColorPolicy::Never)
        .color_capable_terminal()
        .run(&app(), cmd(), ["colorapp", "list"]);
    refused.assert_success();
    assert!(
        !refused.stdout().contains(ESC),
        "got: {:?}",
        refused.stdout()
    );

    // The values are the same either way: only the presentation moved.
    assert_eq!(console::strip_ansi_codes(forced.stdout()), refused.stdout());
    assert_eq!(forced.result(), refused.result());
}
