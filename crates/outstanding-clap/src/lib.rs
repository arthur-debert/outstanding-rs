//! # Outstanding Clap Integration
//!
//! This module provides a drop-in replacement for `clap`'s default help generation, leveraging
//! `outstanding`'s powerful templating and styling capabilities.
//!
//! Instead of relying on `clap`'s internal hardcoded help generation, this module extracts the
//! structure of your CLI (commands, arguments, groups) and renders it using a customizable
//! template engine. This allows for:
//!
//! - **Complete Visual Control**: Use `minijinja` templates to define exactly how your help looks.
//! - **Separation of Style and Content**: Define styles (colors, bold, etc.) in a theme, separate from the layout.
//! - **Future-Proofing**: Positioned to leverage future `outstanding` features like adaptive layouts.
//!
//! It is designed to be a "drop-in" helper: you continue defining your `clap::Command` as usual,
//! and simply call `render_help` when you want to display the help message.
//!
//! # Example
//!
//! ```rust
//! # use clap::Command;
//! # use outstanding_clap::{render_help, Config};
//! let cmd = Command::new("my-app").about("My Application");
//! let help = render_help(&cmd, None).unwrap();
//! println!("{}", help);
//! ```

//!
//! # Topics Support
//! 
//! This module also supports a "Topics" system, where you can register valid help topics (like "syntax", "environment", etc.)
//! and have them be resolvable via `help <topic>`.
//! 
//! ```rust
//! # use clap::Command;
//! # use outstanding_clap::TopicHelper;
//! # use outstanding::topics::{Topic, TopicRegistry, TopicType};
//! 
//! let mut topics = TopicRegistry::new();
//! topics.add_topic(Topic::new("syntax", "Syntax Guide...", TopicType::Text, None));
//! 
//! let helper = TopicHelper::new(topics);
//! let cmd = Command::new("my-app");
//! 
//! // In your main loop:
//! // match helper.get_matches(cmd) { ... }
//! ```

use outstanding::topics::{Topic, TopicRegistry};
use outstanding::{render_with_color, Theme, ThemeChoice};
use clap::{Command, Arg, ArgAction};
use console::Style;
use serde::Serialize;
use std::collections::BTreeMap;

/// Helper to integrate Clap with Outstanding Topics.
pub struct TopicHelper {
    registry: TopicRegistry,
}

/// Result of the topic help interception.
#[derive(Debug)]
pub enum TopicHelpResult {
    /// Normal matches found (no help requested).
    Matches(clap::ArgMatches),
    /// Help was rendered for a topic or command. Caller should print or display as needed.
    Help(String),
    /// Error: Subcommand or topic not found.
    /// We return the clap Error so caller can exit or handle it.
    Error(clap::Error),
}

impl TopicHelper {
    pub fn new(registry: TopicRegistry) -> Self {
        Self { registry }
    }

    /// Creates a new builder for constructing a TopicHelper.
    pub fn builder() -> TopicHelperBuilder {
        TopicHelperBuilder::new()
    }

    /// Returns a reference to the topic registry.
    pub fn registry(&self) -> &TopicRegistry {
        &self.registry
    }

    /// Returns a mutable reference to the topic registry.
    pub fn registry_mut(&mut self) -> &mut TopicRegistry {
        &mut self.registry
    }

    /// Prepares the command for topic support.
    /// It disables the default help subcommand so we can capture `help <arg>` manually.
    pub fn augment_command(&self, cmd: Command) -> Command {
        cmd.disable_help_subcommand(true)
            .subcommand(
                Command::new("help")
                    .about("Print this message or the help of the given subcommand(s)")
                    // We allow ignoring errors here because when we run augment_command
                    // inside get_matches we might be deep cloning or similar.
                    // Actually, disable_help_subcommand just sets a flag.
                    .arg(
                        Arg::new("topic")
                            .action(ArgAction::Set)
                            .num_args(1..)
                            .help("The subcommand or topic to print help for"),
                    )
            )
    }

