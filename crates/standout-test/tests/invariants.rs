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
//! The fixture below is downstream-shaped but local to this file. The epic's
//! shared fixture (WS03) is not on this branch yet; when it lands, these
//! `notes_*` functions are what it replaces.

use clap::{Arg, ArgAction, Command};
use console::Style;
use serde_json::json;
use serial_test::serial;
use standout::cli::{App, Output};
use standout::Theme;
use standout_render::OutputMode;
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
// The fixture: downstream-shaped, with a theme that knows nothing of help
// ---------------------------------------------------------------------------

/// An app with its own output vocabulary and help turned on — the combination
/// no fixture in the suite had before, and the one #303 needed.
fn notes_app() -> App {
    App::builder()
        .help_handling(true)
        .theme(Theme::new().add("node", Style::new().cyan()))
        .command("list", |_m, _ctx| Ok(Output::Render(json!({}))), "listed")
        .unwrap()
        .command("add", |_m, _ctx| Ok(Output::Render(json!({}))), "added")
        .unwrap()
        .command(
            "archive",
            |_m, _ctx| Ok(Output::Render(json!({}))),
            "archived",
        )
        .unwrap()
        .build()
        .unwrap()
}

/// The fixture's clap surface: a positional with a value name, a presence
/// flag, a counted flag, an enum option with a default, and a free-form valued
/// option with no enumerable set — the shape whose metavar #302 dropped.
fn notes_command() -> Command {
    Command::new("notes")
        .about("Keep short notes")
        .arg(
            Arg::new("range")
                .value_name("RANGE")
                .help("Range of notes to act on"),
        )
        .arg(
            Arg::new("all")
                .long("all")
                .action(ArgAction::SetTrue)
                .help("Include archived notes"),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .action(ArgAction::Count)
                .help("Raise the detail level"),
        )
        .arg(
            Arg::new("format")
                .long("format")
                .value_name("FORMAT")
                .value_parser(["text", "json"])
                .default_value("text")
                .help("How to print the notes"),
        )
        .arg(
            Arg::new("threshold")
                .long("threshold")
                .value_name("RATIO")
                .help("Similarity threshold, from 0.0 to 1.0"),
        )
        .subcommand(Command::new("list").about("List the notes"))
        .subcommand(Command::new("add").about("Add a note"))
        .subcommand(Command::new("archive").about("Archive a note"))
}

