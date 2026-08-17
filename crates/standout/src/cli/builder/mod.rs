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
use std::rc::Rc;

use super::default_command::ParseFailure;
use super::dispatch::DispatchFn;
use super::group::CommandRecipe;
use super::handler::{CommandContext, Extensions, HandlerResult, Output as HandlerOutput};
use super::help::{render_help, render_help_with_topics, CommandGroup, HelpConfig, HelpLength};
use super::hooks::{ArtifactOutput, HookError, Hooks, RenderedOutput, TextOutput};
use super::questionnaire::QuestionnaireCommand;
use super::result::{HelpDisplay, HelpResult};
use standout_dispatch::verify::ExpectedArg;

pub(crate) type SharedTemplateEngine =
    Rc<RefCell<Box<dyn standout_render::template::TemplateEngine>>>;

/// The presentation configuration a command declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TemplateRef {
    /// A named template that must resolve through the template registry.
    Named(String),
    /// Inline MiniJinja source carried directly on the command.
    Inline(String),
    /// A command that deliberately has no human template.
    Absent(TemplateAbsence),
}

/// Why a command has no human template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TemplateAbsence {
    /// The command performs side effects and intentionally emits no output.
    Silent,
    /// Rendered data is available only through structured output modes.
    StructuredOnly,
    /// The command's success channel is binary data, not presentation text.
    Binary,
}

impl TemplateRef {
    pub(crate) fn explicit(template: impl Into<String>) -> Self {
        let template = template.into();
        if template.is_empty() {
            return Self::Absent(TemplateAbsence::Silent);
        }
        if looks_like_inline_template(&template) {
            return Self::Inline(template);
        }
        if looks_like_template_name(&template) {
            Self::Named(template)
        } else {
            Self::Inline(template)
        }
    }

    pub(crate) fn convention(command_path: &str, template_ext: &str) -> Self {
        let file_path = command_path.replace('.', "/");
        Self::Named(format!("{}{}", file_path, template_ext))
    }
}

fn looks_like_inline_template(template: &str) -> bool {
    template.contains("{{")
        || template.contains("{%")
        || template.contains("{#")
        || template.contains("[/")
        || template.contains('\n')
}

fn looks_like_template_name(template: &str) -> bool {
    template.contains('/')
        || standout_render::template::TEMPLATE_EXTENSIONS
            .iter()
            .any(|extension| template.ends_with(extension))
}

pub(crate) fn refresh_engine_templates(
    engine: &mut dyn standout_render::template::TemplateEngine,
    registry: &TemplateRegistry,
) -> Result<(), standout_render::RenderError> {
    for name in registry.names() {
        let content = registry
            .get_content(name)
            .map_err(|error| standout_render::RenderError::OperationError(error.to_string()))?;
        engine.add_template(name, &content)?;
    }
    Ok(())
}

fn missing_template_message(
    command_path: &str,
    template_name: &str,
    registry: Option<&TemplateRegistry>,
) -> String {
    let mut message = format!(
        "command `{command_path}` references template `{template_name}`, but that template is not registered; add it with .templates(...) or .templates_dir(...)"
    );
    if let Some(registry) = registry {
        let suggestions = nearest_template_names(template_name, registry);
        if !suggestions.is_empty() {
            message.push_str("; did you mean ");
            message.push_str(&suggestions.join(", "));
            message.push('?');
        }
    }
    message
}

fn nearest_template_names(name: &str, registry: &TemplateRegistry) -> Vec<String> {
    let mut candidates: Vec<(usize, String)> = registry
        .names()
        .map(|candidate| (edit_distance(name, candidate), candidate.to_string()))
        .filter(|(distance, candidate)| {
            *distance <= 3 || candidate.contains(name) || name.contains(candidate)
        })
        .collect();
    candidates.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    candidates.dedup_by(|left, right| left.1 == right.1);
    candidates
        .into_iter()
        .take(3)
        .map(|(_, candidate)| format!("`{candidate}`"))
        .collect()
}

fn edit_distance(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    let mut costs: Vec<usize> = (0..=right.len()).collect();

    for (i, left_char) in left.iter().enumerate() {
        let mut previous = costs[0];
        costs[0] = i + 1;
        for (j, right_char) in right.iter().enumerate() {
            let substitution = previous + usize::from(left_char != right_char);
            previous = costs[j + 1];
            costs[j + 1] = (costs[j + 1] + 1).min(costs[j] + 1).min(substitution);
        }
    }

    costs[right.len()]
}

