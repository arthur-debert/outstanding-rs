use crate::context::ContextProvider;
use crate::setup::SetupError;
use crate::topics::Topic;
use crate::RenderData;
use crate::TemplateRegistry;
use crate::{EmbeddedStyles, EmbeddedTemplates, Representation, Theme};

use super::AppBuilder;

impl AppBuilder {
    pub fn ambiguous_width(mut self, policy: crate::AmbiguousWidth) -> Self {
        self.ambiguous_width = policy;
        self
    }

    /// The application's own name. An application that names itself is paged by
    /// `<NAME>_PAGER` before `PAGER`; one that does not is paged by `PAGER`.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    pub fn context(mut self, name: impl Into<String>, value: RenderData) -> Self {
        self.context_registry.add_static(name, value);
        self
    }

    pub fn context_fn<P>(mut self, name: impl Into<String>, provider: P) -> Self
    where
        P: ContextProvider + 'static,
    {
        self.context_registry.add_provider(name, provider);
        self
    }

    pub fn add_topic(mut self, topic: Topic) -> Self {
        self.registry.add_topic(topic);
        self
    }

    pub fn topics_dir(mut self, path: impl AsRef<std::path::Path>) -> Result<Self, SetupError> {
        self.registry
            .add_from_directory(path)
            .map_err(SetupError::Io)?;
        Ok(self)
    }

    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = Some(theme);
        self
    }

    pub fn templates(mut self, templates: EmbeddedTemplates) -> Self {
        let warnings = standout_render::warnings::WarningBuffer::new();
        self.template_registry = Some(templates.into_registry(Some(&warnings)));
        self.startup_warnings.extend(warnings.take());
        self
    }

    pub fn styles(mut self, styles: EmbeddedStyles) -> Self {
        let warnings = standout_render::warnings::WarningBuffer::new();
        self.stylesheet_registry = Some(styles.into_registry(Some(&warnings)));
        self.startup_warnings.extend(warnings.take());
        self
    }

    pub fn styles_dir<P: AsRef<std::path::Path>>(mut self, path: P) -> Result<Self, SetupError> {
        let registry = self
            .stylesheet_registry
            .get_or_insert_with(crate::StylesheetRegistry::new);
        registry
            .add_dir(path)
            .map_err(|e| SetupError::Stylesheet(e.to_string()))?;
        Ok(self)
    }

    pub fn default_theme(mut self, name: &str) -> Self {
        self.default_theme_name = Some(name.to_string());
        self
    }

    pub fn templates_dir<P: AsRef<std::path::Path>>(mut self, path: P) -> Result<Self, SetupError> {
        let registry = self
            .template_registry
            .get_or_insert_with(TemplateRegistry::new);
        registry.add_template_dir(path)?;
        registry.refresh()?;
        Ok(self)
    }

    pub fn output_flag(mut self, name: Option<&str>) -> Self {
        self.output_flag = Some(name.unwrap_or("output").to_string());
        self
    }

    pub fn no_output_flag(mut self) -> Self {
        self.output_flag = None;
        self
    }

    pub fn output_mode_fallback(mut self, mode: Representation) -> Self {
        self.output_mode_fallback = mode;
        self
    }

    pub fn output_file_flag(mut self, name: Option<&str>) -> Self {
        self.output_file_flag = Some(name.unwrap_or("output-file-path").to_string());
        self
    }

    pub fn no_output_file_flag(mut self) -> Self {
        self.output_file_flag = None;
        self
    }

    pub fn color_flag(mut self, name: Option<&str>) -> Self {
        self.color_flag = Some(name.unwrap_or("color").to_string());
        self
    }

    pub fn no_color_flag(mut self) -> Self {
        self.color_flag = None;
        self
    }

    /// Renames the flag that suppresses paging, installed as `--no-pager`.
    pub fn pager_flag(mut self, name: Option<&str>) -> Self {
        self.pager_flag = Some(name.unwrap_or("no-pager").to_string());
        self
    }

    /// Removes the flag that suppresses paging.
    pub fn no_pager_flag(mut self) -> Self {
        self.pager_flag = None;
        self
    }

    pub fn config<C>(mut self, builder: clapfig::TypedBuilder<C>) -> Self
    where
        C: clapfig::DocumentRoot + serde::de::DeserializeOwned + 'static,
    {
        self.config = Some(Box::new(crate::cli::config::TypedSeam::new(builder)));
        self
    }

    pub fn term_settings<C, F>(mut self, accessor: F) -> Self
    where
        C: 'static,
        F: Fn(&C) -> &crate::TermSettings + 'static,
    {
        let accessor: crate::cli::config::TermAccessor<C> = Box::new(accessor);
        self.term_accessor = Some(Box::new(accessor));
        self
    }

    pub fn config_override_flag(mut self, name: &str) -> Self {
        self.config_override_flag = Some(name.to_string());
        self
    }

    pub fn no_config_command(mut self) -> Self {
        self.config_command = false;
        self
    }

    pub fn default_command(mut self, name: &str) -> Self {
        self.default_command = Some(name.to_string());
        self
    }

    pub fn default_command_with<F>(mut self, resolver: F) -> Self
    where
        F: Fn(&crate::cli::DefaultCommandContext<'_>) -> Option<String> + 'static,
    {
        self.default_command_resolver = Some(std::rc::Rc::new(resolver));
        self
    }

    pub fn include_framework_templates(mut self, include: bool) -> Self {
        self.include_framework_templates = include;
        self
    }

    pub fn include_framework_styles(mut self, include: bool) -> Self {
        self.include_framework_styles = include;
        self
    }

    pub fn command_groups(mut self, groups: Vec<super::super::help::CommandGroup>) -> Self {
        self.help_command_groups = Some(groups);
        self
    }

    pub fn help_handling(mut self, enabled: bool) -> Self {
        self.help_handling = enabled;
        self
    }

    pub fn help_word(mut self, enabled: bool) -> Self {
        self.help_word = enabled;
        self
    }

    /// Fail the run on an unresolved style tag instead of degrading; `STANDOUT_STRICT_STYLE_TAGS`
    /// forces it on. See `standout-render/docs/topics/styling-system.md`, "Strict mode".
    pub fn strict_style_tags(mut self, enabled: bool) -> Self {
        self.strict_style_tags = enabled;
        self
    }

    pub fn usage_exit_status(mut self, status: u8) -> Self {
        if status == 0 {
            self.setup_errors.push(SetupError::Config(
                "usage_exit_status(0) would report a rejected command line as shell success"
                    .to_string(),
            ));
            return self;
        }
        self.usage_exit_status = Some(crate::cli::ExitStatus::from(status));
        self
    }
}

#[cfg(test)]
mod tests;
