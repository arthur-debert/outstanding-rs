//! The invariant library, proved against pages that hold and pages that break.
//!
//! An oracle that cannot fail is worse than none: it reads as coverage while
//! asserting nothing. So every invariant here is exercised twice — once
//! against a page that satisfies it, once against a page that violates it —
//! and the failing case asserts on the *message*, because "assertion failed"
//! that does not name the argument, the tag, or the row costs the reader the
//! debugging session the assertion was supposed to save.
//!
//! The violating pages are mostly hand-written. That is deliberate: the
//! defects these invariants were written for (#301, #302, #303) are fixed, so
//! the framework no longer produces a page that breaks them — which is exactly
//! why the oracle must be provable against a page that does.
//!
//! The passing pages come from the epic's shared downstream fixture
//! ([`standout_fixtures::downstream`]): one definition of the
//! downstream shape, shared with the clap-parity differential, so this file
//! cannot drift from the shape the rest of the suite asserts against. The
//! purpose-built apps below (an undefined tag, a nested run) stay local
//! because their shape *is* the test.

use clap::{Arg, Command};
use console::Style;
use serde_json::json;
use serial_test::serial;
use standout::cli::{App, DispatchResult, Output};
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

// ---------------------------------------------------------------------------
// Proving a failure names its offender
// ---------------------------------------------------------------------------

/// Runs `assertion`, requiring it to panic with a message containing `needle`.
///
/// The needle is the offending element the failure has to name.
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

// ---------------------------------------------------------------------------
// The fixture: the shared downstream shape, one definition for every suite
// ---------------------------------------------------------------------------

/// The shared fixture's clap surface: a positional with a value name, a
/// presence flag, a counted flag, an enum option with a default, and a
/// free-form valued option with no enumerable set — the shape whose metavar
/// #302 dropped.
fn fixture_command() -> Command {
    downstream().build().command()
}

/// Renders the shared fixture's help page in `mode`.
fn fixture_help(mode: OutputMode) -> TestResult {
    let fixture = downstream().build();
    TestHarness::new()
        .terminal_width(80)
        .no_color()
        .output_mode(mode)
        .run(fixture.app(), fixture.command(), ["lookma", "--help"])
}

// ---------------------------------------------------------------------------
// Every tag a page emits is defined in the resolved theme
// ---------------------------------------------------------------------------

/// The structural invariant on the fixture, in the two modes that differ most:
/// `Text` erases tags entirely, `Term` applies them. The record is the same
/// either way, which is the property that makes it usable everywhere.
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

/// An app whose output template uses a tag its theme never defines.
fn undefined_tag_app() -> App {
    App::builder()
        .theme(Theme::new().add("node", Style::new().cyan()))
        .command(
            "say",
            |_m, _ctx| Ok(Output::Render(json!({}))),
            "[headline]hello[/headline]",
        )
        .unwrap()
        .build()
        .unwrap()
}

fn say_command() -> Command {
    Command::new("app").subcommand(Command::new("say"))
}

/// The case that justifies plumbing the diagnostics out of the render path at
/// all: in `Text` the page looks perfect — the marker scan passes — while the
/// theme is missing the tag the template emitted. Only the structured record
/// can say so, and it names the tag.
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

/// The collector is bounded by a run, not by the process. `standout_render`'s
/// standalone helpers run the same style-tag pass, and a long-lived embedding
/// calls them outside any run boundary — where a record per render would grow
/// without bound and, worse, be read as the *next* run's evidence.
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

/// Under `Term`, an unresolved tag now degrades before it reaches the page.
/// The structural record still names the missing tag.
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

/// An app whose own theme is complete, run by the handler below.
///
/// Its template emits a tag *it* never defines, so the corruption is the inner
/// run's — and the text it produces is embedded in the outer page.
fn embedded_app() -> App {
    App::builder()
        .theme(Theme::new().add("node", Style::new().cyan()))
        .command(
            "emit",
            |_m, _ctx| Ok(Output::Render(json!({}))),
            "[inner_missing]from the inner run[/inner_missing]",
        )
        .unwrap()
        .build()
        .unwrap()
}

