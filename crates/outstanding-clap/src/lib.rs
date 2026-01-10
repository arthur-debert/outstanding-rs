//! # Outstanding Clap - Clap Integration
//!
//! Batteries-included integration of `outstanding` with `clap`. This crate handles
//! the boilerplate of connecting outstanding's styled output to your clap-based CLI:
//!
//! - Styled help output using outstanding templates
//! - Help topics system (`help <topic>`, `help topics`)
//! - `--output` flag for user output control (enabled by default)
//! - Pager support for long help content
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use clap::Command;
//! use outstanding_clap::Outstanding;
//!
//! // Simplest usage - styled help with --output flag
//! let matches = Outstanding::run(Command::new("my-app"));
//! ```
//!
//! ## With Help Topics
//!
//! ```rust,no_run
//! use clap::Command;
//! use outstanding_clap::Outstanding;
//!
//! let matches = Outstanding::builder()
//!     .topics_dir("docs/topics")  // Load topics from directory
//!     .run(Command::new("my-app"));
//!
//! // Users can now run:
//! //   my-app help topics     - list all topics
//! //   my-app help <topic>    - view specific topic
//! ```
//!
//! ## Configuration Options
//!
//! ```rust,no_run
//! use clap::Command;
//! use outstanding::Theme;
//! use outstanding_clap::Outstanding;
//!
//! let my_theme = Theme::new();  // Customize as needed
//!
//! let matches = Outstanding::builder()
//!     .topics_dir("docs/topics")    // Load topics from directory
//!     .theme(my_theme)              // Custom theme (optional)
//!     .output_flag(Some("format"))  // Custom flag name (default: "output")
//!     .no_output_flag()             // Or disable the flag entirely
//!     .run(Command::new("my-app"));
//! ```
//!
//! ## What This Crate Does
//!
//! The `outstanding` crate provides the core rendering framework (themes, templates,
//! output modes, topic system). This crate provides the **clap integration**:
//!
//! - Intercepts `help`, `help <topic>`, `help topics` subcommands
//! - Injects `--output` flag to all commands
//! - Renders clap command help using outstanding templates
//! - Calls outstanding's topic rendering for topic help
//!
//! For non-clap applications, use `outstanding` directly and write your own
//! argument parsing glue.

pub mod handler;

pub use handler::{CommandContext, CommandResult, FnHandler, Handler, RunResult};

use outstanding::topics::{
    Topic, TopicRegistry, TopicRenderConfig,
    render_topic, render_topics_list,
};
use outstanding::{render_with_output, render_or_serialize, Theme, ThemeChoice, OutputMode};
use clap::{Command, Arg, ArgAction, ArgMatches};
use console::Style;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// Internal result type for dispatch functions.
enum DispatchOutput {
    /// Text output (rendered template or JSON)
    Text(String),
    /// Binary output (bytes, filename)
    Binary(Vec<u8>, String),
    /// No output (silent)
    Silent,
}

/// Type-erased dispatch function.
///
/// Takes ArgMatches and CommandContext, returns the dispatch output or an error.
type DispatchFn = Arc<dyn Fn(&ArgMatches, &CommandContext) -> Result<DispatchOutput, String> + Send + Sync>;

/// Fixed width for the name column in help output (commands, options, topics).
const NAME_COLUMN_WIDTH: usize = 14;

// Re-export core types for convenience
pub use outstanding::topics::{
    Topic as TopicDef, TopicType, TopicRegistry as TopicRegistryDef,
    display_with_pager, render_topic as render_topic_core, render_topics_list as render_topics_list_core,
};

/// Main entry point for outstanding-clap integration.
///
/// Handles help interception, output flag, and topic rendering.
pub struct Outstanding {
    registry: TopicRegistry,
    output_flag: Option<String>,
    output_mode: OutputMode,
    theme: Option<Theme>,
}

