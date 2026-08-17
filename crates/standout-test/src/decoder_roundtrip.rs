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

use clap::builder::PossibleValue;
use clap::{Arg, ArgAction, Command};
use serde_json::json;
use standout::cli::{App, Output};

use crate::page::{find_row, rows, takes_values, visible_args};
use crate::TestHarness;

/// Every adversarial value the workstream accumulated, as one enumerated set:
/// a colon a `split_once(':')` would truncate at, a value carrying the list
/// separator (clap quotes it, and the decoder must not split inside the
/// quotes), bare whitespace (clap's other separator), a quoted value whose
/// `]` must not end the clause, and a mid-word `"` that must stay a literal.
const ADVERSARIAL: [&str; 5] = ["key:value", "a, b", "plain text", "[a b]", "foo\"bar"];

/// The values standout's unquoted comma-joined grammar can state at all:
/// [`ADVERSARIAL`] minus the comma-carrying value, which that grammar has no
/// way to spell distinguishably — `a, b` *is* two values on a standout page,
/// and no decoder can be asked to read it back as one.
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
            sorted(row.labelled_values("default:")),
            sorted(known_defaults(arg)),
            "`{}`: the decoded defaults must equal clap's own set \
             exactly\n--- page ---\n{page}\n------------",
            arg.get_id()
        );
    }
}

/// A command whose value lists render as clap's *inline* clauses — no possible
/// value carries help text — with every adversarial default the clause
/// decoders have been fooled by, plus a space-joined defaults list.
fn inline_clauses() -> Command {
    Command::new("notes")
        .about("Keep short notes")
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

/// The pair standout renders the same metadata through: the app carries a
/// `stat` handler so the command's subcommand has somewhere to go.
fn standout_pair() -> (App, Command) {
    let app = App::builder()
        .help_handling(true)
        .command("stat", |_m, _ctx| Ok(Output::Render(json!({}))), "stat")
        .unwrap()
        .build()
        .unwrap();

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
