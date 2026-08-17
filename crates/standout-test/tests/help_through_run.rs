//! Themed help on the `run()` path, on the same terms as `get_matches_from`.
//!
//! The motivating application for the `help` word enters through `run()`, so
//! every form of help has to work there: the word under the same install
//! policy, `--help` / `-h` through Clap's short-circuit, the word's own
//! arguments, and the pager request. Two entry points that disagree about what
//! `myapp help` means would be the reported defect one layer down — so the
//! tests that matter here assert *agreement*, not just that something was
//! rendered.
//!
//! Both shapes come from the shared downstream fixture
//! ([`standout_fixtures`]): its flat form for the word's install policy, its
//! nested form for the walk that decides which command a help request targets.
//! The fixture also carries the app theme, which is what lets this file assert
//! the themed path through `run()` without a stylesheet on disk. The one app
//! still built here is the broken-theme one, because a theme that cannot
//! resolve is that test's subject.
//!
//! `USAGE` is the discriminator throughout: standout's template renders the
//! section header uppercase, while Clap's own help says `Usage:`.

use clap::Command;
use serde_json::json;
use serial_test::serial;
use standout::cli::{App, ExitStatus, HelpResult, Output, RunErrorKind, SuccessKind};
use standout::Theme;
use standout_fixtures::downstream;
use standout_test::TestHarness;

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
    let fixture = downstream().flat().build();
    let result =
        TestHarness::new()
            .text_output()
            .run(fixture.app(), fixture.command(), ["lookma", "help"]);

    result.assert_success();
    assert_eq!(result.success_kind(), Some(SuccessKind::ClapHelp));
    result.assert_stdout_contains("Diff a git range");
    result.assert_stdout_contains("USAGE");
}

/// The combination no fixture used to carry: an app theme *and* enabled help
/// handling, rendered on the path that applies style tags. An app theme knows
/// nothing of the help template's vocabulary, so a theme that replaced the
/// help theme rather than overlaying it would leave `[header?]` markers on the
/// page — and the option cues would be the first casualties.
#[test]
#[serial]
fn downstream_theme_help_preserves_option_cues_through_run() {
    let fixture = downstream().build();
    let result =
        TestHarness::new()
            .with_color()
            .run(fixture.app(), fixture.command(), ["lookma", "help"]);

    result.assert_success();
    assert_eq!(result.success_kind(), Some(SuccessKind::ClapHelp));
    let output = result.stdout();
    let plain = result.stdout_plain();
    assert!(
        !output.contains("[header?]") && !output.contains("[metavar?]"),
        "app output themes must not leak unresolved help tags:\n{output}"
    );
    assert!(plain.contains("RANGE"), "{plain}");
    assert!(plain.contains("--threshold <RATIO>"), "{plain}");
    assert!(plain.contains("-c, --color <BOOL>"), "{plain}");
    assert!(
        plain.contains("--pattern <pattern>"),
        "fallback metavars should survive the App run path:\n{plain}"
    );
    assert!(
        plain.contains("default: brief") && plain.contains("possible values: brief, full, none"),
        "defaults and enumerated values should survive the App run path:\n{plain}"
    );
    let staged = plain
        .find("--staged")
        .expect("rendered help should include --staged");
    let threshold = plain
        .find("--threshold")
        .expect("rendered help should include --threshold");
    let staged_block = &plain[staged..threshold];
    assert!(
        !staged_block.contains('<') && !staged_block.contains("possible values:"),
        "presence flags should not advertise bool values:\n{plain}"
    );
}