/// Result of the help interception.
#[derive(Debug)]
pub enum HelpResult {
    /// Normal matches found (no help requested).
    Matches(clap::ArgMatches),
    /// Help was rendered. Caller should print or display as needed.
    Help(String),
    /// Help was rendered and should be displayed through a pager.
    PagedHelp(String),
    /// Error: Subcommand or topic not found.
    Error(clap::Error),
}

impl Outstanding {
    /// Creates a new Outstanding instance with default settings.
    ///
    /// By default:
    /// - `--output` flag is enabled
    /// - No topics are loaded
    /// - Default theme is used
    pub fn new() -> Self {
        Self {
            registry: TopicRegistry::new(),
            output_flag: Some("output".to_string()), // Enabled by default
            output_mode: OutputMode::Auto,
            theme: None,
        }
    }

    /// Creates a new Outstanding instance with a pre-configured topic registry.
    pub fn with_registry(registry: TopicRegistry) -> Self {
        Self {
            registry,
            output_flag: Some("output".to_string()),
            output_mode: OutputMode::Auto,
            theme: None,
        }
    }

    /// Creates a new builder for constructing an Outstanding instance.
    pub fn builder() -> OutstandingBuilder {
        OutstandingBuilder::new()
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

    /// Prepares the command for outstanding integration.
    ///
    /// - Disables default help subcommand
    /// - Adds custom `help` subcommand with topic support
    /// - Adds `--output` flag if enabled
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

        // Add output flag if enabled
        if let Some(ref flag_name) = self.output_flag {
            let flag: &'static str = Box::leak(flag_name.clone().into_boxed_str());
            cmd = cmd.arg(
                Arg::new("_output_mode")
                    .long(flag)
                    .value_name("MODE")
                    .global(true)
                    .value_parser(["auto", "term", "text", "term-debug", "json"])
                    .default_value("auto")
                    .help("Output mode: auto, term, text, term-debug, or json")
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
    pub fn run(cmd: Command) -> clap::ArgMatches {
        Self::new().run_with(cmd)
    }

    /// Runs the CLI with this configured Outstanding instance.
    pub fn run_with(&self, cmd: Command) -> clap::ArgMatches {
        self.run_from(cmd, std::env::args())
    }

    /// Like `run_with`, but takes arguments from an iterator.
    pub fn run_from<I, T>(&self, cmd: Command, itr: I) -> clap::ArgMatches
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        match self.get_matches_from(cmd, itr) {
            HelpResult::Matches(m) => m,
            HelpResult::Help(h) => {
                println!("{}", h);
                std::process::exit(0);
            }
            HelpResult::PagedHelp(h) => {
                if display_with_pager(&h).is_err() {
                    println!("{}", h);
                }
                std::process::exit(0);
            }
            HelpResult::Error(e) => e.exit(),
        }
    }

    /// Attempts to get matches, intercepting `help` requests.
    ///
    /// For most use cases, prefer `run()` which handles help display automatically.
    pub fn get_matches(&self, cmd: Command) -> HelpResult {
        self.get_matches_from(cmd, std::env::args())
    }

    /// Attempts to get matches from the given arguments, intercepting `help` requests.
    pub fn get_matches_from<I, T>(&self, cmd: Command, itr: I) -> HelpResult
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let mut cmd = self.augment_command(cmd);

        let matches = match cmd.clone().try_get_matches_from(itr) {
            Ok(m) => m,
            Err(e) => return HelpResult::Error(e),
        };

        // Extract output mode if the flag was configured
        let output_mode = if self.output_flag.is_some() {
            match matches.get_one::<String>("_output_mode").map(|s| s.as_str()) {
                Some("term") => OutputMode::Term,
                Some("text") => OutputMode::Text,
                Some("term-debug") => OutputMode::TermDebug,
                Some("json") => OutputMode::Json,
                _ => OutputMode::Auto,
            }
        } else {
            OutputMode::Auto
        };

        let config = HelpConfig {
            output_mode: Some(output_mode),
            theme: self.theme.clone(),
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
                        HelpResult::PagedHelp(h)
                    } else {
                        HelpResult::Help(h)
                    };
                }
            }
        }

        HelpResult::Matches(matches)
    }

    /// Handles a request for specific help e.g. `help foo`
    fn handle_help_request(&self, cmd: &mut Command, keywords: &[&str], use_pager: bool, config: Option<HelpConfig>) -> HelpResult {
        let sub_name = keywords[0];

        // 0. Check for "topics" - list all available topics
        if sub_name == "topics" {
            let topic_config = TopicRenderConfig {
                output_mode: config.as_ref().and_then(|c| c.output_mode),
                theme: config.as_ref().and_then(|c| c.theme.clone()),
                ..Default::default()
            };
            if let Ok(h) = render_topics_list(&self.registry, &format!("{} help", cmd.get_name()), Some(topic_config)) {
                return if use_pager {
                    HelpResult::PagedHelp(h)
                } else {
                    HelpResult::Help(h)
                };
            }
        }

        // 1. Check if it's a real command
        if find_subcommand(cmd, sub_name).is_some() {
            if let Some(target) = find_subcommand_recursive(cmd, keywords) {
                if let Ok(h) = render_help(target, config.clone()) {
                    return if use_pager {
                        HelpResult::PagedHelp(h)
                    } else {
                        HelpResult::Help(h)
                    };
                }
            }
        }

        // 2. Check if it is a topic
        if let Some(topic) = self.registry.get_topic(sub_name) {
            let topic_config = TopicRenderConfig {
                output_mode: config.as_ref().and_then(|c| c.output_mode),
                theme: config.as_ref().and_then(|c| c.theme.clone()),
                ..Default::default()
            };
            if let Ok(h) = render_topic(topic, Some(topic_config)) {
                return if use_pager {
                    HelpResult::PagedHelp(h)
                } else {
                    HelpResult::Help(h)
                };
            }
        }

        // 3. Not found
        let err = cmd.error(
            clap::error::ErrorKind::InvalidSubcommand,
            format!("The subcommand or topic '{}' wasn't recognized", sub_name)
        );
        HelpResult::Error(err)
    }
}

