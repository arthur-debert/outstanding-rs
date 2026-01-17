//! Help data extraction from clap commands.

use crate::topics::TopicRegistry;
use clap::Command;
use serde::Serialize;
use std::collections::BTreeMap;

/// Fixed width for the name column in help output (commands, options, topics).
pub(crate) const NAME_COLUMN_WIDTH: usize = 14;

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
    pub commands: Vec<T>,
    pub options: Vec<T>,
}

#[derive(Serialize)]
pub(crate) struct Subcommand {
    pub name: String,
    pub about: String,
    pub padding: String,
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

pub(crate) fn extract_help_data(cmd: &Command) -> HelpData {
    let name = cmd.get_name().to_string();
    let about = cmd.get_about().map(|s| s.to_string()).unwrap_or_default();
    let usage = cmd
        .clone()
        .render_usage()
        .to_string()
        .strip_prefix("Usage: ")
        .unwrap_or(&cmd.clone().render_usage().to_string())
        .to_string();

    // Group Subcommands
    let mut sub_cmds = Vec::new();
    let mut subs: Vec<_> = cmd.get_subcommands().filter(|s| !s.is_hide_set()).collect();
    subs.sort_by_key(|s| s.get_display_order());

    for sub in subs {
        let name = sub.get_name().to_string();
        let pad = NAME_COLUMN_WIDTH.saturating_sub(name.len() + 1);

        let sub_data = Subcommand {
            name,
            about: sub.get_about().map(|s| s.to_string()).unwrap_or_default(),
            padding: " ".repeat(pad),
        };
        sub_cmds.push(sub_data);
    }

    let subcommands = if sub_cmds.is_empty() {
        vec![]
    } else {
        vec![Group {
            title: Some("Commands".to_string()),
            commands: sub_cmds,
            options: vec![],
        }]
    };

    // Group Options
    let mut opt_groups: BTreeMap<Option<String>, Vec<OptionData>> = BTreeMap::new();
    let mut args: Vec<_> = cmd.get_arguments().filter(|a| !a.is_hide_set()).collect();
    args.sort_by_key(|a| a.get_display_order());

    for arg in args {
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

        let pad = NAME_COLUMN_WIDTH.saturating_sub(name.len());
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

pub(crate) fn extract_help_data_with_topics(cmd: &Command, registry: &TopicRegistry) -> HelpData {
    let mut data = extract_help_data(cmd);

    let topics = registry.list_topics();
    if !topics.is_empty() {
        data.learn_more = topics
            .iter()
            .map(|t| {
                let pad = NAME_COLUMN_WIDTH.saturating_sub(t.name.len() + 1);
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
        let data = extract_help_data(&cmd);
        assert_eq!(data.name, "test");
        assert_eq!(data.about, "A test command");
    }

    #[test]
    fn test_extract_subcommands() {
        let cmd = Command::new("root")
            .subcommand(Command::new("sub1").about("Sub 1"))
            .subcommand(Command::new("sub2").about("Sub 2"));

        let data = extract_help_data(&cmd);
        assert_eq!(data.subcommands.len(), 1);
        assert_eq!(data.subcommands[0].commands.len(), 2);
    }

    #[test]
    fn test_ordering_declaration() {
        let cmd = Command::new("root")
            .subcommand(Command::new("Zoo"))
            .subcommand(Command::new("Air"));

        let data = extract_help_data(&cmd);
        assert_eq!(data.subcommands[0].commands[0].name, "Zoo");
        assert_eq!(data.subcommands[0].commands[1].name, "Air");
    }

    #[test]
    fn test_mixed_headings() {
        let cmd = Command::new("root")
            .arg(Arg::new("opt1").long("opt1"))
            .arg(Arg::new("custom").long("custom").help_heading("Custom"));

        let data = extract_help_data(&cmd);
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
        let data = extract_help_data(&cmd);

        assert_eq!(data.subcommands[0].commands[0].name, "second");
        assert_eq!(data.subcommands[0].commands[1].name, "first");
    }
}
