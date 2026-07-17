//! App builder and main entry point for CLI integration.
//!
//! This module provides the [`AppBuilder`] type (re-exported as [`App`](super::App))
//! for configuring CLI applications with commands, hooks, templates, themes,
//! and app-level state.
//!
//! # App State
//!
//! App-level state (database connections, configuration, API clients) can be
//! injected via `.app_state()` and accessed in handlers via `ctx.app_state`:
//!
//! ```rust,ignore
//! App::new()
//!     .app_state(Database::connect()?)
//!     .app_state(Config::load()?)
//!     .command("list", |matches, ctx| {
//!         let db = ctx.app_state.get_required::<Database>()?;
//!         Ok(Output::Render(db.list()?))
//!     }, "{{ items }}")
//!     .build()?
//! ```
//!
//! The builder is split into submodules by concern:
//! - [`config`]: Configuration methods (themes, templates, context, flags)
//! - [`commands`]: Command and handler registration
//! - [`execution`]: Dispatch macro integration and command execution
//! - [`rendering`]: Template rendering and data serialization

mod commands;
mod config;
mod execution;
mod rendering;

use crate::context::ContextRegistry;
use crate::setup::SetupError;
use crate::topics::{
    display_with_pager, render_topic, render_topics_list, TopicRegistry, TopicRenderConfig,
};
use crate::TemplateRegistry;
use crate::{render_auto, OutputMode, Theme};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use super::dispatch::{insert_default_command, DispatchFn};
use super::group::CommandRecipe;
use super::handler::{CommandContext, Extensions, HandlerResult, Output as HandlerOutput};
use super::help::{render_help, render_help_with_topics, CommandGroup, HelpConfig};
use super::hooks::{ArtifactOutput, HookError, Hooks, RenderedOutput, TextOutput};
use super::result::HelpResult;
use standout_dispatch::verify::ExpectedArg;

/// Stores a pending command recipe along with its resolved template.
struct PendingCommand {
    recipe: Box<dyn CommandRecipe>,
    template: String,
}

/// Main entry point for standout-clap integration.
///
/// `AppBuilder` is re-exported as `App` in the public API. It serves as both
/// the builder for configuration and the runtime for command dispatch, rendering,
/// and help.
///
/// # Example
///
/// ```rust
/// use standout::cli::App;
///
/// let standout = App::new()
///     .help_handling(true)
///     .topics_dir(".").unwrap()
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
/// use standout::cli::App;
/// use crate::context::RenderContext;
/// use minijinja::Value;
///
/// App::new()
///     // Static context
///     .context("app_version", Value::from("1.0.0"))
///
///     // Dynamic context (computed at render time)
///     .context_fn("terminal", |ctx: &RenderContext| {
///         Value::from_iter([
///             ("width", Value::from(ctx.terminal_width.unwrap_or(80))),
///             ("is_tty", Value::from(ctx.output_mode == standout::OutputMode::Term)),
///         ])
///     })
///     .command("list", handler, "Width: {{ terminal.width }}")
///     .build()?
///     .run(cmd, args);
/// ```
pub struct AppBuilder {
    pub(crate) registry: TopicRegistry,
    pub(crate) output_flag: Option<String>,
    pub(crate) output_file_flag: Option<String>,
    pub(crate) theme: Option<Theme>,
    /// Stylesheet registry (built from embedded styles)
    pub(crate) stylesheet_registry: Option<crate::StylesheetRegistry>,
    /// Template registry (built from embedded templates)
    pub(crate) template_registry: Option<Rc<TemplateRegistry>>,
    pub(crate) default_theme_name: Option<String>,
    /// Pending commands - closures are created lazily at dispatch time
    pending_commands: RefCell<HashMap<String, PendingCommand>>,
    /// Finalized dispatch functions (lazily created from pending_commands)
    finalized_commands: RefCell<Option<HashMap<String, DispatchFn>>>,
    pub(crate) command_hooks: HashMap<String, Hooks>,
    pub(crate) context_registry: ContextRegistry,
    pub(crate) template_dir: Option<PathBuf>,
    pub(crate) template_ext: String,
    /// Static default command to use when no subcommand is specified
    pub(crate) default_command: Option<String>,
    /// Invocation-aware default command chooser, consulted before the static
    /// default. See [`crate::cli::default_command`].
    pub(crate) default_command_resolver: Option<crate::cli::DefaultCommandResolver>,
    /// Whether to include framework-supplied templates (default: true)
    pub(crate) include_framework_templates: bool,
    /// Whether to include framework-supplied styles (default: true)
    pub(crate) include_framework_styles: bool,
    /// App-level state shared across all dispatches.
    ///
    /// Stored as `Rc<Extensions>` so it can be cloned cheaply into CommandContext.
    /// During builder phase, `Rc::get_mut` is used since only the builder holds the Rc.
    pub(crate) app_state: Rc<Extensions>,

