use crate::context::ContextProvider;
use crate::setup::SetupError;
use crate::topics::Topic;
use crate::TemplateRegistry;
use crate::{EmbeddedStyles, EmbeddedTemplates, Representation, Theme};
use minijinja::Value;

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

    pub fn context(mut self, name: impl Into<String>, value: Value) -> Self {
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

    /// Removes the flag that suppresses paging, leaving no way to turn a
    /// resolved pager off for one invocation.
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
}

#[cfg(test)]
mod tests {
    use super::super::OUTPUT_MODE_ARG;
    use super::*;
    use crate::EmbeddedTemplates;

    const TEMPLATES: &[(&str, &str)] = &[
        ("info", "{{ name }} v{{ version }}"),
        ("info-2", "{{ title }} by {{ author }} ({{ year }})"),
        ("info-3", "Width: {{ terminal_width }}"),
        ("info-4", "Mode: {{ mode }}"),
        ("test", "{{ value }}"),
        ("list", "{{ app_name }}: list"),
        ("info-5", "{{ app_name }}: info"),
        ("test-2", "Count: {{ count }}, Doubled: {{ doubled_count }}"),
        ("test-3", "Debug: {{ config.debug }}, Max: {{ config.max_items }}"),
        ("list-2", "{% for item in items %}{{ item }}{% if not loop.last %}{{ separator }}{% endif %}{% endfor %}"),
        ("test-4", "{{ data }} + {{ extra }}"),
        ("list-3", "n={{ n }}"),
        ("sibling", "n={{ n }}"),
    ];

    use crate::cli::handler::FnHandler;
    use crate::cli::handler::Output as HandlerOutput;
    use crate::context::RenderContext;
    use crate::{ColorPolicy, Representation};
    use clap::Command;