#[test]
#[serial]
fn the_help_flags_render_themed_help_through_run() {
    let fixture = downstream().flat().without_help_word().build();

    for flag in ["--help", "-h"] {
        let result = TestHarness::new().text_output().run(
            fixture.app(),
            fixture.command(),
            ["lookma", flag],
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
    let fixture = downstream().flat().build();
    let result = TestHarness::new().text_output().run(
        fixture.app(),
        fixture.command(),
        ["lookma", "help", "ranges"],
    );

    result.assert_success();
    result.assert_stdout_contains("two revisions separated by two dots");
}

#[test]
#[serial]
fn the_help_word_renders_a_subcommands_help_through_run() {
    let fixture = downstream().build();

    let word = TestHarness::new().text_output().run(
        fixture.app(),
        fixture.command(),
        ["lookma", "help", "review"],
    );
    word.assert_success();
    word.assert_stdout_contains("Review a range hunk by hunk");
    drop(word);

    let flag = TestHarness::new().text_output().run(
        fixture.app(),
        fixture.command(),
        ["lookma", "review", "--help"],
    );
    flag.assert_success();
    flag.assert_stdout_contains("Review a range hunk by hunk");
}

// --- the word's own arguments ----------------------------------------------

#[test]
#[serial]
fn the_help_word_honours_the_output_flag_through_run() {
    let fixture = downstream().flat().build();

    // The mode reaches the renderer: `term-debug` leaves style tags visible.
    let tagged = TestHarness::new().run(
        fixture.app(),
        fixture.command(),
        ["lookma", "help", "--output", "term-debug"],
    );
    tagged.assert_success();
    tagged.assert_stdout_contains("[header]USAGE[/header]");
    drop(tagged);

    // `json` is not a serialization of help — like every structured mode it
    // strips the style tags off the same rendered template.
    let json = TestHarness::new().run(
        fixture.app(),
        fixture.command(),
        ["lookma", "help", "--output", "json"],
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
    // doc cannot go stale quietly: the word is a subcommand, so clap parses its
    // line in full, globals included, while `--help` short-circuits before the
    // parse completes — so its render has no mode to honour and falls back to
    // `Auto`.
    let fixture = downstream().flat().build();

    let word = TestHarness::new().no_color().run(
        fixture.app(),
        fixture.command(),
        ["lookma", "help", "--output", "term-debug"],
    );
    word.assert_stdout_contains("[header]USAGE[/header]");
    drop(word);

    let flag = TestHarness::new().no_color().run(
        fixture.app(),
        fixture.command(),
        ["lookma", "--help", "--output", "term-debug"],
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
    let fixture = downstream().flat().build();
    let result = TestHarness::new().text_output().run(
        fixture.app(),
        fixture.command(),
        ["lookma", "help", "--page"],
    );

    result.assert_success();
    assert_eq!(result.success_kind(), Some(SuccessKind::PagedHelp));
    result.assert_stdout_contains("USAGE");
}

// --- which command the help request targets ---------------------------------

/// Root help carries the root's own arguments; a subcommand's help carries its
/// own, so this tells the two renderings apart on `-h` and `--help` alike.
fn assert_is_root_help(rendered: &str) {
    assert!(
        rendered.contains("Git range to diff"),
        "expected the root's help, got:\n{rendered}"
    );
}

#[test]
#[serial]
fn an_option_value_is_not_read_as_the_targeted_command() {
    // `--output-file-path` takes a value, so `review` is that value and the
    // help request is the root's. A walk that skipped every token starting
    // with `-` and took the next word would render `review`'s help here.
    let fixture = downstream().build();
    let args = ["lookma", "--output-file-path", "review", "--help"];

    let dispatched = TestHarness::new().run(fixture.app(), fixture.command(), args);
    dispatched.assert_success();
    assert_is_root_help(dispatched.stdout());
    drop(dispatched);

    // The two entry points share the walk, so they answer alike.
    assert_is_root_help(&configured_help(fixture.app(), fixture.command(), &args));
}

#[test]
#[serial]
fn a_short_option_value_is_not_read_as_the_targeted_command() {
    let fixture = downstream().build();
    let result = TestHarness::new().run(
        fixture.app(),
        fixture.command(),
        ["lookma", "-p", "review", "--help"],
    );

    result.assert_success();
    assert_is_root_help(result.stdout());
}

#[test]
#[serial]
fn the_walk_stops_where_the_help_request_is() {
    // Help was asked for before any command was named, so it is the root's; a
    // walk that strode past the flag would answer `review`.
    let fixture = downstream().build();

    for flag in ["--help", "-h"] {
        let result =
            TestHarness::new().run(fixture.app(), fixture.command(), ["lookma", flag, "review"]);

        result.assert_success();
        assert_is_root_help(result.stdout());
    }
}

// --- a help that cannot be rendered is the app's bug, not the user's --------

/// A theme whose alias names a style that does not exist: rendering fails
/// validation. This is the shape a downstream app hits when it loads an
/// override stylesheet from a directory at runtime and the file is malformed —
/// and it is the opposite failure from the fixture's own theme, which resolves
/// fine and is merely incomplete.
fn broken_theme() -> Theme {
    Theme::new().add("header", "no-such-style")
}

fn app_with_a_broken_theme() -> App {
    App::builder()
        .help_handling(true)
        .theme(broken_theme())
        .command("review", |_m, _ctx| Ok(Output::Render(json!({}))), "listed")
        .unwrap()
        .build()
        .unwrap()
}

#[test]
#[serial]
fn a_help_that_cannot_be_rendered_is_not_a_usage_error() {
    // The user's line was fine; the application's theme was not. Reporting it
    // as `ClapUsage` would blame the line and exit with the usage status.
    let fixture = downstream().build();

    for args in [
        &["lookma", "help"][..],
        &["lookma", "--help"][..],
        &["lookma", "-h"][..],
    ] {
        let result = TestHarness::new().run(&app_with_a_broken_theme(), fixture.command(), args);

        result.assert_error();
        result.assert_error_kind(RunErrorKind::Render);
        result.assert_exit_status(ExitStatus::FAILURE);
        result.assert_error_contains("failed to render help");
    }
}

#[test]
#[serial]
fn a_render_failure_is_not_disguised_as_an_unrecognized_topic() {
    // `review` is a real command. A render failure used to be swallowed by the
    // `if let Ok` around each rendering step, so the request fell through to
    // "the subcommand or topic 'review' wasn't recognized" — a usage error, and
    // an untrue one.
    let fixture = downstream().build();
    let result = TestHarness::new().run(
        &app_with_a_broken_theme(),
        fixture.command(),
        ["lookma", "help", "review"],
    );

    result.assert_error();
    result.assert_error_kind(RunErrorKind::Render);
    result.assert_error_contains("failed to render help");
    assert!(
        !result
            .error()
            .unwrap_or_default()
            .contains("wasn't recognized"),
        "a broken theme is not an unknown topic: {:?}",
        result.error()
    );
}

// --- agreement between the two entry points --------------------------------

#[test]
#[serial]
fn both_entry_points_render_the_same_help() {
    let fixture = downstream().flat().build();

    for args in [
        ["lookma", "help", "--output", "text"],
        ["lookma", "--help", "--output", "text"],
    ] {
        let dispatched = TestHarness::new().run(fixture.app(), fixture.command(), args);
        dispatched.assert_success();
        let configured = configured_help(fixture.app(), fixture.command(), &args);
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
    let fixture = downstream().flat().without_help_word().build();
    let result =
        TestHarness::new()
            .text_output()
            .run(fixture.app(), fixture.command(), ["lookma", "help"]);

    result.assert_success();
    result.assert_stdout_eq("range=help");
}

#[test]
#[serial]
fn the_escape_delivers_the_literal_word_through_run() {
    // No forced output mode here: the harness appends its `--output` flag to
    // the end of the line, and everything after `--` is a positional.
    let fixture = downstream().flat().build();
    let result = TestHarness::new().run(fixture.app(), fixture.command(), ["lookma", "--", "help"]);

    result.assert_success();
    result.assert_stdout_eq("range=help");
}

#[test]
#[serial]
fn a_normal_invocation_is_untouched_through_run() {
    let fixture = downstream().flat().build();
    let result = TestHarness::new().text_output().run(
        fixture.app(),
        fixture.command(),
        ["lookma", "main..HEAD"],
    );

    result.assert_success();
    result.assert_stdout_eq("range=main..HEAD");
}
