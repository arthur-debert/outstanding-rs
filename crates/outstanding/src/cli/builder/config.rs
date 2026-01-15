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
use crate::topics::Topic;
use crate::TemplateRegistry;
use crate::{EmbeddedStyles, EmbeddedTemplates, Theme};
use minijinja::Value;
use std::path::PathBuf;

use super::AppBuilder;

impl AppBuilder {
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
    /// use outstanding::cli::App;
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
    /// use outstanding::cli::App;
    /// use crate::context::RenderContext;
    /// use minijinja::Value;
    ///
    /// App::builder()
    ///     // Provide terminal info
    ///     .context_fn("terminal", |ctx: &RenderContext| {
    ///         Value::from_iter([
    ///             ("width", Value::from(ctx.terminal_width.unwrap_or(80))),
    ///             ("is_tty", Value::from(ctx.output_mode == outstanding::OutputMode::Term)),
    ///         ])
    ///     })
    ///
    ///     // Provide a table formatter with resolved width
    ///     .context_fn("table", |ctx: &RenderContext| {
    ///         let formatter = TableFormatter::new(&spec, ctx.terminal_width.unwrap_or(80));
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
    /// Silently ignores non-existent directories.
    pub fn topics_dir(mut self, path: impl AsRef<std::path::Path>) -> Self {
        let _ = self.registry.add_from_directory_if_exists(path);
        self
    }

    /// Sets a custom theme for help rendering.
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
    /// Templates set here will be used to resolve template paths when registering
    /// commands. Call this method *before* `.commands()` or `.group()` to ensure
    /// templates are available for resolution.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use outstanding::{embed_templates, cli::App};
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
        self.template_registry = Some(TemplateRegistry::from(templates));
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
    /// use outstanding::cli::App;
    ///
    /// App::builder()
    ///     .styles(embed_styles!("src/styles"))
    ///     .default_theme("dark")
    ///     .command("list", handler, template)
    ///     .build()?
    ///     .run(cmd, args);
    /// ```
    pub fn styles(mut self, styles: EmbeddedStyles) -> Self {
        self.stylesheet_registry = Some(crate::StylesheetRegistry::from(styles));
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
    pub fn styles_dir<P: AsRef<std::path::Path>>(mut self, path: P) -> Self {
        let registry = self
            .stylesheet_registry
            .get_or_insert_with(crate::StylesheetRegistry::new);
        let _ = registry.add_dir(path);
        self
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

    /// Sets the base directory for convention-based template resolution.
    ///
    /// When a command is registered without an explicit template, the template
    /// path is derived from the command path:
    /// - Command `db.migrate` → `{template_dir}/db/migrate{template_ext}`
    ///
    /// This is for file-based template loading at render time. For embedded
    /// templates, use `.templates()` instead.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// App::builder()
    ///     .template_dir("templates")
    ///     .group("db", |g| g
    ///         .command("migrate", handler))  // uses "templates/db/migrate.j2"
    /// ```
    pub fn template_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.template_dir = Some(path.into());
        self
    }

    /// Adds a template directory to the registry for runtime loading.
    ///
    /// Templates from directories are loaded immediately and merged with any
    /// embedded templates. Directory templates take precedence over embedded
    /// templates with the same name.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// App::builder()
    ///     .templates(embed_templates!("src/templates"))
    ///     .templates_dir("~/.myapp/templates")  // User overrides
    /// ```
    pub fn templates_dir<P: AsRef<std::path::Path>>(mut self, path: P) -> Self {
        let registry = self
            .template_registry
            .get_or_insert_with(TemplateRegistry::new);
        let _ = registry.add_template_dir(path);
        self
    }

    /// Sets the file extension for convention-based template resolution.
    ///
    /// Default is `.j2`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// App::builder()
    ///     .template_dir("templates")
    ///     .template_ext(".jinja2")
    ///     .group("db", |g| g
    ///         .command("migrate", handler))  // uses "templates/db/migrate.jinja2"
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
    /// When the CLI is invoked without a subcommand (a "naked" invocation),
    /// the default command is automatically inserted and the arguments are reparsed.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use outstanding::cli::App;
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
            );

        let cmd = Command::new("app").subcommand(Command::new("info"));
        let matches = cmd.try_get_matches_from(["app", "info"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

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
            );

        let cmd = Command::new("app").subcommand(Command::new("info"));
        let matches = cmd.try_get_matches_from(["app", "info"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

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
            );

        let cmd = Command::new("app").subcommand(Command::new("info"));
        let matches = cmd.try_get_matches_from(["app", "info"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

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
            );

        let cmd = Command::new("app").subcommand(Command::new("info"));
        let matches = cmd.try_get_matches_from(["app", "info"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

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
            );

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

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
            .command(
                "info",
                |_m, _ctx| Ok(HandlerOutput::Render(json!({}))),
                "{{ app_name }}: info",
            );

        let cmd = Command::new("app")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("info"));

        // Test "list" command
        let matches = cmd.clone().try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);
        assert_eq!(result.output(), Some("MyApp: list"));

        // Test "info" command
        let matches = cmd.try_get_matches_from(["app", "info"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);
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
            );

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

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
            );

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

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
            );

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Text);

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
            );

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let result = builder.dispatch(matches, OutputMode::Json);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        // JSON output should only contain handler data, not context
        assert!(output.contains("\"data\": \"value\""));
        assert!(!output.contains("extra"));
        assert!(!output.contains("should_not_appear"));
    }

    #[test]
    fn test_template_dir_convention() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .template_dir("templates")
            .template_ext(".jinja2")
            .group("db", |g| {
                // No explicit template - should resolve to "templates/db/migrate.jinja2"
                g.command("migrate", |_m, _ctx| {
                    Ok(HandlerOutput::Render(json!({"ok": true})))
                })
            });

        // Verify the builder has the commands registered
        assert!(builder.commands.contains_key("db.migrate"));
    }
}
