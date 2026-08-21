//! App builder and built entry point for CLI integration.
//!
//! This module provides [`AppBuilder`] for configuring CLI applications with
//! commands, hooks, templates, themes, and app-level state. Calling
//! [`AppBuilder::build`] consumes the builder and returns the executable
//! [`App`] that owns parsing, dispatch, rendering, and run entry points.
//!
//! # App State
//!
//! App-level state (database connections, configuration, API clients) can be
//! injected via `.app_state()` and accessed in handlers via `ctx.app_state`:
//!
//! ```rust,ignore
//! App::builder()
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
    default_topic_theme, display_with_pager, topic_data, topics_list_data, TopicRegistry,
    DEFAULT_TOPICS_LIST_TEMPLATE, DEFAULT_TOPIC_TEMPLATE,
};
use crate::TemplateRegistry;
use crate::{
    render_request_split, ColorPolicy, InputSources, OutputMode, RenderError, RenderRequest,
    TargetProperties, Theme, TEMPLATE_EXTENSIONS,
};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::Serialize;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use super::default_command::ParseFailure;
use super::dispatch::DispatchFn;
use super::group::CommandRecipe;
use super::handler::{CommandContext, Extensions, HandlerResult, Output as HandlerOutput};
use super::help::data::{extract_help_data, extract_help_data_with_topics};
use super::help::{
    default_help_theme, human_help_format, named_or_inline_template, render_via_request,
    CommandGroup, HelpConfig, HelpLength, DEFAULT_HELP_TEMPLATE,
};
use super::hooks::{ArtifactOutput, HookError, HookPhase, Hooks, RenderedOutput, TextOutput};
use super::questionnaire::QuestionnaireCommand;
use super::result::{HelpDisplay, HelpResult};
use standout_dispatch::verify::ExpectedArg;
use standout_render::warnings::WarningBuffer;

pub(crate) type SharedTemplateEngine =
    Rc<RefCell<Box<dyn standout_render::template::TemplateEngine>>>;

/// The presentation configuration a command declared.
///
/// Glue-private: keeps [`TemplateRef::Convention`] until `build()` materializes
/// it to a registry name. The public render-time [`crate::TemplateRef`] lives
/// in `standout-render` and has only `Named` / `Inline` / `Absent`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TemplateRef {
    /// A named template that must resolve through the template registry.
    Named(String),
    /// A convention-derived template name.
    ///
    /// The command path is stored until `build()` so `.template_ext(...)`
    /// applies regardless of whether it was called before or after command
    /// registration. Build validation materializes it to the final registry
    /// name.
    Convention(String),
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
    pub(crate) fn inline(template: impl Into<String>) -> Self {
        Self::Inline(template.into())
    }

    pub(crate) fn convention(command_path: &str) -> Self {
        Self::Convention(command_path.to_string())
    }

    pub(crate) fn convention_name(command_path: &str, template_ext: &str) -> String {
        let file_path = command_path.replace('.', "/");
        format!("{}{}", file_path, template_ext)
    }
}

pub(crate) fn inline_template_ref(
    template: impl Into<String>,
    api: &str,
) -> Result<TemplateRef, SetupError> {
    let template = template.into();
    if template.is_empty() {
        return Err(SetupError::Config(format!(
            "{api} received an empty template; use .silent(), .structured_only(), or .binary() to declare template absence"
        )));
    }
    Ok(TemplateRef::inline(template))
}

#[derive(Debug, Clone)]
pub(crate) struct TemplateRefreshError {
    name: String,
    location: String,
    message: String,
}

impl TemplateRefreshError {
    fn new(
        name: impl Into<String>,
        registry: &TemplateRegistry,
        message: impl Into<String>,
    ) -> Self {
        let name = name.into();
        Self {
            location: template_location(registry, &name),
            name,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for TemplateRefreshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.name.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(
                f,
                "template `{}`{} could not be refreshed: {}",
                self.name, self.location, self.message
            )
        }
    }
}

impl std::error::Error for TemplateRefreshError {}

pub(crate) fn template_location(registry: &TemplateRegistry, name: &str) -> String {
    match registry.get(name) {
        Ok(standout_render::template::ResolvedTemplate::File(path)) => {
            format!(" at `{}`", path.display())
        }
        Ok(standout_render::template::ResolvedTemplate::Inline(_)) | Err(_) => String::new(),
    }
}

pub(crate) fn refresh_engine_templates(
    engine: &mut dyn standout_render::template::TemplateEngine,
    registry: &TemplateRegistry,
) -> Result<(), TemplateRefreshError> {
    for name in registry.names() {
        let content = registry
            .get_content(name)
            .map_err(|error| TemplateRefreshError::new(name, registry, error.to_string()))?;
        engine
            .add_template(name, &content)
            .map_err(|error| TemplateRefreshError::new(name, registry, error.to_string()))?;
    }
    Ok(())
}

pub(crate) fn refresh_named_template(
    registry: &TemplateRegistry,
    name: &str,
) -> Result<(), TemplateRefreshError> {
    // Existence / readability check only: `render_request` loads the whole
    // registry into the engine (cached per registry generation). A registered
    // file that disappeared must still error with its path (ADR-0019). A name
    // missing from the original map is retried after a directory re-walk so
    // a file that appeared after build() is still found.
    match registry.get_content(name) {
        Ok(_) => Ok(()),
        Err(standout_render::RegistryError::NotFound { .. }) => {
            let mut refreshed = registry.clone();
            refreshed
                .refresh()
                .map_err(|error| TemplateRefreshError::new(name, registry, error.to_string()))?;
            refreshed
                .get_content(name)
                .map_err(|error| TemplateRefreshError::new(name, &refreshed, error.to_string()))?;
            Ok(())
        }
        Err(error) => Err(TemplateRefreshError::new(name, registry, error.to_string())),
    }
}

