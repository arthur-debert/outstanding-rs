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
mod execution;
mod rendering;

use crate::context::ContextRegistry;
use crate::setup::SetupError;
use crate::topics::{
    default_topic_theme, topic_data, topics_list_data, TopicRegistry, DEFAULT_TOPICS_LIST_TEMPLATE,
    DEFAULT_TOPIC_TEMPLATE,
};
use crate::TemplateRegistry;
use crate::{
    render_request_split, ColorPolicy, InputSources, OutputMode, RenderError, RenderRequest,
    TargetProperties, Theme, TEMPLATE_EXTENSIONS,
};
use clap::parser::ValueSource;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TemplateRef {
    Named(String),
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

/// Environment variable that forces [`AppBuilder::strict_style_tags`] on,
/// letting a dev shell, CI job, or test run opt into the hard failure without a
/// code change. Read once at [`AppBuilder::build`]; a truthy value wins over the
/// builder setting, and it can only turn strict mode on, never off.
pub const STRICT_STYLE_TAGS_ENV: &str = "STANDOUT_STRICT_STYLE_TAGS";

/// Whether the environment value enables strict mode. `1`, `true`, `yes`, and
/// `on` (any case) enable it; anything else — including an unset variable — does
/// not. Kept pure so the parsing is testable without touching process state.
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
    pub(crate) registry: TopicRegistry,
    pub(crate) output_flag: Option<String>,
    pub(crate) output_mode_fallback: OutputMode,
    pub(crate) output_file_flag: Option<String>,
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
    pub(crate) startup_warnings: Vec<String>,
    pub(crate) strict_style_tags: bool,
}

impl App {
    pub fn builder() -> AppBuilder {
        AppBuilder::new()
    }
}

pub struct AppBuilder {
    pub(crate) registry: TopicRegistry,
    pub(crate) output_flag: Option<String>,
    pub(crate) output_mode_fallback: OutputMode,
    pub(crate) output_file_flag: Option<String>,
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

    pub(crate) version: Option<&'static str>,

    pub(crate) startup_warnings: Vec<String>,

    pub(crate) strict_style_tags: bool,
}

impl AppBuilder {
    pub(crate) fn new() -> Self {
        Self {
            registry: TopicRegistry::new(),
            output_flag: Some("output".to_string()),
            output_mode_fallback: OutputMode::Auto,
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
            registry: self.registry,
            output_flag: self.output_flag,
            output_mode_fallback: self.output_mode_fallback,
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
            strict_style_tags: self.strict_style_tags
                || strict_style_tags_from_env(std::env::var_os(STRICT_STYLE_TAGS_ENV)),
        };

        app.ensure_commands_finalized();

        Ok(app)
    }

    fn validate_command_templates(&self) -> Result<(), SetupError> {
        for (path, pending) in self.pending_commands.borrow().iter() {
            let name = match &pending.template {
                TemplateRef::Named(name) => name.clone(),
                TemplateRef::Absent(_) => continue,
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

    fn render_help_for_display_help_error(
        &self,
        cmd: &mut Command,
        args: &[std::ffi::OsString],
        target: Option<crate::TargetProperties>,
        warnings: Option<standout_render::warnings::WarningBuffer>,
    ) -> HelpDisplay {
        let request = Self::help_request(cmd, args);
        let format = human_help_format(self.extract_output_mode_from_unparsed(args));
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
        use_pager: bool,
        config: HelpConfig,
        format: OutputMode,
        target: crate::TargetProperties,
        warnings: Option<standout_render::warnings::WarningBuffer>,
    ) -> HelpDisplay {
        let sub_name = keywords[0];

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

    pub(crate) fn installs_help_word(&self, cmd: &Command) -> bool {
        self.help_word
            || cmd.get_subcommands().next().is_some()
            || cmd.get_positionals().next().is_none()
    }

    pub fn extract_output_mode(&self, matches: &ArgMatches) -> OutputMode {
        if self.output_flag.is_none() {
            return self.output_mode_fallback;
        }
        match matches.try_get_one::<String>("_output_mode") {
            // The flag carries the fallback as clap's default, so a
            // `DefaultValue` source means the user never typed `--output`.
            Ok(Some(value))
                if matches.value_source("_output_mode") != Some(ValueSource::DefaultValue) =>
            {
                parse_output_mode_flag(value.as_str()).unwrap_or(self.output_mode_fallback)
            }
            _ => self.output_mode_fallback,
        }
    }

    pub(crate) fn extract_output_mode_from_unparsed(
        &self,
        args: &[std::ffi::OsString],
    ) -> OutputMode {
        let Some(flag) = self.output_flag.as_deref() else {
            return self.output_mode_fallback;
        };
        last_unparsed_flag_value(flag, args)
            .and_then(parse_output_mode_flag)
            .unwrap_or(self.output_mode_fallback)
    }

    pub fn run_command<F, T>(
        &self,
        path: &str,
        matches: &ArgMatches,
        handler: F,
        template: crate::TemplateRef,
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

        if let Some(hooks) = hooks {
            hooks.run_pre_dispatch(matches, &mut ctx)?;
        }

        let result = handler(matches, &ctx);

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
                    template: template.clone(),
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

        if let Some(hooks) = hooks {
            hooks.run_post_output(matches, &ctx, output)
        } else {
            Ok(output)
        }
    }

    pub fn verify_command(&self, cmd: &Command) -> Result<(), SetupError> {
        self.malformed_registrations()?;
        self.validate_questionnaire_surfaces(cmd)?;
        self.unreachable_registrations(cmd)?;
        let expected_args: HashMap<String, Vec<ExpectedArg>> = self
            .pending_commands
            .borrow()
            .iter()
            .map(|(path, cmd)| (path.clone(), cmd.recipe.expected_args()))
            .collect();
        super::app::verify_recursive(cmd, &expected_args, &[], true)
    }
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

pub(crate) const OUTPUT_MODE_FLAG_VALUES: [&str; 9] = [
    "auto",
    "term",
    "text",
    "term-debug",
    "json",
    "yaml",
    "xml",
    "csv",
    "ndjson",
];

fn parse_output_mode_flag(value: &str) -> Option<OutputMode> {
    match value {
        "auto" => Some(OutputMode::Auto),
        "term" => Some(OutputMode::Term),
        "text" => Some(OutputMode::Text),
        "term-debug" => Some(OutputMode::TermDebug),
        "json" => Some(OutputMode::Json),
        "yaml" => Some(OutputMode::Yaml),
        "xml" => Some(OutputMode::Xml),
        "csv" => Some(OutputMode::Csv),
        "ndjson" => Some(OutputMode::Ndjson),
        _ => None,
    }
}

pub(crate) fn output_mode_flag_spelling(mode: OutputMode) -> &'static str {
    OUTPUT_MODE_FLAG_VALUES
        .into_iter()
        .find(|value| parse_output_mode_flag(value) == Some(mode))
        .unwrap_or("auto")
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
