//! Help data extraction from clap commands.

use crate::topics::TopicRegistry;
use clap::Command;
use serde::Serialize;
use std::collections::BTreeMap;

use super::config::CommandGroup;

/// Minimum width for the name column in help output (commands, options, topics).
///
/// This is a floor, not a fixed width: whenever a list's longest name plus
/// [`NAME_COLUMN_GAP`] exceeds it, the whole column widens via
/// [`name_column_width`].
pub(crate) const NAME_COLUMN_WIDTH: usize = 14;

/// Minimum gap between a name and its description when the column widens.
pub(crate) const NAME_COLUMN_GAP: usize = 2;

/// Width of the name column for a list whose longest rendered name (including
/// any template-added suffix, e.g. the colon after subcommand and topic names)
/// is `max_name_len`.
///
/// The larger of [`NAME_COLUMN_WIDTH`] and `max_name_len + NAME_COLUMN_GAP`:
/// the column widens whenever the longest name plus the minimum gap exceeds
/// the floor — including a name that merely fills the floor — so every name
/// keeps at least [`NAME_COLUMN_GAP`] spaces before its description (issue
/// #297).
pub(crate) fn name_column_width(max_name_len: usize) -> usize {
    NAME_COLUMN_WIDTH.max(max_name_len + NAME_COLUMN_GAP)
}

/// Column width shared by a set of subcommands.
///
/// The `+ 1` accounts for the colon the template appends to each name; all
/// groups of one help page pass the full subcommand list so they align.
fn subcommand_column_width(subs: &[&Command]) -> usize {
    name_column_width(
        subs.iter()
            .map(|s| s.get_name().len() + 1)
            .max()
            .unwrap_or(0),
    )
}

#[derive(Serialize)]
pub(crate) struct HelpData {
    pub name: String,
    pub about: String,
    pub usage: String,
    pub subcommands: Vec<Group<Subcommand>>,
    pub options: Vec<Group<OptionData>>,
    pub examples: String,
    pub learn_more: Vec<TopicListItem>,
}

#[derive(Serialize)]
pub(crate) struct Group<T> {
    pub title: Option<String>,
    pub help: Option<String>,
    pub commands: Vec<T>,
    pub options: Vec<T>,
}

#[derive(Serialize)]
pub(crate) struct Subcommand {
    pub name: String,
    pub about: String,
    pub padding: String,
    pub separator: bool,
}