    /// Attempts to get matches from the command line, intercepting `help` requests.
    /// Returns a `TopicHelpResult`.
    pub fn get_matches(&self, cmd: Command) -> TopicHelpResult {
        self.get_matches_from(cmd, std::env::args())
    }

    /// Attempts to get matches from the given arguments, intercepting `help` requests.
    pub fn get_matches_from<I, T>(&self, cmd: Command, itr: I) -> TopicHelpResult
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let mut cmd = self.augment_command(cmd);

        let matches = match cmd.clone().try_get_matches_from(itr) {
            Ok(m) => m,
            Err(e) => return TopicHelpResult::Error(e),
        };

        if let Some((name, sub_matches)) = matches.subcommand() {
            if name == "help" {
                if let Some(topic_args) = sub_matches.get_many::<String>("topic") {
                    let keywords: Vec<_> = topic_args.map(|s| s.as_str()).collect();
                     if !keywords.is_empty() {
                         return self.handle_help_request(&mut cmd, &keywords);
                     }
                }
                // If "help" is called without args, return the root help
                if let Ok(h) = render_help(&cmd, None) {
                    return TopicHelpResult::Help(h);
                }
            }
        }

        TopicHelpResult::Matches(matches)
    }

    /// Handles a request for specific help e.g. `help foo`
    fn handle_help_request(&self, cmd: &mut Command, keywords: &[&str]) -> TopicHelpResult {
        let sub_name = keywords[0];
        
        // 1. Check if it's a real command
        if find_subcommand(cmd, sub_name).is_some() {
             if let Some(target) = find_subcommand_recursive(cmd, keywords) {
                 if let Ok(h) = render_help(target, None) {
                     return TopicHelpResult::Help(h);
                 }
             }
             // If recursive find fails but top level existed, maybe print top level help?
             // Or let it be an error?
             // Fallthrough to topic check? No, standard clap behavior forbids ambiguity?
             // If a command exists, we favor it.
        }

        // 2. Check if it is a topic
        if let Some(topic) = self.registry.get_topic(sub_name) {
             if let Ok(h) = render_topic(topic, None) {
                 return TopicHelpResult::Help(h);
             }
        }
        
        // 3. Not found
        let err = cmd.error(
            clap::error::ErrorKind::InvalidSubcommand, 
            format!("The subcommand or topic '{}' wasn't recognized", sub_name)
        );
        TopicHelpResult::Error(err)
    }
}

/// Builder for constructing a TopicHelper with topics and directories.
///
/// # Example
/// ```rust
/// # use outstanding_clap::TopicHelper;
/// let helper = TopicHelper::builder()
///     .add_directory("docs/topics")
///     .build();
/// ```
#[derive(Default)]
pub struct TopicHelperBuilder {
    registry: TopicRegistry,
}

impl TopicHelperBuilder {
    /// Creates a new builder.
    pub fn new() -> Self {
        Self {
            registry: TopicRegistry::new(),
        }
    }

    /// Adds a topic to the helper.
    pub fn add_topic(mut self, topic: Topic) -> Self {
        self.registry.add_topic(topic);
        self
    }

    /// Adds topics from a directory. Only .txt and .md files are processed.
    /// Silently ignores non-existent directories.
    pub fn add_directory(mut self, path: impl AsRef<std::path::Path>) -> Self {
        let _ = self.registry.add_from_directory_if_exists(path);
        self
    }

    /// Builds the TopicHelper with all configured topics.
    pub fn build(self) -> TopicHelper {
        TopicHelper::new(self.registry)
    }
}

fn find_subcommand_recursive<'a>(cmd: &'a Command, keywords: &[&str]) -> Option<&'a Command> {
    let mut current = cmd;
    for k in keywords {
        if let Some(sub) = find_subcommand(current, k) {
            current = sub;
        } else {
            return None;
        }
    }
    Some(current)
}

fn find_subcommand<'a>(cmd: &'a Command, name: &str) -> Option<&'a Command> {
    cmd.get_subcommands().find(|s| s.get_name() == name || s.get_aliases().any(|a| a == name))
}

