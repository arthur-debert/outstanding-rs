//! Help data extraction from clap commands.
//!
//! Extraction answers *what* a help page says: names, value syntax,
//! descriptions, defaults, possible values, and the width each section's name
//! column needs. It does not answer what the page looks like — no padding
//! strings and no style tags. Laying rows out is the template's job, which
//! aligns them with `standout-render`'s tabular `pad_right` filter against the
//! widths computed here.
//!
//! The split matters because width is a *display* measurement. Resolving it
//! through [`crate::tabular`] counts terminal columns, so a CJK name measures
//! its real width and a name carrying style tags measures its visible text —
//! neither of which byte length gets right.

use crate::tabular::{Column, FlatDataSpec, Width};
use crate::topics::TopicRegistry;
use clap::Command;
use serde::Serialize;
use std::collections::BTreeMap;

use super::config::{CommandGroup, HelpLength};

/// Floor for a section's name column, in display columns.
///
/// The `min` of the [`Width::Bounded`] name column: a section whose names all
/// fit keeps this width, so a small option set renders at a stable indent
/// instead of hugging its longest name.
const NAME_COLUMN_MIN: usize = 12;

/// Gap between the name column and the description column.
///
/// The tabular spec's separator, and the literal gap the template writes
/// between the two columns; the two must agree for a row to land where its
/// width was resolved for.
const COLUMN_SEPARATOR: &str = "  ";

/// Terminal width assumed when the real one cannot be detected.
const ASSUMED_TERMINAL_WIDTH: usize = 80;

/// Resolves the width of one section's name column from that section's names.
///
/// The layout is two tabular columns — a [`Width::Bounded`] name column and a
/// [`Width::Fill`] description column — so the name column comes out as wide as
/// the widest name (never less than [`NAME_COLUMN_MIN`]) and the terminal's
/// remaining space lands in the description, not in the gap before it.
///
/// Callers pass every name a section renders, including names split across
/// groups: one width per section is what keeps a grouped command list aligned
/// down the page instead of per group.
pub(crate) fn resolve_name_column(names: &[&str]) -> usize {
    let spec = FlatDataSpec::builder()
        .column(Column::new(Width::Bounded {
            min: Some(NAME_COLUMN_MIN),
            max: None,
        }))
        .column(Column::new(Width::Fill))
        .separator(COLUMN_SEPARATOR)
        .build();

    let rows: Vec<Vec<&str>> = names.iter().map(|name| vec![*name]).collect();
    let terminal_width = standout_render::detect_terminal_width().unwrap_or(ASSUMED_TERMINAL_WIDTH);

    spec.resolve_widths_from_data(terminal_width, &rows)
        .get(0)
        .unwrap_or(NAME_COLUMN_MIN)
}

#[derive(Serialize)]
pub(crate) struct HelpData {
    pub name: String,
    pub about: String,
    pub usage: String,
    pub subcommands: Vec<Group<Subcommand>>,
    pub subcommands_width: usize,
    pub arguments: Vec<Group<OptionData>>,
    pub arguments_width: usize,
    pub options: Vec<Group<OptionData>>,
    pub options_width: usize,
    pub examples: String,
    pub learn_more: Vec<TopicListItem>,
    pub learn_more_width: usize,
}

/// A titled block of rows within a section.
///
/// Groups subdivide a section for display; they do not subdivide its column,
/// whose width lives on [`HelpData`] per section.
#[derive(Serialize)]
pub(crate) struct Group<T> {
    pub title: Option<String>,
    pub help: Option<String>,
    pub items: Vec<T>,
}

#[derive(Serialize)]
pub(crate) struct Subcommand {
    pub name: String,
    pub about: String,
    pub separator: bool,
}

/// A row in the ARGUMENTS or OPTIONS section.
///
/// Positionals and flags differ only in how they are named and how the template
/// tags them, so both are carried by this one shape; a positional leaves
/// `short`, `long`, and `value_name` empty because its `name` already is the
/// rendered metavar.
#[derive(Serialize)]
pub(crate) struct OptionData {
    pub name: String,
    pub value_name: Option<String>,
    pub help: String,
    pub short: Option<char>,
    pub long: Option<String>,
    pub default: Option<String>,
    pub possible_values: Vec<String>,
}