fn missing_template_message(
    command_path: &str,
    template_name: &str,
    registry: Option<&TemplateRegistry>,
) -> String {
    let has_application_templates =
        registry.is_some_and(TemplateRegistry::has_application_templates);
    let mut message = if has_application_templates {
        format!(
            "command `{command_path}` references template `{template_name}`, but that template is not registered; add it with .templates(embed_templates!(\"src/templates\")) or .templates_dir(\"path/to/templates\")"
        )
    } else {
        format!(
            "command `{command_path}` references template `{template_name}`, but no application templates are configured; add .templates(embed_templates!(\"src/templates\")) or .templates_dir(\"path/to/templates\") before .build(), or declare no presentation with .structured_only(), .silent(), or .binary()"
        )
    };

    let Some(registry) = registry else {
        return message;
    };
    if !has_application_templates {
        return message;
    }

    let suggestions = nearest_template_names(template_name, registry);
    if !suggestions.is_empty() {
        message.push_str("; did you mean ");
        message.push_str(&suggestions.join(", "));
        message.push('?');
    } else {
        let available = available_template_names(registry);
        if !available.is_empty() {
            message.push_str("; available templates: ");
            message.push_str(&available.join(", "));
        }
    }
    message
}

fn available_template_names(registry: &TemplateRegistry) -> Vec<String> {
    canonical_template_names(registry)
        .into_iter()
        .take(5)
        .map(|candidate| format!("`{candidate}`"))
        .collect()
}

fn nearest_template_names(name: &str, registry: &TemplateRegistry) -> Vec<String> {
    let mut candidates: Vec<(usize, String)> = canonical_template_names(registry)
        .into_iter()
        .map(|candidate| (edit_distance(name, &candidate), candidate))
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

fn canonical_template_names(registry: &TemplateRegistry) -> Vec<String> {
    let mut names = BTreeMap::<String, String>::new();
    for name in registry.names() {
        let key = template_alias_key(name).to_string();
        match names.entry(key) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(name.to_string());
            }
            std::collections::btree_map::Entry::Occupied(mut entry)
                if standout_render::extension_priority(name, TEMPLATE_EXTENSIONS)
                    < standout_render::extension_priority(entry.get(), TEMPLATE_EXTENSIONS) =>
            {
                entry.insert(name.to_string());
            }
            std::collections::btree_map::Entry::Occupied(_) => {}
        }
    }
    names.into_values().collect()
}

fn template_alias_key(name: &str) -> &str {
    for extension in TEMPLATE_EXTENSIONS {
        if let Some(stripped) = name.strip_suffix(*extension) {
            return stripped;
        }
    }
    name
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

fn unique_unknown_tag_names<'a>(
    errors: impl IntoIterator<Item = &'a standout_bbparser::UnknownTagError>,
) -> Vec<String> {
    let mut names: Vec<String> = errors.into_iter().map(|error| error.tag.clone()).collect();
    names.sort_unstable();
    names.dedup();
    names
}

fn validate_framework_template_content(
    name: &str,
    content: &str,
    parser: &standout_bbparser::BBParser,
) -> Result<(), SetupError> {
    use standout_bbparser::UnknownTagKind;

    let Err(errors) = parser.validate(content) else {
        return Ok(());
    };

    let malformed = unique_unknown_tag_names(errors.errors.iter().filter(|error| {
        matches!(
            error.kind,
            UnknownTagKind::Unbalanced | UnknownTagKind::UnexpectedClose
        )
    }));
    if !malformed.is_empty() {
        return Err(SetupError::Template(format!(
            "framework template `{name}` contains malformed style markup involving tag(s): {}; fix the template source or disable framework templates with .include_framework_templates(false) if this app does not use them",
            malformed.join(", ")
        )));
    }

    let missing = unique_unknown_tag_names(
        errors
            .errors
            .iter()
            .filter(|error| !parser.styles().contains_key(&error.tag)),
    );
    if !missing.is_empty() {
        return Err(SetupError::Template(format!(
            "framework template `{name}` emits style tag(s) not defined by the resolved theme: {}; enable framework styles with .include_framework_styles(true), define the tag with .theme(...) or .styles(...), or disable framework templates with .include_framework_templates(false)",
            missing.join(", ")
        )));
    }

    Ok(())
}