/// Renders the fixture's help page in `mode`.
fn notes_help(mode: OutputMode) -> TestResult {
    TestHarness::new()
        .terminal_width(80)
        .no_tty()
        .no_color()
        .output_mode(mode)
        .run(&notes_app(), notes_command(), ["notes", "--help"])
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
        let result = notes_help(mode);
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

    let result = notes_help(OutputMode::Text);
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

/// The other direction: under `Term` the corruption reaches the page, and the
/// marker scan is what proves a user would have seen it. No real ANSI is
/// needed — `Term` applies the transform regardless of whether color bytes are
/// produced.
#[test]
#[serial]
fn the_marker_check_catches_the_corruption_that_reaches_the_page() {
    let result = TestHarness::new().output_mode(OutputMode::Term).run(
        &undefined_tag_app(),
        say_command(),
        ["app", "say"],
    );

    assert_eq!(result.stdout(), "[headline?]hello[/headline?]");
    fails_naming("[headline?]", || assert_no_unresolved_tag_markers(&result));
    fails_naming("headline", || assert_every_tag_resolved(&result));
}

#[test]
fn the_marker_check_passes_a_clean_page() {
    assert_no_unresolved_tag_markers_in_page("USAGE\n  notes [OPTIONS]\n");
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
    let styled = notes_help(OutputMode::Term);
    let plain = notes_help(OutputMode::Text);

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
    let result = notes_help(OutputMode::Text);

    // Non-vacuity: the invariant is about rows that exist, so the page must
    // carry both the presence flag it exempts and a real possible-values row
    // it must not confuse with one.
    result.assert_stdout_contains("--all");
    result.assert_stdout_contains("possible values: text, json");

    assert_no_possible_values_for_valueless_args(&result, &notes_command());
}

/// #301, reconstructed: `--all` is a `SetTrue` flag whose bool parser carries
/// `true`/`false`, and a row that lists them tells the user to type
/// `--all true`, which the parser rejects.
#[test]
fn a_possible_values_row_on_a_presence_flag_names_the_flag() {
    let page = "\
OPTIONS
  --all              Include archived notes
                     possible values: true, false
  --format <FORMAT>  How to print the notes
";

    fails_naming("all", || {
        assert_no_possible_values_for_valueless_args_in_page(page, &notes_command())
    });
    fails_naming("possible values: true, false", || {
        assert_no_possible_values_for_valueless_args_in_page(page, &notes_command())
    });
}

/// The negative invariant is about *valueless* arguments only: an option that
/// genuinely takes one keeps its row.
#[test]
fn a_possible_values_row_on_a_valued_option_is_left_alone() {
    let page = "\
OPTIONS
  --all              Include archived notes
  --format <FORMAT>  How to print the notes
                     possible values: text, json
";

    assert_no_possible_values_for_valueless_args_in_page(page, &notes_command());
}

// ---------------------------------------------------------------------------
// A metavar for every value-taking argument
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn the_fixture_page_shows_a_metavar_for_every_valued_argument() {
    let result = notes_help(OutputMode::Text);

    // Non-vacuity: `--threshold` is the row #302 left with no hint at all, so
    // the page must actually be listing it for this assertion to mean anything.
    result.assert_stdout_contains("--threshold");

    assert_metavar_for_valued_args(&result, &notes_command());
}

/// #302, reconstructed: `--threshold` is a free-form ratio with no enumerable
/// set, so when its value name is dropped nothing on the page says it takes a
/// value at all. The prose mentioning a range is the app being lucky, not the
/// formatter doing its job.
#[test]
fn a_dropped_metavar_names_the_argument_that_lost_it() {
    let page = "\
ARGUMENTS
  RANGE              Range of notes to act on

OPTIONS
  --all              Include archived notes
  --format <FORMAT>  How to print the notes
  --threshold        Similarity threshold, from 0.0 to 1.0
";

    fails_naming("threshold", || {
        assert_metavar_for_valued_args_in_page(page, &notes_command())
    });
    fails_naming("RATIO", || {
        assert_metavar_for_valued_args_in_page(page, &notes_command())
    });
}

/// A positional is listed under its metavar, so losing it means the row is
/// filed under the argument id instead — the same defect wearing the
/// ARGUMENTS section's clothes.
#[test]
fn a_positional_listed_under_its_id_instead_of_its_metavar_fails() {
    let page = "\
ARGUMENTS
  range              Range of notes to act on

OPTIONS
  --all                  Include archived notes
  --format <FORMAT>      How to print the notes
  --threshold <RATIO>    Similarity threshold
";

    fails_naming("RANGE", || {
        assert_metavar_for_valued_args_in_page(page, &notes_command())
    });
}

/// Standout's own help leaves a positional's metavar unbracketed and tags it;
/// clap's formatter wraps it in `<>` or `[]` and marks a repeating one with an
/// ellipsis. The assertion takes a page from either — a bracket spelling it did
/// not recognise would match no row and pass by asserting nothing, which is the
/// one failure mode an oracle must not have.
#[test]
fn a_positional_row_is_found_however_its_metavar_is_bracketed() {
    for label in ["RANGE", "<RANGE>", "[RANGE]", "<RANGE>..."] {
        let page = format!(
            "\
ARGUMENTS
  {label:<18} Range of notes to act on
"
        );
        assert_metavar_for_valued_args_in_page(&page, &notes_command());
    }

    // …and the row is genuinely being read, not skipped: the same shapes filed
    // under the id instead of the metavar still fail.
    for label in ["range", "<range>", "[range]"] {
        let page = format!(
            "\
ARGUMENTS
  {label:<18} Range of notes to act on
"
        );
        fails_naming("RANGE", || {
            assert_metavar_for_valued_args_in_page(&page, &notes_command())
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
        let result = notes_help(mode);

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