/// Configuration for the help renderer
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Custom template string. If None, uses the default template.
    pub template: Option<String>,
    /// Custom theme. If None, uses the default theme.
    pub theme: Option<Theme>,
    /// Whether to force color output. If None, auto-detects.
    pub use_color: Option<bool>,
}

/// Renders the help for a clap command using outstanding.
pub fn render_help(cmd: &Command, config: Option<Config>) -> Result<String, outstanding::Error> {
    let config = config.unwrap_or_default();
    let template = config
        .template
        .as_deref()
        .unwrap_or(include_str!("help_template.txt"));

    let theme = config.theme.unwrap_or_else(default_theme);
    let use_color = config
        .use_color
        .unwrap_or_else(|| console::Term::stdout().features().colors_supported());

    let data = extract_help_data(cmd);

    render_with_color(template, &data, ThemeChoice::from(&theme), use_color)
}

/// Renders a topic using outstanding templating.
pub fn render_topic(topic: &Topic, config: Option<Config>) -> Result<String, outstanding::Error> {
    let config = config.unwrap_or_default();
    let template = config
        .template
        .as_deref()
        .unwrap_or(include_str!("topic_template.txt"));

    let theme = config.theme.unwrap_or_else(default_theme);
    let use_color = config
        .use_color
        .unwrap_or_else(|| console::Term::stdout().features().colors_supported());

    let data = TopicData {
        title: topic.title.clone(),
        content: topic.content.clone(),
    };

    render_with_color(template, &data, ThemeChoice::from(&theme), use_color)
}

#[derive(Serialize)]
struct TopicData {
    title: String,
    content: String,
}

fn default_theme() -> Theme {
    // In a real implementation we might parse the YAML, but for now we construct it
    // to match help_theme.yaml effectively.
    Theme::new()
        .add("header", Style::new().bold())
        .add("section_title", Style::new().bold().yellow())
        .add("item", Style::new().green())
        .add("desc", Style::new().dim())
        .add("usage", Style::new().cyan())
        .add("example", Style::new().dim().italic())
        .add("about", Style::new().bold())
}

#[derive(Serialize)]
struct HelpData {
    name: String,
    about: String,
    usage: String,
    subcommands: Vec<Group<Subcommand>>,
    options: Vec<Group<OptionData>>,
    examples: String,
}

#[derive(Serialize)]
struct Group<T> {
    title: Option<String>,
    commands: Vec<T>, // Setup for subcommands
    options: Vec<T>,  // Setup for options
}

#[derive(Serialize)]
struct Subcommand {
    name: String,
    about: String,
    padding: String,
}

#[derive(Serialize)]
struct OptionData {
    name: String,
    help: String,
    padding: String,
    short: Option<char>,
    long: Option<String>,
}

