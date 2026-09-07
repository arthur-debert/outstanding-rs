use super::*;

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
    let block = render_block_for_target(&theme, result.color_policy(), target, result.warnings());
    assert!(
        !block.contains("\x1b["),
        "a never color policy must keep the warning block plain, got {block:?}"
    );
    let styled = render_block_for_target(&theme, ColorPolicy::Always, target, result.warnings());
    assert!(
        styled.contains("\x1b["),
        "Auto on color-capable stderr should style warnings, got {styled:?}"
    );
}
