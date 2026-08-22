use serde_json::json;
use standout::cli::hooks::Hooks;
use standout::cli::{App, Output};

fn build_error(result: Result<standout::cli::App, standout::SetupError>) -> String {
    match result {
        Ok(_) => panic!("expected build to fail"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn render_named_without_registry_names_the_builder_calls() {
    let app = App::builder()
        .include_framework_templates(false)
        .build()
        .unwrap();

    let error = app.render("missing", &json!({}), standout::OutputMode::Text);
    let message = error
        .expect_err("named render needs a template registry")
        .to_string();

    assert!(message.contains("render(\"missing\", ...) needs a template registry"));
    assert!(message.contains(".templates(embed_templates!"));
    assert!(message.contains(".templates_dir(\"path/to/templates\")"));
    assert!(message.contains("render_inline(...)"));
}

#[test]
fn render_named_structured_without_registry_serializes() {
    let app = App::builder()
        .include_framework_templates(false)
        .build()
        .unwrap();
    let data = json!({"name": "Ada"});

    let json = app
        .render("unused", &data, standout::OutputMode::Json)
        .expect("structured render does not need a registry");
    assert!(json.contains("\"name\": \"Ada\""));

    let yaml = app
        .render("unused", &data, standout::OutputMode::Yaml)
        .expect("structured render does not need a registry");
    assert!(yaml.contains("name: Ada"));

    let xml = app
        .render("unused", &data, standout::OutputMode::Xml)
        .expect("structured render does not need a registry");
    assert!(xml.contains("<name>Ada</name>"));

    let csv = app
        .render("unused", &data, standout::OutputMode::Csv)
        .expect("structured render does not need a registry");
    assert!(csv.contains("name"));
    assert!(csv.contains("Ada"));
}

#[test]
fn render_named_structured_with_unregistered_name_serializes() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("show.jinja"), "Hello {{ name }}").unwrap();
    let app = App::builder()
        .templates_dir(dir.path())
        .unwrap()
        .build()
        .unwrap();
    let data = json!({"name": "Ada"});

    for mode in [
        standout::OutputMode::Json,
        standout::OutputMode::Yaml,
        standout::OutputMode::Xml,
        standout::OutputMode::Csv,
    ] {
        app.render("not-registered", &data, mode)
            .unwrap_or_else(|error| {
                panic!("structured {mode:?} ignored the missing name: {error}")
            });
    }
}

#[test]
fn render_named_file_refresh_error_keeps_read_context() {
    let dir = tempfile::tempdir().unwrap();
    let template_path = dir.path().join("show.jinja");
    std::fs::write(&template_path, "Hello {{ name }}").unwrap();

    let app = App::builder()
        .templates_dir(dir.path())
        .unwrap()
        .build()
        .unwrap();
    std::fs::remove_file(&template_path).unwrap();

    let message = app
        .render("show", &json!({"name": "Ada"}), standout::OutputMode::Text)
        .expect_err("deleted template should fail during render refresh")
        .to_string();

    assert!(message.contains("could not refresh the registered template"));
    assert!(message.contains(&template_path.display().to_string()));
    assert!(
        !message.contains("could not find the named template"),
        "{message}"
    );
}

#[test]
fn hook_conflict_error_names_the_phase_and_single_registration_fix() {
    let result = App::builder()
        .command_with(
            "show",
            |_m, _ctx| Ok(Output::Render(json!({"name": "Ada"}))),
            |config| config.template("{{ name }}").pre_dispatch(|_, _| Ok(())),
        )
        .unwrap()
        .hooks("show", Hooks::new().pre_dispatch(|_, _| Ok(())))
        .build();

    let message = build_error(result);

    assert!(message.contains("command `show` registers pre-dispatch hooks"));
    assert!(message.contains("CommandConfig"));
    assert!(message.contains("AppBuilder::hooks"));
    assert!(message.contains("keep each (path, phase) in one registration path"));
}

#[test]
fn help_configuration_errors_name_the_required_call() {
    let result = App::builder()
        .command_groups(vec![standout::cli::help::CommandGroup {
            title: "Commands".into(),
            help: None,
            commands: vec![Some("show".into())],
        }])
        .build();

    let message = build_error(result);

    assert!(message.contains("command_groups requires .help_handling(true)"));
    assert!(message.contains("intercepting help"));
}

#[test]
fn duplicate_command_still_names_the_conflicting_command() {
    let result = App::builder()
        .command("show", |_m, _ctx| Ok(Output::Render(json!({}))), "ok")
        .unwrap()
        .command("show", |_m, _ctx| Ok(Output::Render(json!({}))), "ok");

    let message = match result {
        Ok(_) => panic!("duplicate command must fail"),
        Err(error) => error.to_string(),
    };
    assert!(message.contains("duplicate command: show"));
}

#[test]
fn registered_help_collision_names_the_setting_and_rename_fix() {
    let result = App::builder()
        .help_handling(true)
        .command("help", |_m, _ctx| Ok(Output::Render(json!({}))), "ok")
        .unwrap()
        .build();

    let message = build_error(result);
    assert!(message.contains("duplicate command: help"));
    assert!(message.contains(".help_handling(true)"));
    assert!(message.contains("Rename"));
}

#[test]
fn command_named_template_without_registry_names_the_fix() {
    let app = App::builder()
        .include_framework_templates(false)
        .command_with(
            "show",
            |_m, _ctx| Ok(Output::Render(json!({"name": "Ada"}))),
            |config| config.template_name("show"),
        )
        .unwrap()
        .build()
        .map_err(|error| error.to_string());

    let message = match app {
        Ok(_) => panic!("build catches missing registries before dispatch"),
        Err(message) => message,
    };
    assert!(message.contains("no application templates are configured"));
    assert!(message.contains(".templates(embed_templates!"));
    assert!(message.contains(".templates_dir(\"path/to/templates\")"));
}
