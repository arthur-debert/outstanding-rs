//! The clap-parity differential, over the shape matrix and against itself.
//!
//! The oracle's claim is that themed help states every user-facing fact clap's
//! own formatter would state for the same `Command`. A claim like that is only
//! worth its runtime if three things hold, and this file proves each:
//!
//! 1. **It binds.** Every fact the fixture page carries is removed from a copy
//!    of that page, one at a time, and the differential must fail *naming what
//!    went missing*. Removing a field from the help-data extractor deletes the
//!    same rows, so a red here is what a dropped field looks like. An oracle
//!    that cannot fail reads as coverage while asserting nothing — the exact
//!    failure the ten `OptionData` unit tests of #298 demonstrated.
//! 2. **The facts are clap's.** The same assertion, with no exemptions, runs
//!    against clap's own rendered page. If the derivation invented a fact clap
//!    does not print, or spelled one in standout's terms, that test goes red
//!    before the standout-facing ones can pass by construction.
//! 3. **The exemptions are load-bearing.** Each entry of
//!    [`DELIBERATE_OMISSIONS`] is exercised by a command that carries the
//!    exempted fact: clap's page states it, standout's page does not, and
//!    removing the entry turns the assertion red. An allowlist nobody can tell
//!    from a forgotten field is the thing it exists to prevent.
//!
//! The matrix runs the whole thing across shape (flat / subcommands /
//! subcommand page), help entry point (`-h`, `--help`, the `help` word) and
//! output mode (plain and styled), and each cell also runs the WS04 invariant
//! set — the differential says the facts are present, the invariants say the
//! rows that carry them are well formed.

use clap::{Arg, ArgAction, Command};
use serde_json::json;
use serial_test::serial;
use standout::cli::{App, HelpLength, Output};
use standout_fixtures::downstream;
use standout_test::clap_parity::{
    assert_page_states_clap_facts, assert_page_states_clap_facts_with, assert_states_clap_facts,
    clap_facts, Exemption, Fact, FactKind, Omission, Presence, Subject, DELIBERATE_OMISSIONS,
};
use standout_test::invariants::{
    assert_descriptions_aligned, assert_every_tag_resolved, assert_metavar_for_valued_args,
    assert_no_possible_values_for_valueless_args, assert_no_unresolved_tag_markers,
};
use standout_test::TestHarness;

/// The three help entry points, with the length each asks for.
///
/// The length is not decoration: `-h` owes the reader `about` and `--help`
/// owes `long_about`, so an extractor that copied one field for both passes
/// two of these cells and fails the third.
const ENTRY_POINTS: [(&str, HelpLength); 3] = [
    ("-h", HelpLength::Short),
    ("--help", HelpLength::Long),
    ("help", HelpLength::Long),
];

// ---------------------------------------------------------------------------
// The matrix
// ---------------------------------------------------------------------------

/// Every cell of (shape × entry point × output mode) states every clap fact.
///
/// The styled cells matter as much as the plain ones: `Term` applies the style
/// tags, which is the transform #303 corrupted, and a fact swallowed by markup
/// is as absent as a fact never rendered.
#[test]
#[serial]
fn every_matrix_cell_states_every_clap_fact() {
    for flat in [false, true] {
        let fixture = if flat {
            downstream().flat().build()
        } else {
            downstream().build()
        };

        for (entry, length) in ENTRY_POINTS {
            for styled in [false, true] {
                let harness = if styled {
                    TestHarness::new().with_color()
                } else {
                    TestHarness::new().text_output()
                };
                let result = harness.run(fixture.app(), fixture.command(), ["lookma", entry]);

                result.assert_success();
                assert_states_clap_facts(&result, &fixture.command(), length);

                assert_every_tag_resolved(&result);
                assert_no_unresolved_tag_markers(&result);
                assert_no_possible_values_for_valueless_args(&result, &fixture.command());
                assert_metavar_for_valued_args(&result, &fixture.command());
                assert_descriptions_aligned(&result);
            }
        }
    }
}

