//! The round-trip grounding test for the page decoders.
//!
//! WS05 produced six instances of one defect class across three review
//! rounds: a containment check satisfied by a superstring, a `split_once(':')`
//! truncating a value that carries a colon, a list decoder splitting on a
//! separator the value contains, a clause ending at a `]` belonging to a
//! quoted value, one decoder applied to the other page's grammar, and quote
//! tracking confused by a `"` inside an unquoted value. Each was answered by a
//! targeted test for the shape just found — which protects against the known
//! shapes only.
//!
//! This module is the test that can fail on an unanticipated one. A command's
//! values are known from [`clap::Command`]'s own getters; the command is
//! rendered — by clap's formatter under clap's grammar, by standout under
//! standout's — and the rendering is parsed back through the [`crate::page`]
//! decoders. The parsed multiset must **equal** the known set exactly: a
//! decoder that shears one value in two, swallows a neighbour across a
//! separator, or reads an adjacent clause as more values fails on the count
//! even where every individual containment check would still have passed.
//!
//! It is the same instrument as the differential's `render_long_help()`
//! grounding check ([`crate::clap_parity`]), pointed at the parser instead of
//! at the expectations.
//!
//! The instrument runs at two depths. The enumerated tables pin every
//! adversarial shape found so far, plus the control characters clap can only
//! state through its Rust-debug quoting (`"line\nbreak"` — a decoder that
//! reads `\n` as the letter `n` fails the equality). The property tests then
//! generate values freely over each page's *statable* grammar, so a decoder
//! defect in a shape nobody has enumerated yet still has a generator aimed
//! at it. What a grammar cannot state — a comma in standout's unquoted list,
//! a raw `]` in clap's clause — is excluded where each generator says so,
//! not silently.

use clap::builder::{PossibleValue, PossibleValuesParser};
use clap::{Arg, ArgAction, Command};
use proptest::prelude::*;
use serde_json::json;
use standout::cli::{App, Output};

use crate::page::{find_row, rows, takes_values, visible_args, ClapJoint};
use crate::TestHarness;

/// Every adversarial value the workstream accumulated, as one enumerated set:
/// a colon a `split_once(':')` would truncate at, a value carrying the list
/// separator (clap quotes it, and the decoder must not split inside the
/// quotes), bare whitespace (clap's other separator), a quoted value whose
/// `]` must not end the clause, a mid-word `"` that must stay a literal, and
/// an unquoted comma that is the value's own (`a,b` — clap separates with
/// comma-*space*, and would have quoted a value carrying the space).
const ADVERSARIAL: [&str; 6] = [
    "key:value",
    "a, b",
    "plain text",
    "[a b]",
    "foo\"bar",
    "a,b",
];

/// The values only clap's *quoted* spelling can carry: control characters,
/// which the inline clauses render as Rust-debug escapes (`"line\nbreak"`,
/// `"esc \u{1b}del\u{7f}"`) that must decode back to the characters they
/// stand for. They stay out of [`ADVERSARIAL`] because the long-help bullet
/// region prints a value's name raw — a control character there breaks the
/// page's own line structure, which is clap's rendering limit and not a
/// decoder defect.
const CONTROL: [&str; 4] = [
    "line\nbreak",
    "tab\tstop",
    "back\\slash here",
    "esc \u{1b}del\u{7f}",
];

/// The values standout's unquoted comma-joined grammar can state at all:
/// [`ADVERSARIAL`] minus the comma-carrying values, which that grammar has
/// no way to spell distinguishably — `a, b` *is* two values on a standout
/// page, and no decoder can be asked to read it back as one.
const STANDOUT_REPRESENTABLE: [&str; 4] = ["key:value", "plain text", "[a b]", "foo\"bar"];

/// Multiset comparison: order is the formatter's business, the values are not.
fn sorted(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values
}

/// The possible values clap's getters state for `arg` — the known set the
/// decoded one must reproduce.
fn known_possible_values(arg: &Arg) -> Vec<String> {
    arg.get_possible_values()
        .iter()
        .filter(|value| !value.is_hide_set())
        .map(|value| value.get_name().to_string())
        .collect()
}

