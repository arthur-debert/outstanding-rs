use clap::{Arg, Command};
use standout::cli::{render_help, HelpConfig, HelpLength, HelpResult};
use standout::Representation;
use standout_fixtures::{downstream, Fixture};

fn page(fixture: &Fixture, args: &[&str]) -> String {
    match fixture.app().get_matches_from(
        fixture.command(),
        args,
        &standout::InputSources::from_process(),
    ) {
        HelpResult::Help(text) | HelpResult::PagedHelp(text) => text,
        other => panic!("expected rendered help, got: {other:?}"),
    }
}

fn help_for(args: &[&str]) -> String {
    page(&downstream().build(), args)
}

fn flat_help_for(args: &[&str]) -> String {
    page(&downstream().flat().build(), args)
}

fn row<'a>(output: &'a str, name: &str) -> &'a str {
    output
        .lines()
        .find(|line| line.trim_start().starts_with(name))
        .unwrap_or_else(|| panic!("no row for {name} in:\n{output}"))
}

fn description_column(output: &str, name: &str, description: &str) -> usize {
    let line = row(output, name);
    line.find(description)
        .unwrap_or_else(|| panic!("{name} row has no description {description:?}: {line:?}"))
}

#[test]
fn long_option_name_keeps_its_separator() {
    let output = help_for(&["lookma", "--help"]);
    let line = row(&output, "--output-file-path");

    assert!(
        line.contains("--output-file-path <PATH>  Write output"),
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

#[test]
fn short_names_keep_the_floor_width() {
    let cmd = Command::new("app")
        .disable_help_flag(true)
        .about("App")
        .arg(Arg::new("out").long("out").help("Output"));

    let config = HelpConfig {
        output_mode: Some(Representation::Human),
        ..Default::default()
    };
    let output = render_help(&cmd, Some(config)).unwrap();

    assert_eq!(
        description_column(&output, "--out", "Output"),
        16,
        "{output}"
    );
}

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
        output_mode: Some(Representation::Human),
        length: HelpLength::Long,
        ..Default::default()
    };
    let output = render_help(&cmd, Some(config)).unwrap();
    assert!(output.contains("Only terse"), "{output}");
}

#[test]
fn option_rows_carry_defaults_and_possible_values() {
    let output = help_for(&["lookma", "--help"]);

    assert!(
        output.contains("possible values: json, yaml, csv, ndjson, term-debug"),
        "{output}"
    );
}

#[test]
fn an_apps_own_enumerated_option_carries_its_default_and_values() {
    let output = help_for(&["lookma", "--help"]);

    assert!(output.contains("default: brief"), "{output}");
    assert!(
        output.contains("possible values: brief, full, none"),
        "{output}"
    );
}

#[test]
fn value_taking_options_render_their_metavars() {
    let output = help_for(&["lookma", "--help"]);

    assert!(output.contains("--threshold <RATIO>"), "{output}");
    assert!(output.contains("-c, --color <BOOL>"), "{output}");
    assert!(
        output.contains("--pattern <pattern>"),
        "options without an explicit value_name should keep clap's fallback metavar:\n{output}"
    );
}

#[test]
fn presence_flags_do_not_render_bool_values() {
    let output = help_for(&["lookma", "--help"]);
    let staged = row(&output, "--staged");

    assert!(
        !staged.contains('<'),
        "presence flags take no value:\n{output}"
    );
    assert!(
        !staged.contains("true") && !staged.contains("false"),
        "presence flags should not advertise parser-derived bool values:\n{output}"
    );
}

#[test]
fn default_and_values_lines_align_with_descriptions() {
    let output = help_for(&["lookma", "--help"]);

    let description = description_column(&output, "--output", "Structured output encoding");
    let default = row(&output, "default:").find("default:").unwrap();
    let values = row(&output, "possible values:")
        .find("possible values:")
        .unwrap();

    assert_eq!(default, description, "default line must hang:\n{output}");
    assert_eq!(values, description, "values line must hang:\n{output}");
}

fn section_line(output: &str, header: &str) -> usize {
    output
        .lines()
        .position(|line| line.trim() == header)
        .unwrap_or_else(|| panic!("no {header} section in:\n{output}"))
}

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

#[test]
fn flat_cli_suppresses_a_help_only_commands_section() {
    let output = page(
        &downstream().flat().without_topics().build(),
        &["lookma", "--help"],
    );

    assert!(
        !output.lines().any(|line| line.trim() == "COMMANDS"),
        "a flat CLI has no commands to list:\n{output}"
    );
}

#[test]
fn registered_topics_keep_the_help_word_listed() {
    let output = flat_help_for(&["lookma", "--help"]);

    section_line(&output, "COMMANDS");
    assert!(
        row(&output, "help").contains("Print this message"),
        "the word must be listed so `help <topic>` is discoverable:\n{output}"
    );
}

#[test]
fn flat_cli_help_word_does_not_mention_subcommands() {
    let fixture = downstream().flat().build();
    let augmented = fixture.app().augment_command_with_help(fixture.command());
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
    let fixture = downstream().build();
    let augmented = fixture.app().augment_command_with_help(fixture.command());
    let word = augmented
        .get_subcommands()
        .find(|sub| sub.get_name() == "help")
        .expect("a root with subcommands always gets the word");

    assert_eq!(
        word.get_about().map(|about| about.to_string()),
        Some("Print this message or the help of the given subcommand(s)".to_string())
    );
}

#[test]
fn nested_cli_keeps_its_commands_section() {
    let output = help_for(&["lookma", "--help"]);

    section_line(&output, "COMMANDS");
    assert!(
        row(&output, "review").contains("Review a range hunk by hunk"),
        "{output}"
    );
}

#[test]
fn the_help_flag_clap_generates_gets_a_row() {
    let output = help_for(&["lookma", "--help"]);
    let line = row(&output, "-h, --help");

    assert!(
        line.contains("Print help"),
        "the row must carry clap's own help text: {line:?}"
    );
}

#[test]
fn the_version_flag_clap_generates_gets_a_row() {
    let cmd = Command::new("notes")
        .about("Keep short notes")
        .version("1.2.3")
        .disable_help_subcommand(true);

    let output = render_help(
        &cmd,
        Some(HelpConfig {
            output_mode: Some(Representation::Human),
            ..Default::default()
        }),
    )
    .expect("the themed page renders");

    assert!(
        row(&output, "-V, --version").contains("Print version"),
        "clap accepts `-V`/`--version`, so the page states them:\n{output}"
    );
}

#[test]
fn a_valueless_flag_states_no_default() {
    let output = help_for(&["lookma", "--help"]);
    let line = row(&output, "--staged");

    assert!(
        !output.contains("default: false"),
        "clap builds a `false` default onto a SetTrue flag and hides it from its \
         own page; the themed page hides it too: {line:?}\n{output}"
    );
}