/// Stores a pending command recipe along with its typed template declaration.
struct PendingCommand {
    recipe: Box<dyn CommandRecipe>,
    template: TemplateRef,
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
    pub(crate) questionnaire_commands: HashMap<String, QuestionnaireCommand>,
    pub(crate) context_registry: ContextRegistry,
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
    pub(crate) template_engine: SharedTemplateEngine,

    /// Command groups for organized help display.
    pub(crate) help_command_groups: Option<Vec<CommandGroup>>,

    /// Whether standout intercepts and renders help (default: false).
    ///
    /// When true, standout replaces clap's built-in help subcommand with its
    /// own — where the install policy allows, see `help_word` — and renders
    /// themed, grouped help for every invocation form (`help`, `--help`, `-h`).
    /// Required when using `command_groups` or topics.
    pub(crate) help_handling: bool,

    /// Whether a flat CLI with positionals opts into the `help` word.
    ///
    /// Only consulted for the one shape standout will not decide on its own —
    /// see [`installs_help_word`](AppBuilder::installs_help_word).
    pub(crate) help_word: bool,

    /// Explicit East Asian Ambiguous width policy.
    pub(crate) ambiguous_width: crate::AmbiguousWidth,

    /// Application version metadata, applied to the root clap command.
    ///
    /// `None` leaves the supplied command's own version — whatever the
    /// application configured on clap directly — untouched. See
    /// [`version`](AppBuilder::version).
    ///
    /// Held as `&'static str` because clap's `Str` takes runtime-built strings
    /// only under its `string` feature; the borrow is leaked once, when the
    /// builder is configured, rather than on every parse.
    pub(crate) version: Option<&'static str>,
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
            questionnaire_commands: HashMap::new(),
            context_registry: ContextRegistry::new(),
            template_ext: ".j2".to_string(),
            default_command: None,
            default_command_resolver: None,
            include_framework_templates: true,
            include_framework_styles: true,
            app_state: Rc::new(Extensions::new()),
            template_engine: Rc::new(RefCell::new(Box::new(
                standout_render::template::MiniJinjaEngine::new(),
            ))),
            help_command_groups: None,
            help_handling: false,
            help_word: false,
            ambiguous_width: crate::AmbiguousWidth::Narrow,
            version: None,
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
        self.template_engine = Rc::new(RefCell::new(engine));
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
                self.template_registry.clone(),
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

