//! AppBuilder for constructing App instances.
//!
//! This module provides the [`AppBuilder`] type for configuring and
//! constructing [`App`] instances with commands, hooks, templates, and themes.
//!
//! The builder is split into submodules by concern:
//! - [`config`]: Configuration methods (themes, templates, context, flags)
//! - [`commands`]: Command and handler registration
//! - [`execution`]: Dispatch macro integration and command execution

mod commands;
mod config;
mod execution;

use crate::context::ContextRegistry;
use crate::setup::SetupError;
use crate::topics::TopicRegistry;
use crate::TemplateRegistry;
use crate::{OutputMode, Theme};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use super::app::App;
use super::dispatch::DispatchFn;
use super::group::CommandRecipe;
use super::hooks::Hooks;

/// Stores a pending command recipe along with its resolved template.
struct PendingCommand {
    recipe: Box<dyn CommandRecipe>,
    template: String,
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
    pub(crate) context_registry: ContextRegistry,
    pub(crate) template_dir: Option<PathBuf>,
    pub(crate) template_ext: String,
    /// Default command to use when no subcommand is specified
    pub(crate) default_command: Option<String>,
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
            pending_commands: RefCell::new(HashMap::new()),
            finalized_commands: RefCell::new(None),
            command_hooks: HashMap::new(),
            context_registry: ContextRegistry::new(),
            template_dir: None,
            template_ext: ".j2".to_string(),
            default_command: None,
        }
    }

    /// Ensures all pending commands have been finalized into dispatch functions.
    ///
    /// This method is called lazily on first dispatch. It creates the actual
    /// dispatch closures from the stored recipes, capturing the current theme
    /// and context registry. This deferred creation allows `.theme()` to be
    /// called after `.command()` without affecting the result.
    fn ensure_commands_finalized(&self) {
        // Already finalized?
        if self.finalized_commands.borrow().is_some() {
            return;
        }

        // Get the theme (use default if not set)
        let theme = self.theme.clone().unwrap_or_default();
        let context_registry = &self.context_registry;

        // Build dispatch functions from recipes
        let mut commands = HashMap::new();
        for (path, pending) in self.pending_commands.borrow().iter() {
            let dispatch =
                pending
                    .recipe
                    .create_dispatch(&pending.template, context_registry, &theme);
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
    pub fn parse(self, cmd: clap::Command) -> clap::ArgMatches {
        self.build().expect("Failed to build App").parse_with(cmd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_output_flag_enabled_by_default() {
        let outstanding = AppBuilder::new().build().unwrap();
        assert!(outstanding.output_flag.is_some());
        assert_eq!(outstanding.output_flag.as_deref(), Some("output"));
    }

    #[test]
    fn test_no_output_flag() {
        let outstanding = AppBuilder::new().no_output_flag().build().unwrap();
        assert!(outstanding.output_flag.is_none());
    }

    #[test]
    fn test_custom_output_flag_name() {
        let outstanding = AppBuilder::new()
            .output_flag(Some("format"))
            .build()
            .unwrap();
        assert_eq!(outstanding.output_flag.as_deref(), Some("format"));
    }
}
