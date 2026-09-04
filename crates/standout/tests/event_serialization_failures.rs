//! An event value that does not serialize fails the run, whether the handler
//! propagates the `emit` failure or swallows it.

use clap::{ArgMatches, Command};
use serde::Serialize;
use standout::cli::{
    App, CommandContext, DiagnosticKind, EventsFnHandler, HandlerResult, Output, Results,
    RunErrorKind,
};
use standout::{ColorPolicy, EmbeddedTemplates, Representation};
use standout_test::{TestHarness, TestResult};
use std::collections::HashMap;

const TEMPLATES: &[(&str, &str)] = &[("apply", "{{ add }} added"), ("apply.event", "an event")];

/// A map keyed by a tuple has no JSON object key, so serializing it fails.
#[derive(Serialize)]
struct Unserializable(HashMap<(u8, u8), u8>);

fn command() -> Command {
    Command::new("app").subcommand(Command::new("apply"))
}

fn run(propagates: bool, representation: Representation) -> TestResult {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "apply",
            EventsFnHandler::new(
                move |_: &ArgMatches,
                      _: &CommandContext,
                      results: &mut Results<Unserializable>|
                      -> HandlerResult<serde_json::Value> {
                    let event = Unserializable([((1u8, 2u8), 3u8)].into_iter().collect());
                    let emitted = results.emit(event);
                    if propagates {
                        emitted?;
                    }
                    Ok(Output::Render(serde_json::json!({ "add": 1 })))
                },
            ),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();

    TestHarness::new()
        .color(ColorPolicy::Never)
        .output_mode(representation)
        .run(&app, command(), ["app", "apply"])
}

#[test]
fn an_event_that_does_not_serialize_fails_the_run_as_a_render_failure() {
    for propagates in [true, false] {
        for representation in [Representation::Ndjson, Representation::Json] {
            let result = run(propagates, representation);

            assert_eq!(
                result.error_kind(),
                Some(RunErrorKind::Render),
                "propagates={propagates} {representation:?}"
            );
            assert_eq!(
                result.expect_diagnostic().kind,
                DiagnosticKind::Render,
                "propagates={propagates} {representation:?}"
            );
        }
    }
}

#[test]
fn the_summary_of_a_run_whose_event_failed_never_reaches_the_consumer() {
    for propagates in [true, false] {
        let result = run(propagates, Representation::Ndjson);
        assert!(
            !result.stdout().contains("added"),
            "propagates={propagates}: {}",
            result.stdout()
        );
    }
}