#[derive(Serialize)]
pub(crate) struct TopicListItem {
    pub name: String,
    pub title: String,
}

/// The name a flag is listed under: `-s, --long`, or its id when it has neither.
fn flag_name(arg: &clap::Arg) -> String {
    let mut name = String::new();
    if let Some(short) = arg.get_short() {
        name.push_str(&format!("-{}", short));
    }
    if let Some(long) = arg.get_long() {
        if !name.is_empty() {
            name.push_str(", ");
        }
        name.push_str(&format!("--{}", long));
    }
    if name.is_empty() {
        name = arg.get_id().to_string();
    }
    name
}

/// The name a positional is listed under: its declared value name, else its id.
///
/// Clap wraps the value name in `<>` or `[]` to show whether it is required;
/// standout leaves the brackets off and tags the name `[metavar]` instead, so
/// the distinction is the theme's to draw rather than punctuation baked into
/// every row.
fn positional_name(arg: &clap::Arg) -> String {
    arg.get_value_names()
        .and_then(|names| names.first())
        .map(|name| name.to_string())
        .unwrap_or_else(|| arg.get_id().to_string())
}

/// Whether the argument consumes command-line values.
///
/// Clap's bool presence flags (`SetTrue`/`SetFalse`) still carry a bool value
/// parser, but they do not accept a value in argv. Help must follow the parse
/// contract rather than the parser implementation detail.
fn takes_values(arg: &clap::Arg) -> bool {
    arg.get_num_args()
        .map(|range| range.takes_values())
        .unwrap_or_else(|| arg.get_action().takes_values())
}

/// The rendered value suffix for an option, without its leading space.
///
/// This uses only public, no-build [`clap::Arg`] data because direct
/// [`render_help`](super::render_help) callers can pass an unbuilt command.
/// The full clap display formatter requires Clap's internal build step.
fn flag_value_name(arg: &clap::Arg) -> Option<String> {
    if !takes_values(arg) {
        return None;
    }

    let names = arg
        .get_value_names()
        .map(|names| names.iter().map(ToString::to_string).collect::<Vec<_>>())
        .unwrap_or_else(|| vec![arg.get_id().to_string()]);

    Some(
        names
            .iter()
            .map(|name| format!("<{name}>"))
            .collect::<Vec<_>>()
            .join(" "),
    )
}

/// The argument's default, as the row should read it — `None` when it has none.
fn default_value(arg: &clap::Arg) -> Option<String> {
    let defaults = arg.get_default_values();
    if defaults.is_empty() {
        return None;
    }
    Some(
        defaults
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(", "),
    )
}

/// The argument's selectable values, hidden ones left out.
fn possible_values(arg: &clap::Arg) -> Vec<String> {
    if !takes_values(arg) {
        return Vec::new();
    }

    arg.get_possible_values()
        .iter()
        .filter(|value| !value.is_hide_set())
        .map(|value| value.get_name().to_string())
        .collect()
}

fn option_row(name: String, value_name: Option<String>, arg: &clap::Arg) -> OptionData {
    OptionData {
        name,
        value_name,
        help: arg.get_help().map(|s| s.to_string()).unwrap_or_default(),
        short: arg.get_short(),
        long: arg.get_long().map(|s| s.to_string()),
        default: default_value(arg),
        possible_values: possible_values(arg),
    }
}

/// Splits rows into groups by clap's `help_heading`, preserving argument order.
fn group_by_heading(rows: Vec<(Option<String>, OptionData)>) -> Vec<Group<OptionData>> {
    let mut by_heading: BTreeMap<Option<String>, Vec<OptionData>> = BTreeMap::new();
    for (heading, row) in rows {
        by_heading.entry(heading).or_default().push(row);
    }
    by_heading
        .into_iter()
        .map(|(title, items)| Group {
            title,
            help: None,
            items,
        })
        .collect()
}

/// Every name a section renders, for width resolution.
fn section_names(groups: &[Group<OptionData>]) -> Vec<&str> {
    groups
        .iter()
        .flat_map(|group| group.items.iter().map(|item| item.name.as_str()))
        .collect()
}

