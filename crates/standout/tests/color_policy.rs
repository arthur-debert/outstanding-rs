use clap::Command;
use clapfig::{Clapfig, SearchPath};
use console::Style;
use serde::{Deserialize, Serialize};
use serde_json::json;
use standout::cli::{
    App, AppBuilder, DispatchResult, EventsFnHandler, FnHandler, HandlerResult, HelpResult, Output,
    Results, RunErrorKind, StreamSink, TermSettings,
};
use standout::{ColorPolicy, EmbeddedTemplates, InputSources, Representation, TemplateRef, Theme};
use standout_test::{serial, TestHarness};

const ESC: char = '\u{1b}';
const CYAN: &str = "\u{1b}[36m";
const RESET: &str = "\u{1b}[0m";

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

fn written_to_a_file(
    app: &App,
    command: Command,
    args: &[&str],
    path: std::path::PathBuf,
) -> String {
    let mut argv: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    argv.push("--output-file-path".to_string());
    argv.push(path.display().to_string());
    TestHarness::new()
        .color_capable_terminal()
        .run(app, command, argv)
        .assert_success();
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn a_batch_run_writes_the_policy_it_resolved_into_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let written = |when: &str| {
        written_to_a_file(
            &app(),
            cmd(),
            &["colorapp", "list", "--color", when],
            dir.path().join(format!("{when}.txt")),
        )
    };

    assert_eq!(written("never"), "milk");
    assert_eq!(
        written("auto"),
        "milk",
        "a file is not a terminal, so auto resolves to no escapes"
    );
    assert_eq!(written("always"), format!("{CYAN}milk{RESET}"));
}

#[test]
fn the_events_and_the_summary_reach_the_file_under_one_policy() {
    let dir = tempfile::tempdir().unwrap();
    let written = |when: &str| {
        written_to_a_file(
            &emitting_app(),
            emitting_command(),
            &["colorapp", "apply", "--color", when],
            dir.path().join(format!("{when}.txt")),
        )
    };

    assert_eq!(written("never"), "one\n1 done");
    assert_eq!(written("auto"), "one\n1 done");
    assert_eq!(
        written("always"),
        format!("{CYAN}one{RESET}\n{CYAN}1 done{RESET}")
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

struct Cwd(std::path::PathBuf);

impl Cwd {
    fn enter(dir: &std::path::Path) -> Self {
        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir).unwrap();
        Self(previous)
    }
}

impl Drop for Cwd {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.0);
    }
}

struct NoColorAbsent(Option<std::ffi::OsString>);

impl NoColorAbsent {
    fn enter() -> Self {
        let previous = std::env::var_os("NO_COLOR");
        std::env::remove_var("NO_COLOR");
        Self(previous)
    }
}

impl Drop for NoColorAbsent {
    fn drop(&mut self) {
        if let Some(previous) = self.0.take() {
            std::env::set_var("NO_COLOR", previous);
        }
    }
}

/// The partial-adoption entry point, which resolves the same run facts the
/// argv path does.
fn run_command_list(app: &App, args: &[&str], named: ColorPolicy) -> String {
    let matches = match app.get_matches_from(cmd(), args, &InputSources::from_process()) {
        HelpResult::Matches(matches) => matches,
        other => panic!("{other:?}"),
    };
    let sub = matches.subcommand_matches("list").unwrap();
    app.run_command(
        "list",
        sub,
        FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({ "name": "milk" })))),
        TemplateRef::Inline(TEMPLATES[0].1.to_string()),
        named,
        StreamSink::new(Vec::new()),
    )
    .unwrap()
    .as_text()
    .unwrap()
    .to_string()
}

#[test]
fn run_command_lets_the_typed_flag_outrank_the_policy_the_caller_named() {
    let forced = run_command_list(
        &app(),
        &["colorapp", "list", "--color", "always"],
        ColorPolicy::Never,
    );
    assert_eq!(forced, format!("{CYAN}milk{RESET}"));

    let refused = run_command_list(
        &app(),
        &["colorapp", "list", "--color", "never"],
        ColorPolicy::Always,
    );
    assert_eq!(refused, "milk");
}

#[test]
#[serial]
fn a_run_that_cannot_load_its_config_still_reports_the_typed_policy() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("colorapp.toml"), "bogus_key = 1\n").unwrap();
    let _cwd = Cwd::enter(dir.path());

    let app = configured_app();
    let matches = match app.get_matches_from(
        cmd(),
        ["colorapp", "list", "--color", "never"],
        &InputSources::from_process(),
    ) {
        HelpResult::Matches(matches) => matches,
        other => panic!("{other:?}"),
    };
    let run = app.dispatch(matches, Representation::Human);

    assert!(
        matches!(run.outcome(), DispatchResult::Error(_)),
        "the bad file should have failed the run: {:?}",
        run.outcome()
    );
    assert_eq!(run.color_policy(), ColorPolicy::Never);
}

#[test]
#[serial]
fn run_command_reads_the_term_color_key_under_an_auto_policy() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("colorapp.toml"), COLOR_ALWAYS).unwrap();
    let _no_color = NoColorAbsent::enter();
    let _cwd = Cwd::enter(dir.path());

    let configured = run_command_list(&configured_app(), &["colorapp", "list"], ColorPolicy::Auto);
    assert_eq!(configured, format!("{CYAN}milk{RESET}"));
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
