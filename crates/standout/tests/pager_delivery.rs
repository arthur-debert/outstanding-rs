//! Which runs the framework decides to page, read back off `delivery()`.
//!
//! The decision is taken in dispatch and carried out at the process edge, so
//! every case here is settled without a terminal and without starting a pager.

use clap::{ArgMatches, Command};
use serde::Serialize;
use serde_json::json;
use serial_test::serial;
use standout::cli::hooks::TextOutput;
use standout::cli::RunErrorKind;
use standout::cli::{
    App, CommandContext, Delivery, EventsFnHandler, FnHandler, Output, RenderedOutput, Results,
    Summary, SummaryResult,
};
use standout::{ColorPolicy, EmbeddedTemplates, InputSources, Representation, TargetProperties};
use standout_test::{ScopedEnv, TestHarness, TestResult};

const TEMPLATES: &[(&str, &str)] = &[
    ("log", "{{ entries }} entries"),
    ("apply", "{{ done }} applied"),
    ("apply.event", "applying {{ event.resource }}"),
    ("status", "clean"),
];

const PAGER: &str = "sed -n 1p";

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Event<'a> {
    Applying { resource: &'a str },
}

fn command() -> Command {
    Command::new("myapp")
        .subcommand(Command::new("log"))
        .subcommand(Command::new("apply"))
        .subcommand(Command::new("status"))
}

fn app() -> App {
    App::builder()
        .name("myapp")
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "log",
            FnHandler::new(|_: &ArgMatches, _: &CommandContext| {
                Ok(Output::Render(json!({ "entries": 3 })))
            }),
            |cfg| cfg.pageable(),
        )
        .unwrap()
        .command_with(
            "apply",
            EventsFnHandler::new(
                |_: &ArgMatches,
                 _: &CommandContext,
                 results: &mut Results<Event>|
                 -> SummaryResult<serde_json::Value> {
                    results.emit(Event::Applying { resource: "web" })?;
                    Ok(Summary::Render(json!({ "done": 1 })))
                },
            ),
            |cfg| cfg.pageable(),
        )
        .unwrap()
        .command_with(
            "status",
            FnHandler::new(|_: &ArgMatches, _: &CommandContext| Ok(Output::Render(json!({})))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap()
}

/// `MYAPP` is the name the application gave the builder, not clap's.
fn on_a_terminal() -> TestHarness {
    TestHarness::new()
        .stdout_is_terminal(true)
        .env("MYAPP_PAGER", PAGER)
        .env_remove("PAGER")
}

fn run<const N: usize>(harness: TestHarness, args: [&str; N]) -> TestResult {
    harness.run(&app(), command(), args)
}

#[test]
#[serial]
fn an_eligible_batch_page_on_a_terminal_goes_to_the_pager() {
    let result = run(on_a_terminal(), ["myapp", "log"]);

    result.assert_success();
    assert_eq!(result.delivery(), &Delivery::Pager(PAGER.to_string()));
    assert_eq!(result.stdout(), "3 entries");
}

#[test]
#[serial]
fn an_undeclared_command_pages_nothing() {
    let result = run(on_a_terminal(), ["myapp", "status"]);

    result.assert_success();
    assert_eq!(result.delivery(), &Delivery::Stdout);
}

#[test]
#[serial]
fn an_incremental_command_pages_nothing() {
    let result = run(on_a_terminal(), ["myapp", "apply"]);

    result.assert_success();
    assert_eq!(result.delivery(), &Delivery::Stdout);
    result.assert_stdout_contains("applying web");
}

#[test]
#[serial]
fn a_structured_encoding_pages_nothing() {
    for encoding in [
        Representation::Json,
        Representation::Yaml,
        Representation::Ndjson,
    ] {
        let result = run(on_a_terminal().output_mode(encoding), ["myapp", "log"]);

        result.assert_success();
        assert_eq!(
            result.delivery(),
            &Delivery::Stdout,
            "{encoding:?} should never page"
        );
    }
}

#[test]
#[serial]
fn a_stdout_that_is_not_a_terminal_pages_nothing() {
    let result = run(on_a_terminal().stdout_is_terminal(false), ["myapp", "log"]);

    result.assert_success();
    assert_eq!(result.delivery(), &Delivery::Stdout);
    assert_eq!(result.stdout(), "3 entries");
}

#[test]
#[serial]
fn no_pager_writes_the_page_straight_to_stdout() {
    let result = run(on_a_terminal(), ["myapp", "log", "--no-pager"]);

    result.assert_success();
    assert_eq!(result.delivery(), &Delivery::Stdout);
    assert_eq!(result.stdout(), "3 entries");
}

#[test]
#[serial]
fn a_named_output_file_takes_the_page_the_pager_would_have() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("log.txt");

    let result = run(
        on_a_terminal(),
        ["myapp", "log", "--output-file-path", path.to_str().unwrap()],
    );

    result.assert_success();
    assert_eq!(result.delivery(), &Delivery::File(path.clone()));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "3 entries");
}

