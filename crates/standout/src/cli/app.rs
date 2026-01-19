//! App struct and implementation for CLI integration.
//!
//! This module provides the [`App`] type which is the main entry point
//! for standout-clap integration.

use crate::setup::SetupError;
use crate::topics::{
    display_with_pager, render_topic, render_topics_list, TopicRegistry, TopicRenderConfig,
};
use crate::{render_auto, OutputMode, Theme};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::Serialize;

use super::core::AppCore;
use super::dispatch::{
    extract_command_path, get_deepest_matches, has_subcommand, insert_default_command,
    DispatchOutput, Dispatchable,
};
use super::help::{render_help, render_help_with_topics, HelpConfig};
use super::hooks::Hooks;
use super::mode::{HandlerMode, ThreadSafe};
use super::result::HelpResult;
use crate::cli::handler::{CommandContext, HandlerResult, Output as HandlerOutput, RunResult};
use crate::cli::hooks::{HookError, RenderedOutput};
use std::collections::HashMap;

use super::mode::Local;
use super::LocalAppBuilder;

/// Gets the current terminal width, or None if not available.
pub(crate) fn get_terminal_width() -> Option<usize> {
    terminal_size::terminal_size().map(|(w, _)| w.0 as usize)
}

/// Main entry point for standout-clap integration.
///
/// Handles help interception, output flag, topic rendering, command hooks,
/// and template rendering.
///
/// # Rendering Templates
///
/// When configured with templates and styles, `App` can render templates
/// directly:
///
/// ```rust,ignore
/// use standout::cli::App;
/// use standout::OutputMode;
///
/// let app = App::<standout::cli::ThreadSafe>::builder()
///     .templates(embed_templates!("src/templates"))
///     .styles(embed_styles!("src/styles"))
///     .build()?;
///
/// let output = app.render("list", &data, OutputMode::Term)?;
/// ```
pub struct App<M: HandlerMode = ThreadSafe> {
    /// Shared core configuration and functionality.
    pub(crate) core: AppCore,
    /// Topic registry for help topics (App-specific).
    pub(crate) registry: TopicRegistry,
    /// Registered command handlers.
    pub(crate) commands: HashMap<String, M::DispatchFn>,
}

impl App<ThreadSafe> {
    /// Creates a new builder for constructing an App instance.
    pub fn builder() -> super::AppBuilder {
        super::AppBuilder::new()
    }
}

impl App<Local> {
    /// Creates a new builder for constructing a LocalApp instance.
    pub fn builder() -> LocalAppBuilder {
        LocalAppBuilder::new()
    }
}

impl<M: HandlerMode> App<M> {
    /// Creates a new App instance with default settings.
    ///
    /// By default:
    /// - `--output` flag is enabled
    /// - No topics are loaded
    /// - Default theme is used
    /// - No hooks are registered
    pub fn new() -> Self {
        Self {
            core: AppCore::new(),
            registry: TopicRegistry::new(),
            commands: HashMap::new(),
        }
    }

    /// Creates a new App instance with a pre-configured topic registry.
    pub fn with_registry(registry: TopicRegistry) -> Self {
        Self {
            core: AppCore::new(),
            registry,
            commands: HashMap::new(),
        }
    }

    /// Returns a reference to the topic registry.
    pub fn registry(&self) -> &TopicRegistry {
        &self.registry
    }

    /// Returns a mutable reference to the topic registry.
    pub fn registry_mut(&mut self) -> &mut TopicRegistry {
        &mut self.registry
    }

    // =========================================================================
    // Delegated accessors (from AppCore)
    // =========================================================================

    /// Returns the current output mode.
    pub fn output_mode(&self) -> OutputMode {
        self.core.output_mode()
    }

    /// Returns the hooks registered for a specific command path.
    pub fn get_hooks(&self, path: &str) -> Option<&Hooks> {
        self.core.get_hooks(path)
    }

    /// Returns the default theme, if configured.
    pub fn theme(&self) -> Option<&Theme> {
        self.core.theme()
    }

