//! The batch path of `docs/spec/typed-command-output.md`: one typed value, the
//! representation `--output` may name, and the values, bytes and delivery the
//! harness reports separately.

use clap::Command;
use serde::Serialize;
use serial_test::serial;
use standout::cli::{App, Delivery, ExitStatus, FnHandler, Output, RunErrorKind};
use standout::{ColorPolicy, EmbeddedTemplates, Representation};
use standout_test::TestHarness;

const TEMPLATES: &[(&str, &str)] = &[("list", "{{ count }} items for {{ owner }}")];

#[derive(Serialize)]
struct Listing {
    owner: String,
    count: usize,
}

fn app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "list",
            FnHandler::new(|_matches, _ctx| {
                Ok(Output::Render(Listing {
                    owner: "ada".to_string(),
                    count: 3,
                }))
            }),
            |config| config.template_name("list"),
        )
        .unwrap()
        .build()
        .unwrap()
}

fn command() -> Command {
    Command::new("app").subcommand(Command::new("list"))
}

#[test]
#[serial]
fn one_typed_value_renders_a_page_bare_and_serializes_under_output_json() {
    let human = TestHarness::new().run(&app(), command(), ["app", "list"]);
    human.assert_success();
    human.assert_stdout_eq("3 items for ada");

    let json = TestHarness::new().output_mode(Representation::Json).run(
        &app(),
        command(),
        ["app", "list"],
    );
    json.assert_success();
    let document: serde_json::Value = serde_json::from_str(json.stdout()).unwrap();
    assert_eq!(document["owner"], "ada");
    assert_eq!(document["count"], 3);
}

#[test]
#[serial]
fn the_result_is_the_same_value_whatever_representation_ran() {
    let expected = serde_json::json!({"owner": "ada", "count": 3});

    let human = TestHarness::new().run(&app(), command(), ["app", "list"]);
    assert_eq!(human.result(), Some(&expected));
    assert_eq!(human.stdout(), "3 items for ada");

    let json = TestHarness::new().output_mode(Representation::Json).run(
        &app(),
        command(),
        ["app", "list"],
    );
    assert_eq!(json.result(), Some(&expected));
    assert_ne!(json.stdout(), human.stdout());
}

#[test]
#[serial]
fn delivery_names_stdout_or_the_file_the_user_asked_for() {
    let to_stdout = TestHarness::new().run(&app(), command(), ["app", "list"]);
    assert_eq!(to_stdout.delivery(), &Delivery::Stdout);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.txt");
    let to_file = TestHarness::new().run(
        &app(),
        command(),
        [
            "app",
            "list",
            &format!("--output-file-path={}", path.display()),
        ],
    );
    to_file.assert_success();
    assert_eq!(to_file.delivery(), &Delivery::File(path.clone()));
    assert_eq!(to_file.stdout(), "");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "3 items for ada");
}

#[test]
#[serial]
fn the_output_flag_accepts_the_structured_encodings_and_the_style_tag_view() {
    for encoding in ["json", "yaml", "csv", "ndjson", "term-debug"] {
        let result = TestHarness::new().run(
            &app(),
            command(),
            ["app", "list", &format!("--output={encoding}")],
        );
        result.assert_success();
    }
}

#[test]
#[serial]
fn a_retired_output_value_is_a_usage_error_with_exit_two() {
    for retired in ["term", "text", "auto"] {
        let result = TestHarness::new().run(
            &app(),
            command(),
            ["app", "list", &format!("--output={retired}")],
        );
        result.assert_error_kind(RunErrorKind::ClapUsage);
        result.assert_exit_status(ExitStatus::USAGE_ERROR);
        assert!(
            result.error().unwrap().contains("invalid value"),
            "`{retired}` must read as an invalid value, got {:?}",
            result.error()
        );
    }
}

#[test]
#[serial]
fn the_color_policy_and_the_destination_are_separate_settings() {
    let piped = TestHarness::new().run(&app(), command(), ["app", "list"]);
    assert!(!piped.stdout().contains('\u{1b}'));

    let forced =
        TestHarness::new()
            .color(ColorPolicy::Always)
            .run(&app(), command(), ["app", "list"]);
    assert_eq!(forced.stdout_plain(), "3 items for ada");

    let terminal = TestHarness::new()
        .color_capable_terminal()
        .color(ColorPolicy::Never)
        .run(&app(), command(), ["app", "list"]);
    assert!(
        !terminal.stdout().contains('\u{1b}'),
        "a never policy refuses color on a terminal too, got {:?}",
        terminal.stdout()
    );
}