#[test]
#[serial]
fn an_environment_naming_no_pager_pages_nothing() {
    let harness = TestHarness::new()
        .stdout_is_terminal(true)
        .env_remove("MYAPP_PAGER")
        .env_remove("PAGER");

    let result = run(harness, ["myapp", "log"]);

    result.assert_success();
    assert_eq!(result.delivery(), &Delivery::Stdout);
}

#[test]
#[serial]
fn a_post_output_hook_decides_what_there_is_to_page() {
    let payload = App::builder()
        .name("myapp")
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "log",
            FnHandler::new(|_: &ArgMatches, _: &CommandContext| {
                Ok(Output::Render(json!({ "entries": 3 })))
            }),
            |cfg| {
                cfg.pageable().post_output(|_, _, _| {
                    Ok(RenderedOutput::Binary(b"\x00\x01".to_vec(), "bin".into()))
                })
            },
        )
        .unwrap()
        .build()
        .unwrap();
    let result = on_a_terminal().run(&payload, command(), ["myapp", "log"]);

    result.assert_success();
    assert_eq!(result.delivery(), &Delivery::Stdout);

    let rewritten = App::builder()
        .name("myapp")
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "log",
            FnHandler::new(|_: &ArgMatches, _: &CommandContext| {
                Ok(Output::Render(json!({ "entries": 3 })))
            }),
            |cfg| {
                cfg.pageable().post_output(|_, _, _| {
                    Ok(RenderedOutput::Text(TextOutput::plain(
                        "the hook's page".to_string(),
                    )))
                })
            },
        )
        .unwrap()
        .build()
        .unwrap();
    let result = on_a_terminal().run(&rewritten, command(), ["myapp", "log"]);

    result.assert_success();
    assert_eq!(result.delivery(), &Delivery::Pager(PAGER.to_string()));
    assert_eq!(result.stdout(), "the hook's page");
}

#[test]
#[serial]
fn help_reports_the_same_decision_a_command_does() {
    let paged = run(on_a_terminal(), ["myapp", "--help"]);
    paged.assert_success();
    assert_eq!(paged.delivery(), &Delivery::Pager(PAGER.to_string()));

    let declined = run(on_a_terminal(), ["myapp", "--help", "--no-pager"]);
    declined.assert_success();
    assert_eq!(declined.delivery(), &Delivery::Stdout);

    let word = run(on_a_terminal(), ["myapp", "help"]);
    word.assert_success();
    assert_eq!(word.delivery(), &Delivery::Pager(PAGER.to_string()));
}

#[test]
#[serial]
fn a_help_page_the_strict_style_check_rejects_pages_nothing() {
    let strict = App::builder()
        .name("myapp")
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .strict_style_tags(true)
        .command_with(
            "log",
            FnHandler::new(|_: &ArgMatches, _: &CommandContext| {
                Ok(Output::Render(json!({ "entries": 3 })))
            }),
            |cfg| cfg.pageable(),
        )
        .unwrap()
        .build()
        .unwrap();

    let described = Command::new("myapp")
        .subcommand(Command::new("log").about("[bogus]every entry so far[/bogus]"));
    let result = on_a_terminal().run(&strict, described, ["myapp", "--help"]);

    result.assert_error_kind(RunErrorKind::Render);
    assert_eq!(result.delivery(), &Delivery::Stdout);
}

#[test]
#[serial]
fn dispatch_reports_the_decision_the_argv_path_reports() {
    let _env = ScopedEnv::new().set("MYAPP_PAGER", PAGER).remove("PAGER");

    let app = app();
    let target = TargetProperties::detect();
    let dispatched = app.dispatch(
        command().try_get_matches_from(["myapp", "log"]).unwrap(),
        Representation::Human,
    );
    let from_argv = app.run_with_color(
        command(),
        ["myapp", "log"],
        target,
        ColorPolicy::Auto,
        InputSources::from_process(),
    );

    let expected = if target.stdout_is_terminal {
        Delivery::Pager(PAGER.to_string())
    } else {
        Delivery::Stdout
    };
    assert_eq!(dispatched.delivery(), &expected);
    assert_eq!(from_argv.delivery(), &expected);
}
