use super::*;

#[test]
fn test_context_static_value() {
    use serde_json::json;

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .context("version", RenderData::from("1.0.0"))
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
        .context("author", RenderData::from("Alice"))
        .context("year", RenderData::from(2024))
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
            RenderData::from(ctx.terminal_width.unwrap_or(80))
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
            RenderData::from(format!("{:?}", ctx.representation))
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
        .context("value", RenderData::from("from_context"))
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
        .context("app_name", RenderData::from("MyApp"))
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
            RenderData::from(count * 2)
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
            RenderData::from_iter([
                ("debug", RenderData::from(true)),
                ("max_items", RenderData::from(100)),
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
        .context("separator", RenderData::from(" | "))
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
        .context("extra", RenderData::from("should_not_appear"))
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
