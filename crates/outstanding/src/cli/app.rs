//! App struct and implementation for CLI integration.
//!
//! This module provides the [`App`] type which is the main entry point
//! for outstanding-clap integration.

use crate::setup::SetupError;
use crate::topics::{
    display_with_pager, render_topic, render_topics_list, TopicRegistry, TopicRenderConfig,
};
use crate::TemplateRegistry;
use crate::{render_auto, OutputMode, Theme};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::Serialize;
use std::collections::HashMap;

use super::help::{render_help, render_help_with_topics, HelpConfig};
use super::result::HelpResult;
use crate::cli::handler::{CommandContext, HandlerResult, Output as HandlerOutput};
use crate::cli::hooks::{HookError, Hooks, RenderedOutput};

/// Gets the current terminal width, or None if not available.
pub(crate) fn get_terminal_width() -> Option<usize> {
    terminal_size::terminal_size().map(|(w, _)| w.0 as usize)
}

/// Main entry point for outstanding-clap integration.
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
/// use outstanding::cli::App;
/// use outstanding::OutputMode;
///
/// let app = App::builder()
///     .templates(embed_templates!("src/templates"))
///     .styles(embed_styles!("src/styles"))
///     .build()?;
///
/// let output = app.render("list", &data, OutputMode::Term)?;
/// ```
pub struct App {
    pub(crate) registry: TopicRegistry,
    pub(crate) output_flag: Option<String>,
    pub(crate) output_file_flag: Option<String>,
    pub(crate) output_mode: OutputMode,
    pub(crate) theme: Option<Theme>,
    pub(crate) command_hooks: HashMap<String, Hooks>,
    /// Template registry for embedded templates (None means use file-based resolution)
    pub(crate) template_registry: Option<TemplateRegistry>,
    /// Stylesheet registry for accessing themes
    pub(crate) stylesheet_registry: Option<crate::StylesheetRegistry>,
}

impl App {
    /// Creates a new App instance with default settings.
    ///
    /// By default:
    /// - `--output` flag is enabled
    /// - No topics are loaded
    /// - Default theme is used
    /// - No hooks are registered
    pub fn new() -> Self {
        Self {
            registry: TopicRegistry::new(),
            output_flag: Some("output".to_string()), // Enabled by default
            output_file_flag: Some("output-file-path".to_string()),
            output_mode: OutputMode::Auto,
            theme: None,
            command_hooks: HashMap::new(),
            template_registry: None,
            stylesheet_registry: None,
        }
    }

    /// Creates a new App instance with a pre-configured topic registry.
    pub fn with_registry(registry: TopicRegistry) -> Self {
        Self {
            registry,
            output_flag: Some("output".to_string()),
            output_file_flag: Some("output-file-path".to_string()),
            output_mode: OutputMode::Auto,
            theme: None,
            command_hooks: HashMap::new(),
            template_registry: None,
            stylesheet_registry: None,
        }
    }

    /// Creates a new builder for constructing an App instance.
    pub fn builder() -> super::AppBuilder {
        super::AppBuilder::new()
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

    /// Returns the hooks registered for a specific command path.
    pub fn get_hooks(&self, path: &str) -> Option<&Hooks> {
        self.command_hooks.get(path)
    }

    /// Returns the default theme, if configured.
    pub fn theme(&self) -> Option<&Theme> {
        self.theme.as_ref()
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
        self.stylesheet_registry
            .as_mut()
            .ok_or_else(|| SetupError::Config("No stylesheet registry configured".into()))?
            .get(name)
            .map_err(|_| SetupError::ThemeNotFound(name.to_string()))
    }

    /// Returns the names of all available templates.
    ///
    /// Returns an empty iterator if no template registry is configured.
    pub fn template_names(&self) -> impl Iterator<Item = &str> {
        self.template_registry
            .as_ref()
            .map(|r| r.names())
            .into_iter()
            .flatten()
    }

    /// Returns the names of all available themes.
    ///
    /// Returns an empty vector if no stylesheet registry is configured.
    pub fn theme_names(&self) -> Vec<String> {
        self.stylesheet_registry
            .as_ref()
            .map(|r| r.names().map(String::from).collect())
            .unwrap_or_default()
    }

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
        // For JSON/YAML/XML/CSV modes, serialize directly
        if mode.is_structured() {
            return self.serialize_data(data, mode);
        }

        // Get template registry
        let registry = self
            .template_registry
            .as_ref()
            .ok_or_else(|| SetupError::Config("No template registry configured".into()))?;

        // Build MiniJinja environment with all templates
        let mut env = minijinja::Environment::new();
        crate::rendering::template::filters::register_filters(&mut env);

        for name in registry.names() {
            if let Ok(content) = registry.get_content(name) {
                env.add_template_owned(name.to_string(), content)
                    .map_err(|e| SetupError::Template(e.to_string()))?;
            }
        }

        let tmpl = env
            .get_template(template)
            .map_err(|e| SetupError::Template(e.to_string()))?;
        tmpl.render(data)
            .map_err(|e| SetupError::Template(e.to_string()))
    }

