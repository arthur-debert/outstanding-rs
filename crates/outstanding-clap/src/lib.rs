//! # Outstanding Clap
//!
//! Styled help output for clap. Just pass your command:
//!
//! ```rust,no_run
//! use clap::Command;
//!
//! let matches = outstanding_clap::run(Command::new("my-app"));
//! ```
//!
//! With help topics from a directory:
//!
//! ```rust,no_run
//! use clap::Command;
//! use outstanding_clap::TopicHelper;
//!
//! let matches = TopicHelper::builder()
//!     .add_directory("docs/topics")
//!     .build()
//!     .run(Command::new("my-app"));
//! ```
//!
//! Help display, pager support, and errors are handled automatically.

use outstanding::topics::{Topic, TopicRegistry};
use outstanding::{render_with_output, Theme, ThemeChoice, OutputMode};
use clap::{Command, Arg, ArgAction};
use console::Style;
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::Write;
use std::process::{Command as ProcessCommand, Stdio};

/// Fixed width for the name column in help output (commands, options, topics).
const NAME_COLUMN_WIDTH: usize = 14;

/// Helper to integrate Clap with Outstanding Topics.
pub struct TopicHelper {
    registry: TopicRegistry,
    output_flag: Option<String>,
    output_mode: OutputMode,
}

/// Result of the topic help interception.
#[derive(Debug)]
pub enum TopicHelpResult {
    /// Normal matches found (no help requested).
    Matches(clap::ArgMatches),
    /// Help was rendered for a topic or command. Caller should print or display as needed.
    Help(String),
    /// Help was rendered and should be displayed through a pager.
    PagedHelp(String),
    /// Error: Subcommand or topic not found.
    /// We return the clap Error so caller can exit or handle it.
    Error(clap::Error),
}

impl TopicHelper {
    pub fn new(registry: TopicRegistry) -> Self {
        Self {
            registry,
            output_flag: None,
            output_mode: OutputMode::Auto,
        }
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

    /// Returns the current output mode.
    pub fn output_mode(&self) -> OutputMode {
        self.output_mode
    }

    /// Prepares the command for topic support.
    /// It disables the default help subcommand so we can capture `help <arg>` manually.
    /// If an output flag is configured, it's added as a global argument.
    pub fn augment_command(&self, cmd: Command) -> Command {
        let mut cmd = cmd.disable_help_subcommand(true)
            .subcommand(
                Command::new("help")
                    .about("Print this message or the help of the given subcommand(s)")
                    .arg(
                        Arg::new("topic")
                            .action(ArgAction::Set)
                            .num_args(1..)
                            .help("The subcommand or topic to print help for"),
                    )
                    .arg(
                        Arg::new("page")
                            .long("page")
                            .action(ArgAction::SetTrue)
                            .help("Display help through a pager"),
                    )
            );

        // Add output flag if configured
        if let Some(ref flag_name) = self.output_flag {
            // Leak the string to get a 'static reference required by clap
            // This is safe since the command is built once per program run
            let flag: &'static str = Box::leak(flag_name.clone().into_boxed_str());
            cmd = cmd.arg(
                Arg::new("_output_mode")
                    .long(flag)
                    .value_name("MODE")
                    .global(true)
                    .value_parser(["auto", "term", "text", "term-debug"])
                    .default_value("auto")
                    .help("Output mode: auto, term, text, or term-debug")
            );
        }

        cmd
    }

    /// Runs the CLI, handling help display automatically.
    ///
    /// This is the recommended entry point. It:
    /// - Intercepts `help` subcommand and displays styled help
    /// - Handles pager display when `--page` is used
    /// - Exits on errors
    /// - Returns `ArgMatches` only for actual commands
    ///
    /// # Example
    /// ```rust,no_run
    /// # use clap::Command;
    /// # use outstanding_clap::TopicHelper;
    /// let helper = TopicHelper::builder()
    ///     .add_directory("docs/topics")
    ///     .build();
    ///
    /// let matches = helper.run(Command::new("my-app"));
    /// // Handle your actual commands here
    /// ```
    pub fn run(&self, cmd: Command) -> clap::ArgMatches {
        self.run_from(cmd, std::env::args())
    }

    /// Like `run`, but takes arguments from an iterator.
    pub fn run_from<I, T>(&self, cmd: Command, itr: I) -> clap::ArgMatches
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        match self.get_matches_from(cmd, itr) {
            TopicHelpResult::Matches(m) => m,
            TopicHelpResult::Help(h) => {
                println!("{}", h);
                std::process::exit(0);
            }
            TopicHelpResult::PagedHelp(h) => {
                if let Err(_) = display_with_pager(&h) {
                    println!("{}", h);
                }
                std::process::exit(0);
            }
            TopicHelpResult::Error(e) => e.exit(),
        }
    }

