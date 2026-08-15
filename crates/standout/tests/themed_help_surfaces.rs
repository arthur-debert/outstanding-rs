//! What themed help puts on the page, asserted on rendered output.
//!
//! Covers the three defects that surfaced together when lookma migrated off
//! clap's formatter — a name column that collided with its descriptions
//! (#297), information clap surfaces that the template had no slot for (#298),
//! and a `help` word that described subcommands a flat CLI does not have
//! (#299). They are one test file because they are one page: each is only
//! visible in what the reader ends up seeing.
//!
//! Assertions run in `Text` mode, so a row is its final characters and column
//! positions are directly comparable.

use clap::{Arg, ArgAction, Command};
use standout::cli::{render_help, App, HelpConfig, HelpLength, HelpResult};
use standout::topics::{Topic, TopicRegistry, TopicType};
use standout::OutputMode;

/// The reported shape: a flat CLI whose option set includes standout's own
/// `--output-file-path`, which is longer than the old fixed column.
fn lookma() -> Command {
    Command::new("lookma")
        .about("Diff a git range")
        .long_about("Diff a git range.\n\nNames a change the way a human would.")
        .arg(
            Arg::new("range")
                .value_name("RANGE")
                .help("Git range to diff, e.g. main..HEAD"),
        )
        .arg(
            Arg::new("staged")
                .long("staged")
                .action(ArgAction::SetTrue)
                .help("Diff the staged changes"),
        )
}

fn app() -> App {
    App::new()
        .help_handling(true)
        .help_word(true)
        .build()
        .unwrap()
}

fn help_for(args: &[&str]) -> String {
    match app().get_matches_from(lookma(), args) {
        HelpResult::Help(text) | HelpResult::PagedHelp(text) => text,
        other => panic!("expected rendered help, got: {other:?}"),
    }
}

/// The row whose name cell starts with `name`.
fn row<'a>(output: &'a str, name: &str) -> &'a str {
    output
        .lines()
        .find(|line| line.trim_start().starts_with(name))
        .unwrap_or_else(|| panic!("no row for {name} in:\n{output}"))
}

/// The column its description starts at.
fn description_column(output: &str, name: &str, description: &str) -> usize {
    let line = row(output, name);
    line.find(description)
        .unwrap_or_else(|| panic!("{name} row has no description {description:?}: {line:?}"))
}

// ---------------------------------------------------------------------------
// #297 — the name column
// ---------------------------------------------------------------------------

/// The reported symptom: `--output-file-path` ran straight into its
/// description, because the column was fixed at 14 and padding saturated to
/// zero.
#[test]
fn long_option_name_keeps_its_separator() {
    let output = help_for(&["lookma", "--help"]);
    let line = row(&output, "--output-file-path");

    assert!(
        line.contains("--output-file-path  Write output"),
        "the longest option must keep a gap before its description: {line:?}"
    );
    assert!(
        !line.contains("--output-file-pathWrite"),
        "the reported collision is back: {line:?}"
    );
}

#[test]
fn every_option_description_starts_at_one_column() {
    let output = help_for(&["lookma", "--help"]);

    let columns: Vec<usize> = [
        ("--staged", "Diff the staged changes"),
        ("--output-file-path", "Write output"),
    ]
    .iter()
    .map(|(name, description)| description_column(&output, name, description))
    .collect();

    assert!(
        columns.iter().all(|column| *column == columns[0]),
        "options must share one column, got {columns:?}\n{output}"
    );
}

/// Sections size independently, the way clap's do: the long flag in OPTIONS
/// must not drag the ARGUMENTS column out with it.
#[test]
fn arguments_and_options_columns_are_independent() {
    let output = help_for(&["lookma", "--help"]);

    let arguments = description_column(&output, "RANGE", "Git range");
    let options = description_column(&output, "--output-file-path", "Write output");

    assert!(
        arguments < options,
        "ARGUMENTS should keep the narrow column; got {arguments} and {options}\n{output}"
    );
}

/// A short option set keeps the layout it had before the column could widen.
#[test]
fn short_names_keep_the_floor_width() {
    let cmd = Command::new("app")
        .disable_help_flag(true)
        .about("App")
        .arg(Arg::new("out").long("out").help("Output"));

    let config = HelpConfig {
        output_mode: Some(OutputMode::Text),
        ..Default::default()
    };
    let output = render_help(&cmd, Some(config)).unwrap();

    // Two-space indent, a 12-column name cell, then the two-column gap.
    assert_eq!(
        description_column(&output, "--out", "Output"),
        16,
        "{output}"
    );
}

// ---------------------------------------------------------------------------
// #298 — the information clap surfaces
// ---------------------------------------------------------------------------

#[test]
fn short_help_renders_about_and_long_help_renders_long_about() {
    let short = help_for(&["lookma", "-h"]);
    let long = help_for(&["lookma", "--help"]);

    assert!(short.contains("Diff a git range"), "{short}");
    assert!(
        !short.contains("Names a change the way a human would"),
        "-h must stay terse:\n{short}"
    );
    assert!(
        long.contains("Names a change the way a human would"),
        "--help must render long_about:\n{long}"
    );
}

/// The `help` word is the spelled-out request, so it reads like `--help`.
#[test]
fn help_word_renders_long_about() {
    let word = help_for(&["lookma", "help"]);
    assert!(
        word.contains("Names a change the way a human would"),
        "{word}"
    );
}