/// A subcommand's own page is a page too, and its facts are the subcommand's:
/// its `about`, its positional, its value name.
#[test]
#[serial]
fn a_subcommand_page_states_the_subcommands_clap_facts() {
    let fixture = downstream().build();
    let review = fixture
        .command()
        .find_subcommand("review")
        .expect("the fixture's review subcommand")
        .clone();

    for (entry, length) in ENTRY_POINTS {
        if entry == "help" {
            // The word routes to the parent's page for a topic, not to a
            // subcommand's; `help review` is the parent shape's own cell.
            continue;
        }
        let result = TestHarness::new().text_output().run(
            fixture.app(),
            fixture.command(),
            ["lookma", "review", entry],
        );

        result.assert_success();
        assert_states_clap_facts(&result, &review, length);
    }
}

/// `help <command>` renders the subcommand's page, so it owes the same facts
/// as `review --help` — the two entry points must not disagree about what a
/// help request for `review` says.
#[test]
#[serial]
fn the_help_word_targeting_a_subcommand_states_its_facts() {
    let fixture = downstream().build();
    let review = fixture
        .command()
        .find_subcommand("review")
        .expect("the fixture's review subcommand")
        .clone();

    let result = TestHarness::new().text_output().run(
        fixture.app(),
        fixture.command(),
        ["lookma", "help", "review"],
    );

    result.assert_success();
    assert_states_clap_facts(&result, &review, HelpLength::Long);
}

// ---------------------------------------------------------------------------
// The derivation is clap's, and it is not empty
// ---------------------------------------------------------------------------

/// Clap's own page states every fact the oracle derives from the command.
///
/// This is what grounds the differential. The expectations are checked with an
/// **empty** allowlist against the output of clap's formatter, so a fact the
/// derivation invented — or spelled the way standout renders it rather than
/// the way clap does — fails here, before any standout page is involved.
#[test]
fn clap_states_every_fact_the_oracle_derives() {
    for flat in [false, true] {
        let fixture = if flat {
            downstream().flat().build()
        } else {
            downstream().build()
        };

        for length in [HelpLength::Short, HelpLength::Long] {
            let page = clap_page(fixture.command(), length);
            assert_page_states_clap_facts_with(&page, &fixture.command(), length, &[]);
        }
    }
}

/// The fact list actually covers the metadata the themed-help cluster lost.
///
/// A derivation that quietly produced nothing would make every cell above
/// green. This names the facts the fixture must yield — the ones #298, #301
/// and #302 were about — so an accidental narrowing of the walk is a failure
/// here rather than a silence everywhere.
#[test]
fn the_derivation_covers_the_metadata_the_cluster_lost() {
    let fixture = downstream().build();
    let facts = clap_facts(&fixture.command(), HelpLength::Long);

    let stated = |kind: FactKind, subject: Subject, expected: &str| {
        assert!(
            facts.iter().any(|fact| fact.kind() == kind
                && *fact.subject() == subject
                && fact.expected() == expected
                && fact.presence() == Presence::Stated),
            "no stated {:?} fact {expected:?} for {subject}",
            kind
        );
    };
    let suppressed = |kind: FactKind, subject: Subject, expected: &str| {
        assert!(
            facts.iter().any(|fact| fact.kind() == kind
                && *fact.subject() == subject
                && fact.expected() == expected
                && fact.presence() == Presence::Suppressed),
            "no suppressed {:?} fact {expected:?} for {subject}",
            kind
        );
    };
    let arg = |id: &str| Subject::Argument(id.to_string());

    // The long about, which is the half of the length axis `-h` never shows.
    stated(
        FactKind::Purpose,
        Subject::Command("lookma".into()),
        "Diff a git range.\n\nNames a change the way a human would.",
    );

    // Metavars, declared and fallen back to (#302).
    stated(FactKind::ArgMetavar, arg("range"), "RANGE");
    stated(FactKind::ArgMetavar, arg("threshold"), "RATIO");
    stated(FactKind::ArgMetavar, arg("pattern"), "pattern");

    // A default and its enumerated set (#298).
    stated(FactKind::ArgDefault, arg("summary"), "brief");
    for value in ["brief", "full", "none"] {
        stated(FactKind::ArgPossibleValue, arg("summary"), value);
    }

    // Clap's suppression rules, which #301 was the absence of: a presence flag
    // advertises neither the bool default clap gives it nor bool values, and a
    // value parser's hidden spellings stay hidden.
    suppressed(FactKind::ArgDefault, arg("staged"), "false");
    stated(FactKind::ArgPossibleValue, arg("color"), "true");
    suppressed(FactKind::ArgPossibleValue, arg("color"), "yes");

    // The subcommand list and the positional/option split.
    for name in ["review", "stat", "export"] {
        stated(
            FactKind::SubcommandName,
            Subject::Subcommand(name.into()),
            name,
        );
    }
    assert!(
        facts
            .iter()
            .any(|fact| fact.kind() == FactKind::Classification),
        "the fixture has both a positional and options, so classification is a fact about it"
    );

    // Clap's own `--help` argument is marked as clap's, which is what lets the
    // allowlist exempt it without naming a flag in a string.
    assert!(
        facts
            .iter()
            .any(|fact| fact.is_clap_generated() && fact.expected() == "--help"),
        "clap's generated help argument should be marked generated"
    );
    assert!(
        facts
            .iter()
            .filter(|fact| *fact.subject() == arg("summary"))
            .all(|fact| !fact.is_clap_generated()),
        "an argument the application declared is not clap's"
    );
}