impl Default for Outstanding {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for constructing an Outstanding instance.
///
/// # Example
///
/// ```rust
/// use outstanding_clap::Outstanding;
///
/// let outstanding = Outstanding::builder()
///     .topics_dir("docs/topics")
///     .output_flag(Some("format"))
///     .build();
/// ```
pub struct OutstandingBuilder {
    registry: TopicRegistry,
    output_flag: Option<String>,
    theme: Option<Theme>,
    commands: HashMap<String, DispatchFn>,
}

impl Default for OutstandingBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl OutstandingBuilder {
    /// Creates a new builder with default settings.
    ///
    /// By default, the `--output` flag is enabled.
    pub fn new() -> Self {
        Self {
            registry: TopicRegistry::new(),
            output_flag: Some("output".to_string()), // Enabled by default
            theme: None,
            commands: HashMap::new(),
        }
    }

    /// Adds a topic to the registry.
    pub fn add_topic(mut self, topic: Topic) -> Self {
        self.registry.add_topic(topic);
        self
    }

    /// Adds topics from a directory. Only .txt and .md files are processed.
    /// Silently ignores non-existent directories.
    pub fn topics_dir(mut self, path: impl AsRef<std::path::Path>) -> Self {
        let _ = self.registry.add_from_directory_if_exists(path);
        self
    }

    /// Sets a custom theme for help rendering.
    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = Some(theme);
        self
    }

    /// Configures the name of the output flag.
    ///
    /// When set, an `--<flag>=<auto|term|text|term-debug>` option is added
    /// to all commands. The output mode is then used for all renders.
    ///
    /// Default flag name is "output". Pass `Some("format")` to use `--format`.
    ///
    /// To disable the output flag entirely, use `no_output_flag()`.
    pub fn output_flag(mut self, name: Option<&str>) -> Self {
        self.output_flag = Some(name.unwrap_or("output").to_string());
        self
    }