/// The reentrant shape: a handler that drives another app and renders its
/// output into its own page. Both renders leave a tag unresolved.
fn nesting_app() -> App {
    App::builder()
        .theme(Theme::new().add("node", Style::new().cyan()))
        .command(
            "say",
            |_m, _ctx| {
                let inner = embedded_app().run_to_string(
                    Command::new("inner").subcommand(Command::new("emit")),
                    ["inner", "emit"],
                );
                let embedded = match inner.outcome() {
                    DispatchResult::Handled(output) => output.as_str().to_string(),
                    other => panic!("the inner run must succeed, got {:?}", other),
                };
                Ok(Output::Render(json!({ "embedded": embedded })))
            },
            "[headline]{{ embedded }}[/headline]",
        )
        .unwrap()
        .build()
        .unwrap()
}

/// A run nested inside another run is one page, so it must be one verdict.
///
/// The inner run opens a diagnostics window inside the outer one and closes it
/// while the outer run is still going. If closing it ended the outer window —
/// or handed the outer run an empty batch — `assert_every_tag_resolved` would
/// pass on a page carrying the inner run's unresolved tag, which is an oracle
/// silently asserting nothing. It sees both tags instead.
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

/// The other nesting shape: the handler drives an app and throws the result
/// away, so nothing it rendered reaches the outer page.
fn discarding_app() -> App {
    App::builder()
        .theme(Theme::new().add("headline", Style::new().bold()))
        .command(
            "say",
            |_m, _ctx| {
                let discarded = embedded_app().run_to_string(
                    Command::new("inner").subcommand(Command::new("emit")),
                    ["inner", "emit"],
                );
                assert!(
                    matches!(discarded.outcome(), DispatchResult::Handled(_)),
                    "the inner run must succeed"
                );
                // …and its output goes no further.
                Ok(Output::Render(json!({})))
            },
            "[headline]nothing from the inner run[/headline]",
        )
        .unwrap()
        .build()
        .unwrap()
}

/// A nested run the handler discarded is still the outer run's business.
///
/// This is the price of the unconditional fold, pinned rather than papered
/// over. The capture layer sees a `String` handed back to a handler; whether it
/// was embedded or dropped is a fact about the handler's control flow that no
/// part of the render path can observe. So the choice is between two total
/// policies, and they fail in opposite directions: reporting a discarded run's
/// tags over-reports — the failure names a tag genuinely rendered from a theme
/// genuinely lacking it — while not reporting them would equally silence
/// `a_nested_run_cannot_hide_its_unresolved_tags_from_the_outer_one` above,
/// where the corruption *is* on the outer page and, in `Text` mode, leaves the
/// page no evidence anyone could recover it from. A false pass is the one
/// failure mode an oracle must not have; a false failure names a real defect.
///
/// The depth is what makes the over-report legible and filterable: everything
/// the outer run rendered itself sits at `0`.
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

// ---------------------------------------------------------------------------
// Styling may not change layout or content
// ---------------------------------------------------------------------------

/// The `Term` page and the `Text` page must be the same page.
///
/// Note what this cell can and cannot prove today: an in-process `Term` render
/// emits no escapes for a theme whose styles do not set `force_styling` (#329),
/// so the stripping is currently a no-op here and what is being compared is the
/// layout the two transforms produce. That is still the assertion's subject —
/// `Term` applies tags and `Text` erases them, and the pages must agree — and
/// it strengthens on its own once #329 is settled.
#[test]
#[serial]
fn the_styled_fixture_page_strips_back_to_the_plain_one() {
    let styled = fixture_help(OutputMode::Term);
    let plain = fixture_help(OutputMode::Text);

    assert_styling_preserves_layout(&styled, &plain);
}

