//! Main entry point types for CLI integration.
//!
//! This module provides [`App`] and [`AppBuilder`] for integrating
//! outstanding with clap-based CLIs.

use crate::context::{ContextProvider, ContextRegistry, RenderContext};
use crate::render::TemplateRegistry;
use crate::setup::SetupError;
use crate::topics::{
    display_with_pager, render_topic, render_topics_list, Topic, TopicRegistry, TopicRenderConfig,
};
use crate::{
    render_auto, render_auto_with_context, write_binary_output, write_output, EmbeddedStyles,
    EmbeddedTemplates, OutputDestination, OutputMode, Theme,
};
use clap::{Arg, ArgAction, ArgMatches, Command};
use minijinja::Value;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use super::dispatch::{extract_command_path, get_deepest_matches, DispatchFn, DispatchOutput};
use super::group::{CommandConfig, GroupBuilder, GroupEntry};
use super::help::{render_help, render_help_with_topics, HelpConfig};
use super::result::HelpResult;
use crate::cli::handler::{
    CommandContext, FnHandler, Handler, HandlerResult, Output as HandlerOutput, RunResult,
};
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
    pub(crate) stylesheet_registry: Option<crate::stylesheet::StylesheetRegistry>,
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
    pub fn builder() -> AppBuilder {
        AppBuilder::new()
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
        crate::render::filters::register_filters(&mut env);

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

/// Builder for constructing an App instance.
///
/// # Example
///
/// ```rust
/// use outstanding::cli::App;
///
/// let outstanding = App::builder()
///     .topics_dir("docs/topics")
///     .output_flag(Some("format"))
///     .build();
/// ```
///
/// # Context Injection
///
/// You can inject additional context objects into templates using `.context()` for
/// static values and `.context_fn()` for dynamic values computed at render time:
///
/// ```rust,ignore
/// use outstanding::cli::App;
/// use crate::context::RenderContext;
/// use minijinja::Value;
///
/// App::builder()
///     // Static context
///     .context("app_version", Value::from("1.0.0"))
///
///     // Dynamic context (computed at render time)
///     .context_fn("terminal", |ctx: &RenderContext| {
///         Value::from_iter([
///             ("width", Value::from(ctx.terminal_width.unwrap_or(80))),
///             ("is_tty", Value::from(ctx.output_mode == outstanding::OutputMode::Term)),
///         ])
///     })
///     .command("list", handler, "Width: {{ terminal.width }}")
///     .build()?
///     .run(cmd, args);
/// ```
pub struct AppBuilder {
    registry: TopicRegistry,
    output_flag: Option<String>,
    output_file_flag: Option<String>,
    theme: Option<Theme>,
    /// Stylesheet registry (built from embedded styles)
    stylesheet_registry: Option<crate::stylesheet::StylesheetRegistry>,
    /// Template registry (built from embedded templates)
    template_registry: Option<TemplateRegistry>,
    default_theme_name: Option<String>,
    commands: HashMap<String, DispatchFn>,
    command_hooks: HashMap<String, Hooks>,
    context_registry: ContextRegistry,
    template_dir: Option<PathBuf>,
    template_ext: String,
}

impl Default for AppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AppBuilder {
    /// Creates a new builder with default settings.
    ///
    /// By default, the `--output` flag is enabled and no hooks are registered.
    pub fn new() -> Self {
        Self {
            registry: TopicRegistry::new(),
            output_flag: Some("output".to_string()), // Enabled by default
            output_file_flag: Some("output-file-path".to_string()),
            theme: None,
            stylesheet_registry: None,
            template_registry: None,
            default_theme_name: None,
            commands: HashMap::new(),
            command_hooks: HashMap::new(),
            context_registry: ContextRegistry::new(),
            template_dir: None,
            template_ext: ".j2".to_string(),
        }
    }

    /// Adds a static context value available to all templates.
    ///
    /// Static context values are created once and reused for all renders.
    /// Use this for values that don't change between renders (app version,
    /// configuration, etc.).
    ///
    /// # Arguments
    ///
    /// * `name` - The name to use in templates (e.g., "app" for `{{ app.version }}`)
    /// * `value` - The value to inject (must be convertible to minijinja::Value)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use outstanding::cli::App;
    /// use minijinja::Value;
    ///
    /// App::builder()
    ///     .context("app_version", Value::from("1.0.0"))
    ///     .context("config", Value::from_iter([
    ///         ("debug", Value::from(true)),
    ///         ("max_items", Value::from(100)),
    ///     ]))
    ///     .command("info", handler, "Version: {{ app_version }}, Debug: {{ config.debug }}")
    /// ```
    pub fn context(mut self, name: impl Into<String>, value: Value) -> Self {
        self.context_registry.add_static(name, value);
        self
    }

    /// Adds a dynamic context provider that computes values at render time.
    ///
    /// Dynamic providers receive a [`RenderContext`] with information about the
    /// current render environment (terminal width, output mode, theme, handler data).
    /// Use this for values that depend on runtime conditions.
    ///
    /// # Arguments
    ///
    /// * `name` - The name to use in templates
    /// * `provider` - A closure that receives `&RenderContext` and returns a `Value`
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use outstanding::cli::App;
    /// use crate::context::RenderContext;
    /// use minijinja::Value;
    ///
    /// App::builder()
    ///     // Provide terminal info
    ///     .context_fn("terminal", |ctx: &RenderContext| {
    ///         Value::from_iter([
    ///             ("width", Value::from(ctx.terminal_width.unwrap_or(80))),
    ///             ("is_tty", Value::from(ctx.output_mode == outstanding::OutputMode::Term)),
    ///         ])
    ///     })
    ///
    ///     // Provide a table formatter with resolved width
    ///     .context_fn("table", |ctx: &RenderContext| {
    ///         let formatter = TableFormatter::new(&spec, ctx.terminal_width.unwrap_or(80));
    ///         Value::from_object(formatter)
    ///     })
    ///
    ///     .command("list", handler, "{% for item in items %}{{ table.row([item.name, item.value]) }}\n{% endfor %}")
    /// ```
    pub fn context_fn<P>(mut self, name: impl Into<String>, provider: P) -> Self
    where
        P: ContextProvider + 'static,
    {
        self.context_registry.add_provider(name, provider);
        self
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

    /// Sets embedded templates from `embed_templates!` macro.
    ///
    /// Use this to load templates from embedded sources. In debug mode,
    /// if the source path exists, templates are loaded from disk for hot-reload.
    /// In release mode, embedded content is used.
    ///
    /// Templates set here will be used to resolve template paths when registering
    /// commands. Call this method *before* `.commands()` or `.group()` to ensure
    /// templates are available for resolution.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use outstanding::{embed_templates, cli::App};
    ///
    /// App::builder()
    ///     .templates(embed_templates!("src/templates"))
    ///     .styles(embed_styles!("src/styles"))
    ///     .default_theme("default")
    ///     .commands(Commands::dispatch_config())
    ///     .build()?
    ///     .run(cmd, args);
    /// ```
    pub fn templates(mut self, templates: EmbeddedTemplates) -> Self {
        self.template_registry = Some(TemplateRegistry::from(templates));
        self
    }

    /// Sets embedded styles from `embed_styles!` macro.
    ///
    /// Use this to load themes from embedded YAML stylesheets. Combined with
    /// `default_theme()` to select which theme to use.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use crate::{embed_styles};
    /// use outstanding::cli::App;
    ///
    /// App::builder()
    ///     .styles(embed_styles!("src/styles"))
    ///     .default_theme("dark")
    ///     .command("list", handler, template)
    ///     .build()?
    ///     .run(cmd, args);
    /// ```
    pub fn styles(mut self, styles: EmbeddedStyles) -> Self {
        self.stylesheet_registry = Some(crate::stylesheet::StylesheetRegistry::from(styles));
        self
    }

    /// Adds a stylesheet directory for runtime loading.
    ///
    /// Stylesheets from directories are loaded immediately and merged with any
    /// embedded stylesheets. Directory styles take precedence over embedded
    /// styles with the same name.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// App::builder()
    ///     .styles(embed_styles!("src/styles"))
    ///     .styles_dir("~/.myapp/themes")  // User overrides
    /// ```
    pub fn styles_dir<P: AsRef<std::path::Path>>(mut self, path: P) -> Self {
        let registry = self
            .stylesheet_registry
            .get_or_insert_with(crate::stylesheet::StylesheetRegistry::new);
        let _ = registry.add_dir(path);
        self
    }

    /// Sets the default theme name when using embedded styles.
    ///
    /// If not specified, "default" is used.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// App::builder()
    ///     .styles(embed_styles!("src/styles"))
    ///     .default_theme("dark")
    /// ```
    pub fn default_theme(mut self, name: &str) -> Self {
        self.default_theme_name = Some(name.to_string());
        self
    }

    /// Sets the base directory for convention-based template resolution.
    ///
    /// When a command is registered without an explicit template, the template
    /// path is derived from the command path:
    /// - Command `db.migrate` → `{template_dir}/db/migrate{template_ext}`
    ///
    /// This is for file-based template loading at render time. For embedded
    /// templates, use `.templates()` instead.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// App::builder()
    ///     .template_dir("templates")
    ///     .group("db", |g| g
    ///         .command("migrate", handler))  // uses "templates/db/migrate.j2"
    /// ```
    pub fn template_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.template_dir = Some(path.into());
        self
    }

    /// Adds a template directory to the registry for runtime loading.
    ///
    /// Templates from directories are loaded immediately and merged with any
    /// embedded templates. Directory templates take precedence over embedded
    /// templates with the same name.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// App::builder()
    ///     .templates(embed_templates!("src/templates"))
    ///     .templates_dir("~/.myapp/templates")  // User overrides
    /// ```
    pub fn templates_dir<P: AsRef<std::path::Path>>(mut self, path: P) -> Self {
        let registry = self
            .template_registry
            .get_or_insert_with(TemplateRegistry::new);
        let _ = registry.add_template_dir(path);
        self
    }

    /// Sets the file extension for convention-based template resolution.
    ///
    /// Default is `.j2`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// App::builder()
    ///     .template_dir("templates")
    ///     .template_ext(".jinja2")
    ///     .group("db", |g| g
    ///         .command("migrate", handler))  // uses "templates/db/migrate.jinja2"
    /// ```
    pub fn template_ext(mut self, ext: impl Into<String>) -> Self {
        self.template_ext = ext.into();
        self
    }

    /// Registers commands from a dispatch closure (used by the `dispatch!` macro).
    ///
    /// This method accepts a closure that configures a [`GroupBuilder`] with commands
    /// and nested groups. It's typically used with the [`dispatch!`] macro:
    ///
    /// ```rust,ignore
    /// use outstanding::cli::{dispatch, App};
    ///
    /// App::builder()
    ///     .template_dir("templates")
    ///     .commands(dispatch! {
    ///         db: {
    ///             migrate => db::migrate,
    ///             backup => db::backup,
    ///         },
    ///         version => version,
    ///     })
    ///     .build()
    /// ```
    ///
    /// The closure receives an empty [`GroupBuilder`] and should return it with
    /// commands added. Each top-level entry becomes a command or group.
    pub fn commands<F>(mut self, configure: F) -> Self
    where
        F: FnOnce(GroupBuilder) -> GroupBuilder,
    {
        let builder = configure(GroupBuilder::new());

        // Register all entries from the group builder
        for (name, entry) in builder.entries {
            match entry {
                GroupEntry::Command { mut handler } => {
                    let template = handler
                        .template()
                        .map(String::from)
                        .unwrap_or_else(|| self.resolve_template(&name));

                    if let Some(hooks) = handler.take_hooks() {
                        self.command_hooks.insert(name.clone(), hooks);
                    }

                    let dispatch = handler.register(&name, template, self.context_registry.clone());
                    self.commands.insert(name, dispatch);
                }
                GroupEntry::Group { builder: nested } => {
                    self.register_group(&name, nested);
                }
            }
        }

        self
    }

    /// Creates a command group for organizing related commands.
    ///
    /// Groups allow nested command hierarchies with a fluent API:
    ///
    /// ```rust,ignore
    /// App::builder()
    ///     .template_dir("templates")
    ///     .group("db", |g| g
    ///         .command("migrate", db::migrate)
    ///         .command("backup", db::backup))
    ///     .group("app", |g| g
    ///         .command("start", app::start)
    ///         .group("config", |g| g
    ///             .command("get", app::config_get)
    ///             .command("set", app::config_set)))
    ///     .build()
    /// ```
    ///
    /// Commands within groups use dot notation for paths:
    /// - `db.migrate`, `db.backup`
    /// - `app.start`, `app.config.get`, `app.config.set`
    pub fn group<F>(mut self, name: &str, configure: F) -> Self
    where
        F: FnOnce(GroupBuilder) -> GroupBuilder,
    {
        let builder = configure(GroupBuilder::new());
        self.register_group(name, builder);
        self
    }

    /// Registers a command handler with inline configuration.
    ///
    /// Use this to set explicit template or hooks without using `.hooks()` separately:
    ///
    /// ```rust,ignore
    /// App::builder()
    ///     .command_with("list", handler, |cfg| cfg
    ///         .template("custom/list.j2")
    ///         .pre_dispatch(validate_auth)
    ///         .post_output(copy_to_clipboard))
    ///     .build()
    /// ```
    pub fn command_with<F, T, C>(mut self, path: &str, handler: F, configure: C) -> Self
    where
        F: Fn(&ArgMatches, &CommandContext) -> HandlerResult<T> + Send + Sync + 'static,
        T: Serialize + Send + Sync + 'static,
        C: FnOnce(CommandConfig<FnHandler<F, T>>) -> CommandConfig<FnHandler<F, T>>,
    {
        let config = CommandConfig::new(FnHandler::new(handler));
        let config = configure(config);

        // Resolve template
        let template = config
            .template
            .clone()
            .unwrap_or_else(|| self.resolve_template(path));

        // Register hooks if present
        if let Some(hooks) = config.hooks {
            self.command_hooks.insert(path.to_string(), hooks);
        }

        // Register the command
        let context_registry = self.context_registry.clone();
        let dispatch: DispatchFn = Arc::new(
            move |matches: &ArgMatches, ctx: &CommandContext, hooks: Option<&Hooks>| {
                let result = config.handler.handle(matches, ctx);

                match result {
                    Ok(HandlerOutput::Render(data)) => {
                        let mut json_data = serde_json::to_value(&data)
                            .map_err(|e| format!("Failed to serialize handler result: {}", e))?;

                        if let Some(hooks) = hooks {
                            json_data = hooks
                                .run_post_dispatch(matches, ctx, json_data)
                                .map_err(|e| format!("Hook error: {}", e))?;
                        }

                        let theme = Theme::new();
                        let render_ctx = RenderContext::new(
                            ctx.output_mode,
                            get_terminal_width(),
                            &theme,
                            &json_data,
                        );

                        let output = render_auto_with_context(
                            &template,
                            &json_data,
                            &theme,
                            ctx.output_mode,
                            &context_registry,
                            &render_ctx,
                        )
                        .map_err(|e| e.to_string())?;
                        Ok(DispatchOutput::Text(output))
                    }
                    Err(e) => Err(format!("Error: {}", e)),
                    Ok(HandlerOutput::Silent) => Ok(DispatchOutput::Silent),
                    Ok(HandlerOutput::Binary { data, filename }) => {
                        Ok(DispatchOutput::Binary(data, filename))
                    }
                }
            },
        );

        self.commands.insert(path.to_string(), dispatch);
        self
    }

    /// Helper to register a group's commands recursively.
    fn register_group(&mut self, prefix: &str, builder: GroupBuilder) {
        for (name, entry) in builder.entries {
            let path = format!("{}.{}", prefix, name);

            match entry {
                GroupEntry::Command { mut handler } => {
                    // Resolve template
                    let template = handler
                        .template()
                        .map(String::from)
                        .unwrap_or_else(|| self.resolve_template(&path));

                    // Extract and register hooks
                    if let Some(hooks) = handler.take_hooks() {
                        self.command_hooks.insert(path.clone(), hooks);
                    }

                    // Register the dispatch function
                    let dispatch = handler.register(&path, template, self.context_registry.clone());
                    self.commands.insert(path, dispatch);
                }
                GroupEntry::Group { builder: nested } => {
                    self.register_group(&path, nested);
                }
            }
        }
    }

    /// Resolves a template from a command path using conventions.
    ///
    /// Resolution order:
    /// 1. If template_registry is set, look up by command path (e.g., "db/migrate.j2")
    /// 2. If template_dir is set, return the file path for runtime loading
    /// 3. Otherwise return empty string (JSON serialization fallback)
    fn resolve_template(&self, command_path: &str) -> String {
        let file_path = command_path.replace('.', "/");
        let template_name = format!("{}{}", file_path, self.template_ext);

        // First, try to get content from embedded templates
        if let Some(ref registry) = self.template_registry {
            if let Ok(content) = registry.get_content(&template_name) {
                return content;
            }
        }

        // Fall back to file path if template_dir is configured
        if let Some(ref dir) = self.template_dir {
            return format!("{}/{}", dir.display(), template_name);
        }

        // No template found - will use JSON serialization in structured modes
        String::new()
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

    /// Configures the name of the output file path flag.
    ///
    /// When set, an `--<flag>=<PATH>` option is added to all commands.
    ///
    /// Default flag name is "output-file-path".
    ///
    /// To disable the output file flag entirely, use `no_output_file_flag()`.
    pub fn output_file_flag(mut self, name: Option<&str>) -> Self {
        self.output_file_flag = Some(name.unwrap_or("output-file-path").to_string());
        self
    }

    /// Disables the output file flag entirely.
    pub fn no_output_file_flag(mut self) -> Self {
        self.output_file_flag = None;
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
    /// use outstanding::cli::{App, Output, HandlerResult};
    /// use serde::Serialize;
    ///
    /// #[derive(Serialize)]
    /// struct ListOutput { items: Vec<String> }
    ///
    /// App::builder()
    ///     .command("list", |_m, _ctx| -> HandlerResult<ListOutput> {
    ///         Ok(Output::Render(ListOutput { items: vec!["one".into()] }))
    ///     }, "{% for item in items %}{{ item }}\n{% endfor %}")
    ///     .parse(cmd);
    /// ```
    pub fn command<F, T>(self, path: &str, handler: F, template: &str) -> Self
    where
        F: Fn(&ArgMatches, &CommandContext) -> HandlerResult<T> + Send + Sync + 'static,
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
    /// use outstanding::cli::{App, Handler, HandlerResult, Output, CommandContext};
    /// use clap::ArgMatches;
    /// use serde::Serialize;
    ///
    /// struct ListHandler { db: Database }
    ///
    /// impl Handler for ListHandler {
    ///     type Output = Vec<Item>;
    ///     fn handle(&self, _m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<Self::Output> {
    ///         Ok(Output::Render(self.db.list()?))
    ///     }
    /// }
    ///
    /// App::builder()
    ///     .command_handler("list", ListHandler { db }, "{% for item in items %}...")
    ///     .parse(cmd);
    /// ```
    pub fn command_handler<H, T>(mut self, path: &str, handler: H, template: &str) -> Self
    where
        H: Handler<Output = T> + 'static,
        T: Serialize + 'static,
    {
        let template = template.to_string();
        let handler = Arc::new(handler);
        let context_registry = self.context_registry.clone();

        let dispatch: DispatchFn = Arc::new(
            move |matches: &ArgMatches, ctx: &CommandContext, hooks: Option<&Hooks>| {
                let result = handler.handle(matches, ctx);

                match result {
                    Ok(HandlerOutput::Render(data)) => {
                        // Convert to serde_json::Value for post-dispatch hooks
                        let mut json_data = serde_json::to_value(&data)
                            .map_err(|e| format!("Failed to serialize handler result: {}", e))?;

                        // Run post-dispatch hooks if present
                        if let Some(hooks) = hooks {
                            json_data = hooks
                                .run_post_dispatch(matches, ctx, json_data)
                                .map_err(|e| format!("Hook error: {}", e))?;
                        }

                        // Build render context for context providers
                        let theme = Theme::new();
                        let render_ctx = RenderContext::new(
                            ctx.output_mode,
                            get_terminal_width(),
                            &theme,
                            &json_data,
                        );

                        // Render the (potentially modified) data with context
                        let output = render_auto_with_context(
                            &template,
                            &json_data,
                            &theme,
                            ctx.output_mode,
                            &context_registry,
                            &render_ctx,
                        )
                        .map_err(|e| e.to_string())?;
                        Ok(DispatchOutput::Text(output))
                    }
                    Err(e) => Err(format!("Error: {}", e)),
                    Ok(HandlerOutput::Silent) => Ok(DispatchOutput::Silent),
                    Ok(HandlerOutput::Binary { data, filename }) => {
                        Ok(DispatchOutput::Binary(data, filename))
                    }
                }
            },
        );

        self.commands.insert(path.to_string(), dispatch);
        self
    }

    /// Registers hooks for a specific command path.
    ///
    /// Hooks are executed around the command handler:
    /// - Pre-dispatch hooks run before the handler
    /// - Post-dispatch hooks run after the handler, before rendering (receives raw data)
    /// - Post-output hooks run after rendering, can transform output
    ///
    /// Multiple hooks at the same phase are chained in registration order.
    /// Hooks abort on first error.
    ///
    /// # Arguments
    ///
    /// * `path` - Command path using dot notation (e.g., "list" or "config.get")
    /// * `hooks` - The hooks configuration
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use outstanding::cli::{App, Hooks, Output, HookError};
    /// use serde_json::json;
    ///
    /// App::builder()
    ///     .command("list", handler, template)
    ///     .hooks("list", Hooks::new()
    ///         .pre_dispatch(|_m, ctx| {
    ///             println!("Running: {:?}", ctx.command_path);
    ///             Ok(())
    ///         })
    ///         .post_dispatch(|_m, _ctx, mut data| {
    ///             // Modify raw data before rendering
    ///             if let Some(obj) = data.as_object_mut() {
    ///                 obj.insert("processed".into(), json!(true));
    ///             }
    ///             Ok(data)
    ///         })
    ///         .post_output(|_m, _ctx, output| {
    ///             if let RenderedOutput::Text(ref text) = output {
    ///                 // Copy to clipboard, log, etc.
    ///             }
    ///             Ok(output)
    ///         }))
    ///     .build()?
    ///     .run(cmd, args);
    /// ```
    pub fn hooks(mut self, path: &str, hooks: Hooks) -> Self {
        self.command_hooks.insert(path.to_string(), hooks);
        self
    }

    /// Dispatches to a registered handler if one matches the command path.
    ///
    /// Returns `RunResult::Handled(output)` if a handler was found and executed,
    /// or `RunResult::NoMatch(matches)` if no handler matched.
    ///
    /// If hooks are registered for the command, they are executed:
    /// - Pre-dispatch hooks run before the handler
    /// - Post-dispatch hooks run after the handler but before rendering
    /// - Post-output hooks run after rendering
    ///
    /// Hook errors abort execution and return the error as handled output.
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

            // Get hooks for this command (used for pre-dispatch, post-dispatch, and post-output)
            let hooks = self.command_hooks.get(&path_str);

            // Run pre-dispatch hooks if registered
            if let Some(hooks) = hooks {
                if let Err(e) = hooks.run_pre_dispatch(&matches, &ctx) {
                    return RunResult::Handled(format!("Hook error: {}", e));
                }
            }

            // Get the subcommand matches for the deepest command
            let sub_matches = get_deepest_matches(&matches);

            // Run the handler (post-dispatch hooks are run inside dispatch function)
            let dispatch_output = match dispatch(sub_matches, &ctx, hooks) {
                Ok(output) => output,
                Err(e) => return RunResult::Handled(e),
            };

            // Convert to Output enum for post-output hooks
            let output = match dispatch_output {
                DispatchRenderedOutput::Text(s) => RenderedOutput::Text(s),
                DispatchRenderedOutput::Binary(b, f) => RenderedOutput::Binary(b, f),
                DispatchRenderedOutput::Silent => RenderedOutput::Silent,
            };

            // Run post-output hooks if registered
            let mut final_output = if let Some(hooks) = hooks {
                match hooks.run_post_output(&matches, &ctx, output) {
                    Ok(o) => o,
                    Err(e) => return RunResult::Handled(format!("Hook error: {}", e)),
                }
            } else {
                output
            };

            // Handle file output if configured
            if self.output_file_flag.is_some() {
                if let Some(path_str) = matches
                    .try_get_one::<String>("_output_file_path")
                    .unwrap_or(None)
                {
                    let path = PathBuf::from(path_str);
                    let dest = OutputDestination::File(path);

                    match &final_output {
                        RenderedOutput::Text(s) => {
                            if let Err(e) = write_output(s, &dest) {
                                return RunResult::Handled(format!("Error writing output: {}", e));
                            }
                            // Suppress further output
                            final_output = RenderedOutput::Silent;
                        }
                        RenderedOutput::Binary(b, _) => {
                            if let Err(e) = write_binary_output(b, &dest) {
                                return RunResult::Handled(format!("Error writing output: {}", e));
                            }
                            final_output = RenderedOutput::Silent;
                        }
                        RenderedOutput::Silent => {}
                    }
                }
            }

            // Convert back to RunResult
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
    ///
    /// This is the recommended entry point when using the command handler system.
    /// It augments the command with `--output` flag, parses arguments, and
    /// dispatches to registered handlers.
    ///
    /// # Returns
    ///
    /// - `RunResult::Handled(output)` if a registered handler processed the command
    /// - `RunResult::NoMatch(matches)` if no handler matched (for manual handling)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use outstanding::cli::{App, HandlerResult, Output, RunResult};
    ///
    /// let result = App::builder()
    ///     .command("list", |_m, _ctx| Ok(HandlerOutput::Render(vec!["a", "b"]), "{{ . }}")
    ///     .dispatch_from(cmd, std::env::args());
    ///
    /// match result {
    ///     RunResult::Handled(output) => println!("{}", output),
    ///     RunResult::NoMatch(matches) => {
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

        // Dispatch to handler
        self.dispatch(matches, output_mode)
    }

    /// Runs the CLI: parses arguments, dispatches to handlers, and prints output.
    ///
    /// This is the main entry point for command execution. It handles everything:
    /// parsing, dispatch, rendering, and output.
    ///
    /// # Returns
    ///
    /// - `true` if a handler processed and printed output
    /// - `false` if no handler matched (caller should handle manually)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use outstanding::cli::{App, HandlerResult, Output};
    ///
    /// let handled = App::builder()
    ///     .command("list", |_m, _ctx| Ok(HandlerOutput::Render(vec!["a", "b"]), "{{ . }}")
    ///     .build()?
    ///     .run(cmd, std::env::args());
    ///
    /// if !handled {
    ///     // Handle unregistered commands manually
    /// }
    /// ```
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
                // For binary output, write to stdout or the suggested file
                // By default, we write to the suggested filename
                if let Err(e) = std::fs::write(&filename, &bytes) {
                    eprintln!("Error writing {}: {}", filename, e);
                } else {
                    eprintln!("Wrote {} bytes to {}", bytes.len(), filename);
                }
                true
            }
            RunResult::NoMatch(_) => false,
        }
    }

    /// Runs the CLI and returns the rendered output as a string.
    ///
    /// Similar to `run()`, but returns the output instead of printing it.
    /// Useful for testing or when you need to capture and process the output.
    ///
    /// # Returns
    ///
    /// - `RunResult::Handled(output)` - Handler executed, output is the rendered string
    /// - `RunResult::Binary(bytes, filename)` - Handler produced binary output
    /// - `RunResult::NoMatch(matches)` - No handler matched
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use outstanding::cli::{App, HandlerResult, Output, RunResult};
    ///
    /// let result = App::builder()
    ///     .command("list", |_m, _ctx| Ok(HandlerOutput::Render(vec!["a", "b"]), "{{ . }}")
    ///     .build()?
    ///     .run_to_string(cmd, std::env::args());
    ///
    /// match result {
    ///     RunResult::Handled(output) => println!("{}", output),
    ///     RunResult::Binary(bytes, filename) => std::fs::write(filename, bytes)?,
    ///     RunResult::NoMatch(matches) => { /* handle manually */ }
    /// }
    /// ```
    pub fn run_to_string<I, T>(&self, cmd: Command, args: I) -> RunResult
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        self.dispatch_from(cmd, args)
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
                    .help("Output mode: auto, term, text, term-debug, or json"),
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

    /// Builds the App instance.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A `default_theme()` was specified but the theme wasn't found in the stylesheet registry
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let outstanding = App::builder()
    ///     .styles(embed_styles!("src/styles"))
    ///     .default_theme("dark")
    ///     .build()?;
    /// ```
    pub fn build(mut self) -> Result<App, SetupError> {
        // Resolve theme: explicit theme takes precedence, then stylesheet registry
        let theme = if let Some(theme) = self.theme {
            Some(theme)
        } else if let Some(ref mut registry) = self.stylesheet_registry {
            let theme_name = self.default_theme_name.as_deref().unwrap_or("default");
            let theme = registry
                .get(theme_name)
                .map_err(|_| SetupError::ThemeNotFound(theme_name.to_string()))?;
            Some(theme)
        } else {
            None
        };

        Ok(App {
            registry: self.registry,
            output_flag: self.output_flag,
            output_file_flag: self.output_file_flag,
            output_mode: OutputMode::Auto,
            theme,
            command_hooks: self.command_hooks,
            template_registry: self.template_registry,
            stylesheet_registry: self.stylesheet_registry,
        })
    }

    /// Builds and parses CLI arguments in one step.
    ///
    /// # Panics
    ///
    /// Panics if building fails (e.g., theme not found). For proper error handling,
    /// use `build()` followed by `parse_with()` instead.
    pub fn parse(self, cmd: Command) -> clap::ArgMatches {
        self.build().expect("Failed to build App").parse_with(cmd)
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

    #[test]
    fn test_builder_output_flag_enabled_by_default() {
        let outstanding = App::builder().build().unwrap();
        assert!(outstanding.output_flag.is_some());
        assert_eq!(outstanding.output_flag.as_deref(), Some("output"));
    }

    #[test]
    fn test_no_output_flag() {
        let outstanding = App::builder().no_output_flag().build().unwrap();
        assert!(outstanding.output_flag.is_none());
    }

    #[test]
    fn test_custom_output_flag_name() {
        let outstanding = App::builder().output_flag(Some("format")).build().unwrap();
        assert_eq!(outstanding.output_flag.as_deref(), Some("format"));
    }

    #[test]
    fn test_command_registration() {
        use serde_json::json;

        let builder = App::builder().command(
            "list",
            |_m, _ctx| Ok(HandlerOutput::Render(json!({"items": ["a", "b"]}))),
            "Items: {{ items }}",
        );

        assert!(builder.commands.contains_key("list"));
    }

    #[test]
    fn test_dispatch_to_handler() {
        use serde_json::json;

        let builder = App::builder().command(
            "list",
            |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 42}))),
            "Count: {{ count }}",
        );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Count: 42"));
    }

    #[test]
    fn test_dispatch_unhandled_fallthrough() {
        use serde_json::json;

        let builder =
            App::builder().command("list", |_m, _ctx| Ok(HandlerOutput::Render(json!({}))), "");

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

        let builder = App::builder().command(
            "list",
            |_m, _ctx| Ok(HandlerOutput::Render(json!({"name": "test", "value": 123}))),
            "{{ name }}: {{ value }}",
        );

        let cmd = Command::new("app").subcommand(Command::new("list"));

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

        let builder = App::builder().command(
            "config.get",
            |_m, _ctx| Ok(HandlerOutput::Render(json!({"key": "value"}))),
            "{{ key }}",
        );

        let cmd =
            Command::new("app").subcommand(Command::new("config").subcommand(Command::new("get")));

        let matches = cmd.try_get_matches_from(["app", "config", "get"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("value"));
    }

    #[test]
    fn test_dispatch_silent_result() {
        let builder = App::builder().command("quiet", |_m, _ctx| Ok(HandlerOutput::Silent), "");

        let cmd = Command::new("app").subcommand(Command::new("quiet"));

        let matches = cmd.try_get_matches_from(["app", "quiet"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some(""));
    }

    #[test]
    fn test_dispatch_error_result() {
        let builder = App::builder().command(
            "fail",
            |_m, _ctx| Err(anyhow::anyhow!("something went wrong")),
            "",
        );

        let cmd = Command::new("app").subcommand(Command::new("fail"));

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

        let builder = App::builder().command(
            "list",
            |_m, _ctx| Ok(HandlerOutput::Render(json!({"items": ["a", "b"]}))),
            "Items: {{ items }}",
        );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let result = builder.dispatch_from(cmd, ["app", "list"]);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Items: [\"a\", \"b\"]"));
    }

    #[test]
    fn test_dispatch_from_with_json_flag() {
        use serde_json::json;

        let builder = App::builder().command(
            "list",
            |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 5}))),
            "Count: {{ count }}",
        );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let result = builder.dispatch_from(cmd, ["app", "--output=json", "list"]);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("\"count\": 5"));
    }

    #[test]
    fn test_dispatch_from_unhandled() {
        use serde_json::json;

        let builder =
            App::builder().command("list", |_m, _ctx| Ok(HandlerOutput::Render(json!({}))), "");

        let cmd = Command::new("app")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("other"));

        let result = builder.dispatch_from(cmd, ["app", "other"]);

        assert!(!result.is_handled());
    }

    // ============================================================================
    // Hook Integration Tests
    // ============================================================================

    #[test]
    fn test_hooks_registration() {
        use crate::cli::hooks::Hooks;

        let builder = App::builder().hooks("list", Hooks::new().pre_dispatch(|_, _| Ok(())));

        assert!(builder.command_hooks.contains_key("list"));
    }

    #[test]
    fn test_dispatch_with_pre_dispatch_hook() {
        use crate::cli::hooks::Hooks;
        use serde_json::json;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let hook_called = Arc::new(AtomicBool::new(false));
        let hook_called_clone = hook_called.clone();

        let builder = App::builder()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 1}))),
                "{{ count }}",
            )
            .hooks(
                "list",
                Hooks::new().pre_dispatch(move |_, _ctx| {
                    hook_called_clone.store(true, Ordering::SeqCst);
                    Ok(())
                }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert!(hook_called.load(Ordering::SeqCst));
        assert_eq!(result.output(), Some("1"));
    }

    #[test]
    fn test_dispatch_pre_dispatch_hook_abort() {
        use crate::cli::hooks::{HookError, Hooks};

        let builder = App::builder()
            .command(
                "list",
                |_m, _ctx| -> HandlerResult<()> {
                    panic!("Handler should not be called");
                },
                "",
            )
            .hooks(
                "list",
                Hooks::new()
                    .pre_dispatch(|_, _ctx| Err(HookError::pre_dispatch("blocked by hook"))),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("Hook error"));
        assert!(output.contains("blocked by hook"));
    }

    #[test]
    fn test_dispatch_with_post_output_hook() {
        use crate::cli::hooks::{Hooks, Output};
        use serde_json::json;

        let builder = App::builder()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "hello"}))),
                "{{ msg }}",
            )
            .hooks(
                "list",
                Hooks::new().post_output(|_, _ctx, output| {
                    if let RenderedOutput::Text(text) = output {
                        Ok(RenderedOutput::Text(text.to_uppercase()))
                    } else {
                        Ok(output)
                    }
                }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("HELLO"));
    }

    #[test]
    fn test_dispatch_post_output_hook_chain() {
        use crate::cli::hooks::{Hooks, Output};
        use serde_json::json;

        let builder = App::builder()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "test"}))),
                "{{ msg }}",
            )
            .hooks(
                "list",
                Hooks::new()
                    .post_output(|_, _ctx, output| {
                        if let RenderedOutput::Text(text) = output {
                            Ok(RenderedOutput::Text(format!("[{}]", text)))
                        } else {
                            Ok(output)
                        }
                    })
                    .post_output(|_, _ctx, output| {
                        if let RenderedOutput::Text(text) = output {
                            Ok(RenderedOutput::Text(text.to_uppercase()))
                        } else {
                            Ok(output)
                        }
                    }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("[TEST]"));
    }

    #[test]
    fn test_dispatch_post_output_hook_abort() {
        use crate::cli::hooks::{HookError, Hooks};
        use serde_json::json;

        let builder = App::builder()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "hello"}))),
                "{{ msg }}",
            )
            .hooks(
                "list",
                Hooks::new().post_output(|_, _ctx, _output| {
                    Err(HookError::post_output("post-processing failed"))
                }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("Hook error"));
        assert!(output.contains("post-processing failed"));
    }

    #[test]
    fn test_dispatch_hooks_for_nested_command() {
        use crate::cli::hooks::{Hooks, Output};
        use serde_json::json;

        let builder = App::builder()
            .command(
                "config.get",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"value": "secret"}))),
                "{{ value }}",
            )
            .hooks(
                "config.get",
                Hooks::new().post_output(|_, _ctx, output| {
                    if let RenderedOutput::Text(_) = output {
                        Ok(RenderedOutput::Text("***".into()))
                    } else {
                        Ok(output)
                    }
                }),
            );

        let cmd =
            Command::new("app").subcommand(Command::new("config").subcommand(Command::new("get")));

        let matches = cmd.try_get_matches_from(["app", "config", "get"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("***"));
    }

    #[test]
    fn test_dispatch_no_hooks_for_command() {
        use crate::cli::hooks::Hooks;
        use serde_json::json;

        // Register hooks for "list" but dispatch "other"
        let builder = App::builder()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "list"}))),
                "{{ msg }}",
            )
            .command(
                "other",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "other"}))),
                "{{ msg }}",
            )
            .hooks(
                "list",
                Hooks::new().post_output(|_, _ctx, _| {
                    panic!("Should not be called for 'other' command");
                }),
            );

        let cmd = Command::new("app")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("other"));

        let matches = cmd.try_get_matches_from(["app", "other"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("other"));
    }

    #[test]
    fn test_dispatch_binary_output_with_hook() {
        use crate::cli::hooks::{Hooks, Output};

        let builder = App::builder()
            .command(
                "export",
                |_m, _ctx| -> HandlerResult<()> {
                    Ok(HandlerOutput::Binary {
                        data: vec![1, 2, 3],
                        filename: "out.bin".into(),
                    })
                },
                "",
            )
            .hooks(
                "export",
                Hooks::new().post_output(|_, _ctx, output| {
                    if let RenderedOutput::Binary(mut bytes, filename) = output {
                        bytes.push(4);
                        Ok(RenderedOutput::Binary(bytes, filename))
                    } else {
                        Ok(output)
                    }
                }),
            );

        let cmd = Command::new("app").subcommand(Command::new("export"));

        let matches = cmd.try_get_matches_from(["app", "export"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_binary());
        let (bytes, filename) = result.binary().unwrap();
        assert_eq!(bytes, &[1, 2, 3, 4]);
        assert_eq!(filename, "out.bin");
    }

    #[test]
    fn test_hooks_passed_to_built_outstanding() {
        use crate::cli::hooks::Hooks;

        let outstanding = App::builder()
            .hooks("list", Hooks::new().pre_dispatch(|_, _| Ok(())))
            .build()
            .unwrap();

        assert!(outstanding.get_hooks("list").is_some());
        assert!(outstanding.get_hooks("other").is_none());
    }

    #[test]
    fn test_run_command_with_hooks() {
        use crate::cli::hooks::{Hooks, Output};
        use serde::Serialize;

        #[derive(Serialize)]
        struct Data {
            value: i32,
        }

        let outstanding = App::builder()
            .hooks(
                "test",
                Hooks::new().post_output(|_, _ctx, output| {
                    if let RenderedOutput::Text(text) = output {
                        Ok(RenderedOutput::Text(format!("wrapped: {}", text)))
                    } else {
                        Ok(output)
                    }
                }),
            )
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let sub_matches = matches.subcommand_matches("test").unwrap();

        let result = outstanding.run_command(
            "test",
            sub_matches,
            |_m, _ctx| Ok(HandlerOutput::Render(Data { value: 42 })),
            "{{ value }}",
        );

        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.as_text(), Some("wrapped: 42"));
    }

    #[test]
    fn test_run_command_pre_dispatch_abort() {
        use crate::cli::hooks::{HookError, Hooks};

        let outstanding = App::builder()
            .hooks(
                "test",
                Hooks::new().pre_dispatch(|_, _ctx| Err(HookError::pre_dispatch("access denied"))),
            )
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let sub_matches = matches.subcommand_matches("test").unwrap();

        let result = outstanding.run_command::<_, ()>(
            "test",
            sub_matches,
            |_m, _ctx| {
                panic!("Handler should not be called");
            },
            "",
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("access denied"));
    }

    #[test]
    fn test_run_command_without_hooks() {
        use serde::Serialize;

        #[derive(Serialize)]
        struct Data {
            msg: String,
        }

        let outstanding = App::builder().build().unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let sub_matches = matches.subcommand_matches("test").unwrap();

        let result = outstanding.run_command(
            "test",
            sub_matches,
            |_m, _ctx| {
                Ok(HandlerOutput::Render(Data {
                    msg: "hello".into(),
                }))
            },
            "{{ msg }}",
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_text(), Some("hello"));
    }

    #[test]
    fn test_run_command_silent() {
        let outstanding = App::builder().build().unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let sub_matches = matches.subcommand_matches("test").unwrap();

        let result = outstanding.run_command::<_, ()>(
            "test",
            sub_matches,
            |_m, _ctx| Ok(HandlerOutput::Silent),
            "",
        );

        assert!(result.is_ok());
        assert!(result.unwrap().is_silent());
    }

    #[test]
    fn test_run_command_binary() {
        use crate::cli::hooks::Hooks;

        let outstanding = App::builder()
            .hooks(
                "export",
                Hooks::new().post_output(|_, _ctx, output| {
                    // Verify we receive binary output
                    assert!(output.is_binary());
                    Ok(output)
                }),
            )
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("export"));
        let matches = cmd.try_get_matches_from(["app", "export"]).unwrap();
        let sub_matches = matches.subcommand_matches("export").unwrap();

        let result = outstanding.run_command::<_, ()>(
            "export",
            sub_matches,
            |_m, _ctx| {
                Ok(HandlerOutput::Binary {
                    data: vec![0xDE, 0xAD],
                    filename: "data.bin".into(),
                })
            },
            "",
        );

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.is_binary());
        let (bytes, filename) = output.as_binary().unwrap();
        assert_eq!(bytes, &[0xDE, 0xAD]);
        assert_eq!(filename, "data.bin");
    }

    // ============================================================================
    // Post-dispatch Hook Integration Tests
    // ============================================================================

    #[test]
    fn test_dispatch_with_post_dispatch_hook() {
        use crate::cli::hooks::Hooks;
        use serde_json::json;

        let builder = App::builder()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 5}))),
                "Count: {{ count }}, Modified: {{ modified }}",
            )
            .hooks(
                "list",
                Hooks::new().post_dispatch(|_, _ctx, mut data| {
                    if let Some(obj) = data.as_object_mut() {
                        obj.insert("modified".into(), json!(true));
                    }
                    Ok(data)
                }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("Count: 5"));
        assert!(output.contains("Modified: true"));
    }

    #[test]
    fn test_dispatch_post_dispatch_hook_abort() {
        use crate::cli::hooks::{HookError, Hooks};
        use serde_json::json;

        let builder = App::builder()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"items": []}))),
                "{{ items }}",
            )
            .hooks(
                "list",
                Hooks::new().post_dispatch(|_, _ctx, data| {
                    // Abort if no items
                    if data
                        .get("items")
                        .and_then(|v| v.as_array())
                        .map(|a| a.is_empty())
                        == Some(true)
                    {
                        return Err(HookError::post_dispatch("no items to display"));
                    }
                    Ok(data)
                }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("Hook error"));
        assert!(output.contains("no items to display"));
    }

    #[test]
    fn test_dispatch_post_dispatch_chain() {
        use crate::cli::hooks::Hooks;
        use serde_json::json;

        let builder = App::builder()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"value": 1}))),
                "{{ value }}",
            )
            .hooks(
                "list",
                Hooks::new()
                    .post_dispatch(|_, _ctx, mut data| {
                        // First hook: multiply by 2
                        if let Some(v) = data.get_mut("value") {
                            *v = json!(v.as_i64().unwrap_or(0) * 2);
                        }
                        Ok(data)
                    })
                    .post_dispatch(|_, _ctx, mut data| {
                        // Second hook: add 10
                        if let Some(v) = data.get_mut("value") {
                            *v = json!(v.as_i64().unwrap_or(0) + 10);
                        }
                        Ok(data)
                    }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        // 1 * 2 = 2, 2 + 10 = 12
        assert_eq!(result.output(), Some("12"));
    }

    #[test]
    fn test_dispatch_all_three_hooks() {
        use crate::cli::hooks::Hooks;
        use serde_json::json;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let call_order = Arc::new(AtomicUsize::new(0));
        let pre_order = call_order.clone();
        let post_dispatch_order = call_order.clone();
        let post_output_order = call_order.clone();

        let builder = App::builder()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"msg": "hello"}))),
                "{{ msg }}",
            )
            .hooks(
                "list",
                Hooks::new()
                    .pre_dispatch(move |_, _ctx| {
                        assert_eq!(pre_order.fetch_add(1, Ordering::SeqCst), 0);
                        Ok(())
                    })
                    .post_dispatch(move |_, _ctx, data| {
                        assert_eq!(post_dispatch_order.fetch_add(1, Ordering::SeqCst), 1);
                        Ok(data)
                    })
                    .post_output(move |_, _ctx, output| {
                        assert_eq!(post_output_order.fetch_add(1, Ordering::SeqCst), 2);
                        Ok(output)
                    }),
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(call_order.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn test_run_command_with_post_dispatch_hook() {
        use crate::cli::hooks::Hooks;
        use serde::Serialize;
        use serde_json::json;

        #[derive(Serialize)]
        struct Data {
            value: i32,
        }

        let outstanding = App::builder()
            .hooks(
                "test",
                Hooks::new().post_dispatch(|_, _ctx, mut data| {
                    if let Some(obj) = data.as_object_mut() {
                        obj.insert("added_by_hook".into(), json!("yes"));
                    }
                    Ok(data)
                }),
            )
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let sub_matches = matches.subcommand_matches("test").unwrap();

        let result = outstanding.run_command(
            "test",
            sub_matches,
            |_m, _ctx| Ok(HandlerOutput::Render(Data { value: 42 })),
            "value={{ value }}, added={{ added_by_hook }}",
        );

        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.as_text(), Some("value=42, added=yes"));
    }

    #[test]
    fn test_run_command_post_dispatch_abort() {
        use crate::cli::hooks::{HookError, Hooks};
        use serde::Serialize;

        #[derive(Serialize)]
        struct Data {
            valid: bool,
        }

        let outstanding = App::builder()
            .hooks(
                "test",
                Hooks::new().post_dispatch(|_, _ctx, data| {
                    if data.get("valid") == Some(&serde_json::json!(false)) {
                        return Err(HookError::post_dispatch("invalid data"));
                    }
                    Ok(data)
                }),
            )
            .build()
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let sub_matches = matches.subcommand_matches("test").unwrap();

        let result = outstanding.run_command(
            "test",
            sub_matches,
            |_m, _ctx| Ok(HandlerOutput::Render(Data { valid: false })),
            "{{ valid }}",
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.message, "invalid data");
        assert_eq!(err.phase, crate::cli::hooks::HookPhase::PostDispatch);
    }

    // ============================================================================
    // Context Injection Tests
    // ============================================================================

    #[test]
    fn test_context_static_value() {
        use serde_json::json;

        let builder = App::builder()
            .context("version", Value::from("1.0.0"))
            .command(
                "info",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"name": "app"}))),
                "{{ name }} v{{ version }}",
            );

        let cmd = Command::new("app").subcommand(Command::new("info"));
        let matches = cmd.try_get_matches_from(["app", "info"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("app v1.0.0"));
    }

    #[test]
    fn test_context_multiple_static_values() {
        use serde_json::json;

        let builder = App::builder()
            .context("author", Value::from("Alice"))
            .context("year", Value::from(2024))
            .command(
                "info",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"title": "Report"}))),
                "{{ title }} by {{ author }} ({{ year }})",
            );

        let cmd = Command::new("app").subcommand(Command::new("info"));
        let matches = cmd.try_get_matches_from(["app", "info"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Report by Alice (2024)"));
    }

    #[test]
    fn test_context_fn_terminal_width() {
        use serde_json::json;

        let builder = App::builder()
            .context_fn("terminal_width", |ctx: &RenderContext| {
                Value::from(ctx.terminal_width.unwrap_or(80))
            })
            .command(
                "info",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({}))),
                "Width: {{ terminal_width }}",
            );

        let cmd = Command::new("app").subcommand(Command::new("info"));
        let matches = cmd.try_get_matches_from(["app", "info"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        // The width will be actual terminal width or 80 in tests
        let output = result.output().unwrap();
        assert!(output.starts_with("Width: "));
    }

    #[test]
    fn test_context_fn_output_mode() {
        use serde_json::json;

        let builder = App::builder()
            .context_fn("mode", |ctx: &RenderContext| {
                Value::from(format!("{:?}", ctx.output_mode))
            })
            .command(
                "info",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({}))),
                "Mode: {{ mode }}",
            );

        let cmd = Command::new("app").subcommand(Command::new("info"));
        let matches = cmd.try_get_matches_from(["app", "info"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Mode: Text"));
    }

    #[test]
    fn test_context_data_takes_precedence() {
        use serde_json::json;

        // Context has "value" but handler data also has "value"
        // Handler data should take precedence
        let builder = App::builder()
            .context("value", Value::from("from_context"))
            .command(
                "test",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"value": "from_data"}))),
                "{{ value }}",
            );

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("from_data"));
    }

    #[test]
    fn test_context_shared_across_commands() {
        use serde_json::json;

        let builder = App::builder()
            .context("app_name", Value::from("MyApp"))
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({}))),
                "{{ app_name }}: list",
            )
            .command(
                "info",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({}))),
                "{{ app_name }}: info",
            );

        let cmd = Command::new("app")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("info"));

        // Test "list" command
        let matches = cmd.clone().try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);
        assert_eq!(result.output(), Some("MyApp: list"));

        // Test "info" command
        let matches = cmd.try_get_matches_from(["app", "info"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);
        assert_eq!(result.output(), Some("MyApp: info"));
    }

    #[test]
    fn test_context_fn_uses_handler_data() {
        use serde_json::json;

        let builder = App::builder()
            .context_fn("doubled_count", |ctx: &RenderContext| {
                let count = ctx.data.get("count").and_then(|v| v.as_i64()).unwrap_or(0);
                Value::from(count * 2)
            })
            .command(
                "test",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 21}))),
                "Count: {{ count }}, Doubled: {{ doubled_count }}",
            );

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Count: 21, Doubled: 42"));
    }

    #[test]
    fn test_context_with_nested_object() {
        use serde_json::json;

        let builder = App::builder()
            .context(
                "config",
                Value::from_iter([
                    ("debug", Value::from(true)),
                    ("max_items", Value::from(100)),
                ]),
            )
            .command(
                "test",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({}))),
                "Debug: {{ config.debug }}, Max: {{ config.max_items }}",
            );

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Debug: true, Max: 100"));
    }

    #[test]
    fn test_context_in_loop() {
        use serde_json::json;

        let builder = App::builder()
            .context("separator", Value::from(" | "))
            .command(
                "list",
                |_m, _ctx| {
                    Ok(HandlerOutput::Render(json!({
                        "items": ["a", "b", "c"]
                    })))
                },
                "{% for item in items %}{{ item }}{% if not loop.last %}{{ separator }}{% endif %}{% endfor %}",
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("a | b | c"));
    }

    #[test]
    fn test_context_json_output_ignores_context() {
        use serde_json::json;

        let builder = App::builder()
            .context("extra", Value::from("should_not_appear"))
            .command(
                "test",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"data": "value"}))),
                "{{ data }} + {{ extra }}",
            );

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Json);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        // JSON output should only contain handler data, not context
        assert!(output.contains("\"data\": \"value\""));
        assert!(!output.contains("extra"));
        assert!(!output.contains("should_not_appear"));
    }

    #[test]
    fn test_dispatch_with_output_file_flag() {
        use serde_json::json;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("output.txt");
        let path_str = file_path.to_str().unwrap();

        let builder = App::builder().command(
            "list",
            |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 42}))),
            "Count: {{ count }}",
        );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let result = builder.dispatch_from(cmd, ["app", "--output-file-path", path_str, "list"]);

        assert!(result.is_handled());
        // Verify output is suppressed (silent)
        assert_eq!(result.output(), Some(""));

        // Verify file content
        let content = std::fs::read_to_string(file_path).unwrap();
        assert_eq!(content, "Count: 42");
    }

    #[test]
    fn test_dispatch_with_custom_output_file_flag() {
        use serde_json::json;
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("out.txt");
        let path_str = file_path.to_str().unwrap();

        let builder = App::builder().output_file_flag(Some("save-to")).command(
            "list",
            |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 99}))),
            "{{ count }}",
        );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let result = builder.dispatch_from(cmd, ["app", "--save-to", path_str, "list"]);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some(""));

        let content = std::fs::read_to_string(file_path).unwrap();
        assert_eq!(content, "99");
    }

    // ============================================================================
    // Nested Builder (Group) Tests
    // ============================================================================

    #[test]
    fn test_group_basic() {
        use serde_json::json;

        let builder = App::builder().group("db", |g| {
            g.command("migrate", |_m, _ctx| {
                Ok(HandlerOutput::Render(json!({"status": "migrated"})))
            })
            .command("backup", |_m, _ctx| {
                Ok(HandlerOutput::Render(json!({"status": "backed_up"})))
            })
        });

        let cmd =
            Command::new("app").subcommand(Command::new("db").subcommand(Command::new("migrate")));

        let matches = cmd.try_get_matches_from(["app", "db", "migrate"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Json);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("migrated"));
    }

    #[test]
    fn test_group_nested() {
        use serde_json::json;

        let builder = App::builder().group("app", |g| {
            g.command("start", |_m, _ctx| {
                Ok(HandlerOutput::Render(json!({"action": "start"})))
            })
            .group("config", |g| {
                g.command("get", |_m, _ctx| {
                    Ok(HandlerOutput::Render(json!({"value": "test_value"})))
                })
                .command("set", |_m, _ctx| {
                    Ok(HandlerOutput::Render(json!({"ok": true})))
                })
            })
        });

        // Test nested command: app.config.get
        let cmd = Command::new("cli").subcommand(
            Command::new("app")
                .subcommand(Command::new("start"))
                .subcommand(
                    Command::new("config")
                        .subcommand(Command::new("get"))
                        .subcommand(Command::new("set")),
                ),
        );

        let matches = cmd
            .try_get_matches_from(["cli", "app", "config", "get"])
            .unwrap();
        let result = builder.dispatch(matches, OutputMode::Json);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("test_value"));
    }

    #[test]
    fn test_group_with_template() {
        use serde_json::json;

        let builder = App::builder().group("db", |g| {
            g.command_with(
                "migrate",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 5}))),
                |cfg| cfg.template("Migrated {{ count }} tables"),
            )
        });

        let cmd =
            Command::new("app").subcommand(Command::new("db").subcommand(Command::new("migrate")));

        let matches = cmd.try_get_matches_from(["app", "db", "migrate"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Migrated 5 tables"));
    }

    #[test]
    fn test_group_with_hooks() {
        use serde_json::json;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let hook_called = Arc::new(AtomicBool::new(false));
        let hook_called_clone = hook_called.clone();

        let builder = App::builder().group("db", |g| {
            g.command_with(
                "migrate",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"done": true}))),
                move |cfg| {
                    cfg.template("{{ done }}").pre_dispatch(move |_, _| {
                        hook_called_clone.store(true, Ordering::SeqCst);
                        Ok(())
                    })
                },
            )
        });

        let cmd =
            Command::new("app").subcommand(Command::new("db").subcommand(Command::new("migrate")));

        let matches = cmd.try_get_matches_from(["app", "db", "migrate"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert!(hook_called.load(Ordering::SeqCst));
    }

    #[test]
    fn test_command_with_inline_config() {
        use serde_json::json;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let builder = App::builder().command_with(
            "list",
            |_m, _ctx| Ok(HandlerOutput::Render(json!({"items": ["a", "b"]}))),
            move |cfg| {
                cfg.template("Items: {{ items | length }}")
                    .pre_dispatch(move |_, _| {
                        counter_clone.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    })
            },
        );

        let cmd = Command::new("app").subcommand(Command::new("list"));

        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Items: 2"));
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_template_dir_convention() {
        use serde_json::json;

        let builder = App::builder()
            .template_dir("templates")
            .template_ext(".jinja2")
            .group("db", |g| {
                // No explicit template - should resolve to "templates/db/migrate.jinja2"
                g.command("migrate", |_m, _ctx| {
                    Ok(HandlerOutput::Render(json!({"ok": true})))
                })
            });

        // Verify the builder has the commands registered
        assert!(builder.commands.contains_key("db.migrate"));
    }

    #[test]
    fn test_multiple_groups() {
        use serde_json::json;

        let builder = App::builder()
            .group("db", |g| {
                g.command("migrate", |_m, _ctx| {
                    Ok(HandlerOutput::Render(json!({"type": "db"})))
                })
            })
            .group("cache", |g| {
                g.command("clear", |_m, _ctx| {
                    Ok(HandlerOutput::Render(json!({"type": "cache"})))
                })
            });

        assert!(builder.commands.contains_key("db.migrate"));
        assert!(builder.commands.contains_key("cache.clear"));
    }

    #[test]
    fn test_group_mixed_with_regular_commands() {
        use serde_json::json;

        let builder = App::builder()
            .command(
                "version",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"v": "1.0.0"}))),
                "{{ v }}",
            )
            .group("db", |g| {
                g.command("migrate", |_m, _ctx| {
                    Ok(HandlerOutput::Render(json!({"ok": true})))
                })
            });

        assert!(builder.commands.contains_key("version"));
        assert!(builder.commands.contains_key("db.migrate"));
    }

    // ============================================================================
    // Dispatch Macro Integration Tests
    // ============================================================================

    #[test]
    fn test_dispatch_macro_simple() {
        use crate::dispatch;
        use serde_json::json;

        let builder = App::builder().commands(dispatch! {
            list => |_m, _ctx| Ok(HandlerOutput::Render(json!({"items": ["a", "b"]})))
        });

        assert!(builder.commands.contains_key("list"));

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Json);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.contains("items"));
    }

    #[test]
    fn test_dispatch_macro_with_groups() {
        use crate::dispatch;
        use serde_json::json;

        let builder = App::builder().commands(dispatch! {
            db: {
                migrate => |_m, _ctx| Ok(HandlerOutput::Render(json!({"migrated": true}))),
                backup => |_m, _ctx| Ok(HandlerOutput::Render(json!({"backed_up": true}))),
            },
            version => |_m, _ctx| Ok(HandlerOutput::Render(json!({"v": "1.0"}))),
        });

        assert!(builder.commands.contains_key("db.migrate"));
        assert!(builder.commands.contains_key("db.backup"));
        assert!(builder.commands.contains_key("version"));

        // Test dispatch to nested command
        let cmd = Command::new("app")
            .subcommand(
                Command::new("db")
                    .subcommand(Command::new("migrate"))
                    .subcommand(Command::new("backup")),
            )
            .subcommand(Command::new("version"));

        let matches = cmd
            .clone()
            .try_get_matches_from(["app", "db", "migrate"])
            .unwrap();
        let result = builder.dispatch(matches, OutputMode::Json);
        assert!(result.is_handled());
        assert!(result.output().unwrap().contains("migrated"));
    }

    #[test]
    fn test_dispatch_macro_with_template() {
        use crate::dispatch;
        use serde_json::json;

        let builder = App::builder().commands(dispatch! {
            list => {
                handler: |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 42}))),
                template: "Count: {{ count }}",
            }
        });

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Count: 42"));
    }

    #[test]
    fn test_dispatch_macro_with_hooks() {
        use crate::dispatch;
        use serde_json::json;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let hook_called = Arc::new(AtomicBool::new(false));
        let hook_called_clone = hook_called.clone();

        let builder = App::builder().commands(dispatch! {
            list => {
                handler: |_m, _ctx| Ok(HandlerOutput::Render(json!({"ok": true}))),
                template: "{{ ok }}",
                pre_dispatch: move |_, _| {
                    hook_called_clone.store(true, Ordering::SeqCst);
                    Ok(())
                },
            }
        });

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert!(hook_called.load(Ordering::SeqCst));
    }

    #[test]
    fn test_dispatch_macro_deeply_nested() {
        use crate::dispatch;
        use serde_json::json;

        let builder = App::builder().commands(dispatch! {
            app: {
                config: {
                    get => |_m, _ctx| Ok(HandlerOutput::Render(json!({"key": "value"}))),
                    set => |_m, _ctx| Ok(HandlerOutput::Render(json!({"ok": true}))),
                },
                start => |_m, _ctx| Ok(HandlerOutput::Render(json!({"started": true}))),
            },
        });

        assert!(builder.commands.contains_key("app.config.get"));
        assert!(builder.commands.contains_key("app.config.set"));
        assert!(builder.commands.contains_key("app.start"));
    }
}