// ---------------------------------------------------------------------------
// The oracle binds: each fact, removed from the page, is caught by name
// ---------------------------------------------------------------------------

/// A dropped default value fails, naming the argument and the value.
#[test]
#[serial]
fn a_dropped_default_fails() {
    let page = drop_lines(&rendered("--help"), "default: brief");
    fails_naming(&["summary", "brief"], || {
        assert_page_states_clap_facts(&page, &downstream().build().command(), HelpLength::Long)
    });
}

/// A default replaced by a strict superstring fails: a page that states
/// `briefly` no longer states that `brief` is the default, and a substring
/// match would have called it parity-clean.
#[test]
#[serial]
fn a_default_replaced_by_a_superstring_fails() {
    let page = rendered("--help").replace("default: brief", "default: briefly");
    fails_naming(&["summary", "brief"], || {
        assert_page_states_clap_facts(&page, &downstream().build().command(), HelpLength::Long)
    });
}

/// A possible value dropped from the list fails, naming the value — the row
/// still exists and still says `possible values`, which is what makes this the
/// case a substring search for the label would miss.
#[test]
#[serial]
fn a_dropped_possible_value_fails() {
    let page = rendered("--help").replace(
        "possible values: brief, full, none",
        "possible values: brief, none",
    );
    fails_naming(&["summary", "full"], || {
        assert_page_states_clap_facts(&page, &downstream().build().command(), HelpLength::Long)
    });
}

/// A row that loses its metavar fails, naming the value name the user can no
/// longer see (#302's shape, on a page rather than in the extractor).
#[test]
#[serial]
fn a_dropped_metavar_fails() {
    let page = rendered("--help").replace("--threshold <RATIO>", "--threshold        ");
    fails_naming(&["threshold", "RATIO"], || {
        assert_page_states_clap_facts(&page, &downstream().build().command(), HelpLength::Long)
    });
}

/// An argument that loses its row entirely fails, naming its spelling.
#[test]
#[serial]
fn a_dropped_argument_row_fails() {
    let page = drop_lines(&rendered("--help"), "--staged");
    fails_naming(&["staged"], || {
        assert_page_states_clap_facts(&page, &downstream().build().command(), HelpLength::Long)
    });
}

/// A subcommand that loses its row fails, naming the command.
#[test]
#[serial]
fn a_dropped_subcommand_row_fails() {
    let page = drop_lines(&rendered("--help"), "Summarize a range by file");
    fails_naming(&["stat"], || {
        assert_page_states_clap_facts(&page, &downstream().build().command(), HelpLength::Long)
    });
}