/// The stripping half, proved on a page that really does carry escapes:
/// `force_styling` is what makes ANSI appear with no TTY under a test binary.
#[test]
#[serial]
fn a_genuinely_styled_page_strips_back_to_its_plain_render() {
    let app = App::builder()
        .theme(Theme::new().add("shout", Style::new().red().force_styling(true)))
        .command(
            "say",
            |_m, _ctx| Ok(Output::Render(json!({}))),
            "[shout]hello[/shout]",
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

/// The failing case is what #303 looked like from this direction: the styled
/// render leaked markup the plain one never had, so the two pages differ in
/// content, not just color.
#[test]
fn a_styled_page_that_gained_markup_fails_and_points_at_the_line() {
    fails_naming("[header?]", || {
        assert_styling_preserves_layout_in_pages(
            "[header?]USAGE[/header?]\n  notes [OPTIONS]\n",
            "USAGE\n  notes [OPTIONS]\n",
        )
    });
}

/// Two pages whose lines all match can still differ — in a trailing newline,
/// which `str::lines()` does not yield. The failure has to say so instead of
/// pointing at a line past the end of both pages and quoting two empty strings.
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

// ---------------------------------------------------------------------------
// No possible-values row for an argument that takes no value
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn the_fixture_page_lists_no_possible_values_for_its_presence_flags() {
    let result = fixture_help(OutputMode::Text);

    // Non-vacuity: the invariant is about rows that exist, so the page must
    // carry both the presence flag it exempts and a real possible-values row
    // it must not confuse with one.
    result.assert_stdout_contains("--staged");
    result.assert_stdout_contains("possible values: brief, full, none");

    assert_no_possible_values_for_valueless_args(&result, &fixture_command());
}

/// #301, reconstructed: `--staged` is a `SetTrue` flag whose bool parser
/// carries `true`/`false`, and a row that lists them tells the user to type
/// `--staged true`, which the parser rejects.
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

/// The fixture's *other* valueless argument: `-v` is `ArgAction::Count`, which
/// takes no value either — an invariant that special-cased `SetTrue` would
/// pass a page that renders a set for the counter.
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

/// The negative invariant is about *valueless* arguments only: an option that
/// genuinely takes one keeps its row.
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

// ---------------------------------------------------------------------------
// A metavar for every value-taking argument
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn the_fixture_page_shows_a_metavar_for_every_valued_argument() {
    let result = fixture_help(OutputMode::Text);

    // Non-vacuity: `--threshold` is the row #302 left with no hint at all, so
    // the page must actually be listing it for this assertion to mean anything.
    result.assert_stdout_contains("--threshold");

    assert_metavar_for_valued_args(&result, &fixture_command());
}

/// #302, reconstructed: `--threshold` is a free-form ratio with no enumerable
/// set, so when its value name is dropped nothing on the page says it takes a
/// value at all. The prose mentioning a range is the app being lucky, not the
/// formatter doing its job.
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

/// A positional is listed under its metavar, so losing it means the row is
/// filed under the argument id instead — the same defect wearing the
/// ARGUMENTS section's clothes.
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

/// Standout's own help leaves a positional's metavar unbracketed and tags it;
/// clap's formatter wraps it in `<>` or `[]` and marks a repeating one with an
/// ellipsis. The assertion takes a page from either — a bracket spelling it did
/// not recognise would match no row and pass by asserting nothing, which is the
/// one failure mode an oracle must not have.
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

    // …and the row is genuinely being read, not skipped: the same shapes filed
    // under the id instead of the metavar still fail. Every spelling above has
    // its counterpart here, because a green from a row the parser could not
    // find is indistinguishable from a green from a row that spells the
    // metavar — the silent pass this whole library exists to rule out.
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

/// The false negative a substring search of the label cannot avoid: an option
/// whose flag spells its own value name. `--output` contains `output`, so a row
/// that lost its metavar entirely "shows" it — the assertion has to read the
/// row's value placeholders, not its whole label.
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

// ---------------------------------------------------------------------------
// Whole-page column alignment
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn the_fixture_page_aligns_every_section() {
    for mode in [OutputMode::Text, OutputMode::Term] {
        let result = fixture_help(mode);

        // Non-vacuity: the sections whose columns are being compared have to
        // hold more than one row each, or "they all agree" says nothing.
        result.assert_stdout_contains("COMMANDS");
        result.assert_stdout_contains("OPTIONS");

        assert_descriptions_aligned(&result);
    }
}

/// The row a hand-written two-row comparison would not have looked at.
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

/// A continuation line is part of the column it hangs under, so a default that
/// drifts fails too.
#[test]
fn a_continuation_line_out_of_the_column_fails() {
    let page = "\
OPTIONS
  --format <FORMAT>  How to print the notes
                  default: text
";

    fails_naming("line 3", || assert_descriptions_aligned_in_page(page));
}

/// Sections with nothing to align — a usage line, an unindented block — state
/// nothing rather than failing.
#[test]
fn a_section_with_no_rows_states_nothing() {
    assert_descriptions_aligned_in_page(
        "USAGE\n  notes [OPTIONS] <RANGE>\n\nEXAMPLES\nnotes add\n",
    );
}