#[derive(Serialize)]
pub(crate) struct OptionData {
    pub name: String,
    pub help: String,
    pub padding: String,
    pub short: Option<char>,
    pub long: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct TopicListItem {
    pub name: String,
    pub title: String,
    pub padding: String,
}

pub(crate) fn extract_help_data(
    cmd: &Command,
    command_groups: Option<&[CommandGroup]>,
) -> HelpData {
    let name = cmd.get_name().to_string();
    let about = cmd.get_about().map(|s| s.to_string()).unwrap_or_default();
    let usage = cmd
        .clone()
        .render_usage()
        .to_string()
        .strip_prefix("Usage: ")
        .unwrap_or(&cmd.clone().render_usage().to_string())
        .to_string();

    // Collect visible subcommands
    let mut subs: Vec<_> = cmd.get_subcommands().filter(|s| !s.is_hide_set()).collect();
    subs.sort_by_key(|s| s.get_display_order());

    let subcommands = if let Some(groups) = command_groups {
        extract_grouped_subcommands(&subs, groups)
    } else {
        extract_default_subcommands(&subs)
    };

    // Group Options
    let mut opt_groups: BTreeMap<Option<String>, Vec<OptionData>> = BTreeMap::new();
    let mut args: Vec<_> = cmd.get_arguments().filter(|a| !a.is_hide_set()).collect();
    args.sort_by_key(|a| a.get_display_order());

    let named: Vec<(String, &clap::Arg)> = args
        .into_iter()
        .map(|arg| {
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
            (name, arg)
        })
        .collect();

    let width = name_column_width(named.iter().map(|(n, _)| n.len()).max().unwrap_or(0));

    for (name, arg) in named {
        let pad = width - name.len();
        let heading = arg.get_help_heading().map(|s| s.to_string());
        let opt_data = OptionData {
            name,
            help: arg.get_help().map(|s| s.to_string()).unwrap_or_default(),
            padding: " ".repeat(pad),
            short: arg.get_short(),
            long: arg.get_long().map(|s| s.to_string()),
        };

        opt_groups.entry(heading).or_default().push(opt_data);
    }

    let options = opt_groups
        .into_iter()
        .map(|(title, opts)| Group {
            title,
            help: None,
            commands: vec![],
            options: opts,
        })
        .collect();

    HelpData {
        name,
        about,
        usage,
        subcommands,
        options,
        examples: String::new(),
        learn_more: vec![],
    }
}

fn extract_default_subcommands(subs: &[&Command]) -> Vec<Group<Subcommand>> {
    let width = subcommand_column_width(subs);
    let sub_cmds: Vec<Subcommand> = subs
        .iter()
        .map(|sub| {
            let name = sub.get_name().to_string();
            let pad = width - (name.len() + 1);
            Subcommand {
                name,
                about: sub.get_about().map(|s| s.to_string()).unwrap_or_default(),
                padding: " ".repeat(pad),
                separator: false,
            }
        })
        .collect();

    if sub_cmds.is_empty() {
        vec![]
    } else {
        vec![Group {
            title: Some("Commands".to_string()),
            help: None,
            commands: sub_cmds,
            options: vec![],
        }]
    }
}

fn extract_grouped_subcommands(
    subs: &[&Command],
    groups: &[CommandGroup],
) -> Vec<Group<Subcommand>> {
    use std::collections::HashMap;

    let width = subcommand_column_width(subs);
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
                        padding: String::new(),
                        separator: true,
                    });
                }
                Some(cmd_name) => {
                    if let Some(sub) = sub_map.remove(cmd_name.as_str()) {
                        let name = sub.get_name().to_string();
                        let pad = width - (name.len() + 1);
                        group_cmds.push(Subcommand {
                            name,
                            about: sub.get_about().map(|s| s.to_string()).unwrap_or_default(),
                            padding: " ".repeat(pad),
                            separator: false,
                        });
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
                commands: group_cmds,
                options: vec![],
            });
        }
    }

    // Ungrouped commands go to auto "Other" group
    if !sub_map.is_empty() {
        // Preserve display_order for remaining commands
        let mut remaining: Vec<_> = sub_map.into_values().collect();
        remaining.sort_by_key(|s| s.get_display_order());
        let other_cmds: Vec<Subcommand> = remaining
            .iter()
            .map(|sub| {
                let name = sub.get_name().to_string();
                let pad = width - (name.len() + 1);
                Subcommand {
                    name,
                    about: sub.get_about().map(|s| s.to_string()).unwrap_or_default(),
                    padding: " ".repeat(pad),
                    separator: false,
                }
            })
            .collect();
        result_groups.push(Group {
            title: Some("Other".to_string()),
            help: None,
            commands: other_cmds,
            options: vec![],
        });
    }

    result_groups
}