    /// Finalizes the App, resolving themes, validating typed template
    /// declarations, loading templates, and preparing for dispatch and
    /// rendering.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A `default_theme()` was specified but the theme wasn't found in the stylesheet registry
    /// - a command references a named template that is not in the template registry
    /// - a registered template fails to compile
    /// - `command_groups` or topics are configured without `.help_handling(true)`
    /// - a command is registered under the root `help` with `.help_handling(true)`,
    ///   which is the name standout installs its own word under
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
            if self.help_word {
                return Err(SetupError::Config(
                    "help_word requires .help_handling(true) — the `help` word is \
                     standout's own subcommand, so there is nothing to install without \
                     help interception"
                        .to_string(),
                ));
            }
        }

        // A command registered under the root `help` is the same collision the
        // parse paths catch on a declared one, seen one step earlier — and
        // unconditionally, without the assembled shape the install policy
        // reads. It needs none: a registered `help` only ever runs if the
        // application's `Command` declares the word too, and a root with
        // subcommands always gets standout's (see `installs_help_word`). So the
        // registration is either shadowed by the word or dead, and both are the
        // author's to resolve.
        if self.help_handling {
            // Lowest path first, so a root carrying several registrations under
            // `help` names the same one on every run.
            let claim = self
                .pending_commands
                .borrow()
                .keys()
                .filter(|path| claims_root_help(path))
                .min()
                .cloned();
            if let Some(path) = claim {
                return Err(duplicate_help_word(&registered_claim(&path)));
            }
        }

        self.validate_command_templates()?;

        // Populate engine with templates from the registry and keep the compile
        // result. Named renders refresh this cache again so file-backed
        // templates can hot reload.
        if let Some(registry) = &self.template_registry {
            refresh_engine_templates(&mut **self.template_engine.borrow_mut(), registry)
                .map_err(|error| SetupError::Template(error.to_string()))?;
        }

        // Finalize commands (now theme is resolved and will be captured correctly)
        self.ensure_commands_finalized();

        Ok(self)
    }

    fn validate_command_templates(&self) -> Result<(), SetupError> {
        for (path, pending) in self.pending_commands.borrow().iter() {
            if let TemplateRef::Named(name) = &pending.template {
                let registry = self.template_registry.as_ref().ok_or_else(|| {
                    SetupError::Template(missing_template_message(path, name, None))
                })?;
                registry.get_content(name).map_err(|_| {
                    SetupError::Template(missing_template_message(path, name, Some(registry)))
                })?;
            }
        }
        Ok(())
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
    /// When `help_handling` is enabled, every help invocation is intercepted and
    /// rendered through standout: `--help` / `-h` always, and the bare `help`
    /// word where the install policy put it (see
    /// [`help_word`](Self::help_word)). When disabled, only output flags are
    /// augmented and clap handles help natively.
    ///
    /// Which command a line means is Clap's answer, read off the parse. Only a
    /// parse that selected no command is naked, and only a naked line resolves
    /// a default command — statically via
    /// [`default_command`](Self::default_command) or per-invocation via
    /// [`default_command_with`](Self::default_command_with). `dispatch_from`
    /// parses through the same seam, so consumers that parse first and build
    /// dispatch state afterwards see one consistent answer.
    ///
    /// A command that declares its own `help` where standout installs the word
    /// is a configuration standout will not serve: it comes back as
    /// [`HelpResult::Error`] carrying
    /// [`SetupError::DuplicateCommand`](crate::SetupError::DuplicateCommand)'s
    /// report, before anything is parsed.
    pub fn get_matches_from<I, T>(&self, cmd: Command, itr: I) -> HelpResult
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let mut cmd = self.augment_command_with_help(cmd);

        // The application's `Command` is only visible from a parse entry point,
        // so a `help` it declares is only refusable here — and only before the
        // parse, which is where Clap's duplicate-subcommand assertion would
        // fire. It is the application's configuration at fault, not the line,
        // but this path speaks `clap::Error` for every failure, so the setup
        // error is raised as one rather than routed around the return type.
        if let Some(error) = self.help_word_collision(&cmd) {
            return HelpResult::Error(clap::Error::raw(
                clap::error::ErrorKind::InvalidSubcommand,
                format!("{error}\n"),
            ));
        }

        // Verbatim, all the way to Clap: a non-UTF8 argument is a real argument.
        let args: Vec<std::ffi::OsString> = itr.into_iter().map(Into::into).collect();

        let matches = match self.parse_with_default_command(&cmd, &args) {
            Ok(matches) => matches,
            Err(ParseFailure::UnknownDefault(e)) => {
                return HelpResult::Error(
                    cmd.clone()
                        .error(clap::error::ErrorKind::InvalidSubcommand, e.to_string()),
                )
            }
            Err(ParseFailure::Clap(e)) => {
                return match self.intercept_display_help(&mut cmd, &args, &e) {
                    Some(display) => display.into(),
                    None => HelpResult::Error(e),
                }
            }
        };

        match self.intercept_help_word(&mut cmd, &matches) {
            Some(display) => display.into(),
            None => HelpResult::Matches(matches),
        }
    }

    /// Answers the `help` word, when Clap routed the line to it.
    ///
    /// The word is a declared subcommand, so Clap parses it and its arguments
    /// like any other; this reads the result. `None` means the line went
    /// somewhere else, which is when the caller's matches stand.
    ///
    /// Both parse paths call this on their parse, so `get_matches_from` and
    /// `dispatch_from` answer the word identically.
    pub(crate) fn intercept_help_word(
        &self,
        cmd: &mut Command,
        matches: &ArgMatches,
    ) -> Option<HelpDisplay> {
        if !self.help_handling {
            return None;
        }
        let (name, sub_matches) = matches.subcommand()?;
        (name == "help").then(|| self.render_help_word(cmd, matches, sub_matches))
    }

    /// Answers Clap's `DisplayHelp` short-circuit, when standout owns help.
    ///
    /// Clap's native `--help`/`-h` is kept on purpose — it short-circuits
    /// argument validation — so the request arrives as an "error" from the
    /// authoritative parse. Both parse paths hand it here to be rendered
    /// through standout instead of surfaced as Clap's own text. `None` means
    /// the error was not a help request (or standout does not own help), and
    /// belongs to the caller.
    pub(crate) fn intercept_display_help(
        &self,
        cmd: &mut Command,
        args: &[std::ffi::OsString],
        error: &clap::Error,
    ) -> Option<HelpDisplay> {
        (self.help_handling && error.kind() == clap::error::ErrorKind::DisplayHelp)
            .then(|| self.render_help_for_display_help_error(cmd, args))
    }

    /// Renders the help the `help` word asked for.
    ///
    /// Its arguments come from Clap: `sub_matches` is the word's own parse
    /// (`topic`, `--page`), and the output mode is read from the root, where
    /// the global flag that carries it lives.
    fn render_help_word(
        &self,
        cmd: &mut Command,
        matches: &ArgMatches,
        sub_matches: &ArgMatches,
    ) -> HelpDisplay {
        let config = HelpConfig {
            output_mode: Some(self.extract_output_mode(matches)),
            theme: self.theme.clone(),
            command_groups: self.help_command_groups.clone(),
            // The word is the spelled-out request, so it reads like `--help`.
            length: HelpLength::Long,
            ..Default::default()
        };
        let use_pager = sub_matches.get_flag("page");

        if let Some(topic_args) = sub_matches.get_many::<String>("topic") {
            let keywords: Vec<_> = topic_args.map(|s| s.as_str()).collect();
            if !keywords.is_empty() {
                return self.handle_help_request(cmd, &keywords, use_pager, Some(config));
            }
        }

        self.render_root_help(cmd, Some(config), use_pager)
    }

    /// Reports a failed help render.
    ///
    /// Every rendering step funnels here, because a broken template or theme is
    /// the application's bug however help was asked for. Reporting it as
    /// [`HelpDisplay::RenderFailed`] is what keeps it from reaching the user as
    /// a usage error — or, worse, as "that topic wasn't recognized", which is
    /// what a swallowed render failure used to look like.
    fn render_failure(cmd: &Command, error: impl std::fmt::Display) -> HelpDisplay {
        HelpDisplay::RenderFailed(cmd.clone().error(
            clap::error::ErrorKind::Io,
            format!("failed to render help: {error}"),
        ))
    }

    /// Renders root help, returning an error if rendering fails.
    fn render_root_help(
        &self,
        cmd: &Command,
        config: Option<HelpConfig>,
        use_pager: bool,
    ) -> HelpDisplay {
        match render_help_with_topics(cmd, &self.registry, config) {
            Ok(text) => HelpDisplay::Rendered {
                text,
                paged: use_pager,
            },
            Err(e) => Self::render_failure(cmd, e),
        }
    }

    /// Handles a `DisplayHelp` error from clap by rendering standout help.
    ///
    /// Which command's help to render is Clap's answer, not a reading of the
    /// arguments: `--help` short-circuits before producing matches and its
    /// error does not name the command it was raised for, so the line is handed
    /// back to Clap with the help flag disabled. Everything that could name a
    /// command precedes the flag, so `ignore_errors` tolerating the now-unknown
    /// flag (and whatever follows it) costs nothing here.
    ///
    /// No output mode is threaded through: the request short-circuited, so
    /// `--output` written after it was never parsed, and honouring only the
    /// half written before it would make the mode depend on where the user put
    /// it. The render falls back to [`OutputMode::Auto`]; the `help` word does
    /// honour the flag, because Clap parses the word's line in full. The
    /// asymmetry is documented in `docs/topics/standout-help.md`.
    ///
    /// Which *spelling* asked is read off the raw line, by
    /// [`help_request`], which reads it off the same re-parse that answers
    /// *which* command was asked about — the error carries neither.
    fn render_help_for_display_help_error(
        &self,
        cmd: &mut Command,
        args: &[std::ffi::OsString],
    ) -> HelpDisplay {
        let request = Self::help_request(cmd, args);

        let config = HelpConfig {
            theme: self.theme.clone(),
            command_groups: self.help_command_groups.clone(),
            length: request.length,
            ..Default::default()
        };

        if request.target.is_empty() {
            return self.render_root_help(cmd, Some(config), false);
        }

        let keywords: Vec<&str> = request.target.iter().map(|s| s.as_str()).collect();
        self.handle_help_request(cmd, &keywords, false, Some(config))
    }

    /// What a `DisplayHelp` line asked for, as Clap reads it.
    ///
    /// Two facts Clap's error does not carry: which command the request was
    /// raised for, and which spelling raised it. Both are read off a parse
    /// rather than off the argument list, per
    /// [ADR-0018](../../../../docs/adr/0018-let-the-parser-classify-the-command-line.md)
    /// — a scan looking for `-h` would have to reimplement `--` termination,
    /// `--flag=value`, short-option clusters, and which options consume the
    /// token after them (`-o h` is not a help request), and a scan wrong about
    /// any of those is a parser with unknown coverage.
    ///
    /// It takes *two* parses, because the two questions want opposite
    /// declarations of the same flag. See [`help_target`](Self::help_target)
    /// and [`help_length`](Self::help_length).
    fn help_request(cmd: &Command, args: &[std::ffi::OsString]) -> HelpRequest {
        HelpRequest {
            target: Self::help_target(cmd, args),
            length: Self::help_length(cmd, args),
        }
    }

    /// Which spelling raised the request: `--help` (long) or `-h` (short).
    ///
    /// The flags are re-declared as ordinary global arguments on a throwaway
    /// clone whose own help flag is disabled, so Clap classifies them instead
    /// of short-circuiting on them. Global, so a request raised deep in the
    /// tree still reports at the root.
    ///
    /// `-h` is declared but never read. Its value is not the answer — absence
    /// of `--help` already is — but declaring it keeps Clap's lexer accurate
    /// for the rest of the line, rather than leaving an unknown token for
    /// `ignore_errors` to absorb.
    fn help_length(cmd: &Command, args: &[std::ffi::OsString]) -> HelpLength {
        let probe = cmd
            .clone()
            .disable_help_flag(true)
            .ignore_errors(true)
            .arg(
                Arg::new(HELP_PROBE_SHORT)
                    .short('h')
                    .action(ArgAction::SetTrue)
                    .global(true)
                    .hide(true),
            )
            .arg(
                Arg::new(HELP_PROBE_LONG)
                    .long("help")
                    .action(ArgAction::SetTrue)
                    .global(true)
                    .hide(true),
            );

        match probe.try_get_matches_from(args) {
            Ok(matches) if matches.get_flag(HELP_PROBE_LONG) => HelpLength::Long,
            _ => HelpLength::Short,
        }
    }

    /// The command chain a help request was raised for, as Clap reads it.
    ///
    /// Empty means the root. Disabling the help flag is what lets the parse run
    /// far enough to answer: with it enabled the parse short-circuits again and
    /// reports nothing.
    ///
    /// The help flag is deliberately left *undeclared* here, which is the
    /// opposite of what [`help_length`](Self::help_length) needs and the reason
    /// the two are separate parses. An undeclared `--help` is an unknown token,
    /// so `ignore_errors` truncates the parse where it appears — and that is
    /// exactly the wanted reading: in `app --help list`, help was asked before
    /// any command was named, so it is the root's, and the walk has to stop at
    /// the flag rather than stride past it to `list`.
    fn help_target(cmd: &Command, args: &[std::ffi::OsString]) -> Vec<String> {
        let Ok(matches) = cmd
            .clone()
            .disable_help_flag(true)
            .ignore_errors(true)
            .try_get_matches_from(args)
        else {
            return Vec::new();
        };

        let mut chain = Vec::new();
        let mut current = &matches;
        while let Some((name, sub)) = current.subcommand() {
            chain.push(name.to_string());
            current = sub;
        }
        chain
    }

    /// Handles a request for specific help e.g. `help foo`
    fn handle_help_request(
        &self,
        cmd: &mut Command,
        keywords: &[&str],
        use_pager: bool,
        config: Option<HelpConfig>,
    ) -> HelpDisplay {
        let sub_name = keywords[0];

        // 0. Check for "topics" - list all available topics
        if sub_name == "topics" {
            let topic_config = TopicRenderConfig {
                output_mode: config.as_ref().and_then(|c| c.output_mode),
                theme: config.as_ref().and_then(|c| c.theme.clone()),
                ..Default::default()
            };
            return match render_topics_list(
                &self.registry,
                &format!("{} help", cmd.get_name()),
                Some(topic_config),
            ) {
                Ok(text) => HelpDisplay::Rendered {
                    text,
                    paged: use_pager,
                },
                Err(e) => Self::render_failure(cmd, e),
            };
        }

        // 1. Check if it's a real command
        if super::app::find_subcommand(cmd, sub_name).is_some() {
            if let Some(target) = super::app::find_subcommand_recursive(cmd, keywords) {
                return match render_help(target, config.clone()) {
                    Ok(text) => HelpDisplay::Rendered {
                        text,
                        paged: use_pager,
                    },
                    Err(e) => Self::render_failure(cmd, e),
                };
            }
        }

        // 2. Check if it is a topic
        if let Some(topic) = self.registry.get_topic(sub_name) {
            let topic_config = TopicRenderConfig {
                output_mode: config.as_ref().and_then(|c| c.output_mode),
                theme: config.as_ref().and_then(|c| c.theme.clone()),
                ..Default::default()
            };
            return match render_topic(topic, Some(topic_config)) {
                Ok(text) => HelpDisplay::Rendered {
                    text,
                    paged: use_pager,
                },
                Err(e) => Self::render_failure(cmd, e),
            };
        }

        // 3. Not found
        let err = cmd.error(
            clap::error::ErrorKind::InvalidSubcommand,
            format!("The subcommand or topic '{}' wasn't recognized", sub_name),
        );
        HelpDisplay::Clap(err)
    }

    /// Augments a command with the `help` word and output flags.
    ///
    /// When `help_handling` is enabled, this disables clap's built-in help
    /// subcommand and installs standout's own, where the install policy allows
    /// it (see [`help_word`](Self::help_word)). Clap's native
    /// `--help`/`-h` flag is kept so it short-circuits arg validation (showing
    /// help even when required args are missing), but `DisplayHelp` errors are
    /// intercepted — by `get_matches_from` and `dispatch_from` alike — and
    /// rendered through standout.
    ///
    /// When `help_handling` is disabled, clap's built-in help is left intact.
    ///
    /// Both parse paths augment through here, so the word's install policy is
    /// the command's shape and never the entry point the application chose.
    ///
    /// The word is installed whether or not the application already claims the
    /// name; what comes back is then a root with two `help` subcommands, which
    /// is a configuration standout refuses rather than serves. Refusing is the
    /// caller's move, not this seam's: augmentation has no error currency, and
    /// Clap's duplicate-subcommand assertion fires when a command is *parsed*,
    /// not when a subcommand is registered — so both parse entry points read
    /// the collision off the augmented root (`help_word_collision`) and refuse
    /// before the parse that would panic. Augmenting by hand and parsing the
    /// result yourself is the one path standout is not on: there Clap answers
    /// first, with the assertion, as it always has.
    ///
    /// # Ordering: shape-dependent decisions come last
    ///
    /// The framework's own surface is augmented **first**, and only then is the
    /// install policy evaluated. The rule behind that is general, and this is
    /// the easiest place to break it:
    ///
    /// > A decision that branches on the command's *assembled* shape may only
    /// > be evaluated once all structural augmentation has completed.
    ///
    /// [`augment_framework_surface`](Self::augment_framework_surface) is the
    /// augmentation in question: it injects the questionnaire surface through
    /// `augment_questionnaire_commands`, which adds a `questions` subcommand at
    /// every registered questionnaire path — the root included. So a root that
    /// declares no subcommands of its own can still have one by the time a user
    /// meets it, and "does this root have subcommands?" asked any earlier
    /// answers for a shape nobody runs.
    ///
    /// [`installs_help_word`](Self::installs_help_word) is the decision that
    /// rule exists for, and the duplicate-`help` refusal reads the same
    /// assembled root one step later, in the callers. A third such decision
    /// belongs below the same line, not above it.
    ///
    /// The opposite constraint exists and is not a contradiction: the
    /// questionnaire *validators* (`validate_questionnaire_surfaces`,
    /// `validate_command_groups`) read the shape the application author wrote,
    /// precisely to catch names that collide with what the framework is about
    /// to inject, so they run before augmentation and must keep doing so.
    pub fn augment_command_with_help(&self, cmd: Command) -> Command {
        let cmd = self.augment_framework_surface(cmd);

        if !self.help_handling {
            return cmd;
        }

        // Disable clap's help subcommand and replace with standout's.
        // Keep clap's native --help/-h flag — it short-circuits validation
        // so `myapp subcmd --help` works even with required args.
        // The resulting DisplayHelp error is intercepted by both parse paths.
        let cmd = cmd.disable_help_subcommand(true);
        if self.installs_help_word(&cmd) {
            // Read before the word is added, so it answers "does the
            // application have commands of its own?" — the shape the word's
            // wording has to be true of. Asked after, every root has one.
            let has_subcommands = cmd.get_subcommands().next().is_some();
            // `subcommand_negates_reqs` is what makes the installed word
            // reachable: without it a root that requires arguments rejects
            // `myapp help` before Clap can route it, which is the defect this
            // whole surface exists to fix. It is set *here*, and only here,
            // because it relaxes the root's requirements for the application's
            // own subcommands too — a semantic an app that did not get the word
            // never asked for.
            cmd.subcommand(help_word_command(has_subcommands))
                .subcommand_negates_reqs(true)
        } else {
            cmd
        }
    }

    /// The collision, when the application claims `help` on a root standout
    /// installed its own word on.
    ///
    /// Read off the *augmented* root, where standout's word is already there,
    /// so two claims on the name means one of them is the application's. That
    /// is the whole reason it is asked here rather than before augmentation: no
    /// caller re-derives whether the word was installed, or under what policy —
    /// it reads what the policy left behind. The `help_handling` guard is what
    /// keeps the report true: with interception off standout claims nothing, so
    /// two `help`s are both the application's and none of standout's business.
    ///
    /// A registration claiming the word is caught earlier, by
    /// [`build`](Self::build); this is the half that cannot be, since the
    /// application's `Command` reaches standout only at parse time.
    pub(crate) fn help_word_collision(&self, augmented: &Command) -> Option<SetupError> {
        if !self.help_handling {
            return None;
        }
        let claims = augmented
            .get_subcommands()
            .filter(|sub| claims_help(sub))
            .count();
        (claims > 1).then(|| duplicate_help_word(DECLARED_CLAIM))
    }

    /// Whether the bare `help` word is installed on this root.
    ///
    /// Installing it reserves the word out of the root's data namespace, which
    /// is only standout's call to make when nothing else could claim it:
    ///
    /// - the root **has subcommands** — a bare word there is already a command;
    /// - the root is **flat with no positionals** — there is nothing to collide
    ///   with;
    /// - the root is **flat with positionals** — a bare word is data
    ///   (`echo help`), so the word is installed only behind
    ///   [`help_word(true)`](Self::help_word).
    ///
    /// `--help` / `-h` are unaffected either way: they are Clap's flags, always
    /// present, and their `DisplayHelp` is rendered through standout.
    ///
    /// # `cmd` must be the assembled command
    ///
    /// This branches on the command's shape, so it may only be asked once all
    /// structural augmentation has run: the framework injects subcommands of
    /// its own (the questionnaire `questions` command, at the root among other
    /// paths), and a root that gains one is a root where a bare word is already
    /// a command. [`augment_command_with_help`](Self::augment_command_with_help)
    /// is the only caller and orders itself accordingly — see the ordering rule
    /// on it, which is the general form of this requirement and the thing to
    /// preserve if a second shape-dependent decision is ever added.
    pub(crate) fn installs_help_word(&self, cmd: &Command) -> bool {
        self.help_word
            || cmd.get_subcommands().next().is_some()
            || cmd.get_positionals().next().is_none()
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
        self.validate_questionnaire_surfaces(cmd)?;
        let expected_args: HashMap<String, Vec<ExpectedArg>> = self
            .pending_commands
            .borrow()
            .iter()
            .map(|(path, cmd)| (path.clone(), cmd.recipe.expected_args()))
            .collect();
        super::app::verify_recursive(cmd, &expected_args, &[], true)
    }
}