/// Every visible label an option section renders, including value syntax.
fn option_section_names(groups: &[Group<OptionData>]) -> Vec<String> {
    groups
        .iter()
        .flat_map(|group| {
            group.items.iter().map(|item| match &item.value_name {
                Some(value_name) => format!("{} {}", item.name, value_name),
                None => item.name.clone(),
            })
        })
        .collect()
}

/// Whether the only command on offer is standout's own `help` word.
///
/// The word is machinery standout installs, not part of the application's
/// surface, so a COMMANDS section listing nothing else is a section about
/// standout — see [`extract`] for when it is dropped.
fn only_the_help_word(subs: &[&Command]) -> bool {
    matches!(subs, [single] if single.get_name() == "help")
}

pub(crate) fn extract_help_data(
    cmd: &Command,
    command_groups: Option<&[CommandGroup]>,
    length: HelpLength,
) -> HelpData {
    extract(cmd, command_groups, length, None)
}

pub(crate) fn extract_help_data_with_topics(
    cmd: &Command,
    registry: &TopicRegistry,
    command_groups: Option<&[CommandGroup]>,
    length: HelpLength,
) -> HelpData {
    extract(cmd, command_groups, length, Some(registry))
}

fn extract(
    cmd: &Command,
    command_groups: Option<&[CommandGroup]>,
    length: HelpLength,
    registry: Option<&TopicRegistry>,
) -> HelpData {
    let name = cmd.get_name().to_string();

    let about = match length {
        HelpLength::Long => cmd.get_long_about().or_else(|| cmd.get_about()),
        HelpLength::Short => cmd.get_about(),
    }
    .map(|s| s.to_string())
    .unwrap_or_default();

    let usage = cmd
        .clone()
        .render_usage()
        .to_string()
        .strip_prefix("Usage: ")
        .unwrap_or(&cmd.clone().render_usage().to_string())
        .to_string();

    let topics = registry
        .map(|registry| registry.list_topics())
        .unwrap_or_default();

    // Collect visible subcommands
    let mut subs: Vec<_> = cmd.get_subcommands().filter(|s| !s.is_hide_set()).collect();
    subs.sort_by_key(|s| s.get_display_order());

    // A flat CLI's COMMANDS section would list standout's `help` word and
    // nothing else — a section whose only entry is the machinery printing it.
    // Registered topics earn it back: there `help <topic>` is a real
    // destination, and the word is how a reader reaches it.
    if only_the_help_word(&subs) && topics.is_empty() {
        subs.clear();
    }

    let subcommands = if let Some(groups) = command_groups {
        extract_grouped_subcommands(&subs, groups)
    } else {
        extract_default_subcommands(&subs)
    };
    let subcommands_width = resolve_name_column(
        &subcommands
            .iter()
            .flat_map(|group| {
                group
                    .items
                    .iter()
                    .filter(|item| !item.separator)
                    .map(|item| item.name.as_str())
            })
            .collect::<Vec<_>>(),
    );

    // Positionals get their own section, ahead of the flags, the way clap
    // orders them: a command's primary argument reads as an afterthought when
    // it is filed under OPTIONS behind every switch.
    let mut args: Vec<_> = cmd.get_arguments().filter(|a| !a.is_hide_set()).collect();
    args.sort_by_key(|a| a.get_display_order());
    let (positionals, flags): (Vec<_>, Vec<_>) =
        args.into_iter().partition(|arg| arg.is_positional());

    let arguments = group_by_heading(
        positionals
            .into_iter()
            .map(|arg| {
                (
                    arg.get_help_heading().map(|s| s.to_string()),
                    option_row(positional_name(arg), None, arg),
                )
            })
            .collect(),
    );
    let arguments_width = resolve_name_column(&section_names(&arguments));

    let options = group_by_heading(
        flags
            .into_iter()
            .map(|arg| {
                (
                    arg.get_help_heading().map(|s| s.to_string()),
                    option_row(flag_name(arg), flag_value_name(arg), arg),
                )
            })
            .collect(),
    );
    let option_names = option_section_names(&options);
    let options_width =
        resolve_name_column(&option_names.iter().map(String::as_str).collect::<Vec<_>>());

    let learn_more: Vec<TopicListItem> = topics
        .iter()
        .map(|topic| TopicListItem {
            name: topic.name.clone(),
            title: topic.title.clone(),
        })
        .collect();
    let learn_more_width = resolve_name_column(
        &learn_more
            .iter()
            .map(|topic| topic.name.as_str())
            .collect::<Vec<_>>(),
    );

    HelpData {
        name,
        about,
        usage,
        subcommands,
        subcommands_width,
        arguments,
        arguments_width,
        options,
        options_width,
        examples: String::new(),
        learn_more,
        learn_more_width,
    }
}