/// The default values clap's getters state for `arg`.
fn known_defaults(arg: &Arg) -> Vec<String> {
    arg.get_default_values()
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect()
}

/// Decodes `page` back through the page parser and requires every value-taking
/// argument's decoded possible values and defaults to equal clap's known sets
/// exactly, as multisets.
#[track_caller]
fn assert_page_roundtrips(page: &str, cmd: &Command) {
    let mut built = cmd.clone();
    built.build();
    let rows = rows(page);

    for arg in visible_args(&built).filter(|arg| takes_values(arg)) {
        let row = find_row(&rows, arg)
            .unwrap_or_else(|| panic!("no row for `{}` on the page:\n{page}", arg.get_id()));

        assert_eq!(
            sorted(row.possible_value_names()),
            sorted(known_possible_values(arg)),
            "`{}`: the decoded possible values must equal clap's own set \
             exactly\n--- page ---\n{page}\n------------",
            arg.get_id()
        );
        assert_eq!(
            sorted(row.labelled_values("default:", ClapJoint::Spaces)),
            sorted(known_defaults(arg)),
            "`{}`: the decoded defaults must equal clap's own set \
             exactly\n--- page ---\n{page}\n------------",
            arg.get_id()
        );
    }
}

/// A command whose value lists render as clap's *inline* clauses — no possible
/// value carries help text — with every adversarial default the clause
/// decoders have been fooled by, plus a space-joined defaults list and the
/// control characters only clap's quoting can state.
///
/// The width is unbounded because clap otherwise wraps at 100 columns off a
/// tty, and a clause wrapped mid-value is a different limitation than the
/// decoding this instrument measures.
fn inline_clauses() -> Command {
    Command::new("notes")
        .about("Keep short notes")
        .term_width(0)
        .arg(
            Arg::new("format")
                .long("format")
                .value_name("FORMAT")
                .action(ArgAction::Set)
                .default_value("plain text")
                .value_parser(ADVERSARIAL)
                .help("How to print the notes"),
        )
        .arg(
            Arg::new("tag")
                .long("tag")
                .value_name("TAG")
                .action(ArgAction::Set)
                .default_value("key:value")
                .help("A colon is the value's own"),
        )
        .arg(
            Arg::new("file")
                .long("file")
                .value_name("FILE")
                .action(ArgAction::Set)
                .default_value("[a b]")
                .help("The bracket belongs to the quoted value"),
        )
        .arg(
            Arg::new("quote")
                .long("quote")
                .value_name("Q")
                .action(ArgAction::Set)
                .default_value("foo\"bar")
                .help("A mid-word quote is a literal"),
        )
        .arg(
            Arg::new("multi")
                .long("multi")
                .value_name("V")
                .action(ArgAction::Append)
                .default_values(["plain text", "a, b"])
                .help("Clap space-joins a defaults list"),
        )
        .arg(
            Arg::new("escape")
                .long("escape")
                .value_name("E")
                .action(ArgAction::Append)
                .default_values(CONTROL)
                .value_parser(CONTROL)
                .help("Debug escapes decode back to their characters"),
        )
}

/// A command whose long help renders the *bullet region*: one possible value
/// carries help text, which is what switches clap's long page over.
fn bulleted_values() -> Command {
    Command::new("notes").about("Keep short notes").arg(
        Arg::new("format")
            .long("format")
            .value_name("FORMAT")
            .action(ArgAction::Set)
            .value_parser(
                ADVERSARIAL
                    .iter()
                    .map(|value| PossibleValue::new(*value))
                    .chain([PossibleValue::new("json").help("One note per line")])
                    .collect::<Vec<_>>(),
            )
            .help("How to print the notes"),
    )
}

