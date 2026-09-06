use crate::tabular::{Column, FlatDataSpec, Width};
use crate::topics::TopicRegistry;
use crate::{AmbiguousWidth, TargetProperties};
use clap::Command;
use serde::Serialize;
use std::collections::BTreeMap;

use super::config::{CommandGroup, HelpLength};

const NAME_COLUMN_MIN: usize = 12;

const COLUMN_SEPARATOR: &str = "  ";

pub(crate) const ASSUMED_TERMINAL_WIDTH: usize = 80;

pub(crate) fn resolve_name_column(
    names: &[&str],
    terminal_width: usize,
    ambiguous_width: AmbiguousWidth,
) -> usize {
    let spec = FlatDataSpec::builder()
        .column(Column::new(Width::Bounded {
            min: Some(NAME_COLUMN_MIN),
            max: None,
        }))
        .column(Column::new(Width::Fill))
        .separator(COLUMN_SEPARATOR)
        .build();

    let rows: Vec<Vec<&str>> = names.iter().map(|name| vec![*name]).collect();
    spec.resolve_widths_from_data_with_policy(terminal_width, &rows, ambiguous_width)
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

fn positional_name(arg: &clap::Arg) -> String {
    arg.get_value_names()
        .and_then(|names| names.first())
        .map(|name| name.to_string())
        .unwrap_or_else(|| arg.get_id().to_string())
}

pub(super) fn takes_values(arg: &clap::Arg) -> bool {
    arg.get_num_args()
        .map(|range| range.takes_values())
        .unwrap_or_else(|| arg.get_action().takes_values())
}

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

pub(super) fn default_value(arg: &clap::Arg) -> Option<String> {
    if !takes_values(arg) {
        return None;
    }

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

pub(super) fn possible_values(arg: &clap::Arg) -> Vec<String> {
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

fn section_names(groups: &[Group<OptionData>]) -> Vec<&str> {
    groups
        .iter()
        .flat_map(|group| group.items.iter().map(|item| item.name.as_str()))
        .collect()
}

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

fn only_the_help_word(subs: &[&Command]) -> bool {
    matches!(subs, [single] if single.get_name() == "help")
}

/// The parent's `disable_help_subcommand` setting decides, whatever build state it is in.
fn is_clap_generated_help_subcommand(parent: &Command, sub: &Command) -> bool {
    sub.get_name() == "help" && !parent.is_disable_help_subcommand_set()
}

pub(crate) fn extract_help_data(
    root: &Command,
    path: &[&str],
    command_groups: Option<&[CommandGroup]>,
    length: HelpLength,
    target: &TargetProperties,
) -> Option<HelpData> {
    extract(root, path, command_groups, length, None, target)
}

pub(crate) fn extract_help_data_with_topics(
    root: &Command,
    path: &[&str],
    registry: &TopicRegistry,
    command_groups: Option<&[CommandGroup]>,
    length: HelpLength,
    target: &TargetProperties,
) -> Option<HelpData> {
    extract(root, path, command_groups, length, Some(registry), target)
}

/// Only a `build()` from the root gives a subcommand its `-h`/`-V` and its parents' usage names.
pub(super) fn build_root(root: &Command) -> Command {
    let mut built = root.clone();
    built.build();
    built
}

pub(super) fn usage_line(cmd: &Command) -> String {
    let usage = cmd.clone().render_usage().to_string();
    usage.strip_prefix("Usage: ").unwrap_or(&usage).to_string()
}

pub(super) fn about_line(cmd: &Command, length: HelpLength) -> String {
    match length {
        HelpLength::Long => cmd.get_long_about().or_else(|| cmd.get_about()),
        HelpLength::Short => cmd.get_about(),
    }
    .map(|s| s.to_string())
    .unwrap_or_default()
}

fn extract(
    root: &Command,
    path: &[&str],
    command_groups: Option<&[CommandGroup]>,
    length: HelpLength,
    registry: Option<&TopicRegistry>,
    target: &TargetProperties,
) -> Option<HelpData> {
    let built = build_root(root);
    let cmd = crate::cli::app::find_subcommand_recursive(&built, path)?;

    let name = cmd.get_name().to_string();
    let about = about_line(cmd, length);
    let usage = usage_line(cmd);

    let topics = registry
        .map(|registry| registry.list_topics())
        .unwrap_or_default();

    let mut subs: Vec<_> = cmd
        .get_subcommands()
        .filter(|s| !s.is_hide_set())
        .filter(|s| !is_clap_generated_help_subcommand(cmd, s))
        .collect();
    subs.sort_by_key(|s| s.get_display_order());

    if only_the_help_word(&subs) && topics.is_empty() {
        subs.clear();
    }

    let subcommands = if let Some(groups) = command_groups {
        extract_grouped_subcommands(&subs, groups)
    } else {
        extract_default_subcommands(&subs)
    };
    let terminal_width = target.width.unwrap_or(ASSUMED_TERMINAL_WIDTH);
    let policy = target.ambiguous_width;
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
        terminal_width,
        policy,
    );

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
    let arguments_width = resolve_name_column(&section_names(&arguments), terminal_width, policy);

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
    let options_width = resolve_name_column(
        &option_names.iter().map(String::as_str).collect::<Vec<_>>(),
        terminal_width,
        policy,
    );

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
        terminal_width,
        policy,
    );

    Some(HelpData {
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
    })
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

    if !sub_map.is_empty() {
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

    fn layout_target() -> TargetProperties {
        TargetProperties {
            width: Some(ASSUMED_TERMINAL_WIDTH),
            stdout_is_terminal: false,
            stderr_is_terminal: false,
            stdout_color_capability: false,
            stderr_color_capability: false,
            color_scheme: crate::ColorMode::Dark,
            icon_mode: crate::IconMode::Classic,
            ambiguous_width: crate::AmbiguousWidth::Narrow,
        }
    }

    fn extract_short(cmd: &Command) -> HelpData {
        extract_help_data(cmd, &[], None, HelpLength::Short, &layout_target()).unwrap()
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
            .disable_help_subcommand(true)
            .subcommand(Command::new("sub1").about("Sub 1"))
            .subcommand(Command::new("sub2").about("Sub 2"));

        let data = extract_short(&cmd);
        assert_eq!(data.subcommands.len(), 1);
        assert_eq!(data.subcommands[0].items.len(), 2);
    }

    #[test]
    fn test_clap_generated_help_flag_is_listed() {
        let cmd = Command::new("root").disable_help_subcommand(true);

        let data = extract_short(&cmd);
        let names: Vec<&str> = data.options[0]
            .items
            .iter()
            .map(|item| item.name.as_str())
            .collect();
        assert_eq!(names, vec!["-h, --help"]);
    }

    #[test]
    fn test_clap_generated_version_flag_is_listed() {
        let cmd = Command::new("root")
            .version("1.0")
            .disable_help_subcommand(true);

        let data = extract_short(&cmd);
        let names: Vec<&str> = data.options[0]
            .items
            .iter()
            .map(|item| item.name.as_str())
            .collect();
        assert_eq!(names, vec!["-h, --help", "-V, --version"]);
    }

    #[test]
    fn test_clap_generated_help_subcommand_is_not_listed() {
        let cmd = Command::new("root")
            .subcommand(Command::new("build").about("Build it"))
            .subcommand(Command::new("test").about("Test it"));

        let names: Vec<String> = extract_short(&cmd).subcommands[0]
            .items
            .iter()
            .map(|item| item.name.clone())
            .collect();
        assert_eq!(names, vec!["build", "test"]);
    }

    #[test]
    fn test_an_already_built_command_still_drops_the_generated_help_word() {
        let mut cmd = Command::new("root")
            .subcommand(Command::new("build").about("Build it"))
            .subcommand(Command::new("test").about("Test it"));
        cmd.build();

        let names: Vec<String> = extract_short(&cmd).subcommands[0]
            .items
            .iter()
            .map(|item| item.name.clone())
            .collect();
        assert_eq!(names, vec!["build", "test"]);
    }

    #[test]
    fn test_an_already_built_commands_own_help_subcommand_is_listed() {
        let mut cmd = Command::new("root")
            .disable_help_subcommand(true)
            .subcommand(Command::new("help").about("Browse the manual"))
            .subcommand(Command::new("build").about("Build it"));
        cmd.build();

        let names: Vec<String> = extract_short(&cmd).subcommands[0]
            .items
            .iter()
            .map(|item| item.name.clone())
            .collect();
        assert_eq!(names, vec!["help", "build"]);
    }

    #[test]
    fn test_an_applications_own_help_subcommand_is_listed() {
        let cmd = Command::new("root")
            .disable_help_subcommand(true)
            .subcommand(Command::new("help").about("Browse the manual"))
            .subcommand(Command::new("build").about("Build it"));

        let names: Vec<String> = extract_short(&cmd).subcommands[0]
            .items
            .iter()
            .map(|item| item.name.clone())
            .collect();
        assert_eq!(names, vec!["help", "build"]);
    }

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

    #[test]
    fn test_name_column_measures_display_width_not_bytes() {
        let cmd = Command::new("root").disable_help_flag(true).arg(
            Arg::new("wide")
                .long("日本語オプション")
                .action(clap::ArgAction::SetTrue)
                .help("Wide"),
        );

        let data = extract_short(&cmd);
        assert_eq!(data.options_width, 18);
    }

    #[test]
    fn test_name_column_uses_target_ambiguous_width_policy() {
        let cmd = Command::new("root").disable_help_flag(true).arg(
            Arg::new("wide")
                .long("↦≈Δ↦≈Δ↦≈Δ↦≈Δ")
                .action(clap::ArgAction::SetTrue)
                .help("Ambiguous"),
        );

        let mut narrow = layout_target();
        narrow.ambiguous_width = crate::AmbiguousWidth::Narrow;
        let mut wide = layout_target();
        wide.ambiguous_width = crate::AmbiguousWidth::Wide;

        let narrow_width = extract_help_data(&cmd, &[], None, HelpLength::Short, &narrow)
            .unwrap()
            .options_width;
        let wide_width = extract_help_data(&cmd, &[], None, HelpLength::Short, &wide)
            .unwrap()
            .options_width;
        assert!(
            wide_width > narrow_width,
            "wide policy must measure ambiguous names wider: {wide_width} vs {narrow_width}"
        );
    }

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

        let data = extract_help_data(
            &cmd,
            &[],
            Some(&groups),
            HelpLength::Short,
            &layout_target(),
        )
        .unwrap();
        assert_eq!(data.subcommands.len(), 2, "expected a Main and an Other");
        assert_eq!(data.subcommands_width, "a-very-long-command-name".len());
    }

    #[test]
    fn test_a_nested_leaf_usage_names_the_path_to_it() {
        let cmd = Command::new("app").subcommand(
            Command::new("nest").subcommand(
                Command::new("inner").subcommand(
                    Command::new("leaf")
                        .arg(Arg::new("all").long("all").action(clap::ArgAction::SetTrue)),
                ),
            ),
        );

        let data = extract_help_data(
            &cmd,
            &["nest", "inner", "leaf"],
            None,
            HelpLength::Short,
            &layout_target(),
        )
        .unwrap();
        assert_eq!(data.name, "leaf");
        assert_eq!(data.usage, "app nest inner leaf [OPTIONS]");
        assert!(extract_help_data(
            &cmd,
            &["nest", "twig"],
            None,
            HelpLength::Short,
            &layout_target()
        )
        .is_none());
    }

    #[test]
    fn test_empty_subcommands() {
        let cmd = Command::new("root");
        let data = extract_short(&cmd);
        assert!(data.subcommands.is_empty());
    }

    #[test]
    fn test_short_length_uses_about_long_uses_long_about() {
        let cmd = Command::new("root")
            .about("Terse")
            .long_about("The full story");

        assert_eq!(extract_short(&cmd).about, "Terse");
        assert_eq!(
            extract_help_data(&cmd, &[], None, HelpLength::Long, &layout_target())
                .unwrap()
                .about,
            "The full story"
        );
    }

    #[test]
    fn test_long_length_falls_back_to_about() {
        let cmd = Command::new("root").about("Terse");
        assert_eq!(
            extract_help_data(&cmd, &[], None, HelpLength::Long, &layout_target())
                .unwrap()
                .about,
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

        let data = extract_help_data_with_topics(
            &cmd,
            &[],
            &registry,
            None,
            HelpLength::Short,
            &layout_target(),
        )
        .unwrap();
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

        let data = extract_help_data_with_topics(
            &cmd,
            &[],
            &registry,
            None,
            HelpLength::Short,
            &layout_target(),
        )
        .unwrap();
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
