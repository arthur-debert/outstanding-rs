//! [`AppBuilder`] configures a CLI application (commands, hooks, templates,
//! themes, app-level state); [`AppBuilder::build`] consumes it into the
//! executable [`App`] that owns parsing, dispatch, rendering, and run entry
//! points. Split by concern into [`config`], [`commands`], [`execution`] and
//! [`rendering`].

#[cfg(test)]
mod test_support;

mod build;
mod help;
mod presentation;
mod run_command;
mod templates;

mod commands;
mod config;
pub(crate) mod execution;
mod rendering;

use help::{claims_root_help, duplicate_help_word, registered_claim};
pub(crate) use presentation::{
    output_mode_flag_spelling, COLOR_ARG, COLOR_FLAG_DEFAULT, COLOR_FLAG_VALUES, NO_PAGER_ARG,
    OUTPUT_FILE_ARG, OUTPUT_MODE_ARG, OUTPUT_MODE_FLAG_VALUES,
};
pub(crate) use templates::{
    refresh_engine_templates, refresh_named_template, SharedTemplateEngine, TemplateAbsence,
    TemplateRef,
};

use crate::context::ContextRegistry;
use crate::setup::SetupError;
use crate::topics::TopicRegistry;
use crate::TemplateRegistry;
use crate::{Representation, Theme};
use clap::Command;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::config::{
    claims_config_command, config_command_collision, config_option_collision, config_tree_claim,
    ConfigSeam,
};
use super::dispatch::DispatchFn;
use super::group::CommandRecipe;
use super::handler::{ExitStatus, Extensions};
use super::help::CommandGroup;
use super::hooks::{HookPhase, Hooks};
use super::questionnaire::QuestionnaireCommand;
use standout_dispatch::verify::ExpectedArg;

struct PendingCommand {
    recipe: Box<dyn CommandRecipe>,
    template: TemplateRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HookRegistrationSource {
    AppBuilderHooks,
    CommandConfig,
}

impl HookRegistrationSource {
    fn describe(self, path: &str) -> String {
        match self {
            Self::AppBuilderHooks => format!("`AppBuilder::hooks(\"{path}\", ..)`"),
            Self::CommandConfig => format!(
                "the command's own `CommandConfig` (`command_with(\"{path}\", ..)`, or \
                 `pre_dispatch`/`post_dispatch`/`post_output` on the `#[derive(Dispatch)]` variant)"
            ),
        }
    }
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
    pub(crate) command_input_chains: HashMap<String, Hooks>,
    pub(crate) command_questionnaire_resolution: HashMap<String, Hooks>,
    pub(crate) questionnaire_commands: HashMap<String, QuestionnaireCommand>,
    pub(crate) config_exempt_commands: HashSet<String>,
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
    pub(crate) usage_exit_status: Option<ExitStatus>,
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
    pub(crate) command_input_chains: HashMap<String, Hooks>,
    pub(crate) command_questionnaire_resolution: HashMap<String, Hooks>,
    pub(crate) hook_phase_sources: HashMap<(String, HookPhase), HookRegistrationSource>,
    pub(crate) setup_errors: Vec<SetupError>,
    pub(crate) questionnaire_commands: HashMap<String, QuestionnaireCommand>,
    pub(crate) config_exempt_commands: HashSet<String>,
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

    pub(crate) usage_exit_status: Option<ExitStatus>,

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
            command_input_chains: HashMap::new(),
            command_questionnaire_resolution: HashMap::new(),
            hook_phase_sources: HashMap::new(),
            setup_errors: Vec::new(),
            questionnaire_commands: HashMap::new(),
            config_exempt_commands: HashSet::new(),
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
            usage_exit_status: None,
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

    pub fn verify_command(&self, cmd: &Command) -> Result<(), SetupError> {
        let propagated = self.validated_command_tree(cmd)?;
        let expected_args: HashMap<String, Vec<ExpectedArg>> = self
            .pending_commands
            .borrow()
            .iter()
            .map(|(path, cmd)| (path.clone(), cmd.recipe.expected_args()))
            .collect();
        super::app::verify_recursive(&propagated, &expected_args, &[], true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
