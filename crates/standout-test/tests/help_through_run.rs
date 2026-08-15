//! Themed help on the `run()` path, on the same terms as `get_matches_from`.
//!
//! The motivating application for name-first selection enters through `run()`,
//! so every form of help has to work there: the `help` word under the same
//! install policy, `--help` / `-h` through Clap's short-circuit, the word's own
//! arguments, and the pager request. Two entry points that disagree about what
//! `myapp help` means would be the same defect the ordering change removed,
//! one layer down — so the tests that matter here assert *agreement*, not just
//! that something was rendered.
//!
//! `USAGE` is the discriminator throughout: standout's template renders the
//! section header uppercase, while Clap's own help says `Usage:`.

use clap::{Arg, ArgAction, ArgGroup, Command};
use serde_json::json;
use serial_test::serial;
use standout::cli::{App, HelpResult, Output, SuccessKind};
use standout::topics::{Topic, TopicType};
use standout_test::TestHarness;

/// The shape from the issue: one optional positional, one flag, and a required
/// group over the two — a root whose requirements fire before anything can
/// decide what a bare word meant.
fn flat_required_command() -> Command {
    Command::new("app")
        .about("Flat app")
        .arg(Arg::new("range").help("A revision range"))
        .arg(
            Arg::new("staged")
                .long("staged")
                .action(ArgAction::SetTrue)
                .help("Use the staged diff"),
        )
        .group(
            ArgGroup::new("target")
                .args(["range", "staged"])
                .required(true),
        )
}

/// A flat app whose only handler is the root's, as `command_with("", …)` apps
/// are shaped. `help_word` toggles the opt-in the install policy asks for.
fn flat_app(help_word: bool) -> App {
    App::builder()
        .help_handling(true)
        .help_word(help_word)
        .add_topic(Topic::new(
            "Ranges",
            "A range is two revisions separated by two dots.",
            TopicType::Text,
            Some("ranges".to_string()),
        ))
        .command(
            "",
            |m, _ctx| {
                Ok(Output::Render(json!({
                    "range": m.get_one::<String>("range").cloned().unwrap_or_default(),
                })))
            },
            "range={{ range }}",
        )
        .unwrap()
        .build()
        .unwrap()
}

fn subcommand_command() -> Command {
    Command::new("app")
        .about("Subcommand app")
        .arg(
            Arg::new("file")
                .short('f')
                .action(ArgAction::Set)
                .help("A file to read"),
        )
        .subcommand(Command::new("list").about("List the things"))
}

fn subcommand_app() -> App {
    App::builder()
        .help_handling(true)
        .command("list", |_m, _ctx| Ok(Output::Render(json!({}))), "listed")
        .unwrap()
        .build()
        .unwrap()
}

/// The text `get_matches_from` renders for the same line, for agreement checks.
fn configured_help(app: &App, cmd: Command, args: &[&str]) -> String {
    match app.get_matches_from(cmd, args) {
        HelpResult::Help(h) | HelpResult::PagedHelp(h) => h,
        other => panic!("expected rendered help, got {other:?}"),
    }
}

// --- the word and the flags ------------------------------------------------

#[test]
#[serial]
fn the_help_word_renders_themed_help_through_run() {
    let result = TestHarness::new().text_output().run(
        &flat_app(true),
        flat_required_command(),
        ["app", "help"],
    );

    result.assert_success();
    assert_eq!(result.success_kind(), Some(SuccessKind::ClapHelp));
    result.assert_stdout_contains("Flat app");
    result.assert_stdout_contains("USAGE");
}

#[test]
#[serial]
fn the_help_flags_render_themed_help_through_run() {
    for flag in ["--help", "-h"] {
        let result = TestHarness::new().text_output().run(
            &flat_app(false),
            flat_required_command(),
            ["app", flag],
        );

        result.assert_success();
        assert_eq!(result.success_kind(), Some(SuccessKind::ClapHelp));
        // Rendered by standout, not handed back as Clap's own text: the root's
        // required group did not fire either way, but the header did not come
        // from Clap.
        result.assert_stdout_contains("USAGE");
    }
}

#[test]
#[serial]
fn the_help_word_renders_a_topic_through_run() {
    let result = TestHarness::new().text_output().run(
        &flat_app(true),
        flat_required_command(),
        ["app", "help", "ranges"],
    );

    result.assert_success();
    result.assert_stdout_contains("two revisions separated by two dots");
}

#[test]
#[serial]
fn the_help_word_renders_a_subcommands_help_through_run() {
    let app = subcommand_app();

    let word =
        TestHarness::new()
            .text_output()
            .run(&app, subcommand_command(), ["app", "help", "list"]);
    word.assert_success();
    word.assert_stdout_contains("List the things");
    drop(word);

    let flag =
        TestHarness::new()
            .text_output()
            .run(&app, subcommand_command(), ["app", "list", "--help"]);
    flag.assert_success();
    flag.assert_stdout_contains("List the things");
}

// --- the word's own arguments ----------------------------------------------

#[test]
#[serial]
fn the_help_word_honours_the_output_flag_through_run() {
    // The mode reaches the renderer: `term-debug` leaves style tags visible.
    let tagged = TestHarness::new().run(
        &flat_app(true),
        flat_required_command(),
        ["app", "help", "--output", "term-debug"],
    );
    tagged.assert_success();
    tagged.assert_stdout_contains("[header]USAGE[/header]");
    drop(tagged);

    // `json` is not a serialization of help — like every structured mode it
    // strips the style tags off the same rendered template.
    let json = TestHarness::new().run(
        &flat_app(true),
        flat_required_command(),
        ["app", "help", "--output", "json"],
    );
    json.assert_success();
    json.assert_stdout_contains("USAGE");
    assert!(
        !json.stdout().contains("[header]"),
        "structured modes strip style tags:\n{}",
        json.stdout()
    );
}