/// Clap's grammar, both lengths: whatever spelling clap picks for each value
/// — quoted or bare, inline clause or bullet — the decoders must reproduce
/// the declared multiset from the rendered page.
#[test]
fn claps_pages_decode_back_to_claps_own_values() {
    for cmd in [inline_clauses(), bulleted_values()] {
        let mut built = cmd.clone();
        built.build();
        for page in [
            built.render_help().to_string(),
            built.render_long_help().to_string(),
        ] {
            assert_page_roundtrips(&page, &cmd);
        }
    }
}

/// An app with a `stat` handler, so a command's `stat` subcommand has
/// somewhere to go when standout renders its help.
fn stat_app() -> App {
    App::builder()
        .help_handling(true)
        .command("stat", |_m, _ctx| Ok(Output::Render(json!({}))), "stat")
        .unwrap()
        .build()
        .unwrap()
}

/// The pair standout renders the same metadata through: the app carries a
/// `stat` handler so the command's subcommand has somewhere to go.
fn standout_pair() -> (App, Command) {
    let app = stat_app();

    let cmd = Command::new("notes")
        .about("Keep short notes")
        .arg(
            Arg::new("format")
                .long("format")
                .value_name("FORMAT")
                .action(ArgAction::Set)
                .default_value("plain text")
                .value_parser(STANDOUT_REPRESENTABLE)
                .help("How to print the notes"),
        )
        .arg(
            Arg::new("tag")
                .long("tag")
                .value_name("TAG")
                .action(ArgAction::Set)
                .default_value("key:value")
                .help("A colon is the value's own"),
        )
        .arg(
            Arg::new("file")
                .long("file")
                .value_name("FILE")
                .action(ArgAction::Set)
                .default_value("[a b]")
                .help("Standout never brackets its clause, so the value keeps its own"),
        )
        .arg(
            Arg::new("quote")
                .long("quote")
                .value_name("Q")
                .action(ArgAction::Set)
                .default_value("foo\"bar")
                .help("A mid-word quote is a literal"),
        )
        .subcommand(Command::new("stat").about("Summarize the notes"));

    (app, cmd)
}

/// Standout's grammar against standout's page: the same instrument over the
/// unquoted comma-joined spelling, which decodes differently from clap's —
/// the divergence the two decoders exist for, asserted end to end.
#[test]
#[serial_test::serial]
fn standouts_page_decodes_back_to_claps_own_values() {
    let (app, cmd) = standout_pair();
    let result = TestHarness::new()
        .text_output()
        .run(&app, cmd.clone(), ["notes", "--help"]);
    result.assert_success();
    let page = result.stdout_plain();

    assert!(
        page.contains("possible values: key:value, plain text, [a b], foo\"bar"),
        "the page only tests standout's grammar if it spells the list raw and \
         unquoted:\n{page}"
    );
    assert_page_roundtrips(&page, &cmd);
}

/// A character mix biased toward the decoders' hazards — the separators,
/// quotes, escapes, and clause punctuation — alongside arbitrary Unicode.
fn hazard_char() -> impl Strategy<Value = char> {
    prop_oneof![
        proptest::char::any(),
        proptest::sample::select(vec![
            ' ', ',', '"', '\\', '\n', '\t', '[', ']', ':', '=', '.', 'a',
        ]),
    ]
}

/// Whether clap's page states `value` unambiguously for this parser.
///
/// Clap quotes (and Rust-debug escapes) a value that is empty or carries
/// whitespace; every other value renders raw, and four raw spellings are
/// genuine page limits rather than decoder defects: a raw `]` is
/// indistinguishable from the bracket that ends the clause, a raw leading
/// `"` reads as clap's own quoting, a raw control character (one that is
/// not whitespace, so it never triggers the quoting) is stripped by clap's
/// own writer and reaches the page as nothing, and a value spelling one of
/// the page's clause labels is indistinguishable from that label to a
/// parser that is deliberately tolerant of two formatters' layouts.
fn clap_statable(value: &str) -> bool {
    let raw = !value.is_empty() && !value.contains(char::is_whitespace);
    let lower = value.to_ascii_lowercase();
    !(raw && (value.contains(']') || value.starts_with('"') || value.chars().any(char::is_control)))
        && !lower.contains("default:")
        && !lower.contains("possible values:")
}

