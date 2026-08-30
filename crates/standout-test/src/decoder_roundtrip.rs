use crate::page::{find_row, rows, takes_values, visible_args, ClapJoint};
use crate::TestHarness;
use clap::builder::{PossibleValue, PossibleValuesParser};
use clap::{Arg, ArgAction, Command};
use proptest::prelude::*;
use serde_json::json;
use standout::cli::{App, Output};
const ADVERSARIAL: [&str; 7] = [
    "key:value",
    "key: value",
    "a, b",
    "plain text",
    "[a b]",
    "foo\"bar",
    "a,b",
];
const CONTROL: [&str; 4] = [
    "line\nbreak",
    "tab\tstop",
    "back\\slash here",
    "esc \u{1b}del\u{7f}",
];
const STANDOUT_REPRESENTABLE: [&str; 4] = ["key:value", "plain text", "[a b]", "foo\"bar"];
fn sorted(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values
}
fn known_possible_values(arg: &Arg) -> Vec<String> {
    arg.get_possible_values()
        .iter()
        .filter(|value| !value.is_hide_set())
        .map(|value| value.get_name().to_string())
        .collect()
}
fn known_defaults(arg: &Arg) -> Vec<String> {
    arg.get_default_values()
        .iter()
        .map(|value| value.to_string_lossy().into_owned())
        .collect()
}
#[track_caller]
fn assert_page_roundtrips(page: &str, cmd: &Command) {
    let mut built = cmd.clone();
    built.build();
    let rows = rows(page);
    for arg in visible_args(&built).filter(|arg| takes_values(arg)) {
        let row = find_row(&rows, arg)
            .unwrap_or_else(|| panic!("no row for `{}` on the page:\n{page}", arg.get_id()));
        assert_eq!(
            sorted(row.possible_value_names(arg)),
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
fn stat_app() -> App {
    App::builder()
        .help_handling(true)
        .command("stat", |_m, _ctx| Ok(Output::Render(json!({}))), "stat")
        .unwrap()
        .build()
        .unwrap()
}
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
fn hazard_char() -> impl Strategy<Value = char> {
    prop_oneof![
        proptest::char::any(),
        proptest::sample::select(vec![
            ' ', ',', '"', '\\', '\n', '\t', '[', ']', ':', '=', '.', 'a',
        ]),
    ]
}
fn clap_statable(value: &str) -> bool {
    let raw = !value.is_empty() && !value.contains(char::is_whitespace);
    let lower = value.to_ascii_lowercase();
    !(raw && (value.contains(']') || value.starts_with('"') || value.chars().any(char::is_control)))
        && !lower.contains("default:")
        && !lower.contains("possible values:")
}
fn clap_value() -> impl Strategy<Value = String> {
    proptest::collection::vec(hazard_char(), 0..8)
        .prop_map(String::from_iter)
        .prop_filter("the page cannot state the value unambiguously", |value| {
            clap_statable(value)
        })
}
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