    /// Disables the output flag entirely.
    ///
    /// By default, `--output` is added to all commands. Call this to disable it.
    pub fn no_output_flag(mut self) -> Self {
        self.output_flag = None;
        self
    }

    /// Registers a command handler (closure) with a template.
    ///
    /// The handler will be invoked when the command path matches. The path uses
    /// dot notation for nested commands (e.g., "config.get" matches `app config get`).
    ///
    /// # Arguments
    ///
    /// * `path` - Command path using dot notation (e.g., "list" or "config.get")
    /// * `handler` - The handler closure
    /// * `template` - MiniJinja template for rendering output
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use outstanding_clap::{Outstanding, CommandResult};
    /// use serde::Serialize;
    ///
    /// #[derive(Serialize)]
    /// struct ListOutput { items: Vec<String> }
    ///
    /// Outstanding::builder()
    ///     .command("list", |_m, _ctx| {
    ///         CommandResult::Ok(ListOutput { items: vec!["one".into()] })
    ///     }, "{% for item in items %}{{ item }}\n{% endfor %}")
    ///     .run(cmd);
    /// ```
    pub fn command<F, T>(self, path: &str, handler: F, template: &str) -> Self
    where
        F: Fn(&ArgMatches, &CommandContext) -> CommandResult<T> + Send + Sync + 'static,
        T: Serialize + Send + Sync + 'static,
    {
        self.command_handler(path, handler::FnHandler::new(handler), template)
    }

    /// Registers a struct handler with a template.
    ///
    /// Use this when your handler needs to carry state (like database connections).
    ///
    /// # Arguments
    ///
    /// * `path` - Command path using dot notation (e.g., "list" or "config.get")
    /// * `handler` - A struct implementing the `Handler` trait
    /// * `template` - MiniJinja template for rendering output
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use outstanding_clap::{Outstanding, Handler, CommandResult, CommandContext};
    /// use clap::ArgMatches;
    /// use serde::Serialize;
    ///
    /// struct ListHandler { db: Database }
    ///
    /// impl Handler for ListHandler {
    ///     type Output = Vec<Item>;
    ///     fn handle(&self, _m: &ArgMatches, _ctx: &CommandContext) -> CommandResult<Self::Output> {
    ///         CommandResult::Ok(self.db.list())
    ///     }
    /// }
    ///
    /// Outstanding::builder()
    ///     .command_handler("list", ListHandler { db }, "{% for item in items %}...")
    ///     .run(cmd);
    /// ```
    pub fn command_handler<H, T>(mut self, path: &str, handler: H, template: &str) -> Self
    where
        H: Handler<Output = T> + 'static,
        T: Serialize + 'static,
    {
        let template = template.to_string();
        let handler = Arc::new(handler);

        let dispatch: DispatchFn = Arc::new(move |matches: &ArgMatches, ctx: &CommandContext| {
            let result = handler.handle(matches, ctx);

            match result {
                CommandResult::Ok(data) => {
                    // Use a default theme for now - will be enhanced later
                    let theme = Theme::new();
                    let output = render_or_serialize(
                        &template,
                        &data,
                        ThemeChoice::from(&theme),
                        ctx.output_mode,
                    )
                    .map_err(|e| e.to_string())?;
                    Ok(DispatchOutput::Text(output))
                }
                CommandResult::Err(e) => Err(format!("Error: {}", e)),
                CommandResult::Silent => Ok(DispatchOutput::Silent),
                CommandResult::Archive(bytes, filename) => Ok(DispatchOutput::Binary(bytes, filename)),
            }
        });

        self.commands.insert(path.to_string(), dispatch);
        self
    }