    /// Optional template engine.
    ///
    /// If not provided, a default MiniJinja engine will be created.
    pub(crate) template_engine: Rc<Box<dyn standout_render::template::TemplateEngine>>,

    /// Command groups for organized help display.
    pub(crate) help_command_groups: Option<Vec<CommandGroup>>,

    /// Whether standout intercepts and renders help (default: false).
    ///
    /// When true, standout disables clap's built-in help and renders its own
    /// themed, grouped help for all invocation forms (`help`, `--help`, `-h`).
    /// Required when using `command_groups` or topics.
    pub(crate) help_handling: bool,
    /// Explicit East Asian Ambiguous width policy.
    pub(crate) ambiguous_width: crate::AmbiguousWidth,
}

impl Default for AppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AppBuilder {
    /// Creates a new App with default settings.
    ///
    /// By default, the `--output` flag is enabled, framework templates and styles
    /// are included, and no hooks are registered.
    pub fn new() -> Self {
        Self {
            registry: TopicRegistry::new(),
            output_flag: Some("output".to_string()), // Enabled by default
            output_file_flag: Some("output-file-path".to_string()),
            theme: None,
            stylesheet_registry: None,
            template_registry: None,
            default_theme_name: None,
            pending_commands: RefCell::new(HashMap::new()),
            finalized_commands: RefCell::new(None),
            command_hooks: HashMap::new(),
            context_registry: ContextRegistry::new(),
            template_dir: None,
            template_ext: ".j2".to_string(),
            default_command: None,
            default_command_resolver: None,
            include_framework_templates: true,
            include_framework_styles: true,
            app_state: Rc::new(Extensions::new()),
            template_engine: Rc::new(Box::new(standout_render::template::MiniJinjaEngine::new())),
            help_command_groups: None,
            help_handling: false,
            ambiguous_width: crate::AmbiguousWidth::Narrow,
        }
    }

    /// Backwards-compatible alias for `new()`.
    pub fn builder() -> Self {
        Self::new()
    }