    #[test]
    fn test_context_static_value() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .context("version", Value::from("1.0.0"))
            .command_with(
                "info",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"name": "app"})))),
                |cfg| cfg,
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("info"));
        let matches = cmd.try_get_matches_from(["app", "info"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("app v1.0.0"));
    }

    #[test]
    fn test_context_multiple_static_values() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .context("author", Value::from("Alice"))
            .context("year", Value::from(2024))
            .command_with(
                "info",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"title": "Report"})))),
                |cfg| cfg.template_name("info-2"),
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("info"));
        let matches = cmd.try_get_matches_from(["app", "info"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Report by Alice (2024)"));
    }

    #[test]
    fn test_context_fn_terminal_width() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .context_fn("terminal_width", |ctx: &RenderContext| {
                Value::from(ctx.terminal_width.unwrap_or(80))
            })
            .command_with(
                "info",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({})))),
                |cfg| cfg.template_name("info-3"),
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("info"));
        let matches = cmd.try_get_matches_from(["app", "info"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        let output = result.output().unwrap();
        assert!(output.starts_with("Width: "));
    }

    #[test]
    fn test_context_fn_output_mode() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .context_fn("mode", |ctx: &RenderContext| {
                Value::from(format!("{:?}", ctx.representation))
            })
            .command_with(
                "info",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({})))),
                |cfg| cfg.template_name("info-4"),
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("info"));
        let matches = cmd.try_get_matches_from(["app", "info"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Mode: Human"));
    }

    #[test]
    fn test_context_data_takes_precedence() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .context("value", Value::from("from_context"))
            .command_with(
                "test",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"value": "from_data"})))),
                |cfg| cfg,
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("from_data"));
    }

    #[test]
    fn test_context_shared_across_commands() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .context("app_name", Value::from("MyApp"))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({})))),
                |cfg| cfg,
            )
            .unwrap()
            .command_with(
                "info",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({})))),
                |cfg| cfg.template_name("info-5"),
            )
            .unwrap();

        let cmd = Command::new("app")
            .subcommand(Command::new("list"))
            .subcommand(Command::new("info"));
        let app = builder.build().unwrap();

        let matches = cmd.clone().try_get_matches_from(["app", "list"]).unwrap();
        let result = app.dispatch(matches, Representation::Human);
        assert_eq!(result.output(), Some("MyApp: list"));

        let matches = cmd.try_get_matches_from(["app", "info"]).unwrap();
        let result = app.dispatch(matches, Representation::Human);
        assert_eq!(result.output(), Some("MyApp: info"));
    }

    #[test]
    fn test_context_fn_uses_handler_data() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .context_fn("doubled_count", |ctx: &RenderContext| {
                let count = ctx.data.get("count").and_then(|v| v.as_i64()).unwrap_or(0);
                Value::from(count * 2)
            })
            .command_with(
                "test",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"count": 21})))),
                |cfg| cfg.template_name("test-2"),
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Count: 21, Doubled: 42"));
    }

    #[test]
    fn test_context_with_nested_object() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .context(
                "config",
                Value::from_iter([
                    ("debug", Value::from(true)),
                    ("max_items", Value::from(100)),
                ]),
            )
            .command_with(
                "test",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({})))),
                |cfg| cfg.template_name("test-3"),
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("Debug: true, Max: 100"));
    }

    #[test]
    fn test_context_in_loop() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .context("separator", Value::from(" | "))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| {
                    Ok(HandlerOutput::Render(json!({
                        "items": ["a", "b", "c"]
                    })))
                }),
                |cfg| cfg.template_name("list-2"),
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("list"));
        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Human);

        assert!(result.is_handled());
        assert_eq!(result.output(), Some("a | b | c"));
    }

    #[test]
    fn test_context_json_output_ignores_context() {
        use serde_json::json;

        let builder = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .context("extra", Value::from("should_not_appear"))
            .command_with(
                "test",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"data": "value"})))),
                |cfg| cfg.template_name("test-4"),
            )
            .unwrap();

        let cmd = Command::new("app").subcommand(Command::new("test"));
        let matches = cmd.try_get_matches_from(["app", "test"]).unwrap();
        let result = builder
            .build()
            .unwrap()
            .dispatch(matches, Representation::Json);

        assert!(result.is_handled());
        let output = result.output().unwrap();
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
            .commands(|g| {
                g.group("db", |g| {
                    g.command("migrate", |_m, _ctx| {
                        Ok(HandlerOutput::Render(json!({"ok": true})))
                    })
                })
            });

        let app = builder.unwrap().build().unwrap();

        let cmd =
            Command::new("app").subcommand(Command::new("db").subcommand(Command::new("migrate")));
        let matches = cmd.try_get_matches_from(["app", "db", "migrate"]).unwrap();
        let result = app.dispatch(matches, Representation::Human);

        assert_eq!(result.output(), Some("true"));
    }

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
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"n": 1})))),
                |cfg| cfg.template_name("ok"),
            )
            .unwrap()
            .build()
            .unwrap();
        let cmd = Command::new("app").subcommand(Command::new("list"));
        let matches = cmd.try_get_matches_from(["app", "list"]).unwrap();
        let result = app.dispatch(matches, Representation::Human);
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
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"n": 1})))),
                |cfg| cfg.template_name("ok"),
            )
            .unwrap()
            .build()
            .unwrap();
        let cmd = Command::new("app").subcommand(Command::new("list"));
        let result = app.run_with(
            cmd,
            ["app", "list"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );
        assert!(result.is_handled());
        assert_hot_reload_walk_warning(result.warnings());
    }

    #[test]
    fn a_never_color_policy_keeps_the_warning_block_plain_on_color_capable_stderr() {
        use crate::cli::CommandContextInput;
        use crate::{AmbiguousWidth, ColorMode, IconMode, InputSources, TargetProperties};
        use serde_json::json;
        use standout_render::warnings::render_block_for_target;

        let app = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, ctx| {
                    ctx.warn("stylesheet fell back");
                    Ok(HandlerOutput::Render(json!({"n": 1})))
                }),
                |cfg| cfg.template_name("list-3"),
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
        let result = app.run_with_sink(
            cmd,
            ["app", "list"],
            target,
            ColorPolicy::Never,
            InputSources::from_process(),
            crate::cli::StreamSink::new(Vec::new()),
        );
        assert_eq!(result.output_mode(), Representation::Human);
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
            render_block_for_target(&theme, result.color_policy(), target, result.warnings());
        assert!(
            !block.contains("\x1b["),
            "a never color policy must keep the warning block plain, got {block:?}"
        );
        let styled =
            render_block_for_target(&theme, ColorPolicy::Always, target, result.warnings());
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

    fn os_args(args: &[&str]) -> Vec<std::ffi::OsString> {
        args.iter().map(Into::into).collect()
    }

    #[test]
    fn unparsed_output_mode_reads_equals_and_space_forms() {
        let app = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .build()
            .unwrap();
        assert_eq!(
            app.extract_output_mode_from_unparsed(&os_args(&["app", "--output=json"])),
            Representation::Json
        );
        assert_eq!(
            app.extract_output_mode_from_unparsed(&os_args(&["app", "--output", "json"])),
            Representation::Json
        );
    }

    #[test]
    fn unparsed_output_mode_stops_at_terminator_and_falls_back_on_bad_values() {
        let app = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .build()
            .unwrap();
        assert_eq!(
            app.extract_output_mode_from_unparsed(&os_args(&["app", "--", "--output=csv"])),
            Representation::Human,
            "arguments after -- are not flags"
        );
        assert_eq!(
            app.extract_output_mode_from_unparsed(&os_args(&[
                "app",
                "--output=csv",
                "--",
                "--output=json"
            ])),
            Representation::Csv,
            "a flag before -- still counts; one after it does not"
        );
        assert_eq!(
            app.extract_output_mode_from_unparsed(&os_args(&["app", "--output=nope"])),
            Representation::Human,
            "unknown value"
        );
        assert_eq!(
            app.extract_output_mode_from_unparsed(&os_args(&["app", "--output"])),
            Representation::Human,
            "missing value"
        );
        assert_eq!(
            app.extract_output_mode_from_unparsed(&os_args(&["app", "--output", "--output=csv"])),
            Representation::Csv,
            "standalone --output must not consume a following --output=csv"
        );
        assert_eq!(
            app.extract_output_mode_from_unparsed(&os_args(&[
                "app", "--output", "--output", "csv"
            ])),
            Representation::Csv,
            "standalone --output must not consume a following --output value"
        );
    }

    #[test]
    fn unparsed_output_mode_skips_argv0() {
        let app = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .build()
            .unwrap();
        assert_eq!(
            app.extract_output_mode_from_unparsed(&os_args(&["--output=csv"])),
            Representation::Human,
            "argv[0] is the program name, even when it looks like a flag"
        );
        assert_eq!(
            app.extract_output_mode_from_unparsed(&os_args(&["--output=json", "--output=csv"])),
            Representation::Csv,
            "a flag-like program name must not count as an occurrence"
        );
        assert_eq!(
            app.extract_output_mode_from_unparsed(&os_args(&["--", "--output=csv"])),
            Representation::Csv,
            "a -- program name must not terminate the scan"
        );
    }

    #[test]
    fn unparsed_output_mode_is_auto_when_the_app_has_no_output_flag() {
        let app = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .no_output_flag()
            .build()
            .unwrap();
        assert_eq!(
            app.extract_output_mode_from_unparsed(&os_args(&["app", "--output=csv"])),
            Representation::Human
        );
    }

    #[test]
    fn the_output_flag_default_spells_the_app_fallback() {
        let default_values = |fallback| {
            let app = AppBuilder::new()
                .templates(EmbeddedTemplates::new(TEMPLATES, ""))
                .output_mode_fallback(fallback)
                .build()
                .unwrap();
            let augmented = app.augment_framework_surface(Command::new("app"));
            let defaults = augmented
                .get_arguments()
                .find(|arg| arg.get_id() == OUTPUT_MODE_ARG)
                .expect("the output flag is declared")
                .get_default_values()
                .to_vec();
            defaults
        };
        assert!(
            default_values(Representation::Human).is_empty(),
            "the human representation has no --output spelling to advertise"
        );
        assert_eq!(
            default_values(Representation::Csv),
            ["csv"],
            "the help page must advertise the encoding the app actually falls back to"
        );
    }

    #[test]
    fn an_unusable_output_value_falls_back_to_the_app_fallback() {
        let app = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .output_mode_fallback(Representation::Human)
            .build()
            .unwrap();
        assert_eq!(
            app.extract_output_mode_from_unparsed(&os_args(&["app", "--output=nope"])),
            Representation::Human
        );
        assert_eq!(
            app.extract_output_mode_from_unparsed(&os_args(&["app", "--output"])),
            Representation::Human
        );
    }

    #[test]
    fn a_setup_validation_error_honours_the_app_fallback() {
        use crate::cli::handler::RunErrorKind;
        use crate::InputSources;
        use serde_json::json;

        let app = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .output_mode_fallback(Representation::Human)
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"n": 1})))),
                |cfg| cfg.template_name("list-3"),
            )
            .unwrap()
            .build()
            .unwrap();
        let result = app.run_with(
            Command::new("app"),
            ["app", "list"],
            color_capable_stderr_target(),
            InputSources::from_process(),
        );
        assert_eq!(result.error_kind(), Some(RunErrorKind::ClapUsage));
        assert_eq!(result.output_mode(), Representation::Human);
    }

    #[test]
    fn clap_usage_error_carries_the_output_flag_and_the_startup_warnings() {
        use crate::cli::handler::RunErrorKind;
        use crate::InputSources;
        use serde_json::json;
        use standout_render::warnings::render_block_for_target;

        let mut app = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"n": 1})))),
                |cfg| cfg.template_name("list-3"),
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
            ["app", "--output=json", "not-a-command"],
            target,
            InputSources::from_process(),
        );
        assert!(
            result.is_error(),
            "unknown command should be a clap usage error, got {:?}",
            result.outcome()
        );
        assert_eq!(result.error_kind(), Some(RunErrorKind::ClapUsage));
        assert_eq!(result.output_mode(), Representation::Json);
        assert!(
            result
                .warnings()
                .iter()
                .any(|warning| warning.contains("stylesheet fell back")),
            "expected startup warning on the clap-error result, got {:?}",
            result.warnings()
        );
        let theme = crate::Theme::default();
        let block = render_block_for_target(&theme, ColorPolicy::Never, target, result.warnings());
        assert!(
            !block.contains("\x1b["),
            "a never color policy must keep warnings plain, got {block:?}"
        );
        let styled =
            render_block_for_target(&theme, ColorPolicy::Always, target, result.warnings());
        assert!(
            styled.contains("\x1b["),
            "Auto on color-capable stderr should style warnings, got {styled:?}"
        );
    }

    #[test]
    fn clap_help_and_version_honour_the_output_flag_from_the_unparsed_line() {
        use crate::InputSources;

        let app = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .version("1.0.0")
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(serde_json::json!({"n": 1})))),
                |cfg| cfg.template_name("list-3"),
            )
            .unwrap()
            .build()
            .unwrap();
        let target = color_capable_stderr_target();
        let cmd = Command::new("app").subcommand(Command::new("list"));
        let help = app.run_with(
            cmd.clone(),
            ["app", "--help", "--output=json"],
            target,
            InputSources::from_process(),
        );
        assert_eq!(
            help.output_mode(),
            Representation::Json,
            "--output after --help must still reach the run"
        );
        let version = app.run_with(
            cmd,
            ["app", "--output=json", "--version"],
            target,
            InputSources::from_process(),
        );
        assert_eq!(
            version.output_mode(),
            Representation::Json,
            "--output before --version must still reach the run"
        );
    }

    #[test]
    fn unparsed_output_mode_skips_help_and_version_spellings_the_command_already_declares() {
        use crate::InputSources;
        use clap::{Arg, ArgAction};
        use serde_json::json;

        let app = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"n": 1})))),
                |cfg| cfg.template_name("list-3"),
            )
            .unwrap()
            .build()
            .unwrap();
        let target = color_capable_stderr_target();
        let cases: [(&str, Command); 6] = [
            (
                "root --help",
                Command::new("app")
                    .disable_help_flag(true)
                    .arg(
                        Arg::new("manual_help")
                            .long("help")
                            .action(ArgAction::SetTrue),
                    )
                    .subcommand(Command::new("list")),
            ),
            (
                "root -h",
                Command::new("app")
                    .disable_help_flag(true)
                    .arg(Arg::new("manual_h").short('h').action(ArgAction::SetTrue))
                    .subcommand(Command::new("list")),
            ),
            (
                "root --version",
                Command::new("app")
                    .disable_version_flag(true)
                    .arg(
                        Arg::new("manual_version")
                            .long("version")
                            .action(ArgAction::SetTrue),
                    )
                    .subcommand(Command::new("list")),
            ),
            (
                "subcommand --help",
                Command::new("app").subcommand(
                    Command::new("list").disable_help_flag(true).arg(
                        Arg::new("manual_help")
                            .long("help")
                            .action(ArgAction::SetTrue),
                    ),
                ),
            ),
            (
                "subcommand -h",
                Command::new("app").subcommand(
                    Command::new("list")
                        .disable_help_flag(true)
                        .arg(Arg::new("manual_h").short('h').action(ArgAction::SetTrue)),
                ),
            ),
            (
                "subcommand --version",
                Command::new("app").subcommand(
                    Command::new("list").disable_version_flag(true).arg(
                        Arg::new("manual_version")
                            .long("version")
                            .action(ArgAction::SetTrue),
                    ),
                ),
            ),
        ];
        for (label, cmd) in cases {
            let result = app.run_with(
                cmd,
                ["app", "--output=json", "not-a-command"],
                target,
                InputSources::from_process(),
            );
            assert!(
                result.is_error(),
                "{label}: expected a clap usage error, got {:?}",
                result.outcome()
            );
            assert_eq!(
                result.output_mode(),
                Representation::Json,
                "{label}: custom help/version spellings must not drop --output=json"
            );
        }
    }

    #[test]
    fn unparsed_output_mode_honours_text_output_on_a_sibling_when_another_branch_owns_the_spelling()
    {
        use crate::InputSources;
        use clap::{Arg, ArgAction};
        use serde_json::json;

        let app = AppBuilder::new()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "list",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"n": 1})))),
                |cfg| cfg.template_name("list-3"),
            )
            .unwrap()
            .command_with(
                "sibling",
                FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"n": 1})))),
                |cfg| cfg,
            )
            .unwrap()
            .build()
            .unwrap();
        let target = color_capable_stderr_target();

        let help_cmd = Command::new("app")
            .subcommand(
                Command::new("list").disable_help_flag(true).arg(
                    Arg::new("manual_help")
                        .long("help")
                        .action(ArgAction::SetTrue),
                ),
            )
            .subcommand(Command::new("sibling"));
        let help = app.run_with(
            help_cmd,
            ["app", "sibling", "--help", "--output=json"],
            target,
            InputSources::from_process(),
        );
        assert_eq!(
            help.output_mode(),
            Representation::Json,
            "sibling --help --output=json must keep Json when list owns --help"
        );

        let short_cmd = Command::new("app")
            .subcommand(
                Command::new("list")
                    .disable_help_flag(true)
                    .arg(Arg::new("manual_h").short('h').action(ArgAction::SetTrue)),
            )
            .subcommand(Command::new("sibling"));
        let short = app.run_with(
            short_cmd,
            ["app", "sibling", "-h", "--output=json"],
            target,
            InputSources::from_process(),
        );
        assert_eq!(
            short.output_mode(),
            Representation::Json,
            "sibling -h --output=json must keep Json when list owns -h"
        );

        let version_cmd = Command::new("app")
            .version("1.0.0")
            .propagate_version(true)
            .subcommand(
                Command::new("list").disable_version_flag(true).arg(
                    Arg::new("manual_version")
                        .long("version")
                        .action(ArgAction::SetTrue),
                ),
            )
            .subcommand(Command::new("sibling"));
        let version = app.run_with(
            version_cmd,
            ["app", "sibling", "--version", "--output=json"],
            target,
            InputSources::from_process(),
        );
        assert_eq!(
            version.output_mode(),
            Representation::Json,
            "sibling --version --output=json must keep Json when list owns --version"
        );
    }
}