/// An arbitrary value clap's page can state — the generator behind the
/// enumerated [`ADVERSARIAL`] and [`CONTROL`] tables.
fn clap_value() -> impl Strategy<Value = String> {
    proptest::collection::vec(hazard_char(), 0..8)
        .prop_map(String::from_iter)
        .prop_filter("the page cannot state the value unambiguously", |value| {
            clap_statable(value)
        })
}

/// An arbitrary value standout's unquoted comma-joined grammar can state:
/// no comma (it *is* the separator), no control characters (the page's own
/// line structure is the grammar), no leading or trailing whitespace and no
/// empty value (an unquoted spelling cannot show them), no brackets (the
/// renderer reads bracketed words as markup — [`STANDOUT_REPRESENTABLE`]
/// keeps a literal `[a b]` covering the ones it treats as text), and no
/// spelling of the page's own labels, and no trailing `\` — the renderer's
/// markup grammar reads `\[` as an escape, so a value's final backslash
/// would swallow the tag that follows it on the rendered line.
fn standout_value() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            proptest::char::range('a', 'z'),
            proptest::sample::select(vec![' ', ':', '"', '\\', '.', '-', '=', 'é']),
        ],
        1..8,
    )
    .prop_map(String::from_iter)
    .prop_filter("the grammar cannot state the value", |value| {
        value.trim() == value
            && !value.ends_with('\\')
            && !value.contains("default:")
            && !value.contains("possible values:")
    })
}

proptest! {
    /// The property behind the enumerated tables, over clap's grammar:
    /// whatever statable values a command declares, both help lengths decode
    /// back to exactly those values — however clap chose to spell each one.
    #[test]
    fn any_statable_value_round_trips_through_claps_page(
        defaults in proptest::collection::vec(clap_value(), 1..3),
        choices in proptest::collection::hash_set(clap_value(), 1..3),
    ) {
        let cmd = Command::new("notes")
            .term_width(0)
            .arg(
                Arg::new("field")
                    .long("field")
                    .value_name("F")
                    .action(ArgAction::Append)
                    .default_values(defaults),
            )
            .arg(
                Arg::new("choice")
                    .long("choice")
                    .value_name("C")
                    .action(ArgAction::Set)
                    .value_parser(PossibleValuesParser::new(
                        choices.into_iter().collect::<Vec<_>>(),
                    )),
            );

        let mut built = cmd.clone();
        built.build();
        for page in [
            built.render_help().to_string(),
            built.render_long_help().to_string(),
        ] {
            assert_page_roundtrips(&page, &cmd);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]
    /// The same property over standout's grammar, through a full render: any
    /// representable default and possible-value set decodes back exactly.
    /// Fewer cases than the clap property because each one runs the whole
    /// harness; the shapes the run can't reach stay pinned by the enumerated
    /// [`STANDOUT_REPRESENTABLE`] test above.
    #[test]
    #[serial_test::serial]
    fn any_representable_value_round_trips_through_standouts_page(
        default in standout_value(),
        choices in proptest::collection::hash_set(standout_value(), 1..3),
    ) {
        let app = stat_app();
        let cmd = Command::new("notes")
            .about("Keep short notes")
            .arg(
                Arg::new("field")
                    .long("field")
                    .value_name("F")
                    .action(ArgAction::Set)
                    .default_value(default),
            )
            .arg(
                Arg::new("choice")
                    .long("choice")
                    .value_name("C")
                    .action(ArgAction::Set)
                    .value_parser(PossibleValuesParser::new(
                        choices.into_iter().collect::<Vec<_>>(),
                    )),
            )
            .subcommand(Command::new("stat").about("Summarize the notes"));

        let result = TestHarness::new()
            .text_output()
            .run(&app, cmd.clone(), ["notes", "--help"]);
        result.assert_success();
        assert_page_roundtrips(&result.stdout_plain(), &cmd);
    }
}