/// A dropped help text fails, naming the argument and the sentence.
#[test]
#[serial]
fn a_dropped_help_text_fails() {
    let page = rendered("--help").replace("Move/rename similarity threshold", "");
    fails_naming(&["threshold", "Move/rename similarity threshold"], || {
        assert_page_states_clap_facts(&page, &downstream().build().command(), HelpLength::Long)
    });
}

/// A page that shows `about` where `--help` owes `long_about` fails.
///
/// No mutation is needed: the short page *is* the page an extractor that
/// copied one description for both lengths would render, so this cell is the
/// length axis proving it binds.
#[test]
#[serial]
fn the_short_about_does_not_satisfy_the_long_page() {
    let page = rendered("-h");
    fails_naming(&["Names a change the way a human would"], || {
        assert_page_states_clap_facts(&page, &downstream().build().command(), HelpLength::Long)
    });
}

/// Positionals folded in with the options fail: the page no longer says which
/// arguments are typed by position.
#[test]
#[serial]
fn merging_the_positional_and_option_sections_fails() {
    let page = rendered("--help").replace("\nARGUMENTS\n", "\nOPTIONS\n");
    fails_naming(&["classification"], || {
        assert_page_states_clap_facts(&page, &downstream().build().command(), HelpLength::Long)
    });
}

/// A fact clap suppresses, printed anyway, fails — the negative direction, and
/// #301's exact shape: a bool default advertised on a flag that takes no value.
#[test]
#[serial]
fn stating_a_fact_clap_suppresses_fails() {
    let page = rendered("--help").replace(
        "  --staged                   Diff the staged changes",
        "  --staged                   Diff the staged changes\n\
         \x20                            default: false",
    );
    fails_naming(&["staged", "false"], || {
        assert_page_states_clap_facts(&page, &downstream().build().command(), HelpLength::Long)
    });
}

// ---------------------------------------------------------------------------
// Every exemption is load-bearing
// ---------------------------------------------------------------------------

/// Clap's own `-h/--help` row is exempt — and the exemption is what is holding
/// the fixture page green, not the page happening to render it.
#[test]
#[serial]
fn the_clap_generated_subjects_exemption_is_load_bearing() {
    let fixture = downstream().build();
    let page = rendered("--help");

    assert_page_states_clap_facts(&page, &fixture.command(), HelpLength::Long);
    fails_naming(&["--help"], || {
        assert_page_states_clap_facts_with(
            &page,
            &fixture.command(),
            HelpLength::Long,
            &without(Omission::ClapGeneratedSubjects),
        )
    });
}

/// An argument's `long_help` and its aliases are exempt, and both exemptions
/// are doing work: clap's page states them, standout's does not.
#[test]
#[serial]
fn the_argument_and_subcommand_exemptions_are_load_bearing() {
    let (app, cmd) = decorated();
    let result = TestHarness::new()
        .text_output()
        .run(&app, cmd.clone(), ["notes", "--help"]);
    result.assert_success();
    let page = result.stdout_plain();

    // The facts are clap's: its own page states every one of them.
    assert_page_states_clap_facts_with(
        &clap_page(cmd.clone(), HelpLength::Long),
        &cmd,
        HelpLength::Long,
        &[],
    );

    // Standout's page is green only because the allowlist says so.
    assert_page_states_clap_facts(&page, &cmd, HelpLength::Long);

    fails_naming(&["long help", "as a ratio"], || {
        assert_page_states_clap_facts_with(
            &page,
            &cmd,
            HelpLength::Long,
            &without(Omission::Kind(FactKind::ArgLongHelp)),
        )
    });
    fails_naming(&["threshold", "--thr"], || {
        assert_page_states_clap_facts_with(
            &page,
            &cmd,
            HelpLength::Long,
            &without(Omission::Kind(FactKind::ArgAlias)),
        )
    });
    fails_naming(&["stat", "st"], || {
        assert_page_states_clap_facts_with(
            &page,
            &cmd,
            HelpLength::Long,
            &without(Omission::Kind(FactKind::SubcommandAlias)),
        )
    });
}

