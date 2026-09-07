use crate::cli::builder::{test_support::EXECUTION_TEMPLATES as TEMPLATES, AppBuilder};
use crate::cli::handler::{FnHandler, HandlerResult, Output as HandlerOutput};
use crate::EmbeddedTemplates;
use clap::Command;

#[test]
fn test_theme_ordering_command_before_theme() {
    use crate::Theme;
    use console::Style;
    use serde_json::json;

    let theme = Theme::new().add("late", Style::new().bold());

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"name": "test"})))),
            |cfg| cfg.template_name("list-10"),
        )
        .unwrap()
        .theme(theme); // Theme set AFTER command registration

    let cmd = Command::new("app").subcommand(Command::new("list"));
    let result = builder.build().unwrap().run_with(
        cmd,
        ["app", "list"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );

    assert!(result.is_handled());
    let output = result.output().unwrap();

    assert!(
        !output.contains("[late?]"),
        "ORDERING BUG: Theme set after .command() was not applied - output: {}",
        output
    );
}

#[test]
fn test_theme_passed_to_dispatch_closure() {
    use crate::Theme;
    use console::Style;
    use serde_json::json;

    let theme = Theme::new().add("test_style", Style::new().bold());

    let builder = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(theme)
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"name": "test"})))),
            |cfg| cfg.template_name("list-11"),
        )
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("list"));
    let result = builder.build().unwrap().run_with(
        cmd,
        ["app", "list"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );

    assert!(result.is_handled());
    let output = result.output().unwrap();

    assert!(
        !output.contains("[test_style?]"),
        "Theme was not passed to dispatch - output: {}",
        output
    );
}

#[test]
fn test_styles_and_default_theme_with_command() {
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();

    fs::write(
        temp_dir.path().join("dark.yaml"),
        r#"
header:
  fg: blue
  bold: true
"#,
    )
    .unwrap();

    let app = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .styles_dir(temp_dir.path())
        .unwrap()
        .default_theme("dark")
        .command_with(
            "list",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"title": "Results"})))),
            |cfg| cfg.template_name("list-12"),
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
    let output = result.output().unwrap();

    assert!(
        !output.contains("[header?]"),
        "ORDERING BUG: .styles() + .default_theme() not applied - output: {}",
        output
    );
}

#[test]
fn test_builder_ordering_theme_before_command() {
    use crate::Theme;
    use console::Style;
    use serde_json::json;

    let theme = Theme::new().add("mystyle", Style::new().bold());

    let app = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(theme)
        .command_with(
            "test",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"x": "value"})))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("test"));
    let result = app.run_with(
        cmd,
        ["app", "test"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );

    assert!(
        !result.output().unwrap().contains("[mystyle?]"),
        "theme -> command ordering failed"
    );
}

#[test]
fn test_builder_ordering_command_before_theme() {
    use crate::Theme;
    use console::Style;
    use serde_json::json;

    let theme = Theme::new().add("mystyle", Style::new().bold());

    let app = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "test",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"x": "value"})))),
            |cfg| cfg,
        )
        .unwrap()
        .theme(theme)
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("test"));
    let result = app.run_with(
        cmd,
        ["app", "test"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );

    assert!(
        !result.output().unwrap().contains("[mystyle?]"),
        "command -> theme ordering failed"
    );
}

#[test]
fn test_builder_ordering_styles_default_theme_command() {
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("mytheme.yaml"),
        "mystyle: { bold: true }",
    )
    .unwrap();

    let app = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .styles_dir(temp_dir.path())
        .unwrap()
        .default_theme("mytheme")
        .command_with(
            "test",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"x": "value"})))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("test"));
    let result = app.run_with(
        cmd,
        ["app", "test"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );

    assert!(
        !result.output().unwrap().contains("[mystyle?]"),
        "styles -> default_theme -> command ordering failed"
    );
}

#[test]
fn test_builder_ordering_command_before_styles() {
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("mytheme.yaml"),
        "mystyle: { bold: true }",
    )
    .unwrap();

    let app = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "test",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"x": "value"})))),
            |cfg| cfg,
        )
        .unwrap()
        .styles_dir(temp_dir.path())
        .unwrap()
        .default_theme("mytheme")
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("test"));
    let result = app.run_with(
        cmd,
        ["app", "test"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );

    assert!(
        !result.output().unwrap().contains("[mystyle?]"),
        "command -> styles -> default_theme ordering failed"
    );
}

#[test]
fn test_builder_ordering_default_theme_before_styles() {
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("mytheme.yaml"),
        "mystyle: { bold: true }",
    )
    .unwrap();

    let app = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .default_theme("mytheme")
        .styles_dir(temp_dir.path())
        .unwrap()
        .command_with(
            "test",
            FnHandler::new(|_m, _ctx| Ok(HandlerOutput::Render(json!({"x": "value"})))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("test"));
    let result = app.run_with(
        cmd,
        ["app", "test"],
        crate::TargetProperties::detect(),
        crate::InputSources::from_process(),
    );

    assert!(
        !result.output().unwrap().contains("[mystyle?]"),
        "default_theme -> styles -> command ordering failed"
    );
}

#[test]
fn test_builder_ordering_all_permutations_with_explicit_theme() {
    use crate::Theme;
    use console::Style;
    use serde_json::json;

    fn make_theme() -> Theme {
        Theme::new().add("perm", Style::new().italic())
    }

    fn make_handler() -> impl Fn(
        &clap::ArgMatches,
        &crate::cli::handler::CommandContext,
    ) -> HandlerResult<serde_json::Value> {
        |_m, _ctx| Ok(HandlerOutput::Render(json!({"val": "test"})))
    }

    let app1 = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(make_theme())
        .command_with("test", FnHandler::new(make_handler()), |cfg| {
            cfg.template_name("test-3")
        })
        .unwrap()
        .context("extra", crate::RenderData::from("x"))
        .build()
        .unwrap();

    let app2 = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with("test", FnHandler::new(make_handler()), |cfg| {
            cfg.template_name("test-3")
        })
        .unwrap()
        .theme(make_theme())
        .context("extra", crate::RenderData::from("x"))
        .build()
        .unwrap();

    let app3 = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .context("extra", crate::RenderData::from("x"))
        .command_with("test", FnHandler::new(make_handler()), |cfg| {
            cfg.template_name("test-3")
        })
        .unwrap()
        .theme(make_theme())
        .build()
        .unwrap();

    let app4 = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .context("extra", crate::RenderData::from("x"))
        .theme(make_theme())
        .command_with("test", FnHandler::new(make_handler()), |cfg| {
            cfg.template_name("test-3")
        })
        .unwrap()
        .build()
        .unwrap();

    let app5 = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with("test", FnHandler::new(make_handler()), |cfg| {
            cfg.template_name("test-3")
        })
        .unwrap()
        .context("extra", crate::RenderData::from("x"))
        .theme(make_theme())
        .build()
        .unwrap();

    let app6 = AppBuilder::new()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(make_theme())
        .context("extra", crate::RenderData::from("x"))
        .command_with("test", FnHandler::new(make_handler()), |cfg| {
            cfg.template_name("test-3")
        })
        .unwrap()
        .build()
        .unwrap();

    for (i, app) in [app1, app2, app3, app4, app5, app6].into_iter().enumerate() {
        let cmd = Command::new("app").subcommand(Command::new("test"));
        let result = app.run_with(
            cmd,
            ["app", "test"],
            crate::TargetProperties::detect(),
            crate::InputSources::from_process(),
        );

        assert!(
            !result.output().unwrap().contains("[perm?]"),
            "Permutation {} failed: style not found",
            i + 1
        );
    }
}
