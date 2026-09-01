use clap::{Arg, Command};
use console::Style;
use serde_json::json;
use serial_test::serial;
use standout::cli::FnHandler;
use standout::cli::{App, DispatchResult, Output};
use standout::EmbeddedTemplates;
use standout::Theme;
use standout_fixtures::downstream;
use standout_render::{OutputMode, TagResolution};
use standout_test::invariants::{
    assert_descriptions_aligned, assert_descriptions_aligned_in_page, assert_every_tag_resolved,
    assert_metavar_for_valued_args, assert_metavar_for_valued_args_in_page,
    assert_no_possible_values_for_valueless_args,
    assert_no_possible_values_for_valueless_args_in_page, assert_no_unresolved_tag_markers,
    assert_no_unresolved_tag_markers_in_page, assert_styling_preserves_layout,
    assert_styling_preserves_layout_in_pages,
};
use standout_test::{TestHarness, TestResult};

const TEMPLATES: &[(&str, &str)] = &[
    ("say", "[headline]hello[/headline]"),
    ("emit", "[inner_missing]from the inner run[/inner_missing]"),
    ("say-2", "[headline]{{ embedded }}[/headline]"),
    ("say-3", "[headline]nothing from the inner run[/headline]"),
    ("say-4", "[shout]hello[/shout]"),
];
#[track_caller]
fn fails_naming(needle: &str, assertion: impl FnOnce()) {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(assertion));
    let payload = match outcome {
        Err(payload) => payload,
        Ok(()) => panic!("expected the assertion to fail, naming {:?}", needle),
    };
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("<panic payload was not a string>");
    assert!(
        message.contains(needle),
        "the failure must name {:?}; it said:\n{}",
        needle,
        message
    );
}
fn fixture_command() -> Command {
    downstream().build().command()
}
fn fixture_help(mode: OutputMode) -> TestResult {
    let fixture = downstream().build();
    TestHarness::new()
        .terminal_width(80)
        .no_color()
        .output_mode(mode)
        .run(fixture.app(), fixture.command(), ["lookma", "--help"])
}
#[test]
#[serial]
fn the_fixture_page_resolves_every_tag_in_both_modes() {
    for mode in [OutputMode::Text, OutputMode::Term] {
        let result = fixture_help(mode);
        result.assert_success();
        assert!(
            !result.tag_resolutions().is_empty(),
            "{mode:?}: rendering a help page must record at least one style-tag pass"
        );
        assert_every_tag_resolved(&result);
        assert_no_unresolved_tag_markers(&result);
    }
}
fn undefined_tag_app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(Theme::new().add("node", Style::new().cyan()))
        .command_with(
            "say",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({})))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap()
}
fn say_command() -> Command {
    Command::new("app").subcommand(Command::new("say"))
}
#[test]
#[serial]
fn the_structural_check_names_a_tag_no_marker_would_reveal() {
    let result =
        TestHarness::new()
            .text_output()
            .run(&undefined_tag_app(), say_command(), ["app", "say"]);
    assert_eq!(
        result.stdout(),
        "hello",
        "Text mode erases the tag, so the page carries no evidence"
    );
    assert_no_unresolved_tag_markers(&result);
    fails_naming("headline", || assert_every_tag_resolved(&result));
    fails_naming("node", || assert_every_tag_resolved(&result));
    assert_eq!(
        result.unresolved_tag_names(),
        ["headline"],
        "the collapsed view names the offending tag once"
    );
    assert!(
        result
            .unresolved_tags()
            .iter()
            .all(|error| error.tag == "headline"),
        "nothing else is blamed: {:?}",
        result.unresolved_tags()
    );
    assert_eq!(
        result.tag_resolutions().len(),
        2,
        "a handled run applies the style-tag pass twice over the same template \
         output — once for the page, once unstyled for a pipe — and each pass \
         is recorded with the transform it ran under"
    );
}
#[test]
#[serial]
fn standalone_renders_neither_accumulate_nor_reach_a_later_run() {
    let theme = Theme::new().add("node", Style::new().cyan());
    for _ in 0..100 {
        standout_render::render("[headline]hello[/headline]", &json!({}), &theme)
            .expect("the standalone render succeeds");
    }
    let result = fixture_help(OutputMode::Text);
    result.assert_success();
    assert!(
        !result.tag_resolutions().is_empty(),
        "the run's own passes are still recorded"
    );
    assert!(
        result.unresolved_tag_names().is_empty(),
        "the standalone renders' unresolved `headline` must not be read as this \
         run's, got {:?}",
        result.unresolved_tag_names()
    );
    assert_every_tag_resolved(&result);
}
#[test]
#[serial]
fn term_output_degrades_unresolved_tags_without_hiding_the_structural_record() {
    let result = TestHarness::new().output_mode(OutputMode::Term).run(
        &undefined_tag_app(),
        say_command(),
        ["app", "say"],
    );
    assert_eq!(result.stdout(), "hello");
    assert_no_unresolved_tag_markers(&result);
    fails_naming("headline", || assert_every_tag_resolved(&result));
}
#[test]
fn the_marker_check_passes_a_clean_page() {
    assert_no_unresolved_tag_markers_in_page("USAGE\n  notes [OPTIONS]\n");
}
fn embedded_app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(Theme::new().add("node", Style::new().cyan()))
        .command_with(
            "emit",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({})))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap()
}
fn nesting_app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(Theme::new().add("node", Style::new().cyan()))
        .command_with(
            "say",
            FnHandler::new(|_m, _ctx| {
                let inner = embedded_app().run_with(
                    Command::new("inner").subcommand(Command::new("emit")),
                    ["inner", "emit"],
                    standout::TargetProperties::detect(),
                    standout::InputSources::from_process(),
                );
                let embedded = match inner.outcome() {
                    DispatchResult::Handled(output) => output.as_str().to_string(),
                    other => panic!("the inner run must succeed, got {:?}", other),
                };
                Ok(Output::Render(json!({ "embedded": embedded })))
            }),
            |cfg| cfg.template_name("say-2"),
        )
        .unwrap()
        .build()
        .unwrap()
}
#[test]
#[serial]
fn a_nested_run_cannot_hide_its_unresolved_tags_from_the_outer_one() {
    let result =
        TestHarness::new()
            .text_output()
            .run(&nesting_app(), say_command(), ["app", "say"]);
    result.assert_success();
    assert_eq!(
        result.stdout(),
        "from the inner run",
        "Text mode erases both tags, so neither page carries evidence"
    );
    assert_no_unresolved_tag_markers(&result);
    assert_eq!(
        result.unresolved_tag_names(),
        ["inner_missing", "headline"],
        "the outer run accounts for the nested run's passes as well as its own, \
         in the order they ran"
    );
    fails_naming("inner_missing", || assert_every_tag_resolved(&result));
    fails_naming("headline", || assert_every_tag_resolved(&result));
    assert_eq!(
        result
            .tag_resolutions()
            .iter()
            .map(TagResolution::nesting_depth)
            .collect::<Vec<_>>(),
        [1, 1, 0, 0],
        "the inner run's passes are marked as having come from one level in, \
         and each run's page is rendered twice"
    );
}
fn discarding_app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(Theme::new().add("headline", Style::new().bold()))
        .command_with(
            "say",
            FnHandler::new(|_m, _ctx| {
                let discarded = embedded_app().run_with(
                    Command::new("inner").subcommand(Command::new("emit")),
                    ["inner", "emit"],
                    standout::TargetProperties::detect(),
                    standout::InputSources::from_process(),
                );
                assert!(
                    matches!(discarded.outcome(), DispatchResult::Handled(_)),
                    "the inner run must succeed"
                );
                Ok(Output::Render(json!({})))
            }),
            |cfg| cfg.template_name("say-3"),
        )
        .unwrap()
        .build()
        .unwrap()
}
#[test]
#[serial]
fn a_discarded_nested_run_is_reported_and_distinguishable() {
    let result =
        TestHarness::new()
            .text_output()
            .run(&discarding_app(), say_command(), ["app", "say"]);
    result.assert_success();
    assert_eq!(
        result.stdout(),
        "nothing from the inner run",
        "the discarded run left nothing on this page"
    );
    assert_eq!(
        result.unresolved_tag_names(),
        ["inner_missing"],
        "the run is still reported as having rendered a tag its theme lacks"
    );
    fails_naming("inner_missing", || assert_every_tag_resolved(&result));
    fails_naming("nested run", || assert_every_tag_resolved(&result));
    let own: Vec<&str> = result
        .tag_resolutions()
        .iter()
        .filter(|pass| pass.nesting_depth() == 0)
        .flat_map(TagResolution::unresolved_tag_names)
        .collect();
    assert!(
        own.is_empty(),
        "and page scope is one filter away: this run's own renders are clean"
    );
}
#[test]
#[serial]
fn the_styled_fixture_page_strips_back_to_the_plain_one() {
    let styled = fixture_help(OutputMode::Term);
    let plain = fixture_help(OutputMode::Text);
    assert_styling_preserves_layout(&styled, &plain);
}
#[test]
#[serial]
fn a_genuinely_styled_page_strips_back_to_its_plain_render() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(Theme::new().add("shout", Style::new().red().force_styling(true)))
        .command_with(
            "say",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({})))),
            |cfg| cfg.template_name("say-4"),
        )
        .unwrap()
        .build()
        .unwrap();
    let styled =
        TestHarness::new()
            .output_mode(OutputMode::Term)
            .run(&app, say_command(), ["app", "say"]);
    assert!(
        styled.stdout().contains('\x1b'),
        "the fixture must actually emit ANSI, got {:?}",
        styled.stdout()
    );
    let plain = TestHarness::new()
        .text_output()
        .run(&app, say_command(), ["app", "say"]);
    assert_styling_preserves_layout(&styled, &plain);
}
#[test]
fn a_styled_page_that_gained_markup_fails_and_points_at_the_line() {
    fails_naming("[header?]", || {
        assert_styling_preserves_layout_in_pages(
            "[header?]USAGE[/header?]\n  notes [OPTIONS]\n",
            "USAGE\n  notes [OPTIONS]\n",
        )
    });
}
#[test]
fn a_trailing_newline_difference_says_what_it_is() {
    fails_naming("trailing line-ending bytes", || {
        assert_styling_preserves_layout_in_pages("USAGE\n  notes\n", "USAGE\n  notes")
    });
}
#[test]
fn a_styled_page_that_lost_a_line_names_the_line_number() {
    fails_naming("line 3", || {
        assert_styling_preserves_layout_in_pages(
            "USAGE\n  notes [OPTIONS]\n",
            "USAGE\n  notes [OPTIONS]\n  notes list\n",
        )
    });
}
#[test]
#[serial]
fn the_fixture_page_lists_no_possible_values_for_its_presence_flags() {
    let result = fixture_help(OutputMode::Text);
    result.assert_stdout_contains("--staged");
    result.assert_stdout_contains("possible values: brief, full, none");
    assert_no_possible_values_for_valueless_args(&result, &fixture_command());
}
#[test]
fn a_possible_values_row_on_a_presence_flag_names_the_flag() {
    let page = "\
OPTIONS
  --staged           Diff the staged changes
                     possible values: true, false
  --summary <STYLE>  How much of each change to describe
";
    fails_naming("staged", || {
        assert_no_possible_values_for_valueless_args_in_page(page, &fixture_command())
    });
    fails_naming("possible values: true, false", || {
        assert_no_possible_values_for_valueless_args_in_page(page, &fixture_command())
    });
}
#[test]
fn a_possible_values_row_on_a_counted_flag_names_the_flag() {
    let page = "\
OPTIONS
  -v                 Raise the detail level
                     possible values: 0, 1, 2
";
    fails_naming("verbose", || {
        assert_no_possible_values_for_valueless_args_in_page(page, &fixture_command())
    });
}
#[test]
fn a_possible_values_row_on_a_valued_option_is_left_alone() {
    let page = "\
OPTIONS
  --staged           Diff the staged changes
  --summary <STYLE>  How much of each change to describe
                     possible values: brief, full, none
";
    assert_no_possible_values_for_valueless_args_in_page(page, &fixture_command());
}
#[test]
#[serial]
fn the_fixture_page_shows_a_metavar_for_every_valued_argument() {
    let result = fixture_help(OutputMode::Text);
    result.assert_stdout_contains("--threshold");
    assert_metavar_for_valued_args(&result, &fixture_command());
}
#[test]
fn a_dropped_metavar_names_the_argument_that_lost_it() {
    let page = "\
ARGUMENTS
  RANGE              Git range to diff, e.g. main..HEAD
OPTIONS
  --staged           Diff the staged changes
  --summary <STYLE>  How much of each change to describe
  --threshold        Move/rename similarity threshold
";
    fails_naming("threshold", || {
        assert_metavar_for_valued_args_in_page(page, &fixture_command())
    });
    fails_naming("RATIO", || {
        assert_metavar_for_valued_args_in_page(page, &fixture_command())
    });
}
#[test]
fn a_positional_listed_under_its_id_instead_of_its_metavar_fails() {
    let page = "\
ARGUMENTS
  range              Git range to diff, e.g. main..HEAD
OPTIONS
  --staged               Diff the staged changes
  --summary <STYLE>      How much of each change to describe
  --threshold <RATIO>    Move/rename similarity threshold
";
    fails_naming("RANGE", || {
        assert_metavar_for_valued_args_in_page(page, &fixture_command())
    });
}
#[test]
fn a_positional_row_is_found_however_its_metavar_is_bracketed() {
    for label in ["RANGE", "<RANGE>", "[RANGE]", "<RANGE>...", "[RANGE]..."] {
        let page = format!(
            "\
ARGUMENTS
  {label:<18} Range of notes to act on
"
        );
        assert_metavar_for_valued_args_in_page(&page, &fixture_command());
    }
    for label in [
        "range",
        "<range>",
        "[range]",
        "<range>...",
        "[range]...",
        "range...",
    ] {
        let page = format!(
            "\
ARGUMENTS
  {label:<18} Range of notes to act on
"
        );
        fails_naming("RANGE", || {
            assert_metavar_for_valued_args_in_page(&page, &fixture_command())
        });
    }
}
#[test]
fn an_option_whose_flag_spells_its_value_name_still_needs_a_metavar() {
    let implicit = Command::new("notes").arg(
        Arg::new("output")
            .long("output")
            .help("Where to write the notes"),
    );
    let declared = Command::new("notes").arg(
        Arg::new("out")
            .long("output")
            .value_name("output")
            .help("Where to write the notes"),
    );
    let stripped = "\
OPTIONS
  --output           Where to write the notes
";
    fails_naming("output", || {
        assert_metavar_for_valued_args_in_page(stripped, &implicit)
    });
    fails_naming("output", || {
        assert_metavar_for_valued_args_in_page(stripped, &declared)
    });
    let intact = "\
OPTIONS
  --output <output>  Where to write the notes
";
    assert_metavar_for_valued_args_in_page(intact, &implicit);
    assert_metavar_for_valued_args_in_page(intact, &declared);
}
#[test]
#[serial]
fn the_fixture_page_aligns_every_section() {
    for mode in [OutputMode::Text, OutputMode::Term] {
        let result = fixture_help(mode);
        result.assert_stdout_contains("COMMANDS");
        result.assert_stdout_contains("OPTIONS");
        assert_descriptions_aligned(&result);
    }
}
#[test]
fn a_row_that_drifts_out_of_the_column_names_its_section_and_line() {
    let page = "\
OPTIONS
  --all              Include archived notes
  --format <FORMAT>  How to print the notes
  --threshold <RATIO>   Similarity threshold
";
    fails_naming("OPTIONS", || assert_descriptions_aligned_in_page(page));
    fails_naming("line 4", || assert_descriptions_aligned_in_page(page));
}
#[test]
fn a_continuation_line_out_of_the_column_fails() {
    let page = "\
OPTIONS
  --format <FORMAT>  How to print the notes
                  default: text
";
    fails_naming("line 3", || assert_descriptions_aligned_in_page(page));
}
#[test]
fn a_section_with_no_rows_states_nothing() {
    assert_descriptions_aligned_in_page(
        "USAGE\n  notes [OPTIONS] <RANGE>\n\nEXAMPLES\nnotes add\n",
    );
}