#[test]
#[serial]
fn the_output_flag_reaches_the_word_but_not_the_flags() {
    // A documented asymmetry (`docs/topics/standout-help.md`), pinned so the
    // doc cannot go stale quietly: the word is answered from its own arm, which
    // parses the root's globals, while `--help` short-circuits inside clap
    // before anything is parsed — so its render has no mode to honour and falls
    // back to `Auto`.
    let app = flat_app(true);

    let word = TestHarness::new().no_color().run(
        &app,
        flat_required_command(),
        ["app", "help", "--output", "term-debug"],
    );
    word.assert_stdout_contains("[header]USAGE[/header]");
    drop(word);

    let flag = TestHarness::new().no_color().run(
        &app,
        flat_required_command(),
        ["app", "--help", "--output", "term-debug"],
    );
    flag.assert_success();
    flag.assert_stdout_contains("USAGE");
    assert!(
        !flag.stdout().contains("[header]"),
        "`--help` renders in Auto, so the requested mode is not applied:\n{}",
        flag.stdout()
    );
}

#[test]
#[serial]
fn a_pager_request_rides_back_as_a_typed_success() {
    // `run()` is the only entry point that may spawn a pager, so the request
    // travels as a kind rather than as a side effect of capturing the text.
    let result = TestHarness::new().text_output().run(
        &flat_app(true),
        flat_required_command(),
        ["app", "help", "--page"],
    );

    result.assert_success();
    assert_eq!(result.success_kind(), Some(SuccessKind::PagedHelp));
    result.assert_stdout_contains("USAGE");
}

// --- which command the help request targets ---------------------------------

/// Root help carries the root's own `about`; a subcommand's help carries its
/// own, so this tells the two renderings apart.
fn assert_is_root_help(rendered: &str) {
    assert!(
        rendered.contains("Subcommand app"),
        "expected the root's help, got:\n{rendered}"
    );
}

#[test]
#[serial]
fn an_option_value_is_not_read_as_the_targeted_command() {
    // `--output-file-path` takes a value, so `list` is that value and the help
    // request is the root's. A walk that skipped every token starting with `-`
    // and took the next word would render `list`'s help here.
    let app = subcommand_app();
    let args = ["app", "--output-file-path", "list", "--help"];

    let dispatched = TestHarness::new().run(&app, subcommand_command(), args);
    dispatched.assert_success();
    assert_is_root_help(dispatched.stdout());
    drop(dispatched);

    // The two entry points share the walk, so they answer alike.
    assert_is_root_help(&configured_help(&app, subcommand_command(), &args));
}

#[test]
#[serial]
fn a_short_option_value_is_not_read_as_the_targeted_command() {
    let result = TestHarness::new().run(
        &subcommand_app(),
        subcommand_command(),
        ["app", "-f", "list", "--help"],
    );

    result.assert_success();
    assert_is_root_help(result.stdout());
}

#[test]
#[serial]
fn the_walk_stops_where_the_help_request_is() {
    // Help was asked for before any command was named, so it is the root's; a
    // walk that strode past the flag would answer `list`.
    for flag in ["--help", "-h"] {
        let result = TestHarness::new().run(
            &subcommand_app(),
            subcommand_command(),
            ["app", flag, "list"],
        );

        result.assert_success();
        assert_is_root_help(result.stdout());
    }
}

// --- agreement between the two entry points --------------------------------

#[test]
#[serial]
fn both_entry_points_render_the_same_help() {
    let app = flat_app(true);

    for args in [
        ["app", "help", "--output", "text"],
        ["app", "--help", "--output", "text"],
    ] {
        let dispatched = TestHarness::new().run(&app, flat_required_command(), args);
        dispatched.assert_success();
        let configured = configured_help(&app, flat_required_command(), &args);
        assert_eq!(
            dispatched.stdout(),
            configured,
            "entry points disagree for {args:?}"
        );
    }
}

// --- the install policy is the command's shape, not the entry point ---------

#[test]
#[serial]
fn a_flat_command_keeps_the_word_as_data_through_run_without_the_opt_in() {
    // No opt-in, so nothing is installed and `help` is what it looks like on a
    // root whose positional is free text: data, reaching the handler.
    let result = TestHarness::new().text_output().run(
        &flat_app(false),
        flat_required_command(),
        ["app", "help"],
    );

    result.assert_success();
    result.assert_stdout_eq("range=help");
}

#[test]
#[serial]
fn the_escape_delivers_the_literal_word_through_run() {
    // No forced output mode here: the harness appends its `--output` flag to
    // the end of the line, and everything after `--` is a positional.
    let result = TestHarness::new().run(
        &flat_app(true),
        flat_required_command(),
        ["app", "--", "help"],
    );

    result.assert_success();
    result.assert_stdout_eq("range=help");
}

#[test]
#[serial]
fn a_normal_invocation_is_untouched_through_run() {
    let result = TestHarness::new().text_output().run(
        &flat_app(true),
        flat_required_command(),
        ["app", "main..HEAD"],
    );

    result.assert_success();
    result.assert_stdout_eq("range=main..HEAD");
}
