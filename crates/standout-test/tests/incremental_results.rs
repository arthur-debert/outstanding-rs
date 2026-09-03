//! An incremental command's values as data: the ordered events and the summary
//! `result()` returns, whatever representation the run selected, beside the
//! bytes `stdout()` and `stderr()` carry and the `delivery()` decision.

use clap::Command;
use serde::Serialize;
use serde_json::json;
use standout::cli::{
    App, Diagnostic, EventsFnHandler, ExitStatus, HandlerResult, Output, Results, RunErrorKind,
};
use standout::ColorPolicy;
use standout::{EmbeddedTemplates, Representation};
use standout_test::TestHarness;

const EVENT_TEMPLATE: &str = concat!(
    r#"{% if event.type == "apply_start" %}starting {{ event.resource }}"#,
    r#"{% else %}done {{ event.resource }}{% endif %}"#,
);

const TEMPLATES: &[(&str, &str)] = &[
    ("apply", "{{ add }} added, {{ remove }} removed"),
    ("apply.event", EVENT_TEMPLATE),
    ("refuse", "{{ add }} added, {{ remove }} removed"),
    ("refuse.event", EVENT_TEMPLATE),
];

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Event<'a> {
    ApplyStart { resource: &'a str },
    ApplyComplete { resource: &'a str },
}

fn command() -> Command {
    Command::new("app")
        .subcommand(Command::new("apply"))
        .subcommand(Command::new("refuse"))
}

fn app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "apply",
            EventsFnHandler::new(
                |_, _ctx, results: &mut Results<Event>| -> HandlerResult<serde_json::Value> {
                    for resource in ["web", "db"] {
                        results.emit(Event::ApplyStart { resource })?;
                        results.emit(Event::ApplyComplete { resource })?;
                    }
                    Ok(Output::Render(json!({ "add": 2, "remove": 0 })))
                },
            ),
            |cfg| cfg,
        )
        .unwrap()
        .command_with(
            "refuse",
            EventsFnHandler::new(
                |_, _ctx, results: &mut Results<Event>| -> HandlerResult<serde_json::Value> {
                    results.emit(Event::ApplyStart { resource: "web" })?;
                    results.emit(Event::ApplyComplete { resource: "web" })?;
                    results.emit(Event::ApplyStart { resource: "db" })?;
                    Err(Diagnostic::error("db: refused")
                        .detail("a resource may refuse an apply")
                        .into())
                },
            ),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap()
}

fn expected_values() -> Vec<serde_json::Value> {
    vec![
        json!({ "type": "apply_start", "resource": "web" }),
        json!({ "type": "apply_complete", "resource": "web" }),
        json!({ "type": "apply_start", "resource": "db" }),
        json!({ "type": "apply_complete", "resource": "db" }),
        json!({ "add": 2, "remove": 0 }),
    ]
}

#[test]
fn the_ordered_events_and_the_summary_are_the_same_values_under_either_representation() {
    let human =
        TestHarness::new()
            .color(ColorPolicy::Never)
            .run(&app(), command(), ["app", "apply"]);
    human.assert_success();
    assert_eq!(human.results(), expected_values());
    assert_eq!(human.result(), Some(&json!({ "add": 2, "remove": 0 })));

    let stream = TestHarness::new().output_mode(Representation::Ndjson).run(
        &app(),
        command(),
        ["app", "apply"],
    );
    stream.assert_success();
    assert_eq!(stream.results(), human.results());
    assert_eq!(stream.result(), human.result());

    // The same values; the bytes are what the representation decides.
    assert_eq!(
        human.stdout(),
        "starting web\ndone web\nstarting db\ndone db\n2 added, 0 removed"
    );
    assert_eq!(stream.stdout().lines().count(), 5, "{}", stream.stdout());
    human.assert_stderr_empty();
    stream.assert_stderr_empty();
}

#[test]
fn a_failure_after_events_keeps_them_and_reports_the_diagnostic_beside_them() {
    let human =
        TestHarness::new()
            .color(ColorPolicy::Never)
            .run(&app(), command(), ["app", "refuse"]);
    human.assert_error_kind(RunErrorKind::Handler);
    human.assert_exit_status(ExitStatus::FAILURE);
    assert_eq!(human.stdout(), "starting web\ndone web\nstarting db\n");
    human.assert_stderr_contains("db: refused");

    let stream = TestHarness::new().output_mode(Representation::Ndjson).run(
        &app(),
        command(),
        ["app", "refuse"],
    );
    stream.assert_error_kind(RunErrorKind::Handler);
    stream.assert_stderr_empty();
    let entries: Vec<serde_json::Value> = stream
        .stdout()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(entries.len(), 4, "{}", stream.stdout());
    assert_eq!(entries[3]["type"], "diagnostic");
    assert_eq!(entries[3]["summary"], "db: refused");

    assert_eq!(stream.results(), human.results());
    assert_eq!(human.results().len(), 3, "the summary never existed");
}

#[test]
fn a_named_output_file_takes_the_whole_run_and_stdout_stays_empty() {
    for representation in [Representation::Human, Representation::Ndjson] {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("run.out");
        let result = TestHarness::new()
            .output_mode(representation)
            .color(ColorPolicy::Never)
            .run(
                &app(),
                command(),
                [
                    "app".to_string(),
                    "apply".to_string(),
                    format!("--output-file-path={}", path.display()),
                ],
            );
        result.assert_success();
        assert_eq!(result.stdout_bytes(), b"", "{representation:?}");
        assert_eq!(result.delivery().path(), Some(path.as_path()));
        assert_eq!(result.results(), expected_values(), "{representation:?}");
        let file = std::fs::read_to_string(&path).unwrap();
        assert_eq!(file.lines().count(), 5, "{representation:?}: {file}");
        assert!(
            file.starts_with("starting web\n") || file.starts_with("{\""),
            "{file}"
        );
    }
}