pub(crate) fn extract_help_data_with_topics(
    cmd: &Command,
    registry: &TopicRegistry,
    command_groups: Option<&[CommandGroup]>,
) -> HelpData {
    let mut data = extract_help_data(cmd, command_groups);

    let topics = registry.list_topics();
    if !topics.is_empty() {
        // +1 accounts for the colon the template appends to each name.
        let width = name_column_width(topics.iter().map(|t| t.name.len() + 1).max().unwrap_or(0));
        data.learn_more = topics
            .iter()
            .map(|t| {
                let pad = width - (t.name.len() + 1);
                TopicListItem {
                    name: t.name.clone(),
                    title: t.title.clone(),
                    padding: " ".repeat(pad),
                }
            })
            .collect();
    }

    data
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Arg;

    #[test]
    fn test_extract_basic() {
        let cmd = Command::new("test").about("A test command");
        let data = extract_help_data(&cmd, None);
        assert_eq!(data.name, "test");
        assert_eq!(data.about, "A test command");
    }

    #[test]
    fn test_extract_subcommands() {
        let cmd = Command::new("root")
            .subcommand(Command::new("sub1").about("Sub 1"))
            .subcommand(Command::new("sub2").about("Sub 2"));

        let data = extract_help_data(&cmd, None);
        assert_eq!(data.subcommands.len(), 1);
        assert_eq!(data.subcommands[0].commands.len(), 2);
    }

    #[test]
    fn test_ordering_declaration() {
        let cmd = Command::new("root")
            .subcommand(Command::new("Zoo"))
            .subcommand(Command::new("Air"));

        let data = extract_help_data(&cmd, None);
        assert_eq!(data.subcommands[0].commands[0].name, "Zoo");
        assert_eq!(data.subcommands[0].commands[1].name, "Air");
    }

    #[test]
    fn test_mixed_headings() {
        let cmd = Command::new("root")
            .arg(Arg::new("opt1").long("opt1"))
            .arg(Arg::new("custom").long("custom").help_heading("Custom"));

        let data = extract_help_data(&cmd, None);
        assert_eq!(data.options.len(), 2);

        let group1 = &data.options[0];
        let group2 = &data.options[1];

        assert!(group1.title.is_none());
        assert_eq!(group1.options[0].name, "--opt1");

        assert_eq!(group2.title.as_deref(), Some("Custom"));
        assert_eq!(group2.options[0].name, "--custom");
    }

    #[test]
    fn test_ordering_derive() {
        use clap::{CommandFactory, Parser};

        #[derive(Parser, Debug)]
        struct Cli {
            #[command(subcommand)]
            command: Commands,
        }

        #[derive(clap::Subcommand, Debug)]
        enum Commands {
            #[command(display_order = 2)]
            First,
            #[command(display_order = 1)]
            Second,
        }

        let cmd = Cli::command();
        let data = extract_help_data(&cmd, None);

        assert_eq!(data.subcommands[0].commands[0].name, "second");
        assert_eq!(data.subcommands[0].commands[1].name, "first");
    }

    #[test]
    fn test_extract_with_command_groups() {
        let cmd = Command::new("root")
            .disable_help_subcommand(true)
            .subcommand(Command::new("init").about("Initialize"))
            .subcommand(Command::new("list").about("List items"))
            .subcommand(Command::new("delete").about("Delete items"))
            .subcommand(Command::new("config").about("Configuration"));

        let groups = vec![
            CommandGroup {
                title: "Main".into(),
                help: None,
                commands: vec![Some("init".into()), Some("list".into())],
            },
            CommandGroup {
                title: "Danger".into(),
                help: Some("Be careful with these.".into()),
                commands: vec![Some("delete".into())],
            },
        ];

        let data = extract_help_data(&cmd, Some(&groups));
        assert_eq!(data.subcommands.len(), 3); // Main, Danger, Other (config is ungrouped)
        assert_eq!(data.subcommands[0].title.as_deref(), Some("Main"));
        assert_eq!(data.subcommands[0].commands.len(), 2);
        assert_eq!(data.subcommands[0].commands[0].name, "init");
        assert_eq!(data.subcommands[0].commands[1].name, "list");
        assert_eq!(data.subcommands[1].title.as_deref(), Some("Danger"));
        assert_eq!(
            data.subcommands[1].help.as_deref(),
            Some("Be careful with these.")
        );
        assert_eq!(data.subcommands[1].commands[0].name, "delete");
        assert_eq!(data.subcommands[2].title.as_deref(), Some("Other"));
        assert_eq!(data.subcommands[2].commands[0].name, "config");
    }

    #[test]
    fn test_extract_with_separators() {
        let cmd = Command::new("root")
            .subcommand(Command::new("a").about("A"))
            .subcommand(Command::new("b").about("B"))
            .subcommand(Command::new("c").about("C"));

        let groups = vec![CommandGroup {
            title: "All".into(),
            help: None,
            commands: vec![
                Some("a".into()),
                None, // separator
                Some("b".into()),
                Some("c".into()),
            ],
        }];

        let data = extract_help_data(&cmd, Some(&groups));
        assert_eq!(data.subcommands.len(), 1);
        let cmds = &data.subcommands[0].commands;
        assert_eq!(cmds.len(), 4); // a, separator, b, c
        assert!(!cmds[0].separator);
        assert_eq!(cmds[0].name, "a");
        assert!(cmds[1].separator);
        assert!(!cmds[2].separator);
        assert_eq!(cmds[2].name, "b");
    }

    #[test]
    fn test_extract_all_grouped_no_other() {
        let cmd = Command::new("root")
            .subcommand(Command::new("a").about("A"))
            .subcommand(Command::new("b").about("B"));

        let groups = vec![CommandGroup {
            title: "All".into(),
            help: None,
            commands: vec![Some("a".into()), Some("b".into())],
        }];

        let data = extract_help_data(&cmd, Some(&groups));
        assert_eq!(data.subcommands.len(), 1); // No "Other" group
        assert_eq!(data.subcommands[0].title.as_deref(), Some("All"));
    }

    #[test]
    fn test_default_group_title_is_commands() {
        let cmd = Command::new("root").subcommand(Command::new("foo").about("Foo"));

        let data = extract_help_data(&cmd, None);
        assert_eq!(data.subcommands[0].title.as_deref(), Some("Commands"));
    }

    #[test]
    fn test_no_subcommands_empty_vec() {
        let cmd = Command::new("root");
        let data = extract_help_data(&cmd, None);
        assert!(data.subcommands.is_empty());
    }

    /// Issue #297: an option name longer than the column floor must widen the
    /// column (for every option) instead of collapsing its padding to zero.
    #[test]
    fn test_long_option_name_widens_column() {
        let cmd = Command::new("root")
            .arg(Arg::new("output").long("output").help("Output format"))
            .arg(
                Arg::new("output_file_path")
                    .long("output-file-path")
                    .help("Write output to file"),
            );

        let data = extract_help_data(&cmd, None);
        let opts = &data.options[0].options;

        let long = opts
            .iter()
            .find(|o| o.name == "--output-file-path")
            .unwrap();
        assert!(
            !long.padding.is_empty(),
            "long option name must keep a separator before its description"
        );

        // Every option's description starts at the same column.
        let columns: Vec<usize> = opts
            .iter()
            .map(|o| o.name.len() + o.padding.len())
            .collect();
        assert!(
            columns.iter().all(|c| *c == columns[0]),
            "options must align to one column, got {:?}",
            columns
        );
        // The column widened to the longest name plus the gap.
        assert_eq!(columns[0], long.name.len() + NAME_COLUMN_GAP);
    }

    #[test]
    fn test_short_option_names_keep_floor_width() {
        let cmd = Command::new("root")
            .disable_help_flag(true)
            .arg(Arg::new("out").long("out").help("Output"));

        let data = extract_help_data(&cmd, None);
        let opt = &data.options[0].options[0];
        assert_eq!(opt.name.len() + opt.padding.len(), NAME_COLUMN_WIDTH);
    }

    #[test]
    fn test_long_subcommand_name_widens_column() {
        let cmd = Command::new("root")
            .subcommand(Command::new("a-very-long-command-name").about("Long"))
            .subcommand(Command::new("short").about("Short"));

        let data = extract_help_data(&cmd, None);
        let cmds = &data.subcommands[0].commands;
        // +1 accounts for the colon the template appends to the name.
        let columns: Vec<usize> = cmds
            .iter()
            .map(|c| c.name.len() + 1 + c.padding.len())
            .collect();
        assert!(
            columns.iter().all(|c| *c == columns[0]),
            "subcommands must align to one column, got {:?}",
            columns
        );
        let long = cmds
            .iter()
            .find(|c| c.name == "a-very-long-command-name")
            .unwrap();
        assert!(!long.padding.is_empty());
    }

    #[test]
    fn test_long_grouped_subcommand_widens_all_groups() {
        let cmd = Command::new("root")
            .subcommand(Command::new("a-very-long-command-name").about("Long"))
            .subcommand(Command::new("short").about("Short"));

        let groups = vec![CommandGroup {
            title: "Main".into(),
            help: None,
            commands: vec![Some("a-very-long-command-name".into())],
        }];

        let data = extract_help_data(&cmd, Some(&groups));
        // "Main" group + auto "Other" group share one column.
        let mut columns = Vec::new();
        for group in &data.subcommands {
            for c in group.commands.iter().filter(|c| !c.separator) {
                columns.push(c.name.len() + 1 + c.padding.len());
            }
        }
        assert!(
            columns.iter().all(|c| *c == columns[0]),
            "grouped subcommands must share one column, got {:?}",
            columns
        );
        assert!(columns[0] > NAME_COLUMN_WIDTH);
    }

    #[test]
    fn test_long_topic_name_widens_column() {
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

        let data = extract_help_data_with_topics(&cmd, &registry, None);
        let columns: Vec<usize> = data
            .learn_more
            .iter()
            .map(|t| t.name.len() + 1 + t.padding.len())
            .collect();
        assert!(
            columns.iter().all(|c| *c == columns[0]),
            "topics must align to one column, got {:?}",
            columns
        );
        assert!(data.learn_more.iter().all(|t| !t.padding.is_empty()));
    }
}