    /// Serializes data to structured format (JSON, YAML, XML, CSV).
    fn serialize_data<T: Serialize>(
        &self,
        data: &T,
        mode: OutputMode,
    ) -> Result<String, SetupError> {
        match mode {
            OutputMode::Json => {
                serde_json::to_string_pretty(data).map_err(|e| SetupError::Template(e.to_string()))
            }
            OutputMode::Yaml => {
                serde_yaml::to_string(data).map_err(|e| SetupError::Template(e.to_string()))
            }
            OutputMode::Xml => {
                quick_xml::se::to_string(data).map_err(|e| SetupError::Template(e.to_string()))
            }
            OutputMode::Csv => {
                let value =
                    serde_json::to_value(data).map_err(|e| SetupError::Template(e.to_string()))?;
                let (headers, rows) = crate::util::flatten_json_for_csv(&value);

                let mut wtr = csv::Writer::from_writer(Vec::new());
                wtr.write_record(&headers)
                    .map_err(|e| SetupError::Template(e.to_string()))?;
                for row in rows {
                    wtr.write_record(&row)
                        .map_err(|e| SetupError::Template(e.to_string()))?;
                }
                let bytes = wtr
                    .into_inner()
                    .map_err(|e| SetupError::Template(e.to_string()))?;
                String::from_utf8(bytes).map_err(|e| SetupError::Template(e.to_string()))
            }
            _ => Err(SetupError::Config(format!(
                "serialize_data called with non-structured mode: {:?}",
                mode
            ))),
        }
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
    /// use outstanding::cli::{App, Hooks, HandlerResult, Output, RenderedOutput};
    ///
    /// let outstanding = App::builder()
    ///     .hooks("list", Hooks::new()
    ///         .post_output(|_ctx, output| {
    ///             // Copy to clipboard
    ///             Ok(output)
    ///         }))
    ///     .build();
    ///
    /// let matches = outstanding.parse_with(cmd);
    ///
    /// match matches.subcommand() {
    ///     Some(("list", sub_m)) => {
    ///         // Hooks are applied automatically
    ///         match outstanding.run_command("list", sub_m, |m, ctx| {
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
            output_mode: self.output_mode,
            command_path: path.split('.').map(String::from).collect(),
        };

        let hooks = self.command_hooks.get(path);

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
                let theme = self.theme.clone().unwrap_or_default();
                match render_auto(template, &json_data, &theme, self.output_mode) {
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

    /// Prepares the command for outstanding integration.
    ///
    /// - Disables default help subcommand
    /// - Adds custom `help` subcommand with topic support
    /// - Adds `--output` flag if enabled
    pub fn augment_command(&self, cmd: Command) -> Command {
        let mut cmd = cmd.disable_help_subcommand(true).subcommand(
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

        // Add output flag if enabled
        if let Some(ref flag_name) = self.output_flag {
            let flag: &'static str = Box::leak(flag_name.clone().into_boxed_str());
            cmd = cmd.arg(
                Arg::new("_output_mode")
                    .long(flag)
                    .value_name("MODE")
                    .global(true)
                    .value_parser([
                        "auto",
                        "term",
                        "text",
                        "term-debug",
                        "json",
                        "yaml",
                        "xml",
                        "csv",
                    ])
                    .default_value("auto")
                    .help("Output mode: auto, term, text, term-debug, json, yaml, xml, or csv"),
            );
        }

        // Add output file flag if enabled
        if let Some(ref flag_name) = self.output_file_flag {
            let flag: &'static str = Box::leak(flag_name.clone().into_boxed_str());
            cmd = cmd.arg(
                Arg::new("_output_file_path")
                    .long(flag)
                    .value_name("PATH")
                    .global(true)
                    .action(ArgAction::Set)
                    .help("Write output to file instead of stdout"),
            );
        }

        cmd
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

        // Extract output mode if the flag was configured
        let output_mode = if self.output_flag.is_some() {
            match matches
                .get_one::<String>("_output_mode")
                .map(|s| s.as_str())
            {
                Some("term") => OutputMode::Term,
                Some("text") => OutputMode::Text,
                Some("term-debug") => OutputMode::TermDebug,
                Some("json") => OutputMode::Json,
                Some("yaml") => OutputMode::Yaml,
                Some("xml") => OutputMode::Xml,
                Some("csv") => OutputMode::Csv,
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
        let outstanding = App::new();
        assert!(outstanding.output_flag.is_some());
        assert_eq!(outstanding.output_flag.as_deref(), Some("output"));
    }
}
