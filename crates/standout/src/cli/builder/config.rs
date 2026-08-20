//! Configuration methods for AppBuilder.
//!
//! This module contains methods for configuring the builder:
//! - Context injection (static and dynamic)
//! - Topics
//! - Themes and styles
//! - Templates
//! - Output flags
//! - Default command

use crate::context::ContextProvider;
use crate::setup::SetupError;
use crate::topics::Topic;
use crate::TemplateRegistry;
use crate::{EmbeddedStyles, EmbeddedTemplates, Theme};
use minijinja::Value;

use super::AppBuilder;

impl AppBuilder {
    /// Selects how East Asian Ambiguous characters occupy terminal columns.
    ///
    /// Narrow is the compatibility default. Standout does not infer this from
    /// locale settings.
    pub fn ambiguous_width(mut self, policy: crate::AmbiguousWidth) -> Self {
        self.ambiguous_width = policy;
        self
    }

    /// Declares the application's version, which clap answers `--version` with.
    ///
    /// The value is applied to the root command wherever standout augments and
    /// parses it, so every entry point — `run`, `run_to_string`,
    /// `dispatch_from`, `get_matches_from`, and the `TestHarness` — answers
    /// `<app> --version` alike: clap's own display, on stdout, exit status 0,
    /// typed as [`SuccessKind::ClapVersion`](crate::cli::SuccessKind::ClapVersion).
    ///
    /// Clap keeps owning the spelling, the formatting, and the short-circuit;
    /// this only says what the version *is*. Leaving it unset leaves the
    /// supplied `clap::Command` exactly as the application configured it,
    /// including a version set on clap directly.
    ///
    /// # Example
    ///
    /// ```rust
    /// use standout::cli::App;
    ///
    /// let app = App::builder()
    ///     .version(env!("CARGO_PKG_VERSION"))
    ///     .build()
    ///     .unwrap();
    /// ```
    pub fn version(mut self, version: impl Into<String>) -> Self {
        // Leaked once here, not per parse: clap's `Str` accepts an owned
        // `String` only under its `string` feature, and an application's
        // version is configured once and lives as long as the process.
        self.version = Some(Box::leak(version.into().into_boxed_str()));
        self
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
    /// use standout::cli::App;
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
    /// use standout::cli::App;
    /// use crate::context::RenderContext;
    /// use minijinja::Value;
    ///
    /// App::builder()
    ///     // Provide terminal info
    ///     .context_fn("terminal", |ctx: &RenderContext| {
    ///         Value::from_iter([
    ///             ("width", Value::from(ctx.terminal_width.unwrap_or(80))),
    ///             ("is_tty", Value::from(ctx.output_mode == standout::OutputMode::Term)),
    ///         ])
    ///     })
    ///
    ///     // Provide a table formatter with resolved width
    ///     .context_fn("table", |ctx: &RenderContext| {
    ///         let formatter = TabularFormatter::new(&spec, ctx.terminal_width.unwrap_or(80));
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
    ///
    /// # Errors
    /// Returns error if directory reading fails.
    pub fn topics_dir(mut self, path: impl AsRef<std::path::Path>) -> Result<Self, SetupError> {
        self.registry
            .add_from_directory(path)
            .map_err(SetupError::Io)?;
        Ok(self)
    }

    /// Sets the application theme, used for command output and help rendering.
    ///
    /// For help and topic rendering the theme is an overlay, not a
    /// replacement: a help tag it defines (`header`, `item`, …) takes its
    /// style, and every tag it leaves undefined keeps the default help
    /// styling — so a theme that only declares the app's own output
    /// vocabulary leaves help fully styled.
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
    /// Templates set here resolve command template names during `build()`, so
    /// call order with `.commands()` and `.group()` does not affect template
    /// lookup.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use standout::{embed_templates, cli::App};
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
        let warnings = standout_render::warnings::WarningBuffer::new();
        self.template_registry = Some(templates.into_registry(Some(&warnings)));
        self.startup_warnings.extend(warnings.take());
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
    /// use standout::cli::App;
    ///
    /// App::builder()
    ///     .styles(embed_styles!("src/styles"))
    ///     .default_theme("dark")
    ///     .command("list", handler, template)
    ///     .build()?
    ///     .run(cmd, args);
    /// ```
    pub fn styles(mut self, styles: EmbeddedStyles) -> Self {
        let warnings = standout_render::warnings::WarningBuffer::new();
        self.stylesheet_registry = Some(styles.into_registry(Some(&warnings)));
        self.startup_warnings.extend(warnings.take());
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
    pub fn styles_dir<P: AsRef<std::path::Path>>(mut self, path: P) -> Result<Self, SetupError> {
        let registry = self
            .stylesheet_registry
            .get_or_insert_with(crate::StylesheetRegistry::new);
        registry
            .add_dir(path)
            .map_err(|e| SetupError::Stylesheet(e.to_string()))?;
        Ok(self)
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

    /// Adds a template directory to the registry for runtime loading.
    ///
    /// Templates from directories are registered for runtime loading and
    /// merged with any embedded templates. File-backed entries are reread
    /// during named renders, so edits are visible inside the same debug
    /// process. Directory templates take precedence over embedded templates
    /// with the same name.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// App::builder()
    ///     .templates(embed_templates!("src/templates"))
    ///     .templates_dir("~/.myapp/templates")  // User overrides
    /// ```
    pub fn templates_dir<P: AsRef<std::path::Path>>(mut self, path: P) -> Result<Self, SetupError> {
        let registry = self
            .template_registry
            .get_or_insert_with(TemplateRegistry::new);
        registry.add_template_dir(path)?;
        registry.refresh()?;
        Ok(self)
    }

    /// Sets the file extension for convention-based template resolution.
    ///
    /// Default is `.j2`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// App::builder()
    ///     .templates_dir("templates")?
    ///     .template_ext(".jinja2")
    ///     .group("db", |g| g
    ///         .command("migrate", handler))  // resolves "db/migrate.jinja2"
    /// ```
    pub fn template_ext(mut self, ext: impl Into<String>) -> Self {
        self.template_ext = ext.into();
        self
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

    /// Sets a default command to use when no subcommand is specified.
    ///
    /// When a parse selects no subcommand (a "naked" invocation), the default
    /// command is inserted and the line is parsed again. This applies to both
    /// the integrated dispatch path (`run` / `dispatch_from` / `run_to_string`)
    /// and configured parsing (`parse_from` / `get_matches_from`).
    ///
    /// For a default that varies per invocation, see
    /// [`default_command_with`](Self::default_command_with); it is consulted
    /// first and falls back to this static name when it declines.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use standout::cli::App;
    ///
    /// // With this configuration:
    /// // - `myapp` becomes `myapp list`
    /// // - `myapp --verbose` becomes `myapp list --verbose`
    /// // - `myapp add foo` stays as `myapp add foo`
    ///
    /// App::builder()
    ///     .default_command("list")
    ///     .command("list", list_handler, "...")
    ///     .command("add", add_handler, "...")
    ///     .build()?
    ///     .run(cmd, args);
    /// ```
    pub fn default_command(mut self, name: &str) -> Self {
        self.default_command = Some(name.to_string());
        self
    }

    /// Chooses the default command per invocation instead of using one fixed name.
    ///
    /// The resolver runs only for a naked invocation — a parse that selected no
    /// subcommand — so explicit commands, nested commands, and root help or
    /// version have all had their say first and it can never override them.
    ///
    /// It receives a [`DefaultCommandContext`](crate::cli::DefaultCommandContext)
    /// exposing the root matches, read-only app state, and whether stdin is a
    /// terminal. Stdin is never read during resolution, so a handler's
    /// `InputChain` still consumes the pipe normally.
    ///
    /// Return `Some(name)` to select a command or `None` to decline, which falls
    /// back to [`default_command`](Self::default_command) if one is set. Both
    /// may be configured together.
    ///
    /// # Failure
    ///
    /// Returning a name that is not a command (or alias) of the `clap::Command`
    /// being parsed fails the run with
    /// [`RunErrorKind::DefaultCommand`](crate::cli::RunErrorKind::DefaultCommand)
    /// (a Clap error on the parse-only path) rather than panicking or letting a
    /// bogus name reach Clap as a usage error. Return `None` to decline.
    ///
    /// # Example
    ///
    /// A CLI that reads a piped payload but is interactive at a terminal:
    ///
    /// ```rust,ignore
    /// use standout::cli::App;
    ///
    /// // - `myapp` at a terminal becomes `myapp list`
    /// // - `cat notes.txt | myapp` becomes `myapp add` (which reads stdin)
    /// // - `myapp done 3` stays as `myapp done 3`
    ///
    /// App::builder()
    ///     .default_command_with(|ctx| {
    ///         Some(if ctx.stdin_is_piped() { "add" } else { "list" }.to_string())
    ///     })
    ///     .command("list", list_handler, "...")
    ///     .command("add", add_handler, "...")
    ///     .build()?
    ///     .run(cmd, args);
    /// ```
    pub fn default_command_with<F>(mut self, resolver: F) -> Self
    where
        F: Fn(&crate::cli::DefaultCommandContext<'_>) -> Option<String> + 'static,
    {
        self.default_command_resolver = Some(std::rc::Rc::new(resolver));
        self
    }

    /// Controls whether framework-supplied templates are included.
    ///
    /// Framework templates (in the `standout/` namespace) provide defaults for
    /// views like `standout/list-view`. They have the lowest priority and can
    /// be overridden by user templates with the same name.
    ///
    /// Default is `true`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use standout::cli::App;
    ///
    /// // Disable framework templates to require explicit configuration
    /// App::builder()
    ///     .include_framework_templates(false)
    ///     .build()?;
    /// ```
    pub fn include_framework_templates(mut self, include: bool) -> Self {
        self.include_framework_templates = include;
        self
    }

    /// Controls whether framework-supplied styles are included.
    ///
    /// Framework styles (prefixed with `standout-`) provide defaults like
    /// `standout-muted`, `standout-error`, etc. They can be overridden by
    /// user styles with the same name.
    ///
    /// Default is `true`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use standout::cli::App;
    ///
    /// // Disable framework styles to use only custom styles
    /// App::builder()
    ///     .include_framework_styles(false)
    ///     .build()?;
    /// ```
    pub fn include_framework_styles(mut self, include: bool) -> Self {
        self.include_framework_styles = include;
        self
    }

    /// Sets command groups for organized help display.
    ///
    /// When set, subcommands in help output are organized into the specified
    /// groups instead of a single "Commands" section. Commands not listed in
    /// any group are auto-appended to an "Other" group.
    ///
    /// Use [`validate_command_groups`](crate::cli::validate_command_groups) in
    /// a `#[test]` to catch typos and stale configs.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use standout::cli::{App, CommandGroup};
    ///
    /// App::builder()
    ///     .command_groups(vec![
    ///         CommandGroup {
    ///             title: "Commands".into(),
    ///             help: None,
    ///             commands: vec![Some("init".into()), Some("list".into())],
    ///         },
    ///         CommandGroup {
    ///             title: "Danger Zone".into(),
    ///             help: Some("These commands are destructive.".into()),
    ///             commands: vec![Some("delete".into()), Some("purge".into())],
    ///         },
    ///     ])
    ///     .build()?;
    /// ```
    pub fn command_groups(mut self, groups: Vec<super::super::help::CommandGroup>) -> Self {
        self.help_command_groups = Some(groups);
        self
    }

    /// Enables standout help handling.
    ///
    /// When enabled, standout intercepts all help invocations (`help`, `--help`,
    /// `-h`) and renders its own themed help instead of clap's default. This is
    /// required for `command_groups` and topics to work.
    ///
    /// Interception is a property of the configuration, not of the entry point:
    /// `dispatch_from` / `run` / `run_to_string` and `get_matches_from` /
    /// `parse_from` answer help identically, under the same install policy for
    /// the [`help` word](Self::help_word).
    ///
    /// Disabled by default — clap's built-in help is used unless you opt in.
    ///
    /// # Errors
    ///
    /// `build()` returns `SetupError::Config` if `command_groups` or topics are
    /// configured without enabling `help_handling`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// App::builder()
    ///     .help_handling(true)
    ///     .command_groups(vec![...])
    ///     .build()?;
    /// ```
    pub fn help_handling(mut self, enabled: bool) -> Self {
        self.help_handling = enabled;
        self
    }

    /// Opts a flat CLI with positionals into the bare `help` word.
    ///
    /// Standout installs `help` on its own for the shapes where the word cannot
    /// mean anything else: a CLI that has subcommands, and a flat CLI with no
    /// positionals. A flat CLI *with* positionals is the one shape only the
    /// application can decide — at the root of such a CLI a bare word is data
    /// (`echo help`, `grep help`), so reserving `help` out of that namespace is
    /// a domain judgement.
    ///
    /// Opting in accepts the cost: the literal word `help` can no longer reach
    /// the positional, and `--` becomes the escape for it (`myapp -- help`).
    /// Without the opt-in, `--help` / `-h` remain the only spelling and still
    /// render themed help.
    ///
    /// This only ever *adds* the word — `false` is the default policy above,
    /// not a way to suppress `help` on a CLI that has subcommands.
    ///
    /// # Errors
    ///
    /// `build()` returns `SetupError::Config` if this is set without
    /// [`help_handling`](Self::help_handling): the `help` word is standout's
    /// own subcommand, so there is nothing to install without interception.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // `mytool <RANGE>` — a revision range is never the word "help".
    /// App::builder()
    ///     .help_handling(true)
    ///     .help_word(true)
    ///     .build()?;
    /// ```
    pub fn help_word(mut self, enabled: bool) -> Self {
        self.help_word = enabled;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::handler::Output as HandlerOutput;
    use crate::context::RenderContext;
    use crate::OutputMode;
    use clap::Command;

    #[test]
    fn test_context_static_value() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .context("version", Value::from("1.0.0"))
            .command(
                "info",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"name": "app"}))),
                "{{ name }} v{{ version }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("info"));
        let matches = cmd.try_get_matches_from(["app", "info"]).unwrap();
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("app v1.0.0"));
    }

    #[test]
    fn test_context_multiple_static_values() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .context("author", Value::from("Alice"))
            .context("year", Value::from(2024))
            .command(
                "info",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"title": "Report"}))),
                "{{ title }} by {{ author }} ({{ year }})",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("info"));
        let matches = cmd.try_get_matches_from(["app", "info"]).unwrap();
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Report by Alice (2024)"));
    }

    #[test]
    fn test_context_fn_terminal_width() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .context_fn("terminal_width", |ctx: &RenderContext| {
                Value::from(ctx.terminal_width.unwrap_or(80))
            })
            .command(
                "info",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({}))),
                "Width: {{ terminal_width }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("info"));
        let matches = cmd.try_get_matches_from(["app", "info"]).unwrap();
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        // The width will be actual terminal width or 80 in tests
        let output = result.output().unwrap();
        assert!(output.starts_with("Width: "));
    }

    #[test]
    fn test_context_fn_output_mode() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .context_fn("mode", |ctx: &RenderContext| {
                Value::from(format!("{:?}", ctx.output_mode))
            })
            .command(
                "info",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({}))),
                "Mode: {{ mode }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("info"));
        let matches = cmd.try_get_matches_from(["app", "info"]).unwrap();
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Mode: Text"));
    }

    #[test]
    fn test_context_data_takes_precedence() {
        use serde_json::json;

        // Context has "value" but handler data also has "value"
        // Handler data should take precedence
        let builder = AppBuilder::new()
            .context("value", Value::from("from_context"))
            .command(
                "test",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"value": "from_data"}))),
                "{{ value }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("from_data"));
    }

    #[test]
    fn test_context_shared_across_commands() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .context("app_name", Value::from("MyApp"))
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({}))),
                "{{ app_name }}: list",
            )
            .unwrap()
            .command(
                "info",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({}))),
                "{{ app_name }}: info",
            )
            .unwrap();

        let cmd = Command::new("app")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("info"));
        let app = builder.build().unwrap();

        // Test "list" command
        let matches = cmd.clone().try_get_matches_from(["app", "list"]).unwrap();
        let result = app.dispatch(matches, OutputMode::Text);
        assert_eq!(result.output(), Some("MyApp: list"));

        // Test "info" command
        let matches = cmd.try_get_matches_from(["app", "info"]).unwrap();
        let result = app.dispatch(matches, OutputMode::Text);
        assert_eq!(result.output(), Some("MyApp: info"));
    }

    #[test]
    fn test_context_fn_uses_handler_data() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .context_fn("doubled_count", |ctx: &RenderContext| {
                let count = ctx.data.get("count").and_then(|v| v.as_i64()).unwrap_or(0);
                Value::from(count * 2)
            })
            .command(
                "test",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 21}))),
                "Count: {{ count }}, Doubled: {{ doubled_count }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Count: 21, Doubled: 42"));
    }

    #[test]
    fn test_context_with_nested_object() {
        use serde_json::json;

        let builder = AppBuilder::new()
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
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Debug: true, Max: 100"));
    }

    #[test]
    fn test_context_in_loop() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .context("separator", Value::from(" | "))
            .command(
                "list",
                |_m, _ctx| {
                    Ok(HandlerOutput::Render(json!({
                        "items": ["a", "b", "c"]
                    })))
                },
                "{% for item in items %}{{ item }}{% if not loop.last %}{{ separator }}{% endif %}{% endfor %}",
            ).unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Text);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("a | b | c"));
    }

    #[test]
    fn test_context_json_output_ignores_context() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .context("extra", Value::from("should_not_appear"))
            .command(
                "test",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"data": "value"}))),
                "{{ data }} + {{ extra }}",
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let result = builder.build().unwrap().dispatch(matches, OutputMode::Json);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        // JSON output should only contain handler data, not context
        assert!(output.contains("\"data\": \"value\""));
        assert!(!output.contains("extra"));
        assert!(!output.contains("should_not_appear"));
    }

    #[test]
    fn test_templates_dir_convention() {
        use serde_json::json;
        let temp_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp_dir.path().join("db")).unwrap();
        std::fs::write(temp_dir.path().join("db/migrate.jinja2"), "{{ ok }}").unwrap();

        let builder = AppBuilder::new()
            .templates_dir(temp_dir.path())
            .unwrap()
            .template_ext(".jinja2")
            .group("db", |g| {
                g.command("migrate", |_m, _ctx| {
                    Ok(HandlerOutput::Render(json!({"ok": true})))
                })
            });

        let app = builder.unwrap().build().unwrap();

        let cmd =
            Command::new("app").subcommand(Command::new("db").subcommand(Command::new("migrate")));
        let matches = cmd.try_get_matches_from(["app", "db", "migrate"]).unwrap();
        let result = app.dispatch(matches, OutputMode::Text);

        assert_eq!(result.output(), Some("true"));
    }

    /// A file path (not a directory) exists, so debug hot-reload attempts a
    /// walk and falls back to the embedded copy.
    fn hot_reload_fallback_templates() -> Option<crate::EmbeddedTemplates> {
        const CARGO_TOML: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        static ENTRIES: &[(&str, &str)] = &[("ok.jinja", "hi")];
        let source = crate::EmbeddedSource::<crate::TemplateResource>::new(ENTRIES, CARGO_TOML);
        source.should_hot_reload().then_some(source)
    }

    fn assert_hot_reload_walk_warning(warnings: &[String]) {
        assert!(
            warnings
                .iter()
                .any(|warning| warning.contains("Failed to walk templates directory")),
            "expected hot-reload fallback warning, got {warnings:?}"
        );
    }

    #[test]
    fn dispatch_returns_embedded_hot_reload_fallback_warnings() {
        use serde_json::json;

        let Some(source) = hot_reload_fallback_templates() else {
            return;
        };
        let app = AppBuilder::new()
            .templates(source)
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"n": 1}))),
                "n={{ n }}",
            )
            .unwrap()
            .build()
            .unwrap();
        let cmd = Command::new("app").subcommand(Command::new("list"));
        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = app.dispatch(matches, OutputMode::Text);
        assert!(result.is_handled());
        assert_hot_reload_walk_warning(result.warnings());
    }

    #[test]
    fn dispatch_from_returns_embedded_hot_reload_fallback_warnings() {
        use serde_json::json;

        let Some(source) = hot_reload_fallback_templates() else {
            return;
        };
        let app = AppBuilder::new()
            .templates(source)
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"n": 1}))),
                "n={{ n }}",
            )
            .unwrap()
            .build()
            .unwrap();
        let cmd = Command::new("app").subcommand(Command::new("list"));
        let result = app.dispatch_from(cmd, ["app", "list"]);
        assert!(result.is_handled());
        assert_hot_reload_walk_warning(result.warnings());
    }

    #[test]
    fn run_with_text_output_keeps_warning_block_plain_on_color_capable_stderr() {
        use crate::cli::CommandContextInput;
        use crate::{AmbiguousWidth, ColorMode, IconMode, InputSources, TargetProperties};
        use serde_json::json;
        use standout_render::warnings::render_block_for_target;

        let app = AppBuilder::new()
            .command(
                "list",
                |_m, ctx| {
                    ctx.warn("stylesheet fell back");
                    Ok(HandlerOutput::Render(json!({"n": 1})))
                },
                "n={{ n }}",
            )
            .unwrap()
            .build()
            .unwrap();
        let target = TargetProperties {
            width: Some(80),
            stdout_is_terminal: false,
            stderr_is_terminal: true,
            stdout_color_capability: false,
            stderr_color_capability: true,
            color_scheme: ColorMode::Dark,
            icon_mode: IconMode::Classic,
            ambiguous_width: AmbiguousWidth::Narrow,
        };
        let cmd = Command::new("app").subcommand(Command::new("list"));
        let result = app.run_with(
            cmd,
            ["app", "--output=text", "list"],
            target,
            InputSources::from_process(),
        );
        assert_eq!(result.output_mode(), OutputMode::Text);
        assert!(
            result
                .warnings()
                .iter()
                .any(|warning| warning.contains("stylesheet fell back")),
            "expected ctx.warn on the run result, got {:?}",
            result.warnings()
        );
        let theme = crate::Theme::default();
        let block =
            render_block_for_target(&theme, result.output_mode(), target, result.warnings());
        assert!(
            !block.contains("\x1b["),
            "--output=text must keep the warning block plain, got {block:?}"
        );
        let styled = render_block_for_target(&theme, OutputMode::Auto, target, result.warnings());
        assert!(
            styled.contains("\x1b["),
            "Auto on color-capable stderr should style warnings, got {styled:?}"
        );
    }

    fn color_capable_stderr_target() -> crate::TargetProperties {
        use crate::{AmbiguousWidth, ColorMode, IconMode, TargetProperties};
        TargetProperties {
            width: Some(80),
            stdout_is_terminal: false,
            stderr_is_terminal: true,
            stdout_color_capability: false,
            stderr_color_capability: true,
            color_scheme: ColorMode::Dark,
            icon_mode: IconMode::Classic,
            ambiguous_width: AmbiguousWidth::Narrow,
        }
    }

    #[test]
    fn clap_usage_error_honours_text_output_for_startup_warnings() {
        use crate::cli::handler::RunErrorKind;
        use crate::InputSources;
        use serde_json::json;
        use standout_render::warnings::render_block_for_target;

        let mut app = AppBuilder::new()
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({"n": 1}))),
                "n={{ n }}",
            )
            .unwrap()
            .build()
            .unwrap();
        app.startup_warnings
            .push("stylesheet fell back".to_string());
        let target = color_capable_stderr_target();
        let cmd = Command::new("app").subcommand(Command::new("list"));
        let result = app.run_with(
            cmd,
            ["app", "--output=text", "not-a-command"],
            target,
            InputSources::from_process(),
        );
        assert!(
            result.is_error(),
            "unknown command should be a clap usage error, got {:?}",
            result.outcome()
        );
        assert_eq!(result.error_kind(), Some(RunErrorKind::ClapUsage));
        assert_eq!(result.output_mode(), OutputMode::Text);
        assert!(
            result
                .warnings()
                .iter()
                .any(|warning| warning.contains("stylesheet fell back")),
            "expected startup warning on the clap-error result, got {:?}",
            result.warnings()
        );
        let theme = crate::Theme::default();
        let block =
            render_block_for_target(&theme, result.output_mode(), target, result.warnings());
        assert!(
            !block.contains("\x1b["),
            "clap usage with --output=text must keep warnings plain, got {block:?}"
        );
        let styled = render_block_for_target(&theme, OutputMode::Auto, target, result.warnings());
        assert!(
            styled.contains("\x1b["),
            "Auto on color-capable stderr should style warnings, got {styled:?}"
        );
    }

    #[test]
    fn clap_help_and_version_honour_text_output_flag_from_unparsed_line() {
        use crate::InputSources;

        let app = AppBuilder::new()
            .version("1.0.0")
            .command(
                "list",
                |_m, _ctx| Ok(HandlerOutput::Render(serde_json::json!({"n": 1}))),
                "n={{ n }}",
            )
            .unwrap()
            .build()
            .unwrap();
        let target = color_capable_stderr_target();
        let cmd = Command::new("app").subcommand(Command::new("list"));
        let help = app.run_with(
            cmd.clone(),
            ["app", "--help", "--output=text"],
            target,
            InputSources::from_process(),
        );
        assert_eq!(
            help.output_mode(),
            OutputMode::Text,
            "--output=text after --help must still opt warnings out of ANSI"
        );
        let version = app.run_with(
            cmd,
            ["app", "--output=text", "--version"],
            target,
            InputSources::from_process(),
        );
        assert_eq!(
            version.output_mode(),
            OutputMode::Text,
            "--output=text before --version must still opt warnings out of ANSI"
        );
    }
}
