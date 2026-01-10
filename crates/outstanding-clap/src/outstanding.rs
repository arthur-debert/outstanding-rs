//! Main entry point types for outstanding-clap integration.
//!
//! This module provides [`Outstanding`] and [`OutstandingBuilder`] for integrating
//! outstanding with clap-based CLIs.

use clap::{Arg, ArgAction, ArgMatches, Command};
use outstanding::topics::{
    display_with_pager, render_topic, render_topics_list, Topic, TopicRegistry, TopicRenderConfig,
};
use outstanding::{render_or_serialize, OutputMode, Theme, ThemeChoice};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;

use crate::dispatch::{extract_command_path, get_deepest_matches, DispatchFn, DispatchOutput};
use crate::handler::{CommandContext, CommandResult, FnHandler, Handler, RunResult};
use crate::help::{render_help, render_help_with_topics, HelpConfig};
use crate::result::HelpResult;

/// Main entry point for outstanding-clap integration.
///
/// Handles help interception, output flag, and topic rendering.
pub struct Outstanding {
    pub(crate) registry: TopicRegistry,
    pub(crate) output_flag: Option<String>,
    pub(crate) output_mode: OutputMode,
    pub(crate) theme: Option<Theme>,
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
        self.command_handler(path, FnHandler::new(handler), template)
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

#[cfg(test)]
mod tests {
    use super::*;

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