    /// Dispatches to a registered handler if one matches the command path.
    ///
    /// Returns `RunResult::Handled(output)` if a handler was found and executed,
    /// or `RunResult::Unhandled(matches)` if no handler matched.
    pub fn dispatch(&self, matches: ArgMatches, output_mode: OutputMode) -> RunResult {
        // Build command path from matches
        let path = extract_command_path(&matches);
        let path_str = path.join(".");

        // Look up handler
        if let Some(dispatch) = self.commands.get(&path_str) {
            let ctx = CommandContext {
                output_mode,
                command_path: path,
            };

            // Get the subcommand matches for the deepest command
            let sub_matches = get_deepest_matches(&matches);

            match dispatch(sub_matches, &ctx) {
                Ok(DispatchOutput::Text(output)) => RunResult::Handled(output),
                Ok(DispatchOutput::Binary(bytes, filename)) => RunResult::Binary(bytes, filename),
                Ok(DispatchOutput::Silent) => RunResult::Handled(String::new()),
                Err(e) => RunResult::Handled(e), // Error message as output
            }
        } else {
            RunResult::Unhandled(matches)
        }
    }

    /// Parses arguments and dispatches to registered handlers.
    ///
    /// This is the recommended entry point when using the command handler system.
    /// It augments the command with `--output` flag, parses arguments, and
    /// dispatches to registered handlers.
    ///
    /// # Returns
    ///
    /// - `RunResult::Handled(output)` if a registered handler processed the command
    /// - `RunResult::Unhandled(matches)` if no handler matched (for manual handling)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use outstanding_clap::{Outstanding, CommandResult, RunResult};
    ///
    /// let result = Outstanding::builder()
    ///     .command("list", |_m, _ctx| CommandResult::Ok(vec!["a", "b"]), "{{ . }}")
    ///     .dispatch_from(cmd, std::env::args());
    ///
    /// match result {
    ///     RunResult::Handled(output) => println!("{}", output),
    ///     RunResult::Unhandled(matches) => {
    ///         // Handle manually
    ///     }
    /// }
    /// ```
    pub fn dispatch_from<I, T>(&self, cmd: Command, args: I) -> RunResult
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        // Augment command with --output flag
        let cmd = self.augment_command_for_dispatch(cmd);

        // Parse arguments
        let matches = match cmd.try_get_matches_from(args) {
            Ok(m) => m,
            Err(e) => {
                // Return error as handled output
                return RunResult::Handled(e.to_string());
            }
        };

        // Extract output mode
        let output_mode = if self.output_flag.is_some() {
            match matches.get_one::<String>("_output_mode").map(|s| s.as_str()) {
                Some("term") => OutputMode::Term,
                Some("text") => OutputMode::Text,
                Some("term-debug") => OutputMode::TermDebug,
                Some("json") => OutputMode::Json,
                _ => OutputMode::Auto,
            }
        } else {
            OutputMode::Auto
        };