#[test]
fn long_help_falls_back_to_about_when_no_long_about() {
    let cmd = Command::new("app")
        .disable_help_flag(true)
        .about("Only terse");

    let config = HelpConfig {
        output_mode: Some(OutputMode::Text),
        length: HelpLength::Long,
        ..Default::default()
    };
    let output = render_help(&cmd, Some(config)).unwrap();
    assert!(output.contains("Only terse"), "{output}");
}

/// `--output` is standout's own flag, so every app enabling themed help lost
/// the list of modes standout itself provides.
#[test]
fn option_rows_carry_defaults_and_possible_values() {
    let output = help_for(&["lookma", "--help"]);

    assert!(output.contains("default: auto"), "{output}");
    assert!(
        output.contains("possible values: auto, term, text, term-debug, json, yaml, xml, csv"),
        "{output}"
    );
}

/// The continuation lines hang under the description column, not the name.
#[test]
fn default_and_values_lines_align_with_descriptions() {
    let output = help_for(&["lookma", "--help"]);

    let description = description_column(&output, "--output", "Output format");
    let default = row(&output, "default:").find("default:").unwrap();
    let values = row(&output, "possible values:")
        .find("possible values:")
        .unwrap();

    assert_eq!(default, description, "default line must hang:\n{output}");
    assert_eq!(values, description, "values line must hang:\n{output}");
}

/// The line index of a section header — matched as a whole line, since
/// "OPTIONS" also appears inside the usage line as `[OPTIONS]`.
fn section_line(output: &str, header: &str) -> usize {
    output
        .lines()
        .position(|line| line.trim() == header)
        .unwrap_or_else(|| panic!("no {header} section in:\n{output}"))
}

/// Positionals used to be filed under OPTIONS behind every flag, which read as
/// though the primary argument were an afterthought.
#[test]
fn positionals_get_their_own_section_before_options() {
    let output = help_for(&["lookma", "--help"]);

    let arguments = section_line(&output, "ARGUMENTS");
    let options = section_line(&output, "OPTIONS");
    assert!(
        arguments < options,
        "ARGUMENTS must precede OPTIONS:\n{output}"
    );

    let range = output
        .lines()
        .position(|line| line.trim_start().starts_with("RANGE"))
        .expect(&output);
    assert!(
        range > arguments && range < options,
        "the positional belongs in ARGUMENTS:\n{output}"
    );
}

// ---------------------------------------------------------------------------
// #299 — the flat-CLI `help` word
// ---------------------------------------------------------------------------

/// A COMMANDS section whose only entry is the machinery printing it.
#[test]
fn flat_cli_suppresses_a_help_only_commands_section() {
    let output = help_for(&["lookma", "--help"]);
    assert!(
        !output.lines().any(|line| line.trim() == "COMMANDS"),
        "a flat CLI has no commands to list:\n{output}"
    );
}

/// Registered topics earn the section back: `help <topic>` is a real
/// destination, and the word is how a reader reaches it.
#[test]
fn registered_topics_keep_the_help_word_listed() {
    let mut registry = TopicRegistry::new();
    registry.add_topic(Topic::new(
        "Storage",
        "Where data is stored",
        TopicType::Text,
        None,
    ));

    let mut app = App::new().help_handling(true).help_word(true);
    for topic in registry.list_topics() {
        app = app.add_topic(topic.clone());
    }
    let app = app.build().unwrap();

    let output = match app.get_matches_from(lookma(), ["lookma", "--help"]) {
        HelpResult::Help(text) | HelpResult::PagedHelp(text) => text,
        other => panic!("expected rendered help, got: {other:?}"),
    };

    section_line(&output, "COMMANDS");
    assert!(
        row(&output, "help").contains("Print this message"),
        "the word must be listed so `help <topic>` is discoverable:\n{output}"
    );
}

/// The wording clap ships points at a namespace a flat CLI does not have.
#[test]
fn flat_cli_help_word_does_not_mention_subcommands() {
    let augmented = app().augment_command_with_help(lookma());
    let word = augmented
        .get_subcommands()
        .find(|sub| sub.get_name() == "help")
        .expect("the word is installed on this shape");

    assert_eq!(
        word.get_about().map(|about| about.to_string()),
        Some("Print this message".to_string())
    );
}

#[test]
fn nested_cli_help_word_still_mentions_subcommands() {
    let cmd = Command::new("app")
        .about("App")
        .subcommand(Command::new("build").about("Build it"));

    let augmented = app().augment_command_with_help(cmd);
    let word = augmented
        .get_subcommands()
        .find(|sub| sub.get_name() == "help")
        .expect("a root with subcommands always gets the word");

    assert_eq!(
        word.get_about().map(|about| about.to_string()),
        Some("Print this message or the help of the given subcommand(s)".to_string())
    );
}

/// A root that has commands of its own keeps its COMMANDS section, `help`
/// included.
#[test]
fn nested_cli_keeps_its_commands_section() {
    let cmd = Command::new("app")
        .about("App")
        .subcommand(Command::new("build").about("Build it"));

    let output = match app().get_matches_from(cmd, ["app", "--help"]) {
        HelpResult::Help(text) | HelpResult::PagedHelp(text) => text,
        other => panic!("expected rendered help, got: {other:?}"),
    };

    section_line(&output, "COMMANDS");
    assert!(row(&output, "build").contains("Build it"), "{output}");
}