/// Every entry states why it is there, so a reader of the allowlist is never
/// left to infer the reason from the omission.
#[test]
fn every_exemption_states_a_reason() {
    for exemption in DELIBERATE_OMISSIONS {
        assert!(
            exemption.reason.split_whitespace().count() >= 10,
            "{:?} is exempt without a reason a reviewer could weigh",
            exemption.omission
        );
    }
}

// ---------------------------------------------------------------------------
// Hidden metadata is respected in both directions
// ---------------------------------------------------------------------------

/// A hidden argument, a hidden subcommand and a hidden possible value are
/// suppressed facts: the page must not carry them, and the oracle says so
/// rather than ignoring them.
#[test]
#[serial]
fn hidden_metadata_stays_off_the_page() {
    let (app, cmd) = concealing();
    let result = TestHarness::new()
        .text_output()
        .run(&app, cmd.clone(), ["notes", "--help"]);
    result.assert_success();
    let page = result.stdout_plain();

    assert_page_states_clap_facts(&page, &cmd, HelpLength::Long);
    assert!(
        !page.contains("--secret") && !page.contains("vault"),
        "the fixture only tests the oracle if the page really hides them:\n{page}"
    );

    // And a page that leaks one is caught.
    let leaked = page.replace("OPTIONS", "OPTIONS\n  --secret <TOKEN>           A secret");
    fails_naming(&["--secret"], || {
        assert_page_states_clap_facts(&leaked, &cmd, HelpLength::Long)
    });
}

// ---------------------------------------------------------------------------
// Length-specific hides follow clap's own visibility rules
// ---------------------------------------------------------------------------

/// The derivation says what each length hides: a `hide_short_help` argument
/// is suppressed at `-h` and stated at `--help`, `hide_long_help` the other
/// way around, and `next_line_help` overrides the length hide — clap's
/// `should_show_arg`, fact by fact.
#[test]
fn the_derivation_follows_length_specific_hides() {
    let cmd = length_concealing();
    let short = clap_facts(&cmd, HelpLength::Short);
    let long = clap_facts(&cmd, HelpLength::Long);

    let spelling = |facts: &[Fact], id: &str, presence: Presence| {
        assert!(
            facts.iter().any(|fact| fact.kind() == FactKind::ArgSpelling
                && *fact.subject() == Subject::Argument(id.to_string())
                && fact.presence() == presence),
            "`{id}` should have a {presence:?} spelling fact"
        );
    };

    spelling(&short, "verbose", Presence::Suppressed);
    spelling(&long, "verbose", Presence::Stated);
    spelling(&short, "terse", Presence::Stated);
    spelling(&long, "terse", Presence::Suppressed);
    spelling(&short, "insistent", Presence::Stated);
    spelling(&long, "insistent", Presence::Stated);
}

/// Clap's own pages ground the length rules: at each length, the page states
/// the arguments clap shows there and none it hides there, under an empty
/// allowlist. An oracle that ignored the length hides would demand a row
/// clap's own formatter refuses to print.
#[test]
fn length_specific_hides_ground_against_clap() {
    let cmd = length_concealing();
    for length in [HelpLength::Short, HelpLength::Long] {
        let page = clap_page(cmd.clone(), length);
        assert_page_states_clap_facts_with(&page, &cmd, length, &[]);
    }
}

/// An argument clap hides from the long page, leaking onto it anyway, fails —
/// the suppressed direction of the length rules.
#[test]
fn an_argument_hidden_from_one_length_leaking_onto_that_page_fails() {
    let cmd = length_concealing();
    let leaked = clap_page(cmd.clone(), HelpLength::Long).replace(
        "Options:",
        "Options:\n      --terse\n          Only the short page lists this\n",
    );
    fails_naming(&["--terse"], || {
        assert_page_states_clap_facts_with(&leaked, &cmd, HelpLength::Long, &[])
    });
}

