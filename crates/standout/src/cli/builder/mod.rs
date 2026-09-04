//! [`AppBuilder`] configures a CLI application (commands, hooks, templates,
//! themes, app-level state); [`AppBuilder::build`] consumes it into the
//! executable [`App`] that owns parsing, dispatch, rendering, and run entry
//! points. Split into submodules by concern: [`config`] (themes, templates,
//! context, flags), [`commands`] (registration), [`execution`] (dispatch and
//! run), [`rendering`] (template rendering and serialization).
//!
//! App-level state (DB connections, config, API clients) is injected via
//! `.app_state()` and read in handlers via `ctx.app_state`.

mod commands;
mod config;
pub(crate) mod execution;
mod rendering;

use crate::context::ContextRegistry;
use crate::setup::SetupError;
use crate::topics::{
    default_topic_theme, topic_data, topics_list_data, TopicRegistry, DEFAULT_TOPICS_LIST_TEMPLATE,
    DEFAULT_TOPIC_TEMPLATE,
};
use crate::TemplateRegistry;
use crate::{
    render_request_split, ColorPolicy, InputSources, RenderError, RenderRequest, Representation,
    Theme, TEMPLATE_EXTENSIONS,
};
use clap::parser::ValueSource;
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::Serialize;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use super::config::{
    claims_config_command, claims_config_path, config_command_collision, config_option_collision,
    config_tree_claim, config_tree_takes_long, ConfigSeam,
};
use super::default_command::ParseFailure;
use super::dispatch::DispatchFn;
use super::group::CommandRecipe;
use super::handler::{
    emits_events, CommandContext, Extensions, Handler, Output as HandlerOutput, Results, StreamSink,
};
use super::help::data::{extract_help_data, extract_help_data_with_topics};
use super::help::{
    default_help_theme, help_is_a_document, human_help_format, named_or_inline_template,
    render_help_document, render_via_request, CommandGroup, HelpConfig, HelpLength,
    DEFAULT_HELP_TEMPLATE,
};
use super::hooks::{ArtifactOutput, HookError, HookPhase, Hooks, RenderedOutput, TextOutput};
use super::questionnaire::QuestionnaireCommand;
use super::result::{HelpDisplay, HelpResult};
use standout_dispatch::verify::ExpectedArg;
use standout_render::warnings::WarningBuffer;