/// Stores a pending command recipe along with its typed template declaration.
struct PendingCommand {
    recipe: Box<dyn CommandRecipe>,
    template: TemplateRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HookRegistrationSource {
    AppBuilderHooks,
    CommandConfig,
}

/// Main entry point for running a configured standout-clap application.
///
/// `App` is produced by [`AppBuilder::build`]. It owns command dispatch,
/// rendering, parsing, and run entry points; the configuring builder does not.
/// Create one with [`App::builder`] and finish configuration with `build()`.
///
/// # Example
///
/// ```rust
/// use standout::cli::App;
///
/// let standout = App::builder()
///     .help_handling(true)
///     .topics_dir(".").unwrap()
///     .output_flag(Some("format"))
///     .build();
/// ```
pub struct App {
    pub(crate) registry: TopicRegistry,
    pub(crate) output_flag: Option<String>,
    pub(crate) output_file_flag: Option<String>,
    /// The one theme `build()` merged (ADR-0020). Always set.
    pub(crate) theme: Theme,
    pub(crate) stylesheet_registry: Option<crate::StylesheetRegistry>,
    pub(crate) template_registry: Option<Rc<TemplateRegistry>>,
    pending_commands: RefCell<HashMap<String, PendingCommand>>,
    finalized_commands: RefCell<Option<HashMap<String, DispatchFn>>>,
    pub(crate) command_hooks: HashMap<String, Hooks>,
    pub(crate) questionnaire_commands: HashMap<String, QuestionnaireCommand>,
    pub(crate) context_registry: ContextRegistry,
    pub(crate) default_command: Option<String>,
    pub(crate) default_command_resolver: Option<crate::cli::DefaultCommandResolver>,
    pub(crate) app_state: Rc<Extensions>,
    pub(crate) template_engine: SharedTemplateEngine,
    pub(crate) help_command_groups: Option<Vec<CommandGroup>>,
    pub(crate) help_handling: bool,
    pub(crate) help_word: bool,
    pub(crate) ambiguous_width: crate::AmbiguousWidth,
    pub(crate) version: Option<&'static str>,
    /// Framework warnings collected while converting embedded templates/styles
    /// at build time (hot-reload fallbacks). Copied into each run's
    /// [`standout_render::warnings::WarningBuffer`] so they return on the run
    /// result instead of printing at construction.
    pub(crate) startup_warnings: Vec<String>,
}

impl App {
    /// Starts configuring a standout CLI application.
    ///
    /// This is the public constructor for the configuring builder. Call
    /// [`AppBuilder::build`] when configuration is complete to obtain the
    /// executable [`App`].
    pub fn builder() -> AppBuilder {
        AppBuilder::new()
    }
}

/// Configures a standout-clap application before it can run.
///
/// `AppBuilder` owns configuration methods only. Running, dispatching,
/// parsing, and rendering are available after [`build`](Self::build) returns
/// an [`App`].
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
/// App::builder()
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
    pub(crate) template_registry: Option<TemplateRegistry>,
    pub(crate) default_theme_name: Option<String>,
    /// Pending commands - closures are created lazily at dispatch time
    pending_commands: RefCell<HashMap<String, PendingCommand>>,
    /// Finalized dispatch functions (lazily created from pending_commands)
    finalized_commands: RefCell<Option<HashMap<String, DispatchFn>>>,
    pub(crate) command_hooks: HashMap<String, Hooks>,
    pub(crate) hook_phase_sources: HashMap<(String, HookPhase), HookRegistrationSource>,
    pub(crate) setup_errors: Vec<SetupError>,
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
    /// App-level state that will be shared across all dispatches after build.
    pub(crate) app_state: Extensions,

    /// Optional template engine.
    ///
    /// Unset until [`AppBuilder::template_engine`] or [`build`](Self::build).
    /// `build()` constructs the default MiniJinja engine when this is `None`
    /// — the only place glue may call `MiniJinjaEngine::new()`.
    pub(crate) template_engine: Option<SharedTemplateEngine>,

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

    /// Framework warnings collected while converting embedded templates/styles
    /// (hot-reload fallbacks). Copied onto [`App`] at build and into each
    /// run's warning buffer.
    pub(crate) startup_warnings: Vec<String>,
}