    /// Gets a theme by name from the stylesheet registry.
    ///
    /// This allows using themes other than the default at runtime.
    ///
    /// # Errors
    ///
    /// Returns an error if no stylesheet registry is configured or if the theme
    /// is not found.
    pub fn get_theme(&mut self, name: &str) -> Result<Theme, SetupError> {
        self.core.get_theme(name)
    }

    /// Returns the names of all available templates.
    ///
    /// Returns an empty iterator if no template registry is configured.
    pub fn template_names(&self) -> impl Iterator<Item = &str> {
        self.core.template_names()
    }

    /// Returns the names of all available themes.
    ///
    /// Returns an empty vector if no stylesheet registry is configured.
    pub fn theme_names(&self) -> Vec<String> {
        self.core.theme_names()
    }

    // =========================================================================
    // Rendering (delegated to AppCore)
    // =========================================================================

    /// Renders a template with the given data.
    ///
    /// Uses the default theme configured at setup time.
    ///
    /// # Arguments
    ///
    /// * `template` - Template name (e.g., "list" for "list.j2")
    /// * `data` - Serializable data to render
    /// * `mode` - Output mode (Term, Text, Json, etc.)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No template registry is configured
    /// - The template is not found
    /// - Rendering fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let output = app.render("list", &items, OutputMode::Term)?;
    /// ```
    pub fn render<T: Serialize>(
        &self,
        template: &str,
        data: &T,
        mode: OutputMode,
    ) -> Result<String, SetupError> {
        self.core.render(template, data, mode)
    }

    /// Renders an inline template string with the given data.
    ///
    /// Unlike `render`, this takes the template content directly.
    /// Still supports `{% include %}` if a template registry is configured.
    pub fn render_inline<T: Serialize>(
        &self,
        template: &str,
        data: &T,
        mode: OutputMode,
    ) -> Result<String, SetupError> {
        self.core.render_inline(template, data, mode)
    }

    // =========================================================================
    // Dispatch
    // =========================================================================

    /// Dispatches to a registered handler if one matches the command path.
    pub fn dispatch(&self, matches: ArgMatches, output_mode: OutputMode) -> RunResult {
        let path = extract_command_path(&matches);
        let path_str = path.join(".");

        if let Some(dispatch) = self.commands.get(&path_str) {
            let ctx = CommandContext { command_path: path };

            let hooks = self.core.get_hooks(&path_str);

            // Run pre-dispatch hooks
            if let Some(hooks) = hooks {
                if let Err(e) = hooks.run_pre_dispatch(&matches, &ctx) {
                    return RunResult::Handled(format!("Hook error: {}", e));
                }
            }

            let sub_matches = get_deepest_matches(&matches);

            // Run the handler (output_mode passed separately as CommandContext is render-agnostic)
            let dispatch_output = match dispatch.dispatch(sub_matches, &ctx, hooks, output_mode) {
                Ok(output) => output,
                Err(e) => return RunResult::Handled(e),
            };

            // Convert to RenderedOutput for post-output hooks
            let output = match dispatch_output {
                DispatchOutput::Text(s) => RenderedOutput::Text(s),
                DispatchOutput::Binary(b, f) => RenderedOutput::Binary(b, f),
                DispatchOutput::Silent => RenderedOutput::Silent,
            };

            // Run post-output hooks
            let final_output = if let Some(hooks) = hooks {
                match hooks.run_post_output(&matches, &ctx, output) {
                    Ok(o) => o,
                    Err(e) => return RunResult::Handled(format!("Hook error: {}", e)),
                }
            } else {
                output
            };

            match final_output {
                RenderedOutput::Text(s) => RunResult::Handled(s),
                RenderedOutput::Binary(b, f) => RunResult::Binary(b, f),
                RenderedOutput::Silent => RunResult::Handled(String::new()),
            }
        } else {
            RunResult::NoMatch(matches)
        }
    }

    /// Parses arguments and dispatches to registered handlers.
    pub fn dispatch_from<I, T>(&self, cmd: Command, args: I) -> RunResult
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let args: Vec<String> = args
            .into_iter()
            .map(|a| a.into().to_string_lossy().into_owned())
            .collect();

        let augmented_cmd = self.core.augment_command(cmd.clone());

        let matches = match augmented_cmd.try_get_matches_from(&args) {
            Ok(m) => m,
            Err(e) => return RunResult::Handled(e.to_string()),
        };