        // Dispatch to handler
        self.dispatch(matches, output_mode)
    }

    /// Parses arguments, dispatches to handlers, and prints output.
    ///
    /// This is the simplest entry point for the command handler system.
    /// It handles everything: parsing, dispatch, and output.
    ///
    /// # Returns
    ///
    /// - `Ok(true)` if a handler processed and printed output
    /// - `Ok(false)` if no handler matched (caller should handle manually)
    /// - `Err(matches)` if no handler matched, with the parsed ArgMatches
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use outstanding_clap::{Outstanding, CommandResult};
    ///
    /// let handled = Outstanding::builder()
    ///     .command("list", |_m, _ctx| CommandResult::Ok(vec!["a", "b"]), "{{ . }}")
    ///     .run_and_print(cmd, std::env::args());
    ///
    /// if !handled {
    ///     // Handle unregistered commands manually
    /// }
    /// ```
    pub fn run_and_print<I, T>(&self, cmd: Command, args: I) -> bool
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        match self.dispatch_from(cmd, args) {
            RunResult::Handled(output) => {
                if !output.is_empty() {
                    println!("{}", output);
                }
                true
            }
            RunResult::Binary(bytes, filename) => {
                // For binary output, write to stdout or the suggested file
                // By default, we write to the suggested filename
                if let Err(e) = std::fs::write(&filename, &bytes) {
                    eprintln!("Error writing {}: {}", filename, e);
                } else {
                    eprintln!("Wrote {} bytes to {}", bytes.len(), filename);
                }
                true
            }
            RunResult::Unhandled(_) => false,
        }
    }

    /// Augments a command for dispatch (adds --output flag without help subcommand).
    fn augment_command_for_dispatch(&self, mut cmd: Command) -> Command {
        if let Some(ref flag_name) = self.output_flag {
            let flag: &'static str = Box::leak(flag_name.clone().into_boxed_str());
            cmd = cmd.arg(
                Arg::new("_output_mode")
                    .long(flag)
                    .value_name("MODE")
                    .global(true)
                    .value_parser(["auto", "term", "text", "term-debug", "json"])
                    .default_value("auto")
                    .help("Output mode: auto, term, text, term-debug, or json")
            );
        }
        cmd
    }

    /// Builds the Outstanding instance.
    pub fn build(self) -> Outstanding {
        Outstanding {
            registry: self.registry,
            output_flag: self.output_flag,
            output_mode: OutputMode::Auto,
            theme: self.theme,
        }
    }

    /// Builds and runs the CLI in one step.
    pub fn run(self, cmd: Command) -> clap::ArgMatches {
        self.build().run_with(cmd)
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

/// Extracts the command path from ArgMatches by following subcommand chain.
fn extract_command_path(matches: &ArgMatches) -> Vec<String> {
    let mut path = Vec::new();
    let mut current = matches;

    while let Some((name, sub)) = current.subcommand() {
        // Skip "help" as it's handled separately
        if name == "help" {
            break;
        }
        path.push(name.to_string());
        current = sub;
    }

    path
}

/// Gets the deepest subcommand matches.
fn get_deepest_matches(matches: &ArgMatches) -> &ArgMatches {
    let mut current = matches;

    while let Some((name, sub)) = current.subcommand() {
        if name == "help" {
            break;
        }
        current = sub;
    }

    current
}

// ============================================================================
// CLAP HELP RENDERING
// ============================================================================

/// Configuration for clap help rendering.
#[derive(Debug, Clone, Default)]
pub struct HelpConfig {
    /// Custom template string. If None, uses the default template.
    pub template: Option<String>,
    /// Custom theme. If None, uses the default theme.
    pub theme: Option<Theme>,
    /// Output mode. If None, uses Auto (auto-detects).
    pub output_mode: Option<OutputMode>,
}

/// Returns the default theme for help rendering.
pub fn default_help_theme() -> Theme {
    Theme::new()
        .add("header", Style::new().bold())
        .add("item", Style::new().bold())
        .add("desc", Style::new())
        .add("usage", Style::new())
        .add("example", Style::new())
        .add("about", Style::new())
}

/// Renders the help for a clap command using outstanding.
pub fn render_help(cmd: &Command, config: Option<HelpConfig>) -> Result<String, outstanding::Error> {
    let config = config.unwrap_or_default();
    let template = config
        .template
        .as_deref()
        .unwrap_or(include_str!("help_template.txt"));

    let theme = config.theme.unwrap_or_else(default_help_theme);
    let mode = config.output_mode.unwrap_or(OutputMode::Auto);

    let data = extract_help_data(cmd);

    render_with_output(template, &data, ThemeChoice::from(&theme), mode)
}

/// Renders the help for a clap command with topics in a "Learn More" section.
pub fn render_help_with_topics(cmd: &Command, registry: &TopicRegistry, config: Option<HelpConfig>) -> Result<String, outstanding::Error> {
    let config = config.unwrap_or_default();
    let template = config
        .template
        .as_deref()
        .unwrap_or(include_str!("help_template.txt"));

    let theme = config.theme.unwrap_or_else(default_help_theme);
    let mode = config.output_mode.unwrap_or(OutputMode::Auto);

    let data = extract_help_data_with_topics(cmd, registry);

    render_with_output(template, &data, ThemeChoice::from(&theme), mode)
}

// ============================================================================
// HELP DATA EXTRACTION
// ============================================================================

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
    commands: Vec<T>,
    options: Vec<T>,
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

#[derive(Serialize)]
struct TopicListItem {
    name: String,
    title: String,
    padding: String,
}

fn extract_help_data(cmd: &Command) -> HelpData {
    let name = cmd.get_name().to_string();
    let about = cmd.get_about().map(|s| s.to_string()).unwrap_or_default();
    let usage = cmd.clone().render_usage().to_string()
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
        examples: String::new(),
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

// ============================================================================
// BACKWARDS COMPATIBILITY (deprecated)
// ============================================================================

/// Alias for Outstanding (deprecated, use Outstanding instead)
#[deprecated(since = "0.4.0", note = "Use Outstanding instead")]
pub type TopicHelper = Outstanding;

/// Alias for OutstandingBuilder (deprecated, use OutstandingBuilder instead)
#[deprecated(since = "0.4.0", note = "Use OutstandingBuilder instead")]
pub type TopicHelperBuilder = OutstandingBuilder;

/// Alias for HelpResult (deprecated, use HelpResult instead)
#[deprecated(since = "0.4.0", note = "Use HelpResult instead")]
pub type TopicHelpResult = HelpResult;

/// Alias for HelpConfig (deprecated, use HelpConfig instead)
#[deprecated(since = "0.4.0", note = "Use HelpConfig instead")]
pub type Config = HelpConfig;

/// Runs a clap command with styled help output.
///
/// This is the simplest entry point for basic CLIs without topics.
pub fn run(cmd: Command) -> clap::ArgMatches {
    Outstanding::run(cmd)
}

/// Like `run`, but takes arguments from an iterator.
pub fn run_from<I, T>(cmd: Command, itr: I) -> clap::ArgMatches
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    Outstanding::new().run_from(cmd, itr)
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

    #[test]
    fn test_output_flag_enabled_by_default() {
        let outstanding = Outstanding::new();
        assert!(outstanding.output_flag.is_some());
        assert_eq!(outstanding.output_flag.as_deref(), Some("output"));
    }

    #[test]
    fn test_builder_output_flag_enabled_by_default() {
        let outstanding = Outstanding::builder().build();
        assert!(outstanding.output_flag.is_some());
        assert_eq!(outstanding.output_flag.as_deref(), Some("output"));
    }

    #[test]
    fn test_no_output_flag() {
        let outstanding = Outstanding::builder()
            .no_output_flag()
            .build();
        assert!(outstanding.output_flag.is_none());
    }

    #[test]
    fn test_custom_output_flag_name() {
        let outstanding = Outstanding::builder()
            .output_flag(Some("format"))
            .build();
        assert_eq!(outstanding.output_flag.as_deref(), Some("format"));
    }

    // ==================== Router Tests ====================

    #[test]
    fn test_command_registration() {
        use serde_json::json;

        let builder = Outstanding::builder()
            .command("list", |_m, _ctx| {
                CommandResult::Ok(json!({"items": ["a", "b"]}))
            }, "Items: {{ items }}");

        assert!(builder.commands.contains_key("list"));
    }

    #[test]
    fn test_dispatch_to_handler() {
        use serde_json::json;

        let builder = Outstanding::builder()
            .command("list", |_m, _ctx| {
                CommandResult::Ok(json!({"count": 42}))
            }, "Count: {{ count }}");

        let cmd = Command::new("app")
            .subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Count: 42"));
    }

    #[test]
    fn test_dispatch_unhandled_fallthrough() {
        use serde_json::json;

        let builder = Outstanding::builder()
            .command("list", |_m, _ctx| {
                CommandResult::Ok(json!({}))
            }, "");

        let cmd = Command::new("app")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("other"));

        let matches = cmd.try_get_matches_from(["app", "other"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(!result.is_handled());
        assert!(result.matches().is_some());
    }

    #[test]
    fn test_dispatch_json_output() {
        use serde_json::json;

        let builder = Outstanding::builder()
            .command("list", |_m, _ctx| {
                CommandResult::Ok(json!({"name": "test", "value": 123}))
            }, "{{ name }}: {{ value }}");

        let cmd = Command::new("app")
            .subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Json);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("\"name\": \"test\""));
        assert!(output.contains("\"value\": 123"));
    }

    #[test]
    fn test_dispatch_nested_command() {
        use serde_json::json;

        let builder = Outstanding::builder()
            .command("config.get", |_m, _ctx| {
                CommandResult::Ok(json!({"key": "value"}))
            }, "{{ key }}");

        let cmd = Command::new("app")
            .subcommand(
                Command::new("config")
                    .subcommand(Command::new("get"))
            );

        let matches = cmd.try_get_matches_from(["app", "config", "get"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("value"));
    }

    #[test]
    fn test_dispatch_silent_result() {
        let builder = Outstanding::builder()
            .command("quiet", |_m, _ctx| {
                CommandResult::<()>::Silent
            }, "");

        let cmd = Command::new("app")
            .subcommand(Command::new("quiet"));

        let matches = cmd.try_get_matches_from(["app", "quiet"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some(""));
    }

    #[test]
    fn test_dispatch_error_result() {
        let builder = Outstanding::builder()
            .command("fail", |_m, _ctx| {
                CommandResult::<()>::Err(anyhow::anyhow!("something went wrong"))
            }, "");

        let cmd = Command::new("app")
            .subcommand(Command::new("fail"));

        let matches = cmd.try_get_matches_from(["app", "fail"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("Error:"));
        assert!(output.contains("something went wrong"));
    }

    #[test]
    fn test_extract_command_path() {
        let cmd = Command::new("app")
            .subcommand(
                Command::new("config")
                    .subcommand(Command::new("get"))
            );

        let matches = cmd.try_get_matches_from(["app", "config", "get"]).unwrap();
        let path = extract_command_path(&matches);

        assert_eq!(path, vec!["config", "get"]);
    }

    #[test]
    fn test_extract_command_path_single() {
        let cmd = Command::new("app")
            .subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let path = extract_command_path(&matches);

        assert_eq!(path, vec!["list"]);
    }

    #[test]
    fn test_extract_command_path_empty() {
        let cmd = Command::new("app");

        let matches = cmd.try_get_matches_from(["app"]).unwrap();
        let path = extract_command_path(&matches);

        assert!(path.is_empty());
    }

    #[test]
    fn test_dispatch_from_basic() {
        use serde_json::json;

        let builder = Outstanding::builder()
            .command("list", |_m, _ctx| {
                CommandResult::Ok(json!({"items": ["a", "b"]}))
            }, "Items: {{ items }}");

        let cmd = Command::new("app")
            .subcommand(Command::new("list"));

        let result = builder.dispatch_from(cmd, ["app", "list"]);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Items: [\"a\", \"b\"]"));
    }

    #[test]
    fn test_dispatch_from_with_json_flag() {
        use serde_json::json;

        let builder = Outstanding::builder()
            .command("list", |_m, _ctx| {
                CommandResult::Ok(json!({"count": 5}))
            }, "Count: {{ count }}");

        let cmd = Command::new("app")
            .subcommand(Command::new("list"));

        let result = builder.dispatch_from(cmd, ["app", "--output=json", "list"]);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("\"count\": 5"));
    }

    #[test]
    fn test_dispatch_from_unhandled() {
        use serde_json::json;

        let builder = Outstanding::builder()
            .command("list", |_m, _ctx| {
                CommandResult::Ok(json!({}))
            }, "");

        let cmd = Command::new("app")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("other"));

        let result = builder.dispatch_from(cmd, ["app", "other"]);

        assert!(!result.is_handled());
    }
}