    /// Adds app-level state that will be available to all handlers.
    ///
    /// App state is immutable and shared across all dispatches via `Rc<Extensions>`.
    /// Use for long-lived resources like database connections, configuration, and
    /// API clients.
    ///
    /// # Shared Mutable State
    ///
    /// To share mutable state (like metrics or caches), use interior mutability:
    ///
    /// ```rust
    /// use standout::cli::{App, Output};
    /// use std::sync::atomic::{AtomicUsize, Ordering};
    ///
    /// struct Metrics {
    ///     requests: AtomicUsize,
    /// }
    ///
    /// let app = App::new()
    ///     .app_state(Metrics { requests: AtomicUsize::new(0) })
    ///     .command("test", |_m, ctx| {
    ///         let metrics = ctx.app_state.get_required::<Metrics>()?;
    ///         metrics.requests.fetch_add(1, Ordering::SeqCst);
    ///         Ok(Output::<()>::Silent)
    ///     }, "").unwrap()
    ///     .build()
    ///     .unwrap();
    /// ```
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use standout::cli::App;
    ///
    /// struct Database { url: String }
    /// struct Config { debug: bool }
    ///
    /// let app = App::new()
    ///     .app_state(Database { url: "postgres://localhost".into() })
    ///     .app_state(Config { debug: true })
    ///     .command("list", |matches, ctx| {
    ///         let db = ctx.app_state.get_required::<Database>()?;
    ///         let config = ctx.app_state.get_required::<Config>()?;
    ///         // Use db and config...
    ///         Ok(Output::Render(vec!["item1", "item2"]))
    ///     }, "{{ items }}")
    ///     .build()?;
    /// ```
    ///
    /// # Type Safety
    ///
    /// Each type can only be stored once. Inserting a second value of the same
    /// type replaces the first:
    ///
    /// ```rust,ignore
    /// App::new()
    ///     .app_state(Config { debug: false })
    ///     .app_state(Config { debug: true })  // Replaces previous Config
    /// ```
    pub fn app_state<T: 'static>(mut self, value: T) -> Self {
        // During builder phase, only the builder holds the Rc, so get_mut succeeds.
        Rc::get_mut(&mut self.app_state)
            .expect("app_state Rc should be exclusively owned during builder phase")
            .insert(value);
        self
    }

    /// sets a custom template engine to be used for rendering.
    ///
    /// If not set, the default MiniJinja engine will be used.
    pub fn template_engine(
        mut self,
        engine: Box<dyn standout_render::template::TemplateEngine>,
    ) -> Self {
        self.template_engine = Rc::new(engine);
        self
    }

    /// Ensures all pending commands have been finalized into dispatch functions.
    ///
    /// This method is called lazily on first dispatch. It creates the actual
    /// dispatch closures from the stored recipes. The theme is NOT captured here -
    /// it is passed at runtime via late binding, which allows `.theme()` to be
    /// called in any order relative to `.command()`.
    fn ensure_commands_finalized(&self) {
        // Already finalized?
        if self.finalized_commands.borrow().is_some() {
            return;
        }

        let context_registry = &self.context_registry;

        // Build dispatch functions from recipes
        let mut commands = HashMap::new();
        for (path, pending) in self.pending_commands.borrow().iter() {
            let dispatch = pending.recipe.create_dispatch(
                &pending.template,
                context_registry,
                self.template_engine.clone(),
            );
            commands.insert(path.clone(), dispatch);
        }

        *self.finalized_commands.borrow_mut() = Some(commands);
    }

    /// Returns the finalized commands map, creating it if necessary.
    fn get_commands(&self) -> std::cell::Ref<'_, HashMap<String, DispatchFn>> {
        self.ensure_commands_finalized();
        std::cell::Ref::map(self.finalized_commands.borrow(), |opt| {
            opt.as_ref()
                .expect("finalized_commands should be Some after ensure_commands_finalized")
        })
    }

    /// Test helper: Check if a command path is registered.
    #[cfg(test)]
    pub(crate) fn has_command(&self, path: &str) -> bool {
        self.pending_commands.borrow().contains_key(path)
    }

    /// Finalizes the App, resolving themes, loading templates, and preparing
    /// for dispatch and rendering.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A `default_theme()` was specified but the theme wasn't found in the stylesheet registry
    /// - `command_groups` or topics are configured without `.help_handling(true)`
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let standout = App::new()
    ///     .styles(embed_styles!("src/styles"))
    ///     .default_theme("dark")
    ///     .build()?;
    /// ```
    pub fn build(mut self) -> Result<Self, SetupError> {
        use crate::assets::FRAMEWORK_TEMPLATES;

        // Add framework templates if enabled (BEFORE finalizing commands)
        if self.include_framework_templates {
            match self.template_registry.as_mut() {
                Some(arc) => {
                    // Get mutable access to the registry
                    if let Some(registry) = Rc::get_mut(arc) {
                        registry.add_framework_entries(FRAMEWORK_TEMPLATES);
                    } else {
                        // Shouldn't happen during build before finalization
                        panic!("template registry was shared before build completed");
                    }
                }
                None => {
                    // Create new registry with just framework templates
                    let mut registry = TemplateRegistry::new();
                    registry.add_framework_entries(FRAMEWORK_TEMPLATES);
                    self.template_registry = Some(Rc::new(registry));
                }
            };
        }

        // Populate engine with templates from registry
        // We use Rc::get_mut to mutate the engine in-place before sharing it
        if let Some(registry) = &self.template_registry {
            if let Some(engine_box) = Rc::get_mut(&mut self.template_engine) {
                for name in registry.names() {
                    if let Ok(content) = registry.get_content(name) {
                        let _ = engine_box.add_template(name, &content);
                    }
                }
            }
        }

        // Resolve theme BEFORE finalization
        // Theme resolution: explicit .theme() takes precedence, then .default_theme() from stylesheet registry
        if self.theme.is_none() {
            if let Some(ref mut registry) = self.stylesheet_registry {
                let resolved = if let Some(name) = &self.default_theme_name {
                    Some(
                        registry
                            .get(name)
                            .map_err(|_| SetupError::ThemeNotFound(name.to_string()))?,
                    )
                } else {
                    // Try defaults in order: default, theme, base
                    registry
                        .get("default")
                        .or_else(|_| registry.get("theme"))
                        .or_else(|_| registry.get("base"))
                        .ok()
                };
                self.theme = resolved;
            }
        }

        // Validate help configuration: features that require help interception
        // must not be used without enabling it.
        if !self.help_handling {
            let has_groups = self.help_command_groups.is_some();
            let has_topics = !self.registry.list_topics().is_empty();
            if has_groups || has_topics {
                let feature = if has_groups {
                    "command_groups"
                } else {
                    "topics"
                };
                return Err(SetupError::Config(format!(
                    "{feature} requires .help_handling(true) — \
                     standout cannot render grouped/topic help without intercepting help"
                )));
            }
        }

        // Finalize commands (now theme is resolved and will be captured correctly)
        self.ensure_commands_finalized();

        Ok(self)
    }

    /// Builds and parses CLI arguments in one step.
    ///
    /// # Panics
    ///
    /// Panics if building fails (e.g., theme not found). For proper error handling,
    /// use `build()` followed by `parse_with()` instead.
    pub fn parse(self, cmd: clap::Command) -> clap::ArgMatches {
        self.build().expect("Failed to build App").parse_with(cmd)
    }

    // =========================================================================
    // Accessors
    // =========================================================================

    /// Returns a reference to the topic registry.
    pub fn registry(&self) -> &TopicRegistry {
        &self.registry
    }

    /// Returns a mutable reference to the topic registry.
    pub fn registry_mut(&mut self) -> &mut TopicRegistry {
        &mut self.registry
    }

    /// Returns the current output mode (always Auto for the App itself;
    /// per-render mode is passed as a parameter).
    pub fn output_mode(&self) -> OutputMode {
        OutputMode::Auto
    }

    /// Returns the hooks registered for a specific command path.
    pub fn get_hooks(&self, path: &str) -> Option<&Hooks> {
        self.command_hooks.get(path)
    }

    /// Returns the default theme, if configured.
    pub fn get_default_theme(&self) -> Option<&Theme> {
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

    // =========================================================================
    // Parsing & Help
    // =========================================================================

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
    /// For most use cases, prefer `parse_with()` which handles help display automatically.
    pub fn get_matches(&self, cmd: Command) -> HelpResult {
        self.get_matches_from(cmd, std::env::args())
    }

    /// Attempts to get matches from the given arguments, intercepting `help` requests.
    ///
    /// When `help_handling` is enabled, all help invocations (`help`, `--help`, `-h`)
    /// are intercepted and rendered through standout. When disabled, only output flags
    /// are augmented and clap handles help natively.
    ///
    /// A naked invocation resolves its default command here too — statically via
    /// [`default_command`](Self::default_command) or per-invocation via
    /// [`default_command_with`](Self::default_command_with) — so the returned
    /// matches name the same command `dispatch_from` would have run. Consumers
    /// that parse first and build dispatch state afterwards therefore see one
    /// consistent answer.
    pub fn get_matches_from<I, T>(&self, cmd: Command, itr: I) -> HelpResult
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let mut cmd = self.augment_command_with_help(cmd);

        // Collect args so we can inspect them if clap returns DisplayHelp.
        let args: Vec<std::ffi::OsString> = itr.into_iter().map(Into::into).collect();

        let matches = match cmd.clone().try_get_matches_from(&args) {
            Ok(m) => m,
            Err(e) => {
                if self.help_handling && e.kind() == clap::error::ErrorKind::DisplayHelp {
                    // Clap's native --help/-h short-circuited parsing.
                    // Render standout help for the appropriate command.
                    return self.render_help_for_display_help_error(&mut cmd, &args);
                }
                return HelpResult::Error(e);
            }
        };

        // Only a successful naked parse can resolve a default command.
        let matches = match self.resolve_default_command(&cmd, &matches) {
            Err(e) => {
                return HelpResult::Error(
                    cmd.clone()
                        .error(clap::error::ErrorKind::InvalidSubcommand, e.to_string()),
                )
            }
            Ok(None) => matches,
            Ok(Some(default_cmd)) => {
                let args: Vec<std::ffi::OsString> = insert_default_command(
                    args.iter().map(|a| a.to_string_lossy().into_owned()),
                    &default_cmd,
                )
                .into_iter()
                .map(Into::into)
                .collect();

                match cmd.clone().try_get_matches_from(&args) {
                    Ok(m) => m,
                    Err(e) => {
                        if self.help_handling && e.kind() == clap::error::ErrorKind::DisplayHelp {
                            return self.render_help_for_display_help_error(&mut cmd, &args);
                        }
                        return HelpResult::Error(e);
                    }
                }
            }
        };

        if !self.help_handling {
            return HelpResult::Matches(matches);
        }

        // Extract output mode
        let output_mode = self.extract_output_mode(&matches);

        let config = HelpConfig {
            output_mode: Some(output_mode),
            theme: self.theme.clone(),
            command_groups: self.help_command_groups.clone(),
            ..Default::default()
        };

        // Check for the `help` subcommand (e.g. `myapp help`, `myapp help build`)
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
                return self.render_root_help(&cmd, Some(config), use_pager);
            }
        }

        HelpResult::Matches(matches)
    }

    /// Renders root help, returning an error if rendering fails.
    fn render_root_help(
        &self,
        cmd: &Command,
        config: Option<HelpConfig>,
        use_pager: bool,
    ) -> HelpResult {
        match render_help_with_topics(cmd, &self.registry, config) {
            Ok(h) => {
                if use_pager {
                    HelpResult::PagedHelp(h)
                } else {
                    HelpResult::Help(h)
                }
            }
            Err(e) => {
                let err = cmd.clone().error(
                    clap::error::ErrorKind::Io,
                    format!("failed to render help: {e}"),
                );
                HelpResult::Error(err)
            }
        }
    }

    /// Handles a `DisplayHelp` error from clap by rendering standout help.
    ///
    /// Walks the original args to determine which subcommand `--help` was
    /// requested for, then renders standout help for that command.
    fn render_help_for_display_help_error(
        &self,
        cmd: &mut Command,
        args: &[std::ffi::OsString],
    ) -> HelpResult {
        // Walk args (skip program name) to find the subcommand chain.
        // Stop at the first arg that isn't a known subcommand or is a flag.
        let subcommand_path = Self::extract_subcommand_path(cmd, args);

        let config = HelpConfig {
            theme: self.theme.clone(),
            command_groups: self.help_command_groups.clone(),
            ..Default::default()
        };

        if subcommand_path.is_empty() {
            return self.render_root_help(cmd, Some(config), false);
        }

        let keywords: Vec<&str> = subcommand_path.iter().map(|s| s.as_str()).collect();
        self.handle_help_request(cmd, &keywords, false, Some(config))
    }

    /// Extracts the subcommand chain from raw args by matching against known subcommands.
    fn extract_subcommand_path(cmd: &Command, args: &[std::ffi::OsString]) -> Vec<String> {
        let mut path = vec![];
        let mut current_cmd = cmd.clone();

        // Skip program name (first arg)
        for arg in args.iter().skip(1) {
            let arg_str = arg.to_string_lossy();

            // Skip flags
            if arg_str.starts_with('-') {
                continue;
            }

            // Check if this arg is a known subcommand at the current level
            let found = current_cmd
                .get_subcommands()
                .find(|s| s.get_name() == arg_str.as_ref())
                .cloned();

            if let Some(sub) = found {
                path.push(sub.get_name().to_string());
                current_cmd = sub;
            } else {
                break;
            }
        }
        path
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
        if super::app::find_subcommand(cmd, sub_name).is_some() {
            if let Some(target) = super::app::find_subcommand_recursive(cmd, keywords) {
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

    /// Augments a command with help subcommand and output flags.
    ///
    /// When `help_handling` is enabled, this disables clap's built-in help
    /// subcommand and replaces it with standout's own. Clap's native `--help`/`-h`
    /// flag is kept so it short-circuits arg validation (showing help even when
    /// required args are missing), but `DisplayHelp` errors are intercepted in
    /// `get_matches_from` and rendered through standout.
    ///
    /// When `help_handling` is disabled, clap's built-in help is left intact.
    pub fn augment_command_with_help(&self, cmd: Command) -> Command {
        let cmd = if self.help_handling {
            // Disable clap's help subcommand and replace with standout's.
            // Keep clap's native --help/-h flag — it short-circuits validation
            // so `myapp subcmd --help` works even with required args.
            // The resulting DisplayHelp error is intercepted in get_matches_from.
            cmd.disable_help_subcommand(true).subcommand(
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
            )
        } else {
            cmd
        };

        // Add output flags
        self.augment_command_for_dispatch(cmd)
    }

    /// Extracts the output mode from parsed ArgMatches.
    pub fn extract_output_mode(&self, matches: &ArgMatches) -> OutputMode {
        if self.output_flag.is_some() {
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
        }
    }

    // =========================================================================
    // Manual Command Execution
    // =========================================================================

    /// Executes a command handler with hooks applied automatically.
    ///
    /// This is for when you handle dispatch manually but still want
    /// to benefit from registered hooks.
    ///
    /// The method:
    /// 1. Runs pre-dispatch hooks (if any)
    /// 2. Calls your handler closure
    /// 3. Renders the result using the template
    /// 4. Runs post-output hooks (if any)
    /// 5. Returns the final output
    ///
    /// # Final writes
    ///
    /// This is the manual-dispatch seam, so it performs no final write. An
    /// [`Output::Artifact`](crate::cli::Output::Artifact) comes back as
    /// [`RenderedOutput::Artifact`] with its report serialized but not
    /// rendered: destination selection, the write, and the receipt-bearing
    /// report belong to [`dispatch`](Self::dispatch) / [`run`](Self::run),
    /// which own that transaction end to end.
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
        let mut ctx = CommandContext::new(
            path.split('.').map(String::from).collect(),
            self.app_state.clone(),
        );

        let hooks = self.command_hooks.get(path);

        // Run pre-dispatch hooks
        if let Some(hooks) = hooks {
            hooks.run_pre_dispatch(matches, &mut ctx)?;
        }

        // Run handler
        let result = handler(matches, &ctx);

        // Convert result to RenderedOutput
        let output = match result {
            Ok(HandlerOutput::Render(data)) => {
                let mut json_data = serde_json::to_value(&data)
                    .map_err(|e| HookError::post_dispatch("Serialization error").with_source(e))?;

                if let Some(hooks) = hooks {
                    json_data = hooks.run_post_dispatch(matches, &ctx, json_data)?;
                }

                let theme = self.theme.clone().unwrap_or_default();
                match render_auto(template, &json_data, &theme, OutputMode::Auto) {
                    Ok(rendered) => RenderedOutput::Text(TextOutput::plain(rendered)),
                    Err(e) => return Err(HookError::post_output("Render error").with_source(e)),
                }
            }
            Err(e) => {
                return Err(HookError::post_output("Handler error").with_source(e));
            }
            Ok(HandlerOutput::Silent) => RenderedOutput::Silent,
            Ok(HandlerOutput::Binary { data, filename }) => RenderedOutput::Binary(data, filename),
            Ok(HandlerOutput::Artifact(artifact)) => {
                let (bytes, suggested_destination, stdout_allowed, report) = artifact.into_parts();
                let report = match report {
                    Some(report) => {
                        let mut json = serde_json::to_value(&report).map_err(|e| {
                            HookError::post_dispatch("Serialization error").with_source(e)
                        })?;
                        if let Some(hooks) = hooks {
                            json = hooks.run_post_dispatch(matches, &ctx, json)?;
                        }
                        Some(json)
                    }
                    None => None,
                };
                RenderedOutput::Artifact(ArtifactOutput {
                    bytes,
                    suggested_destination,
                    stdout_allowed,
                    report,
                })
            }
            Ok(_) => {
                return Err(HookError::post_output(
                    "Unsupported handler output variant: this standout version cannot present it",
                ));
            }
        };

        // Run post-output hooks
        if let Some(hooks) = hooks {
            hooks.run_post_output(matches, &ctx, output)
        } else {
            Ok(output)
        }
    }

    // =========================================================================
    // Verification
    // =========================================================================

    /// Verifies that registered handlers match the CLI command definition.
    ///
    /// Checks that all required arguments expected by handlers are present
    /// in the clap Command definition with compatible types.
    pub fn verify_command(&self, cmd: &Command) -> Result<(), SetupError> {
        let expected_args: HashMap<String, Vec<ExpectedArg>> = self
            .pending_commands
            .borrow()
            .iter()
            .map(|(path, cmd)| (path.clone(), cmd.recipe.expected_args()))
            .collect();
        super::app::verify_recursive(cmd, &expected_args, &[], true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_output_flag_enabled_by_default() {
        let standout = AppBuilder::new().build().unwrap();
        assert!(standout.output_flag.is_some());
        assert_eq!(standout.output_flag.as_deref(), Some("output"));
    }

    #[test]
    fn test_no_output_flag() {
        let standout = AppBuilder::new().no_output_flag().build().unwrap();
        assert!(standout.output_flag.is_none());
    }

    #[test]
    fn test_custom_output_flag_name() {
        let standout = AppBuilder::new()
            .output_flag(Some("format"))
            .build()
            .unwrap();
        assert_eq!(standout.output_flag.as_deref(), Some("format"));
    }

    #[test]
    fn test_theme_fallback_precedence() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();

        // Create base.yaml
        fs::write(temp_dir.path().join("base.yaml"), "style: { fg: blue }").unwrap();

        // 1. Only base exists
        let app = AppBuilder::new()
            .styles_dir(temp_dir.path())
            .unwrap()
            .build()
            .unwrap();

        assert!(app.theme.is_some());
        let theme = app.theme.as_ref().unwrap();
        assert_eq!(theme.name(), Some("base"));

        // 2. theme.yaml exists (should override base)
        fs::write(temp_dir.path().join("theme.yaml"), "style: { fg: red }").unwrap();

        let app = AppBuilder::new()
            .styles_dir(temp_dir.path())
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(app.theme.as_ref().unwrap().name(), Some("theme"));

        // 3. default.yaml exists (should override theme)
        fs::write(temp_dir.path().join("default.yaml"), "style: { fg: green }").unwrap();

        let app = AppBuilder::new()
            .styles_dir(temp_dir.path())
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(app.theme.as_ref().unwrap().name(), Some("default"));
    }

    // ============================================================================
    // App State Tests
    // ============================================================================

    #[test]
    fn test_app_state_single_type() {
        struct Database {
            url: String,
        }

        let app = AppBuilder::new()
            .app_state(Database {
                url: "postgres://localhost".into(),
            })
            .build()
            .unwrap();

        let db = app.app_state.get::<Database>().unwrap();
        assert_eq!(db.url, "postgres://localhost");
    }

    #[test]
    fn test_app_state_multiple_types() {
        struct Database {
            url: String,
        }
        struct Config {
            debug: bool,
        }

        let app = AppBuilder::new()
            .app_state(Database {
                url: "postgres://localhost".into(),
            })
            .app_state(Config { debug: true })
            .build()
            .unwrap();

        let db = app.app_state.get::<Database>().unwrap();
        assert_eq!(db.url, "postgres://localhost");

        let config = app.app_state.get::<Config>().unwrap();
        assert!(config.debug);
    }

    #[test]
    fn test_app_state_replacement() {
        struct Config {
            value: i32,
        }

        let app = AppBuilder::new()
            .app_state(Config { value: 1 })
            .app_state(Config { value: 2 }) // Replaces first
            .build()
            .unwrap();

        let config = app.app_state.get::<Config>().unwrap();
        assert_eq!(config.value, 2);
    }

    #[test]
    fn test_app_state_empty_by_default() {
        struct NotSet;

        let app = AppBuilder::new().build().unwrap();

        assert!(app.app_state.is_empty());
        assert!(app.app_state.get::<NotSet>().is_none());
    }
}