impl Default for AppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AppBuilder {
    /// Creates a new builder with default settings.
    ///
    /// By default, the `--output` flag is enabled, framework templates and styles
    /// are included, and no hooks are registered.
    pub(crate) fn new() -> Self {
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
            hook_phase_sources: HashMap::new(),
            setup_errors: Vec::new(),
            questionnaire_commands: HashMap::new(),
            context_registry: ContextRegistry::new(),
            template_ext: ".j2".to_string(),
            default_command: None,
            default_command_resolver: None,
            include_framework_templates: true,
            include_framework_styles: true,
            app_state: Extensions::new(),
            template_engine: None,
            help_command_groups: None,
            help_handling: false,
            help_word: false,
            ambiguous_width: crate::AmbiguousWidth::Narrow,
            version: None,
            startup_warnings: Vec::new(),
        }
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
    /// let app = App::builder()
    ///     .app_state(Metrics { requests: AtomicUsize::new(0) })
    ///     .command_with("test", |_m, ctx| {
    ///         let metrics = ctx.app_state.get_required::<Metrics>()?;
    ///         metrics.requests.fetch_add(1, Ordering::SeqCst);
    ///         Ok(Output::<()>::Silent)
    ///     }, |cfg| cfg.silent()).unwrap()
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
    /// let app = App::builder()
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
    /// App::builder()
    ///     .app_state(Config { debug: false })
    ///     .app_state(Config { debug: true })  // Replaces previous Config
    /// ```
    pub fn app_state<T: 'static>(mut self, value: T) -> Self {
        self.app_state.insert(value);
        self
    }

    /// Sets a custom template engine to be used for rendering.
    ///
    /// If not set, [`build`](Self::build) constructs the default MiniJinja
    /// engine. That construction is the only `MiniJinjaEngine::new()` in glue.
    pub fn template_engine(
        mut self,
        engine: Box<dyn standout_render::template::TemplateEngine>,
    ) -> Self {
        self.template_engine = Some(Rc::new(RefCell::new(engine)));
        self
    }

    /// Test helper: Check if a command path is registered.
    #[cfg(test)]
    pub(crate) fn has_command(&self, path: &str) -> bool {
        self.pending_commands.borrow().contains_key(path)
    }

    /// Finalizes the builder into an executable App, resolving one
    /// framework-base-plus-application theme, constructing the template engine
    /// if unset (the only `MiniJinjaEngine::new()` in glue), validating typed
    /// template declarations, loading templates, and preparing for dispatch
    /// and rendering.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A `default_theme()` was specified but the theme wasn't found in the stylesheet registry
    /// - a command references a named or convention template that is not in the template registry
    /// - a command relies on a convention template without configuring application templates
    ///   or declaring absence with `.structured_only()`, `.silent()`, or `.binary()`
    /// - a registered template fails to compile
    /// - `command_groups` or topics are configured without `.help_handling(true)`
    /// - a command is registered under the root `help` with `.help_handling(true)`,
    ///   which is the name standout installs its own word under
    /// - the same command path and hook phase are configured through both
    ///   `CommandConfig` and [`hooks`](Self::hooks)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let standout = App::builder()
    ///     .styles(embed_styles!("src/styles"))
    ///     .default_theme("dark")
    ///     .build()?;
    /// ```
    pub fn build(mut self) -> Result<App, SetupError> {
        use crate::assets::FRAMEWORK_TEMPLATES;

        if !self.setup_errors.is_empty() {
            return Err(self.setup_errors.remove(0));
        }

        // Add framework templates if enabled (BEFORE finalizing commands)
        if self.include_framework_templates {
            match self.template_registry.as_mut() {
                Some(registry) => registry.add_framework_entries(FRAMEWORK_TEMPLATES),
                None => {
                    // Create new registry with just framework templates
                    let mut registry = TemplateRegistry::new();
                    registry.add_framework_entries(FRAMEWORK_TEMPLATES);
                    self.template_registry = Some(registry);
                }
            };
        }

        // Resolve theme BEFORE finalization. `App.theme` is non-optional;
        // this `Some` is only the builder's still-unset-until-build slot.
        let app_theme = self.resolve_configured_theme()?;
        self.theme = Some(
            self.framework_base_theme()?
                .merge(app_theme.unwrap_or_else(Theme::new)),
        );

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

        let template_engine = self.template_engine.take().unwrap_or_else(|| {
            Rc::new(RefCell::new(Box::new(
                standout_render::template::MiniJinjaEngine::new(),
            )))
        });

        self.validate_command_templates()?;
        self.validate_framework_template_styles()?;
        self.materialize_convention_templates();

        // Populate engine with templates from the registry and keep the compile
        // result. Named renders refresh this cache again so file-backed
        // templates can hot reload.
        if let Some(registry) = &self.template_registry {
            refresh_engine_templates(&mut **template_engine.borrow_mut(), registry)
                .map_err(|error| SetupError::Template(error.to_string()))?;
        }

        let app = App {
            registry: self.registry,
            output_flag: self.output_flag,
            output_file_flag: self.output_file_flag,
            theme: self
                .theme
                .take()
                .expect("build always resolves a theme before constructing App"),
            stylesheet_registry: self.stylesheet_registry,
            template_registry: self.template_registry.map(Rc::new),
            pending_commands: self.pending_commands,
            finalized_commands: self.finalized_commands,
            command_hooks: self.command_hooks,
            questionnaire_commands: self.questionnaire_commands,
            context_registry: self.context_registry,
            default_command: self.default_command,
            default_command_resolver: self.default_command_resolver,
            app_state: Rc::new(self.app_state),
            template_engine,
            help_command_groups: self.help_command_groups,
            help_handling: self.help_handling,
            help_word: self.help_word,
            ambiguous_width: self.ambiguous_width,
            version: self.version,
            startup_warnings: self.startup_warnings,
        };

        // Finalize commands with built template and theme state in place.
        app.ensure_commands_finalized();

        Ok(app)
    }

    fn validate_command_templates(&self) -> Result<(), SetupError> {
        for (path, pending) in self.pending_commands.borrow().iter() {
            let name = match &pending.template {
                TemplateRef::Named(name) => name.clone(),
                TemplateRef::Convention(command_path) => {
                    TemplateRef::convention_name(command_path, &self.template_ext)
                }
                TemplateRef::Inline(_) | TemplateRef::Absent(_) => continue,
            };
            let Some(registry) = self.template_registry.as_ref() else {
                return Err(SetupError::Template(missing_template_message(
                    path, &name, None,
                )));
            };
            registry.get_content(&name).map_err(|error| {
                let message = match error {
                    standout_render::RegistryError::NotFound { .. } => {
                        missing_template_message(path, &name, Some(registry))
                    }
                    _ => TemplateRefreshError::new(&name, registry, error.to_string()).to_string(),
                };
                SetupError::Template(message)
            })?;
        }
        Ok(())
    }

    fn materialize_convention_templates(&self) {
        let mut pending_commands = self.pending_commands.borrow_mut();
        for pending in pending_commands.values_mut() {
            if let TemplateRef::Convention(command_path) = &pending.template {
                pending.template = TemplateRef::Convention(TemplateRef::convention_name(
                    command_path,
                    &self.template_ext,
                ));
            }
        }
    }

    fn resolve_configured_theme(&mut self) -> Result<Option<Theme>, SetupError> {
        if self.theme.is_some() {
            return Ok(self.theme.take());
        }

        let Some(ref mut registry) = self.stylesheet_registry else {
            if let Some(name) = &self.default_theme_name {
                return Err(SetupError::ThemeNotFound(name.to_string()));
            }
            return Ok(None);
        };

        let resolved = if let Some(name) = &self.default_theme_name {
            Some(
                registry
                    .get(name)
                    .map_err(|_| SetupError::ThemeNotFound(name.to_string()))?,
            )
        } else {
            registry
                .get("default")
                .or_else(|_| registry.get("theme"))
                .or_else(|_| registry.get("base"))
                .ok()
        };
        Ok(resolved)
    }

    fn framework_base_theme(&self) -> Result<Theme, SetupError> {
        let mut theme = Theme::default()
            .merge(default_help_theme())
            .merge(default_topic_theme());

        if self.include_framework_styles {
            let framework_styles =
                Theme::from_yaml(crate::assets::FRAMEWORK_STYLES).map_err(|error| {
                    SetupError::Stylesheet(format!("failed to parse framework styles: {error}"))
                })?;
            theme = theme.merge(framework_styles);
        }

        Ok(theme)
    }

    fn validate_framework_template_styles(&self) -> Result<(), SetupError> {
        use standout_bbparser::{BBParser, TagTransform};

        let Some(registry) = &self.template_registry else {
            return Ok(());
        };
        let Some(theme) = &self.theme else {
            return Ok(());
        };

        let styles = theme.resolve_styles(None).to_resolved_map();
        let parser = BBParser::new(styles, TagTransform::Remove);

        for name in registry.framework_names() {
            let content = registry.get_content(name).map_err(|error| {
                SetupError::Template(
                    TemplateRefreshError::new(name, registry, error.to_string()).to_string(),
                )
            })?;
            validate_framework_template_content(name, &content, &parser)?;
        }

        Ok(())
    }
}