/// A help-less short flag must not swallow the long flag clap renders below
/// it: `-f` sits at two columns and `--long-only` at six, so a parser that
/// filed "deeper indent" as "same row" would leave `--long-only` rowless and
/// fail clap's own page.
#[test]
fn a_help_less_short_flag_does_not_swallow_the_long_flag_below_it() {
    let cmd = Command::new("notes")
        .about("Keep short notes")
        .arg(Arg::new("force").short('f').action(ArgAction::SetTrue))
        .arg(
            Arg::new("long-only")
                .long("long-only")
                .action(ArgAction::SetTrue)
                .help("Spelled out in full"),
        );
    for length in [HelpLength::Short, HelpLength::Long] {
        let page = clap_page(cmd.clone(), length);
        assert_page_states_clap_facts_with(&page, &cmd, length, &[]);
    }
}

// ---------------------------------------------------------------------------
// Clap's value-list spellings all decode to the same names
// ---------------------------------------------------------------------------

/// Clap's other value-list spellings ground the decoder: a value with help
/// turns the long page's list into a `Possible values:` bullet region, a
/// value with whitespace renders quoted, and a quoted default decodes back to
/// the value clap would parse.
#[test]
fn helped_and_quoted_values_ground_against_clap() {
    let cmd = quoted_and_helped();
    for length in [HelpLength::Short, HelpLength::Long] {
        let page = clap_page(cmd.clone(), length);
        assert_page_states_clap_facts_with(&page, &cmd, length, &[]);
    }
}

/// A value dropped from the bullet region fails, naming the value — the
/// region's names live on bullet lines the labelled heading itself never
/// carries.
#[test]
fn a_dropped_possible_value_bullet_fails() {
    let cmd = quoted_and_helped();
    let page = drop_lines(&clap_page(cmd.clone(), HelpLength::Long), "- json");
    fails_naming(&["format", "json"], || {
        assert_page_states_clap_facts_with(&page, &cmd, HelpLength::Long, &[])
    });
}

/// A quoted value dropped from the inline list fails: recognizing
/// `"plain text"` as one value is the decoder's job, and a whitespace split
/// would never have stated it in the first place.
#[test]
fn a_dropped_quoted_possible_value_fails() {
    let cmd = quoted_and_helped();
    let page = clap_page(cmd.clone(), HelpLength::Short).replace("\"plain text\", ", "");
    fails_naming(&["format", "plain text"], || {
        assert_page_states_clap_facts_with(&page, &cmd, HelpLength::Short, &[])
    });
}

// ---------------------------------------------------------------------------
// Fixtures and helpers
// ---------------------------------------------------------------------------

/// A command whose arguments hide from one help length only: one absent from
/// `-h`, one absent from `--help`, and one whose `next_line_help` forces it
/// back onto both pages the way clap's `should_show_arg` says it must be.
///
/// Only clap's own pages render this command: standout's extractor honours
/// `hide` alone today, so a standout page would leak the length-hidden rows —
/// a finding for the framework work, not a fixture for this suite.
fn length_concealing() -> Command {
    Command::new("notes")
        .about("Keep short notes")
        .arg(Arg::new("range").value_name("RANGE").help("A range"))
        .arg(
            Arg::new("verbose")
                .long("verbose")
                .action(ArgAction::SetTrue)
                .hide_short_help(true)
                .help("Only the long page lists this"),
        )
        .arg(
            Arg::new("terse")
                .long("terse")
                .action(ArgAction::SetTrue)
                .hide_long_help(true)
                .help("Only the short page lists this"),
        )
        .arg(
            Arg::new("insistent")
                .long("insistent")
                .action(ArgAction::SetTrue)
                .hide_long_help(true)
                .next_line_help(true)
                .help("Next-line help overrides the length hide"),
        )
}

/// A command whose value lists exercise clap's other spellings: a possible
/// value with whitespace (quoted inline), one with help text (which turns the
/// long page's list into a bullet region), and a default that renders quoted.
fn quoted_and_helped() -> Command {
    Command::new("notes").about("Keep short notes").arg(
        Arg::new("format")
            .long("format")
            .value_name("FORMAT")
            .action(ArgAction::Set)
            .default_value("plain text")
            .value_parser([
                clap::builder::PossibleValue::new("plain text"),
                clap::builder::PossibleValue::new("json").help("One note per line"),
            ])
            .help("How to print the notes"),
    )
}

