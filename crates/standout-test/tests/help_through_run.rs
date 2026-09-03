use clap::Command;
use serde_json::json;
use serial_test::serial;
use standout::cli::FnHandler;
use standout::cli::{App, ExitStatus, HelpResult, Output, RunErrorKind, SuccessKind};
use standout::EmbeddedTemplates;
use standout::Theme;
use standout_fixtures::downstream;
use standout_test::TestHarness;

const TEMPLATES: &[(&str, &str)] = &[("review", "listed")];
fn configured_help(app: &App, cmd: Command, args: &[&str]) -> String {
    match app.get_matches_from(cmd, args, &standout::InputSources::from_process()) {
        HelpResult::Help(h) | HelpResult::PagedHelp(h) => h,
        other => panic!("expected rendered help, got {other:?}"),
    }
}
#[test]
#[serial]
fn the_help_word_renders_themed_help_through_run() {
    let fixture = downstream().flat().build();
    let result = TestHarness::new().stdout_is_terminal(false).run(
        fixture.app(),
        fixture.command(),
        ["lookma", "help"],
    );
    result.assert_success();
    assert_eq!(result.success_kind(), Some(SuccessKind::ClapHelp));
    result.assert_stdout_contains("Diff a git range");
    result.assert_stdout_contains("USAGE");
}
#[test]
#[serial]
fn downstream_theme_help_preserves_option_cues_through_run() {
    let fixture = downstream().build();
    let result = TestHarness::new().stdout_is_terminal(true).run(
        fixture.app(),
        fixture.command(),
        ["lookma", "help"],
    );
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
        let result = TestHarness::new().stdout_is_terminal(false).run(
            fixture.app(),
            fixture.command(),
            ["lookma", flag],
        );
        result.assert_success();
        assert_eq!(result.success_kind(), Some(SuccessKind::ClapHelp));
        result.assert_stdout_contains("USAGE");
    }
}
#[test]
#[serial]
fn the_help_word_renders_a_topic_through_run() {
    let fixture = downstream().flat().build();
    let result = TestHarness::new().stdout_is_terminal(false).run(
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
    let word = TestHarness::new().stdout_is_terminal(false).run(
        fixture.app(),
        fixture.command(),
        ["lookma", "help", "review"],
    );
    word.assert_success();
    word.assert_stdout_contains("Review a range hunk by hunk");
    drop(word);
    let flag = TestHarness::new().stdout_is_terminal(false).run(
        fixture.app(),
        fixture.command(),
        ["lookma", "review", "--help"],
    );
    flag.assert_success();
    flag.assert_stdout_contains("Review a range hunk by hunk");
}
#[test]
#[serial]
fn the_help_word_honours_the_output_flag_through_run() {
    let fixture = downstream().flat().build();
    let tagged = TestHarness::new().run(
        fixture.app(),
        fixture.command(),
        ["lookma", "help", "--output", "term-debug"],
    );
    tagged.assert_success();
    tagged.assert_stdout_contains("[header]USAGE[/header]");
    drop(tagged);
    for mode in ["json", "yaml"] {
        let result = TestHarness::new().run(
            fixture.app(),
            fixture.command(),
            ["lookma", "help", "--output", mode],
        );
        result.assert_success();
        assert_eq!(result.success_kind(), Some(SuccessKind::ClapHelp));
        assert!(
            !result.stdout().contains("USAGE"),
            "{mode}: help is the versioned document, not the page:\n{}",
            result.stdout()
        );
        result.assert_stdout_contains("schema_version");
    }
    let csv = TestHarness::new().run(
        fixture.app(),
        fixture.command(),
        ["lookma", "help", "--output", "csv"],
    );
    csv.assert_error_kind(RunErrorKind::Render);
    let diagnostic = csv.expect_diagnostic();
    assert_eq!(diagnostic.kind, standout::cli::DiagnosticKind::Render);
    assert!(
        diagnostic.summary.contains("no Csv projection"),
        "{diagnostic:?}"
    );
}
#[test]
#[serial]
fn the_help_document_on_a_color_tty_carries_no_ansi() {
    let fixture = downstream().flat().build();
    let result = TestHarness::new().stdout_is_terminal(true).run(
        fixture.app(),
        fixture.command(),
        ["lookma", "help", "--output", "json"],
    );
    result.assert_success();
    assert!(
        !result.stdout().contains('\u{1b}'),
        "the help document is data, never styled:\n{:?}",
        result.stdout()
    );
    let document: serde_json::Value = serde_json::from_str(result.stdout()).unwrap();
    assert_eq!(document["schema_version"], 1);
    assert_eq!(document["name"], "lookma");
}
#[test]
#[serial]
fn structured_topics_still_print_human_text() {
    let fixture = downstream().flat().build();
    let list = TestHarness::new().run(
        fixture.app(),
        fixture.command(),
        ["lookma", "help", "topics", "--output", "json"],
    );
    list.assert_success();
    list.assert_stdout_contains("Available Topics");
    assert!(
        !list.stdout().trim_start().starts_with('{'),
        "topics --output=json must not emit a JSON document:\n{}",
        list.stdout()
    );
    drop(list);
    let topic = TestHarness::new().run(
        fixture.app(),
        fixture.command(),
        ["lookma", "help", "ranges", "--output", "yaml"],
    );
    topic.assert_success();
    topic.assert_stdout_contains("two revisions separated by two dots");
    assert!(
        !topic.stdout().trim_start().starts_with('{'),
        "topic --output=yaml must not emit a structured document:\n{}",
        topic.stdout()
    );
}
#[test]
#[serial]
fn the_output_flag_reaches_the_word_and_the_flags_alike() {
    let fixture = downstream().flat().build();
    for args in [
        &["lookma", "help", "--output", "term-debug"][..],
        &["lookma", "--help", "--output", "term-debug"][..],
        &["lookma", "--output", "term-debug", "--help"][..],
        &["lookma", "-h", "--output=term-debug"][..],
        &[
            "lookma",
            "--output",
            "json",
            "--help",
            "--output",
            "term-debug",
        ][..],
    ] {
        let result = TestHarness::new().stdout_is_terminal(false).run(
            fixture.app(),
            fixture.command(),
            args,
        );
        result.assert_success();
        assert!(
            result.stdout().contains("[header]USAGE[/header]"),
            "{args:?} must render in the typed mode:\n{}",
            result.stdout()
        );
    }
    let text = TestHarness::new().stdout_is_terminal(false).run(
        fixture.app(),
        fixture.command(),
        [
            "lookma",
            "--output",
            "term-debug",
            "--help",
            "--output",
            "text",
        ],
    );
    text.assert_success();
    text.assert_stdout_contains("USAGE");
    assert!(
        !text.stdout().contains("[header]"),
        "the last `--output` wins:\n{}",
        text.stdout()
    );
}
#[test]
#[serial]
fn a_pager_request_rides_back_as_a_typed_success() {
    let fixture = downstream().flat().build();
    let result = TestHarness::new().stdout_is_terminal(false).run(
        fixture.app(),
        fixture.command(),
        ["lookma", "help", "--page"],
    );
    result.assert_success();
    assert_eq!(result.success_kind(), Some(SuccessKind::PagedHelp));
    result.assert_stdout_contains("USAGE");
}
fn assert_is_root_help(rendered: &str) {
    assert!(
        rendered.contains("Git range to diff"),
        "expected the root's help, got:\n{rendered}"
    );
}
#[test]
#[serial]
fn an_option_value_is_not_read_as_the_targeted_command() {
    let fixture = downstream().build();
    let args = ["lookma", "--output-file-path", "review", "--help"];
    let dispatched = TestHarness::new().run(fixture.app(), fixture.command(), args);
    dispatched.assert_success();
    assert_is_root_help(dispatched.stdout());
    drop(dispatched);
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
    let fixture = downstream().build();
    for flag in ["--help", "-h"] {
        let result =
            TestHarness::new().run(fixture.app(), fixture.command(), ["lookma", flag, "review"]);
        result.assert_success();
        assert_is_root_help(result.stdout());
    }
}
fn broken_theme() -> Theme {
    Theme::new().add("header", "no-such-style")
}
fn app_with_a_broken_theme() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .help_handling(true)
        .include_framework_templates(false)
        .theme(broken_theme())
        .command_with(
            "review",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({})))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap()
}
#[test]
#[serial]
fn a_help_that_cannot_be_rendered_is_not_a_usage_error() {
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
#[test]
#[serial]
fn both_entry_points_render_the_same_help() {
    let fixture = downstream().flat().build();
    for args in [["lookma", "help"], ["lookma", "--help"]] {
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
#[test]
#[serial]
fn a_flat_command_keeps_the_word_as_data_through_run_without_the_opt_in() {
    let fixture = downstream().flat().without_help_word().build();
    let result = TestHarness::new().stdout_is_terminal(false).run(
        fixture.app(),
        fixture.command(),
        ["lookma", "help"],
    );
    result.assert_success();
    result.assert_stdout_eq("range=help");
}
#[test]
#[serial]
fn the_escape_delivers_the_literal_word_through_run() {
    let fixture = downstream().flat().build();
    let result = TestHarness::new().run(fixture.app(), fixture.command(), ["lookma", "--", "help"]);
    result.assert_success();
    result.assert_stdout_eq("range=help");
}
#[test]
#[serial]
fn a_normal_invocation_is_untouched_through_run() {
    let fixture = downstream().flat().build();
    let result = TestHarness::new().stdout_is_terminal(false).run(
        fixture.app(),
        fixture.command(),
        ["lookma", "main..HEAD"],
    );
    result.assert_success();
    result.assert_stdout_eq("range=main..HEAD");
}