impl App {
    /// Ensures all pending commands have been finalized into dispatch functions.
    ///
    /// This method is called lazily on first dispatch. It creates the actual
    /// dispatch closures from the stored recipes. The theme is passed at
    /// dispatch time via late binding, which allows `.theme()` to be called in
    /// any order relative to `.command()` before build.
    fn ensure_commands_finalized(&self) {
        if self.finalized_commands.borrow().is_some() {
            return;
        }

        let mut commands = HashMap::new();
        for (path, pending) in self.pending_commands.borrow().iter() {
            let dispatch = pending.recipe.create_dispatch(
                &pending.template,
                &self.context_registry,
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
        std::cell::Ref::map(self.finalized_commands.borrow(), |opt| match opt.as_ref() {
            Some(commands) => commands,
            None => unreachable!("command finalization stores a command map before returning"),
        })
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

    /// Returns the hooks registered for a specific command path.
    pub fn get_hooks(&self, path: &str) -> Option<&Hooks> {
        self.command_hooks.get(path)
    }

    /// CSV projection registered for `path`, if the command declared one.
    ///
    /// Same fact dispatch captures into its request: a `run_command` of a
    /// command with [`crate::StructuredOutputProjection`] must emit the
    /// contract CSV, not generic flattening.
    fn csv_projection_for(&self, path: &str) -> Option<crate::CsvProjection> {
        self.pending_commands
            .borrow()
            .get(path)
            .and_then(|pending| {
                pending
                    .recipe
                    .structured_output_projection()
                    .map(|projection| projection.csv_projection().clone())
            })
    }

    /// Returns the theme `build()` merged (ADR-0020).
    ///
    /// Always present: `build()` computes the framework-base-plus-application
    /// theme unconditionally, so a built [`App`] has no unset theme.
    pub fn get_default_theme(&self) -> &Theme {
        &self.theme
    }

    /// Output-mode fallback used when this invocation has no `--output` flag.
    ///
    /// `App` stores no configurable fallback (`AppBuilder::output_mode()` was
    /// deleted in ROB02); `Auto` is the only value. Named here rather than
    /// inlined at each call site so a later workstream can store a real field
    /// without hunting literals.
    fn output_mode_fallback() -> OutputMode {
        OutputMode::Auto
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

    /// Parses CLI arguments with this built App instance.
    ///
    /// This compatibility entry point is equivalent to [`parse_with`](Self::parse_with).
    pub fn parse(&self, cmd: Command) -> clap::ArgMatches {
        self.parse_with(cmd)
    }

    /// Parses CLI arguments with this built App instance.
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
        self.get_matches_from_with_sources(cmd, itr, &crate::InputSources::from_process())
    }

    /// [`get_matches_from`](Self::get_matches_from) against explicit
    /// [`crate::InputSources`] (stdin terminal fact for default-command
    /// resolution).
    pub fn get_matches_from_with_sources<I, T>(
        &self,
        cmd: Command,
        itr: I,
        sources: &crate::InputSources,
    ) -> HelpResult
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

        let matches = match self.parse_with_default_command(&cmd, &args, sources.stdin()) {
            Ok(matches) => matches,
            Err(ParseFailure::UnknownDefault(e)) => {
                return HelpResult::Error(
                    cmd.clone()
                        .error(clap::error::ErrorKind::InvalidSubcommand, e.to_string()),
                )
            }
            Err(ParseFailure::Clap(e)) => {
                return match self.intercept_display_help(&mut cmd, &args, &e, None, None) {
                    Some(display) => display.into(),
                    None => HelpResult::Error(e),
                }
            }
        };

        match self.intercept_help_word(&mut cmd, &matches, None, None) {
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
        target: Option<crate::TargetProperties>,
        warnings: Option<standout_render::warnings::WarningBuffer>,
    ) -> Option<HelpDisplay> {
        if !self.help_handling {
            return None;
        }
        let (name, sub_matches) = matches.subcommand()?;
        (name == "help").then(|| self.render_help_word(cmd, matches, sub_matches, target, warnings))
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
        target: Option<crate::TargetProperties>,
        warnings: Option<standout_render::warnings::WarningBuffer>,
    ) -> Option<HelpDisplay> {
        (self.help_handling && error.kind() == clap::error::ErrorKind::DisplayHelp)
            .then(|| self.render_help_for_display_help_error(cmd, args, target, warnings))
    }

    /// Destination facts for a help/topics render.
    ///
    /// `run_with` supplies the invocation's target; parse-only entry points
    /// detect at this edge. Ambiguous-width is application policy (ADR-0026).
    fn help_target_properties(
        &self,
        target: Option<crate::TargetProperties>,
    ) -> crate::TargetProperties {
        let mut target = target.unwrap_or_else(crate::TargetProperties::detect);
        target.ambiguous_width = self.ambiguous_width;
        target
    }

    /// The one theme `build()` merged (ADR-0020), including help/topic tags.
    fn help_theme(&self) -> Theme {
        self.theme.clone()
    }

    /// Named registry template when `build()` registered it; otherwise the
    /// default source as [`crate::TemplateRef::Inline`] with tag validation.
    fn help_template(
        &self,
        override_source: Option<&str>,
        named: &str,
        default_source: &str,
    ) -> Result<crate::TemplateRef, RenderError> {
        let theme = self.help_theme();
        if let Some(source) = override_source {
            return super::help::inline_template_ref(source, &theme, named);
        }
        named_or_inline_template(
            self.template_registry.as_deref(),
            named,
            default_source,
            &theme,
        )
    }

    /// Help and topics through [`render_request`] with the app engine, merged
    /// theme, filters, and registry.
    #[allow(clippy::too_many_arguments)]
    fn render_help_surface<T: Serialize>(
        &self,
        data: &T,
        template: crate::TemplateRef,
        format: OutputMode,
        target: crate::TargetProperties,
        warnings: Option<standout_render::warnings::WarningBuffer>,
    ) -> Result<String, RenderError> {
        render_via_request(
            data,
            template,
            self.help_theme(),
            format,
            target,
            self.template_engine.clone(),
            self.template_registry.clone(),
            Some(self.context_registry.clone()),
            warnings,
        )
    }

    fn help_display(
        &self,
        cmd: &Command,
        rendered: Result<String, RenderError>,
        use_pager: bool,
    ) -> HelpDisplay {
        match rendered {
            Ok(text) => HelpDisplay::Rendered {
                text,
                paged: use_pager,
            },
            Err(e) => Self::render_failure(cmd, e),
        }
    }

    /// Renders the help the `help` word asked for.
    ///
    /// Its arguments come from Clap: `sub_matches` is the word's own parse
    /// (`topic`, `--page`), and the output mode is read from the root, where
    /// the global flag that carries it lives. Structured modes map to
    /// [`OutputMode::Auto`] (ADR-0029) on the request; the leaf has no help
    /// flag.
    fn render_help_word(
        &self,
        cmd: &mut Command,
        matches: &ArgMatches,
        sub_matches: &ArgMatches,
        target: Option<crate::TargetProperties>,
        warnings: Option<standout_render::warnings::WarningBuffer>,
    ) -> HelpDisplay {
        let format = human_help_format(self.extract_output_mode(matches));
        let target = self.help_target_properties(target);
        let config = HelpConfig {
            command_groups: self.help_command_groups.clone(),
            // The word is the spelled-out request, so it reads like `--help`.
            length: HelpLength::Long,
            ..Default::default()
        };
        let use_pager = sub_matches.get_flag("page");

        if let Some(topic_args) = sub_matches.get_many::<String>("topic") {
            let keywords: Vec<_> = topic_args.map(|s| s.as_str()).collect();
            if !keywords.is_empty() {
                return self.handle_help_request(
                    cmd, &keywords, use_pager, config, format, target, warnings,
                );
            }
        }

        self.render_root_help(cmd, config, format, target, warnings, use_pager)
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

    /// Renders root help through [`render_request`].
    #[allow(clippy::too_many_arguments)]
    fn render_root_help(
        &self,
        cmd: &Command,
        config: HelpConfig,
        format: OutputMode,
        target: crate::TargetProperties,
        warnings: Option<standout_render::warnings::WarningBuffer>,
        use_pager: bool,
    ) -> HelpDisplay {
        let template = match self.help_template(
            config.template.as_deref(),
            crate::assets::HELP_TEMPLATE_NAME,
            DEFAULT_HELP_TEMPLATE,
        ) {
            Ok(template) => template,
            Err(e) => return Self::render_failure(cmd, e),
        };
        let data = extract_help_data_with_topics(
            cmd,
            &self.registry,
            config.command_groups.as_deref(),
            config.length,
            &target,
        );
        self.help_display(
            cmd,
            self.render_help_surface(&data, template, format, target, warnings),
            use_pager,
        )
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
        target: Option<crate::TargetProperties>,
        warnings: Option<standout_render::warnings::WarningBuffer>,
    ) -> HelpDisplay {
        let request = Self::help_request(cmd, args);
        let format = human_help_format(OutputMode::Auto);
        let target = self.help_target_properties(target);
        let config = HelpConfig {
            command_groups: self.help_command_groups.clone(),
            length: request.length,
            ..Default::default()
        };

        if request.target.is_empty() {
            return self.render_root_help(cmd, config, format, target, warnings, false);
        }

        let keywords: Vec<&str> = request.target.iter().map(|s| s.as_str()).collect();
        self.handle_help_request(cmd, &keywords, false, config, format, target, warnings)
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
    #[allow(clippy::too_many_arguments)]
    fn handle_help_request(
        &self,
        cmd: &mut Command,
        keywords: &[&str],
        use_pager: bool,
        config: HelpConfig,
        format: OutputMode,
        target: crate::TargetProperties,
        warnings: Option<standout_render::warnings::WarningBuffer>,
    ) -> HelpDisplay {
        let sub_name = keywords[0];

        // 0. Check for "topics" - list all available topics
        if sub_name == "topics" {
            let template = match self.help_template(
                None,
                crate::assets::TOPICS_LIST_TEMPLATE_NAME,
                DEFAULT_TOPICS_LIST_TEMPLATE,
            ) {
                Ok(template) => template,
                Err(e) => return Self::render_failure(cmd, e),
            };
            let data =
                topics_list_data(&self.registry, &format!("{} help", cmd.get_name()), &target);
            return self.help_display(
                cmd,
                self.render_help_surface(&data, template, format, target, warnings),
                use_pager,
            );
        }

        // 1. Check if it's a real command
        if super::app::find_subcommand(cmd, sub_name).is_some() {
            if let Some(help_cmd) = super::app::find_subcommand_recursive(cmd, keywords) {
                let template = match self.help_template(
                    config.template.as_deref(),
                    crate::assets::HELP_TEMPLATE_NAME,
                    DEFAULT_HELP_TEMPLATE,
                ) {
                    Ok(template) => template,
                    Err(e) => return Self::render_failure(cmd, e),
                };
                let data = extract_help_data(
                    help_cmd,
                    config.command_groups.as_deref(),
                    config.length,
                    &target,
                );
                return self.help_display(
                    cmd,
                    self.render_help_surface(&data, template, format, target, warnings),
                    use_pager,
                );
            }
        }

        // 2. Check if it is a topic
        if let Some(topic) = self.registry.get_topic(sub_name) {
            let template = match self.help_template(
                None,
                crate::assets::TOPIC_TEMPLATE_NAME,
                DEFAULT_TOPIC_TEMPLATE,
            ) {
                Ok(template) => template,
                Err(e) => return Self::render_failure(cmd, e),
            };
            return self.help_display(
                cmd,
                self.render_help_surface(&topic_data(topic), template, format, target, warnings),
                use_pager,
            );
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
    ///
    /// When this app installs `--output` and `matches` carries `_output_mode`,
    /// that value is the invocation's mode. Otherwise the
    /// [fallback](Self::output_mode_fallback) — including when the parse tree
    /// never declared Standout's argument (`try_get_one` rather than
    /// `get_one`, so an unaugmented manual parse does not panic).
    pub fn extract_output_mode(&self, matches: &ArgMatches) -> OutputMode {
        if self.output_flag.is_none() {
            return Self::output_mode_fallback();
        }
        match matches.try_get_one::<String>("_output_mode") {
            Ok(Some(value)) => parse_output_mode_flag(Some(value.as_str())),
            Ok(None) | Err(_) => Self::output_mode_fallback(),
        }
    }

    /// Resolves `--output` when Clap did not produce matches.
    ///
    /// Usage errors, `--help`, and `--version` short-circuit before
    /// [`extract_output_mode`](Self::extract_output_mode). Warning flush still
    /// reads the run's output mode, so `--output=text` must opt the warning
    /// block out of ANSI on those exits too.
    ///
    /// Scans the raw argument list for the long name the app configured
    /// ([`AppBuilder::output_flag`]): `--flag=value` and `--flag value`. The
    /// first element is the program name (Clap's argv[0]) and is not a flag.
    /// A `--` terminator ends the scan, so arguments after it are not flags.
    /// Unknown values, a missing value, a `--flag` whose next token is another
    /// option, and a flag that appears only after `--` resolve to
    /// [`OutputMode::Auto`]. So does an app that configured no output flag.
    ///
    /// This is a lexical look at that configured long name, not clap's parse:
    /// it does not see aliases or a short spelling. Exactness is not required
    /// here — the command line already failed, and this only styles the warning
    /// block. The machine-contract Spec owns a parse-independent `--output`
    /// reading.
    pub(crate) fn extract_output_mode_from_unparsed(
        &self,
        args: &[std::ffi::OsString],
    ) -> OutputMode {
        let Some(flag) = self.output_flag.as_deref() else {
            return OutputMode::Auto;
        };
        parse_output_mode_flag(last_unparsed_flag_value(flag, args))
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
    /// 1. Inserts [`InputSources::from_process`] and a
    ///    [`standout_render::warnings::WarningBuffer`] into the context (the
    ///    same run extensions `dispatch` / `run_with` insert). The buffer is
    ///    seeded with build-time startup warnings so hooks and render see the
    ///    same in-call destination as the other run edges; this method still
    ///    returns only [`RenderedOutput`] (no final write, no warnings-return
    ///    API)
    /// 2. Runs pre-dispatch hooks (if any)
    /// 3. Calls your handler closure
    /// 4. Renders the result through [`crate::render_request_split`] using the
    ///    app engine, template registry, context registry, merged theme, the
    ///    output mode from [`extract_output_mode`](Self::extract_output_mode),
    ///    and the command's structured-output projection when one is
    ///    registered for `path`. Formatted and raw travel on [`TextOutput`]
    ///    the same way dispatch does
    /// 5. Runs post-output hooks (if any)
    /// 6. Returns the final output
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
        let warnings = WarningBuffer::new();
        self.seed_startup_warnings(&warnings);
        ctx.extensions.insert(InputSources::from_process());
        ctx.extensions.insert(warnings.clone());

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

                let mut target = TargetProperties::detect();
                target.ambiguous_width = self.ambiguous_width;
                let request = RenderRequest {
                    data: json_data,
                    template: crate::TemplateRef::Inline(template.to_string()),
                    theme: self.theme.clone(),
                    format: self.extract_output_mode(matches),
                    color_policy: ColorPolicy::Auto,
                    target,
                    engine: self.template_engine.clone(),
                    registry: self.template_registry.clone(),
                    context_registry: Some(self.context_registry.clone()),
                    csv_projection: self.csv_projection_for(path),
                    extras: HashMap::new(),
                    warnings: Some(warnings),
                };
                match render_request_split(&request) {
                    Ok(rendered) => {
                        RenderedOutput::Text(TextOutput::new(rendered.formatted, rendered.raw))
                    }
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

/// Argument ids for the flags [`App::help_request`] re-declares.
///
/// Named out of the application's namespace: they exist only on a throwaway
/// clone, but a collision with a real argument would silently change what the
/// probe reports.
const HELP_PROBE_SHORT: &str = "__standout_help_short";
const HELP_PROBE_LONG: &str = "__standout_help_long";

fn parse_output_mode_flag(value: Option<&str>) -> OutputMode {
    match value {
        Some("term") => OutputMode::Term,
        Some("text") => OutputMode::Text,
        Some("term-debug") => OutputMode::TermDebug,
        Some("json") => OutputMode::Json,
        Some("yaml") => OutputMode::Yaml,
        Some("xml") => OutputMode::Xml,
        Some("csv") => OutputMode::Csv,
        _ => OutputMode::Auto,
    }
}

/// Last `--flag` / `--flag=value` before a `--` terminator, if any.
///
/// `args` is Clap-style: the first element is the program name and is not
/// scanned. The look is the configured long name only: not clap aliases, not a
/// short spelling. `--flag` with no following value, whose next token is `--`,
/// or whose next token is another option (a token starting with `-`), is a
/// miss — the following option is not consumed. Last occurrence wins, matching
/// clap's `Set` action.
fn last_unparsed_flag_value<'a>(flag: &str, args: &'a [std::ffi::OsString]) -> Option<&'a str> {
    let long = format!("--{flag}");
    let prefix = format!("--{flag}=");
    let mut found = None;
    let mut iter = args.iter().skip(1).peekable();
    while let Some(arg) = iter.next() {
        let Some(arg) = arg.to_str() else {
            continue;
        };
        if arg == "--" {
            break;
        }
        if let Some(value) = arg.strip_prefix(&prefix) {
            found = Some(value);
            continue;
        }
        if arg == long {
            match iter.peek().and_then(|next| next.to_str()) {
                None => found = None,
                Some("--") => {
                    found = None;
                    break;
                }
                Some(next) if next.starts_with('-') => found = None,
                Some(_) => found = iter.next().and_then(|next| next.to_str()),
            }
        }
    }
    found
}

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
    use console::Style;

    #[test]
    fn framework_template_validation_reports_malformed_markup_separately() {
        let parser = standout_bbparser::BBParser::new(
            HashMap::from([("known".to_string(), Style::new())]),
            standout_bbparser::TagTransform::Remove,
        );

        let error =
            validate_framework_template_content("standout/broken", "[known]unclosed", &parser)
                .unwrap_err()
                .to_string();

        assert!(error.contains("malformed style markup"), "{error}");
        assert!(error.contains("known"), "{error}");
        assert!(
            !error.contains("not defined by the resolved theme"),
            "{error}"
        );
    }

    #[test]
    fn framework_template_validation_reports_only_missing_styles() {
        let parser = standout_bbparser::BBParser::new(
            HashMap::from([("known".to_string(), Style::new())]),
            standout_bbparser::TagTransform::Remove,
        );

        let error = validate_framework_template_content(
            "standout/missing",
            "[missing]text[/missing]",
            &parser,
        )
        .unwrap_err()
        .to_string();

        assert!(
            error.contains("not defined by the resolved theme"),
            "{error}"
        );
        assert!(error.contains("missing"), "{error}");
        assert!(!error.contains("malformed style markup"), "{error}");
    }

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
        App::help_request(&probe_command(), &args)
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

        assert_eq!(app.theme.name(), Some("base"));

        // 2. theme.yaml exists (should override base)
        fs::write(temp_dir.path().join("theme.yaml"), "style: { fg: red }").unwrap();

        let app = AppBuilder::new()
            .styles_dir(temp_dir.path())
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(app.theme.name(), Some("theme"));

        // 3. default.yaml exists (should override theme)
        fs::write(temp_dir.path().join("default.yaml"), "style: { fg: green }").unwrap();

        let app = AppBuilder::new()
            .styles_dir(temp_dir.path())
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(app.theme.name(), Some("default"));
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