/// The shared fixture's nested page for one entry point, as plain text.
fn rendered(entry: &str) -> String {
    let fixture = downstream().build();
    let result =
        TestHarness::new()
            .text_output()
            .run(fixture.app(), fixture.command(), ["lookma", entry]);
    result.assert_success();
    result.stdout_plain()
}

/// Clap's own rendering of a command, at one help length.
fn clap_page(cmd: Command, length: HelpLength) -> String {
    let mut cmd = cmd;
    cmd.build();
    match length {
        HelpLength::Short => cmd.render_help().to_string(),
        HelpLength::Long => cmd.render_long_help().to_string(),
    }
}

/// The allowlist with one entry removed.
fn without(omission: Omission) -> Vec<Exemption> {
    DELIBERATE_OMISSIONS
        .iter()
        .filter(|exemption| exemption.omission != omission)
        .copied()
        .collect()
}

/// The page with every line containing `needle` removed.
fn drop_lines(page: &str, needle: &str) -> String {
    page.lines()
        .filter(|line| !line.contains(needle))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Runs `assertion`, requiring it to panic with a message naming each needle.
///
/// The needles are what the failure has to name for a reader to know which
/// fact went missing — an "assertion failed" that does not name the argument
/// and the value costs exactly the debugging session the oracle exists to
/// save.
#[track_caller]
fn fails_naming(needles: &[&str], assertion: impl FnOnce()) {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(assertion));
    let payload = match outcome {
        Err(payload) => payload,
        Ok(()) => panic!("expected the assertion to fail, naming {needles:?}"),
    };
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("<panic payload was not a string>")
        .to_string();

    for needle in needles {
        assert!(
            message.contains(needle),
            "the failure must name {needle:?}; it said:\n{message}"
        );
    }
}

/// A minimal app for the pairs below: help on, one subcommand, no theme.
///
/// The shared fixture deliberately carries no aliases, no `long_help` and
/// nothing hidden — it is shaped like a downstream app, not like a catalogue
/// of clap features. These local pairs exist only to hold the metadata the
/// allowlist and the hidden-metadata rules are about, so each entry can be
/// proved to do work.
fn notes_app() -> App {
    App::builder()
        .help_handling(true)
        .command("stat", |_m, _ctx| Ok(Output::Render(json!({}))), "stat")
        .unwrap()
        .build()
        .unwrap()
}

/// A command carrying every fact the allowlist exempts: an argument with a
/// `long_help`, short and long aliases, and an aliased subcommand.
fn decorated() -> (App, Command) {
    let cmd = Command::new("notes")
        .about("Keep short notes")
        .long_about("Keep short notes.\n\nOne file, one line each.")
        .arg(Arg::new("range").value_name("RANGE").help("A range"))
        .arg(
            Arg::new("threshold")
                .long("threshold")
                .visible_alias("thr")
                .visible_short_alias('t')
                .value_name("RATIO")
                .action(ArgAction::Set)
                .help("Similarity threshold")
                .long_help("Similarity threshold, as a ratio between 0 and 1"),
        )
        .subcommand(
            Command::new("stat")
                .about("Summarize the notes")
                .visible_alias("st"),
        );
    (notes_app(), cmd)
}

/// A command whose hidden metadata must stay off the page: a hidden argument,
/// a hidden subcommand, and a possible value hidden from an otherwise visible
/// enumeration.
fn concealing() -> (App, Command) {
    let cmd = Command::new("notes")
        .about("Keep short notes")
        .arg(Arg::new("range").value_name("RANGE").help("A range"))
        .arg(
            Arg::new("secret")
                .long("secret")
                .value_name("TOKEN")
                .action(ArgAction::Set)
                .hide(true)
                .help("A secret"),
        )
        .arg(
            Arg::new("format")
                .long("format")
                .value_name("FORMAT")
                .action(ArgAction::Set)
                .value_parser([
                    clap::builder::PossibleValue::new("text"),
                    clap::builder::PossibleValue::new("json"),
                    clap::builder::PossibleValue::new("vault").hide(true),
                ])
                .help("How to print the notes"),
        )
        .subcommand(Command::new("stat").about("Summarize the notes").hide(true));
    (notes_app(), cmd)
}