/// Whether this subcommand claims the name `help`.
///
/// Aliases count: Clap resolves an alias to its command, so a subcommand
/// aliased `help` is as much a claim on the word as one named it.
fn claims_help(cmd: &Command) -> bool {
    cmd.get_name() == "help" || cmd.get_all_aliases().any(|alias| alias == "help")
}

/// Whether a registered command path claims the root `help` word.
///
/// The first segment is the claim: `help` is the word itself, and `help.topic`
/// — or anything a `.group("help", …)` registers — hangs a command off it, so
/// the word standout installs is in its way just the same. Deeper segments are
/// the application's own: `db.help` sits where the word is never installed.
fn claims_root_help(path: &str) -> bool {
    path == "help" || path.starts_with("help.")
}

/// The clause naming a `help` declared on the application's clap `Command`.
const DECLARED_CLAIM: &str =
    "this application's clap `Command` declares `help` (as a subcommand name or alias)";

/// The clause naming a registered claim on the root `help`.
fn registered_claim(path: &str) -> String {
    if path == "help" {
        "this application registers a `help` command".to_string()
    } else {
        format!("this application registers `{path}`, hanging a command off the same root word")
    }
}

/// The collision report for an application `help` meeting standout's word.
///
/// [`SetupError::DuplicateCommand`] renders as `duplicate command: {payload}`,
/// which is complete guidance when both colliding commands are the author's:
/// they can see both and delete one. Here one of the two is standout's,
/// injected by an install policy the author may not know exists — so the
/// payload carries what the name alone cannot: the setting that installs the
/// word, and the two ways out. `claim` is the clause naming where the
/// application's own `help` was found, the one thing the author has to go look
/// at.
fn duplicate_help_word(claim: &str) -> SetupError {
    SetupError::DuplicateCommand(format!(
        "help — {claim}, and standout installs a `help` word of its own under \
         .help_handling(true). Rename the application's command, or drop \
         .help_handling(true) to keep the name (help is then clap's own, and \
         command_groups and topics become unavailable)"
    ))
}