        // Check if we need to insert default command
        let matches = if !has_subcommand(&matches) && self.core.default_command().is_some() {
            let default_cmd = self.core.default_command().unwrap();
            let new_args = insert_default_command(args, default_cmd);

            let augmented_cmd = self.core.augment_command(cmd);
            match augmented_cmd.try_get_matches_from(&new_args) {
                Ok(m) => m,
                Err(e) => return RunResult::Handled(e.to_string()),
            }
        } else {
            matches
        };

        // Extract output mode using core
        let output_mode = self.core.extract_output_mode(&matches);

        self.dispatch(matches, output_mode)
    }

    /// Runs the CLI: parses arguments, dispatches to handlers, and prints output.
    ///
    /// # Returns
    ///
    /// - `true` if a handler processed and printed output
    /// - `false` if no handler matched
    pub fn run<I, T>(&self, cmd: Command, args: I) -> bool
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
                if let Err(e) = std::fs::write(&filename, &bytes) {
                    eprintln!("Error writing {}: {}", filename, e);
                } else {
                    eprintln!("Wrote {} bytes to {}", bytes.len(), filename);
                }
                true
            }
            RunResult::Silent => true, // Handler ran successfully, no output
            RunResult::NoMatch(_) => false,
        }
    }

    /// Runs the CLI and returns the rendered output as a string.
    pub fn run_to_string<I, T>(&self, cmd: Command, args: I) -> RunResult
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        self.dispatch_from(cmd, args)
    }

    /// Executes a command handler with hooks applied automatically.
    ///
    /// This is for the **regular API** - when you handle dispatch manually
    /// but still want to benefit from registered hooks.
    ///
    /// The method:
    /// 1. Runs pre-dispatch hooks (if any)
    /// 2. Calls your handler closure
    /// 3. Renders the result using the template
    /// 4. Runs post-output hooks (if any)
    /// 5. Returns the final output
    ///
    /// # Arguments
    ///
    /// * `path` - Command path for hook lookup (e.g., "list" or "config.get")
    /// * `matches` - The ArgMatches for the subcommand
    /// * `handler` - Your handler closure
    /// * `template` - MiniJinja template for rendering
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use standout::cli::{App, Hooks, HandlerResult, Output, RenderedOutput};
    ///
    /// let standout = App::builder()
    ///     .hooks("list", Hooks::new()
    ///         .post_output(|_ctx, output| {
    ///             // Copy to clipboard
    ///             Ok(output)
    ///         }))
    ///     .build();
    ///
    /// let matches = standout.parse_with(cmd);
    ///
    /// match matches.subcommand() {
    ///     Some(("list", sub_m)) => {
    ///         // Hooks are applied automatically
    ///         match standout.run_command("list", sub_m, |m, ctx| {
    ///             let items = fetch_items();
    ///             Ok(HandlerOutput::Render(ListOutput { items })
    ///         }, "{% for item in items %}{{ item }}\n{% endfor %}") {
    ///             Ok(output) => print!("{}", output),
    ///             Err(e) => eprintln!("Error: {}", e),
    ///         }
    ///     }
    ///     _ => {}
    /// }
    /// ```
    pub fn run_command<F, T>(
        &self,
        path: &str,
        matches: &ArgMatches,
        handler: F,
        template: &str,
    ) -> Result<RenderedOutput, HookError>
    where
        F: FnOnce(&ArgMatches, &CommandContext) -> HandlerResult<T>,
        T: Serialize,
    {
        let ctx = CommandContext {
            command_path: path.split('.').map(String::from).collect(),
        };

        let hooks = self.core.get_hooks(path);

        // Run pre-dispatch hooks
        if let Some(hooks) = hooks {
            hooks.run_pre_dispatch(matches, &ctx)?;
        }

        // Run handler
        let result = handler(matches, &ctx);

        // Convert result to RenderedOutput
        let output = match result {
            Ok(HandlerOutput::Render(data)) => {
                // Convert to serde_json::Value for post-dispatch hooks
                let mut json_data = serde_json::to_value(&data)
                    .map_err(|e| HookError::post_dispatch("Serialization error").with_source(e))?;

                // Run post-dispatch hooks if present
                if let Some(hooks) = hooks {
                    json_data = hooks.run_post_dispatch(matches, &ctx, json_data)?;
                }

                // Render the (potentially modified) data
                let theme = self.core.theme().cloned().unwrap_or_default();
                match render_auto(template, &json_data, &theme, self.core.output_mode()) {
                    Ok(rendered) => RenderedOutput::Text(rendered),
                    Err(e) => return Err(HookError::post_output("Render error").with_source(e)),
                }
            }
            Err(e) => {
                return Err(HookError::post_output("Handler error").with_source(e));
            }
            Ok(HandlerOutput::Silent) => RenderedOutput::Silent,
            Ok(HandlerOutput::Binary { data, filename }) => RenderedOutput::Binary(data, filename),
        };

        // Run post-output hooks
        if let Some(hooks) = hooks {
            hooks.run_post_output(matches, &ctx, output)
        } else {
            Ok(output)
        }
    }

    /// Prepares the command for standout integration.
    ///
    /// - Disables default help subcommand
    /// - Adds custom `help` subcommand with topic support
    /// - Adds `--output` flag if enabled
    pub fn augment_command(&self, cmd: Command) -> Command {
        // First add the help subcommand (App-specific, for topic support)
        let cmd = cmd.disable_help_subcommand(true).subcommand(
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
                ),
        );

        // Then delegate to core for output flags
        self.core.augment_command(cmd)
    }

    /// Parses CLI arguments and returns matches.
    ///
    /// This is the recommended entry point for parsing only. It:
    /// - Intercepts `help` subcommand and displays styled help
    /// - Handles pager display when `--page` is used
    /// - Exits on errors
    /// - Returns `ArgMatches` only for actual commands
    ///
    /// For executing command handlers and printing output, use `run()` instead.
    pub fn parse(cmd: Command) -> clap::ArgMatches {
        Self::new().parse_with(cmd)
    }

    /// Parses CLI arguments with this configured App instance.
    pub fn parse_with(&self, cmd: Command) -> clap::ArgMatches {
        self.parse_from(cmd, std::env::args())
    }

    /// Like `parse_with`, but takes arguments from an iterator.
    pub fn parse_from<I, T>(&self, cmd: Command, itr: I) -> clap::ArgMatches
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
    /// For most use cases, prefer `parse()` which handles help display automatically.
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

        // Extract output mode using core
        let output_mode = self.core.extract_output_mode(&matches);

        let config = HelpConfig {
            output_mode: Some(output_mode),
            theme: self.core.theme().cloned(),
            ..Default::default()
        };

        if let Some((name, sub_matches)) = matches.subcommand() {
            if name == "help" {
                let use_pager = sub_matches.get_flag("page");

                if let Some(topic_args) = sub_matches.get_many::<String>("topic") {
                    let keywords: Vec<_> = topic_args.map(|s| s.as_str()).collect();
                    if !keywords.is_empty() {
                        return self.handle_help_request(
                            &mut cmd,
                            &keywords,
                            use_pager,
                            Some(config),
                        );
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
    fn handle_help_request(
        &self,
        cmd: &mut Command,
        keywords: &[&str],
        use_pager: bool,
        config: Option<HelpConfig>,
    ) -> HelpResult {
        let sub_name = keywords[0];

        // 0. Check for "topics" - list all available topics
        if sub_name == "topics" {
            let topic_config = TopicRenderConfig {
                output_mode: config.as_ref().and_then(|c| c.output_mode),
                theme: config.as_ref().and_then(|c| c.theme.clone()),
                ..Default::default()
            };
            if let Ok(h) = render_topics_list(
                &self.registry,
                &format!("{} help", cmd.get_name()),
                Some(topic_config),
            ) {
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
            format!("The subcommand or topic '{}' wasn't recognized", sub_name),
        );
        HelpResult::Error(err)
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
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
    cmd.get_subcommands()
        .find(|s| s.get_name() == name || s.get_aliases().any(|a| a == name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_flag_enabled_by_default() {
        let standout = App::<ThreadSafe>::new();
        assert!(standout.core.output_flag.is_some());
        assert_eq!(standout.core.output_flag.as_deref(), Some("output"));
    }
}