pub(crate) type SharedTemplateEngine =
    Rc<RefCell<Box<dyn standout_render::template::TemplateEngine>>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TemplateRef {
    Named(String),
    Inline(String),
    Absent(TemplateAbsence),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TemplateAbsence {
    Silent,
    StructuredOnly,
    Binary,
}

impl TemplateRef {
    pub(crate) fn convention(command_path: &str) -> Self {
        Self::Named(command_path.replace('.', "/"))
    }
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

fn missing_event_template_message(
    command_path: &str,
    template_name: &str,
    event_name: &str,
) -> String {
    format!(
        "command `{command_path}` produces its result while it runs, so it renders each event from template `{event_name}`, but that template is not registered; add it beside `{template_name}`, or drop the `Results` parameter if the command produces one batch value instead"
    )
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

struct PendingCommand {
    recipe: Box<dyn CommandRecipe>,
    template: TemplateRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HookRegistrationSource {
    AppBuilderHooks,
    CommandConfig,
}

/// Read once at [`AppBuilder::build`]; a truthy value turns strict mode on and never off.
pub const STRICT_STYLE_TAGS_ENV: &str = "STANDOUT_STRICT_STYLE_TAGS";

/// `1`, `true`, `yes` and `on` (any case) enable; anything else, including unset, does not.
fn strict_style_tags_from_env(value: Option<std::ffi::OsString>) -> bool {
    let Some(value) = value else {
        return false;
    };
    let Some(value) = value.to_str() else {
        return false;
    };
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

pub struct App {
    pub(crate) name: Option<String>,
    pub(crate) registry: TopicRegistry,
    pub(crate) output_flag: Option<String>,
    pub(crate) output_mode_fallback: Representation,
    pub(crate) output_file_flag: Option<String>,
    pub(crate) color_flag: Option<String>,
    pub(crate) pager_flag: Option<String>,
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
    pub(crate) version: Option<String>,
    pub(crate) startup_warnings: Vec<String>,
    pub(crate) strict_style_tags: bool,
    pub(crate) config: Option<Rc<dyn ConfigSeam>>,
    pub(crate) config_override_flag: Option<String>,
    pub(crate) config_command: bool,
}

impl App {
    pub fn builder() -> AppBuilder {
        AppBuilder::new()
    }
}

pub struct AppBuilder {
    pub(crate) name: Option<String>,
    pub(crate) registry: TopicRegistry,
    pub(crate) output_flag: Option<String>,
    pub(crate) output_mode_fallback: Representation,
    pub(crate) output_file_flag: Option<String>,
    pub(crate) color_flag: Option<String>,
    pub(crate) pager_flag: Option<String>,
    pub(crate) theme: Option<Theme>,
    pub(crate) stylesheet_registry: Option<crate::StylesheetRegistry>,
    pub(crate) template_registry: Option<TemplateRegistry>,
    pub(crate) default_theme_name: Option<String>,
    pending_commands: RefCell<HashMap<String, PendingCommand>>,
    finalized_commands: RefCell<Option<HashMap<String, DispatchFn>>>,
    pub(crate) command_hooks: HashMap<String, Hooks>,
    pub(crate) hook_phase_sources: HashMap<(String, HookPhase), HookRegistrationSource>,
    pub(crate) setup_errors: Vec<SetupError>,
    pub(crate) questionnaire_commands: HashMap<String, QuestionnaireCommand>,
    pub(crate) context_registry: ContextRegistry,
    pub(crate) default_command: Option<String>,
    pub(crate) default_command_resolver: Option<crate::cli::DefaultCommandResolver>,
    pub(crate) include_framework_templates: bool,
    pub(crate) include_framework_styles: bool,
    pub(crate) app_state: Extensions,

    pub(crate) template_engine: Option<SharedTemplateEngine>,

    pub(crate) help_command_groups: Option<Vec<CommandGroup>>,

    pub(crate) help_handling: bool,

    pub(crate) help_word: bool,

    pub(crate) ambiguous_width: crate::AmbiguousWidth,

    pub(crate) version: Option<String>,

    pub(crate) startup_warnings: Vec<String>,

    pub(crate) strict_style_tags: bool,

    pub(crate) config: Option<Box<dyn ConfigSeam>>,

    pub(crate) term_accessor: Option<Box<dyn std::any::Any>>,

    pub(crate) config_override_flag: Option<String>,

    pub(crate) config_command: bool,
}

impl AppBuilder {
    pub(crate) fn new() -> Self {
        Self {
            name: None,
            registry: TopicRegistry::new(),
            output_flag: Some("output".to_string()),
            output_mode_fallback: Representation::Human,
            output_file_flag: Some("output-file-path".to_string()),
            color_flag: Some("color".to_string()),
            pager_flag: Some("no-pager".to_string()),
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
            default_command: None,
            default_command_resolver: None,
            include_framework_templates: true,
            include_framework_styles: true,
            app_state: Extensions::new(),
            template_engine: None,
            help_command_groups: None,
            help_handling: true,
            help_word: false,
            ambiguous_width: crate::AmbiguousWidth::Narrow,
            version: None,
            startup_warnings: Vec::new(),
            strict_style_tags: false,
            config: None,
            term_accessor: None,
            config_override_flag: None,
            config_command: true,
        }
    }

    pub fn app_state<T: 'static>(mut self, value: T) -> Self {
        self.app_state.insert(value);
        self
    }

    pub fn template_engine(
        mut self,
        engine: Box<dyn standout_render::template::TemplateEngine>,
    ) -> Self {
        self.template_engine = Some(Rc::new(RefCell::new(engine)));
        self
    }

    #[cfg(test)]
    pub(crate) fn has_command(&self, path: &str) -> bool {
        self.pending_commands.borrow().contains_key(path)
    }

    pub fn build(mut self) -> Result<App, SetupError> {
        use crate::assets::FRAMEWORK_TEMPLATES;

        if !self.setup_errors.is_empty() {
            return Err(self.setup_errors.remove(0));
        }

        if self.include_framework_templates {
            match self.template_registry.as_mut() {
                Some(registry) => registry.add_framework_entries(FRAMEWORK_TEMPLATES),
                None => {
                    let mut registry = TemplateRegistry::new();
                    registry.add_framework_entries(FRAMEWORK_TEMPLATES);
                    self.template_registry = Some(registry);
                }
            };
        }

        let app_theme = self.resolve_configured_theme()?;
        self.theme = Some(
            self.framework_base_theme()?
                .merge(app_theme.unwrap_or_else(Theme::new)),
        );

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
                    "{feature} is configured while help handling is off — \
                     standout cannot render grouped/topic help without intercepting help. \
                     Drop the .help_handling(false) call, or drop the {feature}"
                )));
            }
            if self.help_word {
                return Err(SetupError::Config(
                    "help_word is set while help handling is off — the `help` word is \
                     standout's own subcommand, so there is nothing to install without \
                     help interception. Drop the .help_handling(false) call, or drop \
                     .help_word(true)"
                        .to_string(),
                ));
            }
        }

        if self.help_handling {
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

        if let Some(accessor) = self.term_accessor.take() {
            match self.config.as_mut() {
                Some(seam) => seam.attach_term_accessor(accessor)?,
                None => {
                    return Err(SetupError::Config(
                        "term_settings is set without .config(...): there is no configuration \
                         to read the accessor from"
                            .to_string(),
                    ))
                }
            }
        }

        let installs_config_command = self.config_command && self.config.is_some();

        if let Some(flag) = self.config_override_flag.as_deref() {
            if self.config.is_none() {
                return Err(SetupError::Config(format!(
                    "config_override_flag(\"{flag}\") is set without .config(...): there is \
                     no configuration for the flag to override"
                )));
            }
            let taken = [
                self.output_flag.as_deref(),
                self.output_file_flag.as_deref(),
                self.color_flag.as_deref(),
                self.pager_flag.as_deref(),
            ];
            if taken.contains(&Some(flag))
                || (installs_config_command && config_tree_takes_long(flag))
            {
                return Err(SetupError::Config(format!(
                    "config_override_flag(\"{flag}\") names a flag standout already installs"
                )));
            }
        }

        if !self.config_command && self.config.is_none() {
            return Err(SetupError::Config(
                "no_config_command() is set without .config(...): there is no `config` \
                 command to remove"
                    .to_string(),
            ));
        }

        if installs_config_command {
            let taken = [
                ("output_flag", self.output_flag.as_deref()),
                ("output_file_flag", self.output_file_flag.as_deref()),
                ("color_flag", self.color_flag.as_deref()),
                ("pager_flag", self.pager_flag.as_deref()),
            ]
            .into_iter()
            .find_map(|(option, flag)| {
                flag.filter(|flag| config_tree_takes_long(flag))
                    .map(|flag| (option, flag))
            });
            if let Some((option, flag)) = taken {
                return Err(config_option_collision(&format!(
                    "{option}(Some(\"{flag}\")) installs `--{flag}` as a root-global flag"
                )));
            }
            let claim = self
                .pending_commands
                .borrow()
                .keys()
                .filter(|path| claims_config_path(path))
                .min()
                .cloned();
            if let Some(path) = claim {
                return Err(config_command_collision(&format!(
                    "this application registers `{path}`"
                )));
            }
        }

        let template_engine = self.template_engine.take().unwrap_or_else(|| {
            Rc::new(RefCell::new(Box::new(
                standout_render::template::MiniJinjaEngine::new(),
            )))
        });

        self.validate_command_templates()?;
        self.validate_framework_template_styles()?;

        if let Some(registry) = &self.template_registry {
            refresh_engine_templates(&mut **template_engine.borrow_mut(), registry)
                .map_err(|error| SetupError::Template(error.to_string()))?;
        }

        let app = App {
            name: self.name,
            registry: self.registry,
            output_flag: self.output_flag,
            output_mode_fallback: self.output_mode_fallback,
            output_file_flag: self.output_file_flag,
            color_flag: self.color_flag,
            pager_flag: self.pager_flag,
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
            strict_style_tags: self.strict_style_tags
                || strict_style_tags_from_env(std::env::var_os(STRICT_STYLE_TAGS_ENV)),
            config: self.config.map(Rc::from),
            config_override_flag: self.config_override_flag,
            config_command: self.config_command,
        };

        app.ensure_commands_finalized();

        Ok(app)
    }

    fn validate_command_templates(&self) -> Result<(), SetupError> {
        for (path, pending) in self.pending_commands.borrow().iter() {
            let name = match &pending.template {
                TemplateRef::Named(name) => name.clone(),
                TemplateRef::Inline(_) | TemplateRef::Absent(_) => continue,
            };
            let Some(registry) = self.template_registry.as_ref() else {
                return Err(SetupError::Template(missing_template_message(
                    path, &name, None,
                )));
            };
            if pending.recipe.emits_events() {
                let event_name = format!("{name}.event");
                if let Err(error) = registry.get_content(&event_name) {
                    let message = match error {
                        standout_render::RegistryError::NotFound { .. } => {
                            missing_event_template_message(path, &name, &event_name)
                        }
                        _ => TemplateRefreshError::new(&event_name, registry, error.to_string())
                            .to_string(),
                    };
                    return Err(SetupError::Template(message));
                }
                // A handler returning `Output::Silent` renders no summary, and
                // the build cannot read which variant it returns, so a missing
                // summary template is a render error on the run instead.
                if matches!(
                    registry.get_content(&name),
                    Err(standout_render::RegistryError::NotFound { .. })
                ) {
                    continue;
                }
            }
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

    fn resolve_configured_theme(&mut self) -> Result<Option<Theme>, SetupError> {
        if self.theme.is_some() {
            if self.stylesheet_registry.is_some() {
                return Err(SetupError::Config(
                    "the app configures both .theme(...) and .styles(...)/.styles_dir(...); \
                     .theme(...) replaces the whole stylesheet registry, so keep one of \
                     them — merge the stylesheets into the Theme, or drop the .theme(...) \
                     call and select a registered theme with .default_theme(name)"
                        .to_string(),
                ));
            }
            return Ok(self.theme.take());
        }

        let Some(ref mut registry) = self.stylesheet_registry else {
            if let Some(name) = &self.default_theme_name {
                return Err(SetupError::ThemeNotFound(name.to_string()));
            }
            return Ok(None);
        };

        let Some(name) = &self.default_theme_name else {
            return Ok(None);
        };
        Ok(Some(registry.get(name).map_err(|_| {
            SetupError::ThemeNotFound(name.to_string())
        })?))
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
                self.strict_style_tags,
            );
            commands.insert(path.clone(), dispatch);
        }

        *self.finalized_commands.borrow_mut() = Some(commands);
    }

    fn get_commands(&self) -> std::cell::Ref<'_, HashMap<String, DispatchFn>> {
        self.ensure_commands_finalized();
        std::cell::Ref::map(self.finalized_commands.borrow(), |opt| match opt.as_ref() {
            Some(commands) => commands,
            None => unreachable!("command finalization stores a command map before returning"),
        })
    }

    pub fn registry(&self) -> &TopicRegistry {
        &self.registry
    }

    fn emits_events_for(&self, path: &str) -> bool {
        self.pending_commands
            .borrow()
            .get(path)
            .is_some_and(|pending| pending.recipe.emits_events())
    }

    pub(crate) fn pageable_for(&self, path: &str) -> bool {
        self.pending_commands
            .borrow()
            .get(path)
            .is_some_and(|pending| pending.recipe.pageable())
    }

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

    pub fn get_default_theme(&self) -> &Theme {
        &self.theme
    }

    pub fn template_names(&self) -> impl Iterator<Item = &str> {
        self.template_registry
            .as_ref()
            .map(|r| r.names())
            .into_iter()
            .flatten()
    }

    pub fn theme_names(&self) -> Vec<String> {
        self.stylesheet_registry
            .as_ref()
            .map(|r| r.names().map(String::from).collect())
            .unwrap_or_default()
    }

    pub fn get_matches_from<I, T>(
        &self,
        cmd: Command,
        itr: I,
        sources: &crate::InputSources,
    ) -> HelpResult
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        if let Err(error) = self
            .config_override_flag_collision(&cmd)
            .and_then(|()| self.framework_flag_collision(&cmd))
            .and_then(|()| self.config_command_collision(&cmd))
        {
            return HelpResult::Error(clap::Error::raw(
                clap::error::ErrorKind::ArgumentConflict,
                format!("{error}\n"),
            ));
        }

        let mut cmd = self.augment_command_with_help(cmd);

        if let Some(error) = self.help_word_collision(&cmd) {
            return HelpResult::Error(clap::Error::raw(
                clap::error::ErrorKind::InvalidSubcommand,
                format!("{error}\n"),
            ));
        }

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
                let color_policy = self.resolve_color_policy(
                    self.typed_color_from_unparsed(&args),
                    ColorPolicy::Auto,
                    None,
                );
                return match self.intercept_display_help(
                    &mut cmd,
                    &args,
                    &e,
                    None,
                    color_policy,
                    None,
                ) {
                    Some(display) => display.into(),
                    None => HelpResult::Error(e),
                };
            }
        };

        let color_policy =
            self.resolve_color_policy(self.typed_color_policy(&matches), ColorPolicy::Auto, None);
        match self.intercept_help_word(&mut cmd, &matches, None, color_policy, None) {
            Some(display) => display.into(),
            None => HelpResult::Matches(matches),
        }
    }

    pub(crate) fn intercept_help_word(
        &self,
        cmd: &mut Command,
        matches: &ArgMatches,
        target: Option<crate::TargetProperties>,
        color_policy: ColorPolicy,
        warnings: Option<standout_render::warnings::WarningBuffer>,
    ) -> Option<HelpDisplay> {
        if !self.help_handling {
            return None;
        }
        let (name, sub_matches) = matches.subcommand()?;
        (name == "help").then(|| {
            self.render_help_word(cmd, matches, sub_matches, target, color_policy, warnings)
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn intercept_display_help(
        &self,
        cmd: &mut Command,
        args: &[std::ffi::OsString],
        error: &clap::Error,
        target: Option<crate::TargetProperties>,
        color_policy: ColorPolicy,
        warnings: Option<standout_render::warnings::WarningBuffer>,
    ) -> Option<HelpDisplay> {
        (self.help_handling && error.kind() == clap::error::ErrorKind::DisplayHelp).then(|| {
            self.render_help_for_display_help_error(cmd, args, target, color_policy, warnings)
        })
    }

    fn help_target_properties(
        &self,
        target: Option<crate::TargetProperties>,
    ) -> crate::TargetProperties {
        let mut target = target.unwrap_or_else(crate::TargetProperties::detect);
        target.ambiguous_width = self.ambiguous_width;
        target
    }

    fn help_theme(&self) -> Theme {
        self.theme.clone()
    }

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

    #[allow(clippy::too_many_arguments)]
    fn render_help_surface<T: Serialize>(
        &self,
        data: &T,
        template: crate::TemplateRef,
        format: Representation,
        target: crate::TargetProperties,
        color_policy: ColorPolicy,
        warnings: Option<standout_render::warnings::WarningBuffer>,
    ) -> Result<String, RenderError> {
        render_via_request(
            data,
            template,
            self.help_theme(),
            format,
            color_policy,
            target,
            self.template_engine.clone(),
            self.template_registry.clone(),
            Some(self.context_registry.clone()),
            warnings,
        )
    }

    fn help_display(&self, cmd: &Command, rendered: Result<String, RenderError>) -> HelpDisplay {
        match rendered {
            Ok(text) => HelpDisplay::Rendered { text },
            Err(e) => Self::render_failure(cmd, e),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_help_word(
        &self,
        cmd: &mut Command,
        matches: &ArgMatches,
        sub_matches: &ArgMatches,
        target: Option<crate::TargetProperties>,
        color_policy: ColorPolicy,
        warnings: Option<standout_render::warnings::WarningBuffer>,
    ) -> HelpDisplay {
        let format = self.extract_output_mode(matches);
        let target = self.help_target_properties(target);
        let config = HelpConfig {
            command_groups: self.help_command_groups.clone(),
            length: HelpLength::Long,
            ..Default::default()
        };
        if let Some(topic_args) = sub_matches.get_many::<String>("topic") {
            let keywords: Vec<_> = topic_args.map(|s| s.as_str()).collect();
            if !keywords.is_empty() {
                return self.handle_help_request(
                    cmd,
                    &keywords,
                    config,
                    format,
                    target,
                    color_policy,
                    warnings,
                );
            }
        }

        self.render_root_help(cmd, config, format, target, color_policy, warnings)
    }

    fn render_failure(cmd: &Command, error: impl std::fmt::Display) -> HelpDisplay {
        HelpDisplay::RenderFailed(cmd.clone().error(
            clap::error::ErrorKind::Io,
            format!("failed to render help: {error}"),
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn render_root_help(
        &self,
        cmd: &Command,
        config: HelpConfig,
        format: Representation,
        target: crate::TargetProperties,
        color_policy: ColorPolicy,
        warnings: Option<standout_render::warnings::WarningBuffer>,
    ) -> HelpDisplay {
        if help_is_a_document(format) {
            return self.help_document(cmd, &[], config.length, format);
        }
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
            &[],
            &self.registry,
            config.command_groups.as_deref(),
            config.length,
            &target,
        )
        .expect("the root is always at the empty path");
        self.help_display(
            cmd,
            self.render_help_surface(
                &data,
                template,
                human_help_format(format),
                target,
                color_policy,
                warnings,
            ),
        )
    }

    /// `csv` has no help projection and is a render error.
    fn help_document(
        &self,
        cmd: &Command,
        path: &[&str],
        length: HelpLength,
        format: Representation,
    ) -> HelpDisplay {
        match render_help_document(cmd, path, length, format) {
            Ok(Some(text)) => HelpDisplay::Rendered { text },
            Ok(None) => HelpDisplay::Clap(cmd.clone().error(
                clap::error::ErrorKind::InvalidSubcommand,
                format!("The subcommand '{}' wasn't recognized", path.join(" ")),
            )),
            Err(e) => Self::render_failure(cmd, e),
        }
    }

    fn render_help_for_display_help_error(
        &self,
        cmd: &mut Command,
        args: &[std::ffi::OsString],
        target: Option<crate::TargetProperties>,
        color_policy: ColorPolicy,
        warnings: Option<standout_render::warnings::WarningBuffer>,
    ) -> HelpDisplay {
        let request = Self::help_request(cmd, args);
        let format = self.extract_output_mode_from_unparsed(args);
        let target = self.help_target_properties(target);
        let config = HelpConfig {
            command_groups: self.help_command_groups.clone(),
            length: request.length,
            ..Default::default()
        };

        if request.target.is_empty() {
            return self.render_root_help(cmd, config, format, target, color_policy, warnings);
        }

        let keywords: Vec<&str> = request.target.iter().map(|s| s.as_str()).collect();
        self.handle_help_request(
            cmd,
            &keywords,
            config,
            format,
            target,
            color_policy,
            warnings,
        )
    }

    fn help_request(cmd: &Command, args: &[std::ffi::OsString]) -> HelpRequest {
        HelpRequest {
            target: Self::help_target(cmd, args),
            length: Self::help_length(cmd, args),
        }
    }

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

    #[allow(clippy::too_many_arguments)]
    fn handle_help_request(
        &self,
        cmd: &mut Command,
        keywords: &[&str],
        config: HelpConfig,
        format: Representation,
        target: crate::TargetProperties,
        color_policy: ColorPolicy,
        warnings: Option<standout_render::warnings::WarningBuffer>,
    ) -> HelpDisplay {
        let sub_name = keywords[0];
        let page_format = human_help_format(format);

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
                self.render_help_surface(
                    &data,
                    template,
                    page_format,
                    target,
                    color_policy,
                    warnings,
                ),
            );
        }

        if super::app::find_subcommand_recursive(cmd, keywords).is_some() {
            if help_is_a_document(format) {
                return self.help_document(cmd, keywords, config.length, format);
            }
            let template = match self.help_template(
                config.template.as_deref(),
                crate::assets::HELP_TEMPLATE_NAME,
                DEFAULT_HELP_TEMPLATE,
            ) {
                Ok(template) => template,
                Err(e) => return Self::render_failure(cmd, e),
            };
            if let Some(data) = extract_help_data(
                cmd,
                keywords,
                config.command_groups.as_deref(),
                config.length,
                &target,
            ) {
                return self.help_display(
                    cmd,
                    self.render_help_surface(
                        &data,
                        template,
                        page_format,
                        target,
                        color_policy,
                        warnings,
                    ),
                );
            }
        }

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
                self.render_help_surface(
                    &topic_data(topic),
                    template,
                    page_format,
                    target,
                    color_policy,
                    warnings,
                ),
            );
        }

        let err = cmd.error(
            clap::error::ErrorKind::InvalidSubcommand,
            format!("The subcommand or topic '{}' wasn't recognized", sub_name),
        );
        HelpDisplay::Clap(err)
    }

    pub fn augment_command_with_help(&self, cmd: Command) -> Command {
        let cmd = self.augment_framework_surface(cmd);

        if !self.help_handling {
            return cmd;
        }

        let cmd = cmd.disable_help_subcommand(true);
        if self.installs_help_word(&cmd) {
            let has_subcommands = cmd.get_subcommands().next().is_some();
            cmd.subcommand(help_word_command(has_subcommands))
                .subcommand_negates_reqs(true)
        } else {
            cmd
        }
    }

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

    pub(crate) fn installs_config_command(&self) -> bool {
        self.config_command && self.config.is_some()
    }

    pub(crate) fn config_command_collision(&self, cmd: &Command) -> Result<(), SetupError> {
        if !self.installs_config_command() {
            return Ok(());
        }
        if cmd.get_subcommands().any(claims_config_command) {
            return Err(config_command_collision(
                "this application's clap `Command` declares `config` (as a subcommand name or alias)",
            ));
        }
        if let Some(claim) = cmd
            .get_arguments()
            .filter(|arg| arg.is_global_set())
            .find_map(config_tree_claim)
        {
            return Err(config_option_collision(&format!(
                "this application's clap `Command` declares a root-global argument with {claim}"
            )));
        }
        Ok(())
    }

    pub(crate) fn installs_help_word(&self, cmd: &Command) -> bool {
        self.help_word
            || cmd.get_subcommands().next().is_some()
            || cmd.get_positionals().next().is_none()
    }

    pub fn extract_output_mode(&self, matches: &ArgMatches) -> Representation {
        self.typed_output_mode(matches)
            .unwrap_or(self.output_mode_fallback)
    }

    pub(crate) fn typed_output_mode(&self, matches: &ArgMatches) -> Option<Representation> {
        self.output_flag.as_ref()?;
        match matches.try_get_one::<String>(OUTPUT_MODE_ARG) {
            // A `DefaultValue` source means the user never typed `--output`.
            Ok(Some(value))
                if matches.value_source(OUTPUT_MODE_ARG) != Some(ValueSource::DefaultValue) =>
            {
                parse_output_mode_flag(value.as_str())
            }
            _ => None,
        }
    }

    /// The run's color policy, in precedence order: an explicit `--color`, the
    /// policy the caller named, `NO_COLOR`, the resolved `[term] color`, and
    /// last the destination, which `Auto` leaves to `resolve_style_mode`.
    /// `NO_COLOR` is read here only to outrank a configured `always`; below the
    /// key it is a destination fact the process edge already probes.
    pub(crate) fn resolve_color_policy(
        &self,
        typed: Option<ColorPolicy>,
        named: ColorPolicy,
        term: Option<&crate::TermSettings>,
    ) -> ColorPolicy {
        if let Some(policy) = typed {
            return policy;
        }
        if named != ColorPolicy::Auto {
            return named;
        }
        match term.and_then(|term| term.color).map(ColorPolicy::from) {
            Some(ColorPolicy::Always) if no_color_is_set() => ColorPolicy::Never,
            Some(policy) => policy,
            None => ColorPolicy::Auto,
        }
    }

    pub(crate) fn typed_color_policy(&self, matches: &ArgMatches) -> Option<ColorPolicy> {
        self.color_flag.as_ref()?;
        match matches.try_get_one::<String>(COLOR_ARG) {
            // A `DefaultValue` source means the user never typed `--color`.
            Ok(Some(value))
                if matches.value_source(COLOR_ARG) != Some(ValueSource::DefaultValue) =>
            {
                parse_color_flag(value.as_str())
            }
            _ => None,
        }
    }

    /// The pre-parse read the help and usage paths use, where no `ArgMatches` exists yet.
    pub(crate) fn typed_color_from_unparsed(
        &self,
        args: &[std::ffi::OsString],
    ) -> Option<ColorPolicy> {
        self.color_flag
            .as_deref()
            .and_then(|flag| last_unparsed_flag_value(flag, args))
            .and_then(parse_color_flag)
    }

    /// A named file is the run's destination whatever else the invocation asks
    /// for, so the page lands there, never in a pager.
    pub(crate) fn output_file_from_unparsed(
        &self,
        args: &[std::ffi::OsString],
    ) -> Option<std::path::PathBuf> {
        self.output_file_flag
            .as_deref()
            .and_then(|flag| last_unparsed_flag_value(flag, args))
            .map(std::path::PathBuf::from)
    }

    pub(crate) fn paging_is_suppressed(&self, args: &[std::ffi::OsString]) -> bool {
        self.pager_flag
            .as_deref()
            .is_some_and(|flag| unparsed_flag_is_present(flag, args))
    }

    pub(crate) fn extract_output_mode_from_unparsed(
        &self,
        args: &[std::ffi::OsString],
    ) -> Representation {
        let Some(flag) = self.output_flag.as_deref() else {
            return self.output_mode_fallback;
        };
        last_unparsed_flag_value(flag, args)
            .and_then(parse_output_mode_flag)
            .unwrap_or(self.output_mode_fallback)
    }

    /// One handler, hooks and render included. A typed `--color` and
    /// `[term] color` outrank `color_policy`; `sink` takes the handler's events
    /// as it emits them.
    #[allow(clippy::too_many_arguments)]
    pub fn run_command<H>(
        &self,
        path: &str,
        matches: &ArgMatches,
        mut handler: H,
        template: crate::TemplateRef,
        color_policy: ColorPolicy,
        sink: StreamSink,
    ) -> Result<RenderedOutput, HookError>
    where
        H: Handler,
    {
        let config = self
            .resolve_config(matches)
            .map_err(|error| HookError::pre_dispatch("Config error").with_source(error))?;
        let resolved = self.resolve_run(
            matches,
            config.as_ref().and_then(|config| config.term.as_ref()),
            None,
            color_policy,
            self.output_mode_fallback,
            self.process_edge_target(),
        );
        let (output_mode, color_policy, target) = (
            resolved.representation,
            resolved.color_policy,
            resolved.target,
        );
        let mut ctx = CommandContext::new(
            path.split('.').map(String::from).collect(),
            self.app_state.clone(),
        );
        let warnings = WarningBuffer::new();
        self.seed_startup_warnings(&warnings);
        ctx.extensions.insert(InputSources::from_process());
        ctx.extensions.insert(warnings.clone());
        if let Some(config) = config {
            config.install(&mut ctx.extensions);
        }

        let hooks = self.command_hooks.get(path);

        if let Some(hooks) = hooks {
            hooks.run_pre_dispatch(matches, &mut ctx)?;
        }

        let destination = std::rc::Rc::new(crate::cli::events::EventDestination::new(
            sink,
            crate::cli::events::EventContext {
                command_path: path.to_string(),
                template: crate::cli::events::rendered_event_template(&template),
                theme: self.theme.clone(),
                context_registry: self.context_registry.clone(),
                template_engine: self.template_engine.clone(),
                template_registry: self.template_registry.clone(),
                representation: output_mode,
                color_policy,
                target,
                warnings: Some(warnings.clone()),
                strict_style_tags: self.strict_style_tags,
            },
        ));
        let mut results = Results::<H::Event>::for_run(None, destination.clone());
        let handled = handler.handle(matches, &ctx, &mut results);
        drop(results);
        if let Some(failure) = destination.take_failure() {
            return Err(HookError::post_output("Render error").with_source(failure));
        }
        let document_records = emits_events::<H::Event>()
            .then(|| destination.take_document_records())
            .flatten();
        let (output, status) = match handled {
            Ok(output) => output.split_exit_status(),
            Err(e) => return Err(HookError::post_output("Handler error").with_source(e)),
        };
        let reject_status_without_a_carrier = |is_binary: bool, is_artifact: bool| {
            super::dispatch::reject_status_without_a_carrier(status, is_binary, is_artifact)
                .map_err(|e| HookError::post_output("Render error").with_source(e))
        };
        let reject_payload_from_an_emitting_command = |is_binary: bool, is_artifact: bool| {
            super::dispatch::reject_payload_from_an_emitting_command(
                emits_events::<H::Event>(),
                is_binary,
                is_artifact,
            )
            .map_err(|e| HookError::post_output("Render error").with_source(e))
        };
        reject_status_without_a_carrier(output.is_binary(), output.is_artifact())?;

        let render_value = |data: serde_json::Value| -> Result<RenderedOutput, HookError> {
            let request = RenderRequest {
                data,
                template: template.clone(),
                theme: self.theme.clone(),
                format: output_mode,
                color_policy,
                target,
                engine: self.template_engine.clone(),
                registry: self.template_registry.clone(),
                context_registry: Some(self.context_registry.clone()),
                csv_projection: self.csv_projection_for(path),
                extras: HashMap::new(),
                warnings: Some(warnings.clone()),
            };
            render_request_split(&request)
                .map(|rendered| {
                    RenderedOutput::Text(TextOutput::new(rendered.formatted, rendered.raw))
                })
                .map_err(|e| HookError::post_output("Render error").with_source(e))
        };
        // CSV takes the retained events as its rows and leaves the summary out;
        // the other document encodings take the array of records.
        let event_rows = output_mode == crate::Representation::Csv;

        let output = match output {
            HandlerOutput::Render(data) => {
                let mut json_data = serde_json::to_value(&data)
                    .map_err(|e| HookError::post_dispatch("Serialization error").with_source(e))?;

                if let Some(hooks) = hooks {
                    json_data = hooks.run_post_dispatch(matches, &ctx, json_data)?;
                }

                match document_records {
                    Some(mut records) if !event_rows => {
                        records.push(standout_render::result_record(json_data));
                        run_document(records, output_mode)?
                    }
                    Some(records) => render_value(serde_json::Value::Array(records))?,
                    None => render_value(json_data)?,
                }
            }
            HandlerOutput::Silent => match document_records {
                Some(records) if !event_rows => run_document(records, output_mode)?,
                Some(records) => render_value(serde_json::Value::Array(records))?,
                None => RenderedOutput::Silent,
            },
            HandlerOutput::Binary { data, filename } => RenderedOutput::Binary(data, filename),
            HandlerOutput::Artifact(artifact) => {
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
            _ => {
                return Err(HookError::post_output(
                    "Unsupported handler output variant: this standout version cannot present it",
                ));
            }
        };

        let output = match hooks {
            Some(hooks) => hooks.run_post_output(matches, &ctx, output)?,
            None => output,
        };
        reject_status_without_a_carrier(output.is_binary(), output.is_artifact())?;
        reject_payload_from_an_emitting_command(output.is_binary(), output.is_artifact())?;
        super::dispatch::reject_payload_under_stream(
            output_mode,
            output.is_binary(),
            output.is_artifact(),
        )
        .map_err(|e| HookError::post_output("Render error").with_source(e))?;
        Ok(output)
    }

    pub fn verify_command(&self, cmd: &Command) -> Result<(), SetupError> {
        self.malformed_registrations()?;
        self.validate_questionnaire_surfaces(cmd)?;
        self.unreachable_registrations(cmd)?;
        self.config_override_flag_collision(cmd)?;
        self.framework_flag_collision(cmd)?;
        self.config_command_collision(cmd)?;
        let expected_args: HashMap<String, Vec<ExpectedArg>> = self
            .pending_commands
            .borrow()
            .iter()
            .map(|(path, cmd)| (path.clone(), cmd.recipe.expected_args()))
            .collect();
        super::app::verify_recursive(cmd, &expected_args, &[], true)
    }
}

/// The one document an incremental command ends in under `json` or `yaml`. No
/// warning record joins the array: `run_command` owns no stdout of its own.
fn run_document(
    records: Vec<serde_json::Value>,
    output_mode: crate::Representation,
) -> Result<RenderedOutput, HookError> {
    let document = standout_render::serialize_record_array(records, output_mode)
        .map_err(|e| HookError::post_output("Render error").with_source(e))?;
    Ok(RenderedOutput::Text(TextOutput::new(
        document.clone(),
        document,
    )))
}

fn claims_help(cmd: &Command) -> bool {
    cmd.get_name() == "help" || cmd.get_all_aliases().any(|alias| alias == "help")
}

fn claims_root_help(path: &str) -> bool {
    path == "help" || path.starts_with("help.")
}

const DECLARED_CLAIM: &str =
    "this application's clap `Command` declares `help` (as a subcommand name or alias)";

fn registered_claim(path: &str) -> String {
    if path == "help" {
        "this application registers a `help` command".to_string()
    } else {
        format!("this application registers `{path}`, hanging a command off the same root word")
    }
}

fn duplicate_help_word(claim: &str) -> SetupError {
    SetupError::DuplicateCommand(format!(
        "help — {claim}, and standout installs a `help` word of its own, since help \
         handling is on by default. Rename the application's command, or call \
         .help_handling(false) to keep the name (help is then clap's own, and \
         command_groups and topics become unavailable)"
    ))
}

const HELP_PROBE_SHORT: &str = "__standout_help_short";
const HELP_PROBE_LONG: &str = "__standout_help_long";

/// `--output` names a structured encoding and nothing else; the human
/// representation is what a bare invocation renders and has no spelling.
/// `term-debug` stays as the diagnostic view of the template's style tags.
pub(crate) const OUTPUT_MODE_FLAG_VALUES: [&str; 5] =
    ["json", "yaml", "csv", "ndjson", "term-debug"];

fn parse_output_mode_flag(value: &str) -> Option<Representation> {
    match value {
        "json" => Some(Representation::Json),
        "yaml" => Some(Representation::Yaml),
        "csv" => Some(Representation::Csv),
        "ndjson" => Some(Representation::Ndjson),
        "term-debug" => Some(Representation::TermDebug),
        _ => None,
    }
}

/// `None` for the human representation, which the flag cannot name.
pub(crate) fn output_mode_flag_spelling(representation: Representation) -> Option<&'static str> {
    OUTPUT_MODE_FLAG_VALUES
        .into_iter()
        .find(|value| parse_output_mode_flag(value) == Some(representation))
}

/// `--color` decides whether human text carries escape sequences, on its own
/// and whatever `--output` names.
pub(crate) const COLOR_FLAG_VALUES: [&str; 3] = ["auto", "always", "never"];

pub(crate) const COLOR_ARG: &str = "_color";
pub(crate) const NO_PAGER_ARG: &str = "_no_pager";
pub(crate) const OUTPUT_MODE_ARG: &str = "_output_mode";
pub(crate) const OUTPUT_FILE_ARG: &str = "_output_file_path";

pub(crate) const COLOR_FLAG_DEFAULT: &str = "auto";

/// The convention: set and non-empty asks for no color.
fn no_color_is_set() -> bool {
    std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty())
}

fn parse_color_flag(value: &str) -> Option<ColorPolicy> {
    match value {
        "auto" => Some(ColorPolicy::Auto),
        "always" => Some(ColorPolicy::Always),
        "never" => Some(ColorPolicy::Never),
        _ => None,
    }
}

fn unparsed_flag_is_present(flag: &str, args: &[std::ffi::OsString]) -> bool {
    let long = format!("--{flag}");
    args.iter()
        .skip(1)
        .filter_map(|arg| arg.to_str())
        .take_while(|arg| *arg != "--")
        .any(|arg| arg == long)
}

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

#[derive(Debug, Default, PartialEq, Eq)]
struct HelpRequest {
    target: Vec<String>,
    length: HelpLength,
}

fn help_word_command(has_subcommands: bool) -> Command {
    let (about, topic_help) = if has_subcommands {
        (
            "Print this message or the help of the given subcommand(s)",
            "The subcommand or topic to print help for",
        )
    } else {
        ("Print this message", "The topic to print help for")
    };

    Command::new("help").about(about).arg(
        Arg::new("topic")
            .action(ArgAction::Set)
            .num_args(1..)
            .help(topic_help),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use console::Style;

    #[test]
    fn strict_style_tags_env_reads_truthy_spellings_and_nothing_else() {
        for truthy in ["1", "true", "TRUE", "Yes", "on", "  on  "] {
            assert!(
                strict_style_tags_from_env(Some(truthy.into())),
                "{truthy:?} should enable strict mode"
            );
        }
        for falsy in ["0", "false", "no", "off", "", "enabled"] {
            assert!(
                !strict_style_tags_from_env(Some(falsy.into())),
                "{falsy:?} should not enable strict mode"
            );
        }
        assert!(
            !strict_style_tags_from_env(None),
            "an unset variable should not enable strict mode"
        );
    }

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

    #[test]
    fn test_help_request_does_not_mistake_an_option_value_for_a_flag() {
        assert_eq!(request(&["app", "-o", "h"]).length, HelpLength::Short);
        assert!(request(&["app", "-o", "h"]).target.is_empty());
    }

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
    fn a_stylesheet_registry_without_default_theme_leaves_the_framework_base() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("base.yaml"), "style: { fg: blue }").unwrap();

        let app = AppBuilder::new()
            .styles_dir(temp_dir.path())
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(app.theme.name(), None);
    }

    #[test]
    fn default_theme_selects_the_named_registry_entry() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("base.yaml"), "style: { fg: blue }").unwrap();
        fs::write(temp_dir.path().join("theme.yaml"), "style: { fg: red }").unwrap();

        let app = AppBuilder::new()
            .styles_dir(temp_dir.path())
            .unwrap()
            .default_theme("theme")
            .build()
            .unwrap();

        assert_eq!(app.theme.name(), Some("theme"));
    }

    #[test]
    fn styles_combined_with_theme_names_both_calls() {
        use std::fs;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("base.yaml"), "style: { fg: blue }").unwrap();

        let error = match AppBuilder::new()
            .styles_dir(temp_dir.path())
            .unwrap()
            .theme(Theme::new().with_name("computed"))
            .build()
        {
            Ok(_) => panic!("expected .styles(...) with .theme(...) to fail the build"),
            Err(error) => error.to_string(),
        };

        assert!(error.contains(".theme(...)"), "{error}");
        assert!(error.contains(".styles(...)"), "{error}");
    }

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
            .app_state(Config { value: 2 })
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