fn subcommand_row(sub: &Command) -> Subcommand {
    Subcommand {
        name: sub.get_name().to_string(),
        about: sub.get_about().map(|s| s.to_string()).unwrap_or_default(),
        separator: false,
    }
}

fn extract_default_subcommands(subs: &[&Command]) -> Vec<Group<Subcommand>> {
    if subs.is_empty() {
        return vec![];
    }

    vec![Group {
        title: Some("Commands".to_string()),
        help: None,
        items: subs.iter().map(|sub| subcommand_row(sub)).collect(),
    }]
}

fn extract_grouped_subcommands(
    subs: &[&Command],
    groups: &[CommandGroup],
) -> Vec<Group<Subcommand>> {
    use std::collections::HashMap;

    let mut sub_map: HashMap<&str, &Command> = subs.iter().map(|s| (s.get_name(), *s)).collect();
    let mut result_groups: Vec<Group<Subcommand>> = Vec::new();

    for group in groups {
        let mut group_cmds = Vec::new();
        for entry in &group.commands {
            match entry {
                None => {
                    group_cmds.push(Subcommand {
                        name: String::new(),
                        about: String::new(),
                        separator: true,
                    });
                }
                Some(cmd_name) => {
                    if let Some(sub) = sub_map.remove(cmd_name.as_str()) {
                        group_cmds.push(subcommand_row(sub));
                    }
                    // Unknown names silently skipped here.
                    // validate_command_groups catches phantom references at test time.
                }
            }
        }
        if !group_cmds.is_empty() {
            result_groups.push(Group {
                title: Some(group.title.clone()),
                help: group.help.clone(),
                items: group_cmds,
            });
        }
    }

    // Ungrouped commands go to auto "Other" group
    if !sub_map.is_empty() {
        // Preserve display_order for remaining commands
        let mut remaining: Vec<_> = sub_map.into_values().collect();
        remaining.sort_by_key(|s| s.get_display_order());
        result_groups.push(Group {
            title: Some("Other".to_string()),
            help: None,
            items: remaining.iter().map(|sub| subcommand_row(sub)).collect(),
        });
    }

    result_groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Arg;

    fn extract_short(cmd: &Command) -> HelpData {
        extract_help_data(cmd, None, HelpLength::Short)
    }

    #[test]
    fn test_extract_basic() {
        let cmd = Command::new("test").about("A test command");
        let data = extract_short(&cmd);
        assert_eq!(data.name, "test");
        assert_eq!(data.about, "A test command");
    }

    #[test]
    fn test_extract_subcommands() {
        let cmd = Command::new("root")
            .subcommand(Command::new("sub1").about("Sub 1"))
            .subcommand(Command::new("sub2").about("Sub 2"));

        let data = extract_short(&cmd);
        assert_eq!(data.subcommands.len(), 1);
        assert_eq!(data.subcommands[0].items.len(), 2);
    }

    /// Issue #297: the column is resolved from the section's longest name, so a
    /// name past the floor widens it for the whole section instead of losing
    /// its separator.
    #[test]
    fn test_long_option_name_widens_column() {
        let cmd = Command::new("root")
            .arg(Arg::new("output").long("output").help("Output format"))
            .arg(
                Arg::new("output_file_path")
                    .long("output-file-path")
                    .help("Write output to file"),
            );

        let data = extract_short(&cmd);
        assert_eq!(
            data.options_width,
            "--output-file-path <output_file_path>".len()
        );
    }

    #[test]
    fn test_short_option_names_keep_floor_width() {
        let cmd = Command::new("root").disable_help_flag(true).arg(
            Arg::new("out")
                .long("out")
                .action(clap::ArgAction::SetTrue)
                .help("Output"),
        );

        let data = extract_short(&cmd);
        assert_eq!(data.options_width, NAME_COLUMN_MIN);
    }

    /// The name column measures terminal columns, not bytes: a CJK name is
    /// twice as wide as its character count and nowhere near its byte length.
    #[test]
    fn test_name_column_measures_display_width_not_bytes() {
        let cmd = Command::new("root").disable_help_flag(true).arg(
            Arg::new("wide")
                .long("日本語オプション")
                .action(clap::ArgAction::SetTrue)
                .help("Wide"),
        );

        let data = extract_short(&cmd);
        // "--" + 8 CJK characters at 2 columns each.
        assert_eq!(data.options_width, 18);
    }

    /// One column per section, shared across groups, so the auto "Other" group
    /// lines up with the named ones.
    #[test]
    fn test_grouped_subcommands_share_one_column() {
        let cmd = Command::new("root")
            .subcommand(Command::new("a-very-long-command-name").about("Long"))
            .subcommand(Command::new("short").about("Short"));

        let groups = vec![CommandGroup {
            title: "Main".into(),
            help: None,
            commands: vec![Some("short".into())],
        }];

        let data = extract_help_data(&cmd, Some(&groups), HelpLength::Short);
        assert_eq!(data.subcommands.len(), 2, "expected a Main and an Other");
        assert_eq!(data.subcommands_width, "a-very-long-command-name".len());
    }

    #[test]
    fn test_empty_subcommands() {
        let cmd = Command::new("root");
        let data = extract_short(&cmd);
        assert!(data.subcommands.is_empty());
    }

    // --- #298: the information clap surfaces ---

    #[test]
    fn test_short_length_uses_about_long_uses_long_about() {
        let cmd = Command::new("root")
            .about("Terse")
            .long_about("The full story");

        assert_eq!(extract_short(&cmd).about, "Terse");
        assert_eq!(
            extract_help_data(&cmd, None, HelpLength::Long).about,
            "The full story"
        );
    }

    #[test]
    fn test_long_length_falls_back_to_about() {
        let cmd = Command::new("root").about("Terse");
        assert_eq!(
            extract_help_data(&cmd, None, HelpLength::Long).about,
            "Terse"
        );
    }

    #[test]
    fn test_option_carries_default_and_possible_values() {
        let cmd = Command::new("root").disable_help_flag(true).arg(
            Arg::new("output")
                .long("output")
                .default_value("auto")
                .value_parser(["auto", "term", "text"])
                .help("Output format"),
        );

        let data = extract_short(&cmd);
        let opt = &data.options[0].items[0];
        assert_eq!(opt.default.as_deref(), Some("auto"));
        assert_eq!(opt.possible_values, vec!["auto", "term", "text"]);
    }

    #[test]
    fn test_hidden_possible_values_left_out() {
        let cmd = Command::new("root").disable_help_flag(true).arg(
            Arg::new("mode").long("mode").value_parser([
                clap::builder::PossibleValue::new("shown"),
                clap::builder::PossibleValue::new("secret").hide(true),
            ]),
        );

        let data = extract_short(&cmd);
        assert_eq!(data.options[0].items[0].possible_values, vec!["shown"]);
    }

    #[test]
    fn test_presence_bool_has_no_value_name_or_possible_values() {
        let cmd = Command::new("root").disable_help_flag(true).arg(
            Arg::new("staged")
                .long("staged")
                .action(clap::ArgAction::SetTrue)
                .value_parser(clap::builder::BoolishValueParser::new()),
        );

        let data = extract_short(&cmd);
        let opt = &data.options[0].items[0];
        assert_eq!(opt.name, "--staged");
        assert_eq!(opt.value_name, None);
        assert!(opt.possible_values.is_empty());
    }

    #[test]
    fn test_value_taking_bool_keeps_value_name_and_possible_values() {
        let cmd = Command::new("root").disable_help_flag(true).arg(
            Arg::new("color")
                .long("color")
                .value_name("BOOL")
                .action(clap::ArgAction::Set)
                .value_parser(clap::builder::BoolishValueParser::new()),
        );

        let data = extract_short(&cmd);
        let opt = &data.options[0].items[0];
        assert_eq!(opt.name, "--color");
        assert_eq!(opt.value_name.as_deref(), Some("<BOOL>"));
        assert_eq!(opt.possible_values, vec!["true", "false"]);
    }

    #[test]
    fn test_value_taking_option_uses_clap_fallback_metavar() {
        let cmd = Command::new("root").disable_help_flag(true).arg(
            Arg::new("threshold")
                .long("threshold")
                .action(clap::ArgAction::Set),
        );

        let data = extract_short(&cmd);
        assert_eq!(
            data.options[0].items[0].value_name.as_deref(),
            Some("<threshold>")
        );
    }

    #[test]
    fn test_positionals_land_in_arguments_not_options() {
        let cmd = Command::new("root")
            .disable_help_flag(true)
            .arg(Arg::new("range").help("Git range to diff"))
            .arg(Arg::new("staged").long("staged").help("Use the index"));

        let data = extract_short(&cmd);
        assert_eq!(data.arguments[0].items[0].name, "range");
        assert_eq!(data.options[0].items[0].name, "--staged");
        assert!(data
            .options
            .iter()
            .all(|group| group.items.iter().all(|item| item.name != "range")));
    }

    #[test]
    fn test_positional_uses_declared_value_name() {
        let cmd = Command::new("root")
            .disable_help_flag(true)
            .arg(Arg::new("range").value_name("RANGE").help("A range"));

        let data = extract_short(&cmd);
        assert_eq!(data.arguments[0].items[0].name, "RANGE");
    }

    /// The two sections size independently — a long flag must not push the
    /// ARGUMENTS column out with it.
    #[test]
    fn test_arguments_and_options_columns_are_independent() {
        let cmd = Command::new("root")
            .disable_help_flag(true)
            .arg(Arg::new("range").help("A range"))
            .arg(
                Arg::new("output_file_path")
                    .long("output-file-path")
                    .help("Write output to file"),
            );

        let data = extract_short(&cmd);
        assert_eq!(data.arguments_width, NAME_COLUMN_MIN);
        assert_eq!(
            data.options_width,
            "--output-file-path <output_file_path>".len()
        );
    }

    // --- #299: the flat-CLI `help` word ---

    #[test]
    fn test_help_only_commands_section_is_dropped() {
        let cmd = Command::new("root")
            .disable_help_subcommand(true)
            .subcommand(Command::new("help").about("Print this message"));

        let data = extract_short(&cmd);
        assert!(
            data.subcommands.is_empty(),
            "a COMMANDS section listing only standout's own word is noise"
        );
    }

    #[test]
    fn test_help_only_commands_section_kept_when_topics_exist() {
        use crate::topics::{Topic, TopicType};

        let cmd = Command::new("root")
            .disable_help_subcommand(true)
            .subcommand(Command::new("help").about("Print this message"));
        let mut registry = TopicRegistry::new();
        registry.add_topic(Topic::new("Storage", "content", TopicType::Text, None));

        let data = extract_help_data_with_topics(&cmd, &registry, None, HelpLength::Short);
        assert_eq!(
            data.subcommands[0].items[0].name, "help",
            "`help <topic>` is a real destination, so the word stays listed"
        );
    }

    #[test]
    fn test_help_alongside_real_commands_is_kept() {
        let cmd = Command::new("root")
            .disable_help_subcommand(true)
            .subcommand(Command::new("help").about("Print this message"))
            .subcommand(Command::new("build").about("Build it"));

        let data = extract_short(&cmd);
        assert_eq!(data.subcommands[0].items.len(), 2);
    }

    #[test]
    fn test_topics_populate_learn_more() {
        use crate::topics::{Topic, TopicType};

        let cmd = Command::new("root");
        let mut registry = TopicRegistry::new();
        registry.add_topic(Topic::new(
            "A Very Long Topic Name Here",
            "content",
            TopicType::Text,
            None,
        ));
        registry.add_topic(Topic::new("Short", "content", TopicType::Text, None));

        let data = extract_help_data_with_topics(&cmd, &registry, None, HelpLength::Short);
        assert_eq!(data.learn_more.len(), 2);
        assert_eq!(
            data.learn_more_width,
            data.learn_more
                .iter()
                .map(|topic| topic.name.len())
                .max()
                .unwrap()
        );
    }
}