/// Argument ids for the flags [`AppBuilder::help_request`] re-declares.
///
/// Named out of the application's namespace: they exist only on a throwaway
/// clone, but a collision with a real argument would silently change what the
/// probe reports.
const HELP_PROBE_SHORT: &str = "__standout_help_short";
const HELP_PROBE_LONG: &str = "__standout_help_long";

/// What a help request named, once Clap has read the line.
///
/// Defaults to the root and the terse description, which is what an
/// unparseable line gets.
#[derive(Debug, Default, PartialEq, Eq)]
struct HelpRequest {
    /// The command chain the request was raised for; empty means the root.
    target: Vec<String>,
    /// Which description to render.
    length: HelpLength,
}

/// Standout's `help` word, the replacement for clap's built-in one.
///
/// Built in one place because it is both installed on the root and parsed
/// standalone when the word is dispatched, and the two must agree on what
/// arguments the word takes.
///
/// `has_subcommands` is the root's shape, and it is the whole reason this takes
/// an argument: clap's wording points at "the given subcommand(s)", which on a
/// flat CLI names a namespace that cannot exist. The flat shape is the one
/// [`help_word`](AppBuilder::help_word) exists to serve, so it gets a sentence
/// that is true of it.
fn help_word_command(has_subcommands: bool) -> Command {
    let (about, topic_help) = if has_subcommands {
        (
            "Print this message or the help of the given subcommand(s)",
            "The subcommand or topic to print help for",
        )
    } else {
        ("Print this message", "The topic to print help for")
    };

    Command::new("help")
        .about(about)
        .arg(
            Arg::new("topic")
                .action(ArgAction::Set)
                .num_args(1..)
                .help(topic_help),
        )
        .arg(
            Arg::new("page")
                .long("page")
                .action(ArgAction::SetTrue)
                .help("Display help through a pager"),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A root with a value-taking option, a flag, and a positional — enough
    /// shape for the lexing the scan would have had to reimplement.
    fn probe_command() -> Command {
        Command::new("app")
            .arg(Arg::new("out").short('o').long("out"))
            .arg(Arg::new("verbose").short('v').action(ArgAction::SetTrue))
            .arg(Arg::new("range"))
            .subcommand(Command::new("build").arg(Arg::new("target")))
    }

    fn request(args: &[&str]) -> HelpRequest {
        let args: Vec<std::ffi::OsString> = args.iter().map(Into::into).collect();
        AppBuilder::help_request(&probe_command(), &args)
    }

    #[test]
    fn test_help_request_reads_the_spelling() {
        assert_eq!(request(&["app", "--help"]).length, HelpLength::Long);
        assert_eq!(request(&["app", "-h"]).length, HelpLength::Short);
    }

    #[test]
    fn test_help_request_reads_the_target_command() {
        let deep = request(&["app", "build", "--help"]);
        assert_eq!(deep.target, vec!["build".to_string()]);
        assert_eq!(deep.length, HelpLength::Long);

        assert!(request(&["app", "--help"]).target.is_empty());
    }

    /// The two parses answer independently: help was asked before any command
    /// was named, so the target is the root — while the spelling is still
    /// long. Reading both off one parse got this wrong, rendering `build`'s
    /// help for a request that never named it.
    #[test]
    fn test_help_request_separates_the_spelling_from_the_target() {
        let early = request(&["app", "--help", "build"]);
        assert!(
            early.target.is_empty(),
            "the walk must stop at the flag, got {:?}",
            early.target
        );
        assert_eq!(early.length, HelpLength::Long);

        let short = request(&["app", "-h", "build"]);
        assert!(short.target.is_empty());
        assert_eq!(short.length, HelpLength::Short);
    }

    /// `-vh` is a cluster ending in the help flag.
    #[test]
    fn test_help_request_reads_short_flag_clusters() {
        assert_eq!(request(&["app", "-vh"]).length, HelpLength::Short);
    }

    #[test]
    fn test_help_request_reads_inline_values() {
        assert_eq!(
            request(&["app", "--out=x", "--help"]).length,
            HelpLength::Long
        );
    }

    /// The case a hand-rolled scan gets wrong: `h` here is `-o`'s value, not a
    /// help request. Clap consumes it as one, so the parse reports no help
    /// flag and the fallback stands.
    #[test]
    fn test_help_request_does_not_mistake_an_option_value_for_a_flag() {
        assert_eq!(request(&["app", "-o", "h"]).length, HelpLength::Short);
        assert!(request(&["app", "-o", "h"]).target.is_empty());
    }

    /// Past `--`, a `--help` is the application's data.
    #[test]
    fn test_help_request_respects_the_terminator() {
        assert_eq!(request(&["app", "--", "--help"]).length, HelpLength::Short);
    }

    #[test]
    fn test_help_request_defaults_to_the_root_and_short() {
        assert_eq!(request(&["app"]), HelpRequest::default());
    }

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