    /// Attempts to get matches from the command line, intercepting `help` requests.
    /// Returns a `TopicHelpResult`.
    ///
    /// For most use cases, prefer `run()` which handles help display automatically.
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

        // Extract output mode if the flag was configured
        let output_mode = if self.output_flag.is_some() {
            match matches.get_one::<String>("_output_mode").map(|s| s.as_str()) {
                Some("term") => OutputMode::Term,
                Some("text") => OutputMode::Text,
                Some("term-debug") => OutputMode::TermDebug,
                _ => OutputMode::Auto,
            }
        } else {
            OutputMode::Auto
        };

        let config = Config {
            output_mode: Some(output_mode),
            ..Default::default()
        };

        if let Some((name, sub_matches)) = matches.subcommand() {
            if name == "help" {
                let use_pager = sub_matches.get_flag("page");

                if let Some(topic_args) = sub_matches.get_many::<String>("topic") {
                    let keywords: Vec<_> = topic_args.map(|s| s.as_str()).collect();
                     if !keywords.is_empty() {
                         return self.handle_help_request(&mut cmd, &keywords, use_pager, Some(config));
                     }
                }
                // If "help" is called without args, return the root help with topics
                if let Ok(h) = render_help_with_topics(&cmd, &self.registry, Some(config)) {
                    return if use_pager {
                        TopicHelpResult::PagedHelp(h)
                    } else {
                        TopicHelpResult::Help(h)
                    };
                }
            }
        }

