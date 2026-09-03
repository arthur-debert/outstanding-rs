use clap::Command;
use serial_test::serial;
use standout::assets::{HELP_TEMPLATE_NAME, TOPICS_LIST_TEMPLATE_NAME, TOPIC_TEMPLATE_NAME};
use standout::cli::{App, HelpResult};
use standout::ColorPolicy;
use standout::MiniJinjaEngine;
use standout_test::TestHarness;

fn help_command() -> Command {
    Command::new("app")
}

#[test]
fn build_registers_named_help_and_topic_templates() {
    let app = App::builder().help_handling(true).build().unwrap();
    let names: Vec<_> = app.template_names().collect();
    for name in [
        HELP_TEMPLATE_NAME,
        TOPIC_TEMPLATE_NAME,
        TOPICS_LIST_TEMPLATE_NAME,
    ] {
        assert!(names.contains(&name), "expected {name} in {names:?}");
    }
}

#[test]
fn app_override_of_named_help_template_is_used() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("standout");
    std::fs::create_dir(&nested).unwrap();
    std::fs::write(nested.join("help.jinja"), "CUSTOM HELP PAGE\n").unwrap();

    let app = App::builder()
        .help_handling(true)
        .templates_dir(dir.path())
        .unwrap()
        .build()
        .unwrap();

    match app.get_matches_from(
        help_command(),
        ["app", "--help"],
        &standout::InputSources::from_process(),
    ) {
        HelpResult::Help(text) | HelpResult::PagedHelp(text) => {
            assert!(
                text.contains("CUSTOM HELP PAGE"),
                "named override must win:\n{text}"
            );
        }
        other => panic!("expected rendered help, got {other:?}"),
    }
}

#[test]
#[serial]
fn help_path_uses_the_app_engine_from_build() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("standout");
    std::fs::create_dir(&nested).unwrap();
    std::fs::write(nested.join("help.jinja"), "{{ 'custom' | shout }}\n").unwrap();

    let mut engine = MiniJinjaEngine::new();
    engine
        .environment_mut()
        .add_filter("shout", |value: String| value.to_uppercase());

    let app = App::builder()
        .help_handling(true)
        .template_engine(Box::new(engine))
        .templates_dir(dir.path())
        .unwrap()
        .build()
        .unwrap();

    let result =
        TestHarness::new()
            .color(ColorPolicy::Never)
            .run(&app, help_command(), ["app", "--help"]);
    result.assert_success();
    assert!(
        result.stdout().contains("CUSTOM"),
        "help must render through the app engine's filters, got:\n{}",
        result.stdout()
    );
}

#[test]
fn unreadable_named_help_override_surfaces_as_render_error() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("standout");
    std::fs::create_dir(&nested).unwrap();
    let path = nested.join("help.jinja");
    std::fs::write(&path, "CUSTOM HELP PAGE\n").unwrap();

    let app = App::builder()
        .help_handling(true)
        .templates_dir(dir.path())
        .unwrap()
        .build()
        .unwrap();

    std::fs::remove_file(&path).unwrap();

    match app.get_matches_from(
        help_command(),
        ["app", "--help"],
        &standout::InputSources::from_process(),
    ) {
        HelpResult::Error(err) => {
            let msg = err.to_string();
            assert!(
                msg.contains("failed to render help"),
                "broken override must surface as a render failure, got:\n{msg}"
            );
            assert!(
                !msg.contains("CUSTOM HELP PAGE"),
                "must not render the deleted override:\n{msg}"
            );
        }
        other => panic!("expected render failure, got {other:?}"),
    }
}

#[test]
fn standalone_render_help_still_works_without_an_app() {
    use standout::cli::{render_help, HelpConfig};
    use standout::Representation;

    let output = render_help(
        &help_command().about("Demo"),
        Some(HelpConfig {
            output_mode: Some(Representation::Human),
            ..Default::default()
        }),
    )
    .unwrap();
    assert!(output.contains("USAGE"), "{output}");
    assert!(output.contains("Demo"), "{output}");
}
