use clap::Command;
use serde_json::json;
use serial_test::serial;
use standout::cli::{App, Output, RunResult};
use standout::{EmbeddedSource, OutputMode, TemplateResource};
use standout_test::TestHarness;

static ORDERED_TEMPLATES: &[(&str, &str)] = &[("show.jinja", "Hello {{ name }}")];
static BAD_TEMPLATES: &[(&str, &str)] = &[("show.jinja", "{% if")];

fn command() -> Command {
    Command::new("app").subcommand(Command::new("show"))
}

fn build_error(builder: standout::cli::App) -> String {
    match builder.build() {
        Ok(_) => panic!("expected build to fail"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn build_fails_for_missing_named_template_with_near_match() {
    let error = build_error(
        App::builder()
            .templates(EmbeddedSource::<TemplateResource>::new(
                ORDERED_TEMPLATES,
                "/path/that/does/not/exist",
            ))
            .command_with(
                "show",
                |_m, _ctx| Ok(Output::Render(json!({"name": "Ada"}))),
                |cfg| cfg.template("shoe.jinja"),
            )
            .unwrap(),
    );

    assert!(error.contains("command `show` references template `shoe.jinja`"));
    assert!(
        error.contains("`.templates(...) or .templates_dir(...)`")
            || error.contains(".templates(...) or .templates_dir(...)")
    );
    assert!(error.contains("`show.jinja`"));
}

#[test]
fn templates_after_commands_resolve_at_build() {
    let app = App::builder()
        .commands(|g| {
            g.command_with(
                "show",
                |_m, _ctx| Ok(Output::Render(json!({"name": "Ada"}))),
                |cfg| cfg.template("show.jinja"),
            )
        })
        .unwrap()
        .templates(EmbeddedSource::<TemplateResource>::new(
            ORDERED_TEMPLATES,
            "/path/that/does/not/exist",
        ))
        .build()
        .unwrap();

    let result = TestHarness::new()
        .text_output()
        .run(&app, command(), ["app", "show"]);

    result.assert_success();
    assert_eq!(result.stdout(), "Hello Ada");
}

#[test]
fn build_fails_when_registered_template_does_not_compile() {
    let error = build_error(
        App::builder()
            .templates(EmbeddedSource::<TemplateResource>::new(
                BAD_TEMPLATES,
                "/path/that/does/not/exist",
            ))
            .command_with(
                "show",
                |_m, _ctx| Ok(Output::Render(json!({"name": "Ada"}))),
                |cfg| cfg.template("show.jinja"),
            )
            .unwrap(),
    );

    assert!(error.contains("template error"));
}

#[test]
#[serial]
fn templates_dir_hot_reloads_between_renders() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("show.jinja"), "Hello {{ name }}").unwrap();

    let app = App::builder()
        .templates_dir(dir.path())
        .unwrap()
        .command_with(
            "show",
            |_m, _ctx| Ok(Output::Render(json!({"name": "Ada"}))),
            |cfg| cfg.template_name("show"),
        )
        .unwrap()
        .build()
        .unwrap();

    let first = TestHarness::new()
        .text_output()
        .run(&app, command(), ["app", "show"]);
    first.assert_success();
    assert_eq!(first.stdout(), "Hello Ada");

    std::fs::write(dir.path().join("show.jinja"), "Bye {{ name }}").unwrap();

    let second = TestHarness::new()
        .text_output()
        .run(&app, command(), ["app", "show"]);
    second.assert_success();
    assert_eq!(second.stdout(), "Bye Ada");
}

#[test]
#[serial]
fn deleted_file_backed_template_errors_at_render() {
    let dir = tempfile::tempdir().unwrap();
    let template_path = dir.path().join("show.jinja");
    std::fs::write(&template_path, "Hello {{ name }}").unwrap();

    let app = App::builder()
        .templates_dir(dir.path())
        .unwrap()
        .command_with(
            "show",
            |_m, _ctx| Ok(Output::Render(json!({"name": "Ada"}))),
            |cfg| cfg.template_name("show"),
        )
        .unwrap()
        .build()
        .unwrap();

    std::fs::remove_file(&template_path).unwrap();

    let result = TestHarness::new()
        .text_output()
        .run(&app, command(), ["app", "show"]);
    result.assert_error_contains("template `show`");
    result.assert_error_contains(&template_path.display().to_string());
}

#[test]
#[serial]
fn corrupted_file_backed_template_errors_at_render() {
    let dir = tempfile::tempdir().unwrap();
    let template_path = dir.path().join("show.jinja");
    std::fs::write(&template_path, "Hello {{ name }}").unwrap();

    let app = App::builder()
        .templates_dir(dir.path())
        .unwrap()
        .command_with(
            "show",
            |_m, _ctx| Ok(Output::Render(json!({"name": "Ada"}))),
            |cfg| cfg.template_name("show"),
        )
        .unwrap()
        .build()
        .unwrap();

    std::fs::write(&template_path, "{% if").unwrap();

    let result = TestHarness::new()
        .text_output()
        .run(&app, command(), ["app", "show"]);
    result.assert_error_contains("template `show`");
    result.assert_error_contains(&template_path.display().to_string());
}

#[test]
fn structured_only_maps_machine_modes_and_rejects_human_modes() {
    let app = App::builder()
        .command_with(
            "show",
            |_m, _ctx| Ok(Output::Render(json!({"name": "Ada"}))),
            |cfg| cfg.structured_only(),
        )
        .unwrap()
        .build()
        .unwrap();

    for mode in [
        OutputMode::Auto,
        OutputMode::Json,
        OutputMode::Yaml,
        OutputMode::Xml,
        OutputMode::Csv,
    ] {
        let matches = command().try_get_matches_from(["app", "show"]).unwrap();
        let result = app.dispatch(matches, mode);
        assert!(
            matches!(result, RunResult::Handled(_)),
            "expected {mode:?} to serialize, got {result:?}"
        );
    }

    for mode in [OutputMode::Term, OutputMode::Text, OutputMode::TermDebug] {
        let matches = command().try_get_matches_from(["app", "show"]).unwrap();
        let result = app.dispatch(matches, mode);
        assert!(
            matches!(result, RunResult::Error(_)),
            "expected {mode:?} to reject structured-only output, got {result:?}"
        );
    }
}

#[test]
#[serial]
fn structured_only_omitted_output_serializes_json_through_run_to_string() {
    let app = App::builder()
        .command_with(
            "show",
            |_m, _ctx| Ok(Output::Render(json!({"name": "Ada"}))),
            |cfg| cfg.structured_only(),
        )
        .unwrap()
        .build()
        .unwrap();

    let result = TestHarness::new().run(&app, command(), ["app", "show"]);

    result.assert_success();
    let value: serde_json::Value = serde_json::from_str(result.stdout()).unwrap();
    assert_eq!(value["name"], "Ada");
}
