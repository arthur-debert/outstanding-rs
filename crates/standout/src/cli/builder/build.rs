use super::claims_root_help;
use super::duplicate_help_word;
use super::refresh_engine_templates;
use super::registered_claim;
use super::strict_style_tags_from_env;
use super::App;
use super::AppBuilder;
use super::STRICT_STYLE_TAGS_ENV;
use crate::cli::config::claims_config_path;
use crate::cli::config::config_command_collision;
use crate::cli::config::config_option_collision;
use crate::cli::config::config_tree_takes_long;
use crate::cli::help::default_help_theme;
use crate::setup::SetupError;
use crate::topics::default_topic_theme;
use crate::TemplateRegistry;
use crate::Theme;
use std::cell::RefCell;
use std::rc::Rc;

impl AppBuilder {
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
            command_input_chains: self.command_input_chains,
            command_questionnaire_resolution: self.command_questionnaire_resolution,
            questionnaire_commands: self.questionnaire_commands,
            config_exempt_commands: self.config_exempt_commands,
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
            usage_exit_status: self.usage_exit_status,
            config: self.config.map(Rc::from),
            config_override_flag: self.config_override_flag,
            config_command: self.config_command,
        };

        app.ensure_commands_finalized();

        Ok(app)
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
}

#[cfg(test)]
mod tests;