fn extract_help_data(cmd: &Command) -> HelpData {
    let name = cmd.get_name().to_string();
    let about = cmd.get_about().map(|s| s.to_string()).unwrap_or_default();
    let usage = cmd.clone().render_usage().to_string();

    // Group Subcommands
    let mut sub_cmds = Vec::new();
    let mut max_width = 0;

    let mut subs: Vec<_> = cmd.get_subcommands().filter(|s| !s.is_hide_set()).collect();
    // Stable sort by display_order only - preserves declaration order for equal display_orders
    subs.sort_by_key(|s| s.get_display_order());

    for sub in subs {
        let name = sub.get_name().to_string();
        if name.len() > max_width {
            max_width = name.len();
        }

        let sub_data = Subcommand {
            name,
            about: sub.get_about().map(|s| s.to_string()).unwrap_or_default(),
            padding: String::new(), // Calculated later
        };
        sub_cmds.push(sub_data);
    }

    let subcommands = if sub_cmds.is_empty() {
        vec![]
    } else {
        for cmd in &mut sub_cmds {
            let pad = max_width.saturating_sub(cmd.name.len()) + 2;
            cmd.padding = " ".repeat(pad);
        }
        vec![Group {
            title: Some("Commands".to_string()),
            commands: sub_cmds,
            options: vec![],
        }]
    };

    // Group Options
    let mut opt_groups: BTreeMap<Option<String>, Vec<OptionData>> = BTreeMap::new();
    let mut opt_max_width = 0;

    // Clap args are also not sorted by display order by default in iterator
    let mut args: Vec<_> = cmd.get_arguments().filter(|a| !a.is_hide_set()).collect();
    // Stable sort by display_order only - preserves declaration order for equal display_orders
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
            name = arg.get_id().to_string(); // Positional
        }

        if name.len() > opt_max_width {
            opt_max_width = name.len();
        }

        let heading = arg.get_help_heading().map(|s| s.to_string());
        let opt_data = OptionData {
            name,
            help: arg.get_help().map(|s| s.to_string()).unwrap_or_default(),
            padding: String::new(),
            short: arg.get_short(),
            long: arg.get_long().map(|s| s.to_string()),
        };

        opt_groups.entry(heading).or_default().push(opt_data);
    }

    // Sort groups? Clap usually puts 'Arguments'/Generic groups last?
    // BTreeMap sorts by key (Option<String>). None is first.
    // Clap puts "Options" (None heading?) or custom headings.
    // We'll leave BTreeMap order for now (None first, then alphabetical headings).

    let options = opt_groups
        .into_iter()
        .map(|(title, mut opts)| {
            for opt in &mut opts {
                let pad = opt_max_width.saturating_sub(opt.name.len()) + 2;
                opt.padding = " ".repeat(pad);
            }
            Group {
                title,
                commands: vec![],
                options: opts,
            }
        })
        .collect();

    HelpData {
        name,
        about,
        usage,
        subcommands,
        options,
        examples: String::new(), // Clap extraction of examples is tricky via public API
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Arg; // Import Arg explicitly for tests

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
        assert_eq!(data.subcommands.len(), 1); // One group
        assert_eq!(data.subcommands[0].commands.len(), 2);
    }

    #[test]
    fn test_ordering_declaration() {
        // Declaration order should be preserved when display_order is equal.
        // "Zoo" is declared first, "Air" second - they should appear in that order.
        let cmd = Command::new("root")
            .subcommand(Command::new("Zoo"))
            .subcommand(Command::new("Air"));

        let data = extract_help_data(&cmd);
        assert_eq!(data.subcommands[0].commands[0].name, "Zoo");
        assert_eq!(data.subcommands[0].commands[1].name, "Air");
    }

    #[test]
    fn test_mixed_headings() {
        // Some have custom heading, some default (None)
        let cmd = Command::new("root")
            .arg(Arg::new("opt1").long("opt1"))
            .arg(Arg::new("custom").long("custom").help_heading("Custom"));

        let data = extract_help_data(&cmd);
        // Groups: None ("Options") and Some("Custom")
        assert_eq!(data.options.len(), 2);

        // BTreeMap sorts keys: None < Some.
        // So default options come first?
        // BTreeMap: None is less than Some.
        // Wait, `BTreeMap` implementation for `Option<T>` is `None` first? Yes.
        // Let's verify which group contains what.

        let group1 = &data.options[0];
        let group2 = &data.options[1];

        // We expect one to have title None, other "Custom".
        // If None is first:
        assert!(group1.title.is_none());
        assert_eq!(group1.options[0].name, "--opt1");

        assert_eq!(group2.title.as_deref(), Some("Custom"));
        assert_eq!(group2.options[0].name, "--custom");
    }

    #[test]
    // Since 'clap' feature is enabled in cargo test for this module,
    // and 'clap' crate has 'derive' feature enabled in Cargo.toml, we can just run this.
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

        // "Second" has lower order (1), should appear before "First" (2).
        assert_eq!(data.subcommands[0].commands[0].name, "second");
        assert_eq!(data.subcommands[0].commands[1].name, "first");
    }
}