        TopicHelpResult::Matches(matches)
    }

    /// Handles a request for specific help e.g. `help foo`
    fn handle_help_request(&self, cmd: &mut Command, keywords: &[&str], use_pager: bool, config: Option<Config>) -> TopicHelpResult {
        let sub_name = keywords[0];

        // 0. Check for "topics" - list all available topics
        if sub_name == "topics" {
            if let Ok(h) = render_topics_list(&self.registry, cmd, config.clone()) {
                return if use_pager {
                    TopicHelpResult::PagedHelp(h)
                } else {
                    TopicHelpResult::Help(h)
                };
            }
        }

        // 1. Check if it's a real command
        if find_subcommand(cmd, sub_name).is_some() {
             if let Some(target) = find_subcommand_recursive(cmd, keywords) {
                 if let Ok(h) = render_help(target, config.clone()) {
                     return if use_pager {
                         TopicHelpResult::PagedHelp(h)
                     } else {
                         TopicHelpResult::Help(h)
                     };
                 }
             }
        }

        // 2. Check if it is a topic
        if let Some(topic) = self.registry.get_topic(sub_name) {
             if let Ok(h) = render_topic(topic, config) {
                 return if use_pager {
                     TopicHelpResult::PagedHelp(h)
                 } else {
                     TopicHelpResult::Help(h)
                 };
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

/// Displays content through a pager.
///
/// Tries pagers in this order:
/// 1. $PAGER environment variable
/// 2. `less`
/// 3. `more`
///
/// If all pagers fail, falls back to printing directly to stdout.
pub fn display_with_pager(content: &str) -> std::io::Result<()> {
    let pagers = get_pager_candidates();

    for pager in pagers {
        if let Ok(()) = try_pager(&pager, content) {
            return Ok(());
        }
    }

    // Fallback: print directly
    print!("{}", content);
    std::io::stdout().flush()
}

/// Returns the list of pager candidates to try.
fn get_pager_candidates() -> Vec<String> {
    let mut pagers = Vec::new();

    if let Ok(pager) = std::env::var("PAGER") {
        if !pager.is_empty() {
            pagers.push(pager);
        }
    }

    pagers.push("less".to_string());
    pagers.push("more".to_string());

    pagers
}

/// Attempts to run content through a specific pager.
fn try_pager(pager: &str, content: &str) -> std::io::Result<()> {
    let mut child = ProcessCommand::new(pager)
        .stdin(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(content.as_bytes())?;
    }

    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "pager exited with error",
        ))
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
    output_flag: Option<String>,
}

impl TopicHelperBuilder {
    /// Creates a new builder.
    pub fn new() -> Self {
        Self {
            registry: TopicRegistry::new(),
            output_flag: None,
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

    /// Configures the name of the output flag to add to the CLI.
    ///
    /// When set, an `--<flag>=<auto|term|text>` option is automatically
    /// added to the command. The output mode is then used for all renders.
    ///
    /// Default flag name is "output" if this method is called with None.
    /// If this method is never called, no output flag is added.
    ///
    /// # Example
    /// ```rust
    /// # use outstanding_clap::TopicHelper;
    /// // Add --output flag (default name)
    /// let helper = TopicHelper::builder()
    ///     .output_flag(None)
    ///     .build();
    ///
    /// // Add --format flag (custom name)
    /// let helper = TopicHelper::builder()
    ///     .output_flag(Some("format"))
    ///     .build();
    /// ```
    pub fn output_flag(mut self, name: Option<&str>) -> Self {
        self.output_flag = Some(name.unwrap_or("output").to_string());
        self
    }

    /// Builds the TopicHelper with all configured topics.
    pub fn build(self) -> TopicHelper {
        TopicHelper {
            registry: self.registry,
            output_flag: self.output_flag,
            output_mode: OutputMode::Auto,
        }
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

/// Runs a clap command with styled help output.
///
/// This is the simplest entry point for basic CLIs without topics.
/// It handles help display automatically and returns `ArgMatches` for actual commands.
///
/// # Example
/// ```rust,no_run
/// use clap::Command;
/// use outstanding_clap::run;
///
/// let cmd = Command::new("my-app")
///     .about("My Application")
///     .subcommand(Command::new("test"));
///
/// let matches = run(cmd);
/// // Handle your commands here
/// ```
pub fn run(cmd: Command) -> clap::ArgMatches {
    run_from(cmd, std::env::args())
}

/// Like `run`, but takes arguments from an iterator.
pub fn run_from<I, T>(cmd: Command, itr: I) -> clap::ArgMatches
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    TopicHelper::builder().build().run_from(cmd, itr)
}

/// Configuration for the help renderer
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Custom template string. If None, uses the default template.
    pub template: Option<String>,
    /// Custom theme. If None, uses the default theme.
    pub theme: Option<Theme>,
    /// Output mode. If None, uses Auto (auto-detects).
    pub output_mode: Option<OutputMode>,
}

/// Renders the help for a clap command using outstanding.
pub fn render_help(cmd: &Command, config: Option<Config>) -> Result<String, outstanding::Error> {
    let config = config.unwrap_or_default();
    let template = config
        .template
        .as_deref()
        .unwrap_or(include_str!("help_template.txt"));

    let theme = config.theme.unwrap_or_else(default_theme);
    let mode = config.output_mode.unwrap_or(OutputMode::Auto);

    let data = extract_help_data(cmd);

    render_with_output(template, &data, ThemeChoice::from(&theme), mode)
}

/// Renders the help for a clap command with topics in a "Learn More" section.
pub fn render_help_with_topics(cmd: &Command, registry: &TopicRegistry, config: Option<Config>) -> Result<String, outstanding::Error> {
    let config = config.unwrap_or_default();
    let template = config
        .template
        .as_deref()
        .unwrap_or(include_str!("help_template.txt"));

    let theme = config.theme.unwrap_or_else(default_theme);
    let mode = config.output_mode.unwrap_or(OutputMode::Auto);

    let data = extract_help_data_with_topics(cmd, registry);

    render_with_output(template, &data, ThemeChoice::from(&theme), mode)
}

/// Renders a topic using outstanding templating.
pub fn render_topic(topic: &Topic, config: Option<Config>) -> Result<String, outstanding::Error> {
    let config = config.unwrap_or_default();
    let template = config
        .template
        .as_deref()
        .unwrap_or(include_str!("topic_template.txt"));

    let theme = config.theme.unwrap_or_else(default_theme);
    let mode = config.output_mode.unwrap_or(OutputMode::Auto);

    let data = TopicData {
        title: topic.title.clone(),
        content: topic.content.clone(),
    };

    render_with_output(template, &data, ThemeChoice::from(&theme), mode)
}

#[derive(Serialize)]
struct TopicData {
    title: String,
    content: String,
}

/// Renders a list of all available topics.
pub fn render_topics_list(registry: &TopicRegistry, cmd: &Command, config: Option<Config>) -> Result<String, outstanding::Error> {
    let config = config.unwrap_or_default();
    let template = config
        .template
        .as_deref()
        .unwrap_or(include_str!("topics_list_template.txt"));

    let theme = config.theme.unwrap_or_else(default_theme);
    let mode = config.output_mode.unwrap_or(OutputMode::Auto);

    let topics = registry.list_topics();

    let topic_items: Vec<TopicListItem> = topics
        .iter()
        .map(|t| {
            // +1 accounts for the colon added in the template
            let pad = NAME_COLUMN_WIDTH.saturating_sub(t.name.len() + 1);
            TopicListItem {
                name: t.name.clone(),
                title: t.title.clone(),
                padding: " ".repeat(pad),
            }
        })
        .collect();

    let data = TopicsListData {
        usage: format!("{} help <topic>", cmd.get_name()),
        topics: topic_items,
    };

    render_with_output(template, &data, ThemeChoice::from(&theme), mode)
}

#[derive(Serialize)]
struct TopicsListData {
    usage: String,
    topics: Vec<TopicListItem>,
}

#[derive(Serialize)]
struct TopicListItem {
    name: String,
    title: String,
    padding: String,
}

fn default_theme() -> Theme {
    Theme::new()
        .add("header", Style::new().bold())
        .add("item", Style::new().bold())
        .add("desc", Style::new())
        .add("usage", Style::new())
        .add("example", Style::new())
        .add("about", Style::new())
}

#[derive(Serialize)]
struct HelpData {
    name: String,
    about: String,
    usage: String,
    subcommands: Vec<Group<Subcommand>>,
    options: Vec<Group<OptionData>>,
    examples: String,
    learn_more: Vec<TopicListItem>,
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
    // render_usage() returns "Usage: <cmd> [OPTIONS]..." - strip the "Usage: " prefix
    let usage = cmd.clone().render_usage().to_string()
        .strip_prefix("Usage: ")
        .unwrap_or(&cmd.clone().render_usage().to_string())
        .to_string();

    // Group Subcommands
    let mut sub_cmds = Vec::new();

    let mut subs: Vec<_> = cmd.get_subcommands().filter(|s| !s.is_hide_set()).collect();
    // Stable sort by display_order only - preserves declaration order for equal display_orders
    subs.sort_by_key(|s| s.get_display_order());

    for sub in subs {
        let name = sub.get_name().to_string();
        // +1 accounts for the colon added in the template
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
        .map(|(title, opts)| {
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
        learn_more: vec![],
    }
}

fn extract_help_data_with_topics(cmd: &Command, registry: &TopicRegistry) -> HelpData {
    let mut data = extract_help_data(cmd);

    let topics = registry.list_topics();
    if !topics.is_empty() {
        data.learn_more = topics
            .iter()
            .map(|t| {
                // +1 accounts for the colon added in the template
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

    #[test]
    fn test_get_pager_candidates_default() {
        // Clear PAGER to test default behavior
        std::env::remove_var("PAGER");
        let candidates = get_pager_candidates();
        assert_eq!(candidates, vec!["less", "more"]);
    }

    #[test]
    fn test_get_pager_candidates_with_pager_env() {
        std::env::set_var("PAGER", "bat");
        let candidates = get_pager_candidates();
        assert_eq!(candidates[0], "bat");
        assert_eq!(candidates[1], "less");
        assert_eq!(candidates[2], "more");
        std::env::remove_var("PAGER");
    }

    #[test]
    fn test_get_pager_candidates_empty_pager() {
        std::env::set_var("PAGER", "");
        let candidates = get_pager_candidates();
        assert_eq!(candidates, vec!["less", "more"]);
        std::env::remove_var("PAGER");
    }
}
