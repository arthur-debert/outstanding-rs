//! What an incremental command writes under an encoding with no line framing:
//! one document at the end, or the diagnostic in its place.

use clap::{ArgMatches, Command};
use serde_json::{json, Value};
use standout::cli::hooks::TextOutput;
use standout::cli::{
    App, CommandContext, CommandContextInput, EventsFnHandler, ExitStatus, HandlerResult, Output,
    RenderedOutput, Results, RunErrorKind,
};
use standout::{ColorPolicy, EmbeddedTemplates, Representation};
use standout_test::{TestHarness, TestResult};

const TEMPLATES: &[(&str, &str)] = &[
    ("apply", "{{ add }} added"),
    ("apply.event", "{{ event.type }} {{ event.resource }}"),
];

const RESOURCES: [&str; 2] = ["web", "db"];

const WARNING: &str = "a warning the run raised";

const HOOK_WARNING: &str = "a warning the post-output hook raised";

const REPLACEMENT: &str = "the hook's own document";

#[derive(Clone, Copy, PartialEq)]
enum Ending {
    Summary,
    Silent,
    Failure,
    SummaryAndWarning,
}

#[derive(Clone, Copy, PartialEq)]
enum PostOutput {
    None,
    Replaces,
    Warns,
    Unchanged,
}

fn events(results: &mut Results<Value>) -> Result<(), anyhow::Error> {
    for resource in RESOURCES {
        results.emit(json!({ "type": "apply_start", "resource": resource }))?;
        results.emit(json!({ "type": "apply_complete", "resource": resource }))?;
    }
    Ok(())
}

fn app(ending: Ending, hook: PostOutput) -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .output_file_flag(Some("output-file-path"))
        .command_with(
            "apply",
            EventsFnHandler::new(
                move |_: &ArgMatches,
                      ctx: &CommandContext,
                      results: &mut Results<Value>|
                      -> HandlerResult<Value> {
                    events(results)?;
                    let summary = Output::Render(json!({ "add": RESOURCES.len() }));
                    match ending {
                        Ending::Summary => Ok(summary),
                        Ending::Silent => Ok(Output::Silent),
                        Ending::Failure => Err(anyhow::anyhow!("db: refused")),
                        Ending::SummaryAndWarning => {
                            ctx.warn(WARNING);
                            Ok(summary)
                        }
                    }
                },
            ),
            move |cfg| match hook {
                PostOutput::None => cfg,
                PostOutput::Replaces => cfg.post_output(|_, _, _| {
                    Ok(RenderedOutput::Text(TextOutput::new(
                        REPLACEMENT.to_string(),
                        REPLACEMENT.to_string(),
                    )))
                }),
                PostOutput::Warns => cfg.post_output(|_, ctx: &CommandContext, output| {
                    ctx.warn(HOOK_WARNING);
                    Ok(output)
                }),
                PostOutput::Unchanged => cfg.post_output(|_, _, output| Ok(output)),
            },
        )
        .unwrap()
        .build()
        .unwrap()
}

fn command() -> Command {
    Command::new("app").subcommand(Command::new("apply"))
}

fn run_with(ending: Ending, representation: Representation, args: &[&str]) -> TestResult {
    let mut argv = vec!["app"];
    argv.extend_from_slice(args);
    argv.push("apply");
    TestHarness::new()
        .color(ColorPolicy::Never)
        .output_mode(representation)
        .run(&app(ending, PostOutput::None), command(), argv)
}

fn run(ending: Ending, representation: Representation) -> TestResult {
    run_with(ending, representation, &[])
}

fn run_hooked(ending: Ending, hook: PostOutput, representation: Representation) -> TestResult {
    TestHarness::new()
        .color(ColorPolicy::Never)
        .output_mode(representation)
        .run(&app(ending, hook), command(), ["app", "apply"])
}

/// The records a reader takes from the run's stdout, whatever encoding carried
/// them: the `ndjson` stream parsed line by line, and every other encoding's
/// one array parsed as a whole.
fn records(result: &TestResult) -> Vec<Value> {
    match result.output_mode() {
        Representation::Ndjson => result
            .stdout()
            .lines()
            .map(|line| serde_json::from_str(line).expect("an ndjson line is one record"))
            .collect(),
        Representation::Yaml => serde_yaml::from_str(result.stdout()).expect("a yaml document"),
        _ => serde_json::from_str(result.stdout()).expect("a json document"),
    }
}

const DOCUMENT_ENCODINGS: [Representation; 2] = [Representation::Json, Representation::Yaml];

#[test]
fn the_document_is_what_jq_s_makes_of_the_ndjson_run() {
    let framed = records(&run(Ending::Summary, Representation::Ndjson));
    assert_eq!(
        framed,
        vec![
            json!({"type": "apply_start", "resource": "web"}),
            json!({"type": "apply_complete", "resource": "web"}),
            json!({"type": "apply_start", "resource": "db"}),
            json!({"type": "apply_complete", "resource": "db"}),
            json!({"type": "result", "data": {"add": 2}}),
        ]
    );
    for representation in DOCUMENT_ENCODINGS {
        let result = run(Ending::Summary, representation);
        result.assert_success();
        assert_eq!(records(&result), framed, "{representation:?}");
    }
}

#[test]
fn the_warning_entries_line_framing_writes_last_are_in_the_document_too() {
    let framed = records(&run(Ending::SummaryAndWarning, Representation::Ndjson));
    let warning = framed.last().expect("the stream ends in the warning entry");
    assert_eq!(warning["type"], "diagnostic");
    assert_eq!(warning["severity"], "warning");
    assert_eq!(warning["kind"], "framework");
    assert_eq!(warning["summary"], WARNING);

    for representation in DOCUMENT_ENCODINGS {
        let result = run(Ending::SummaryAndWarning, representation);
        assert_eq!(records(&result), framed, "{representation:?}");
        assert_eq!(
            result.stderr(),
            "",
            "{representation:?}: a warning the document carries is not repeated as prose"
        );
    }
}

#[test]
fn a_post_output_hook_that_replaces_the_document_sends_the_warnings_to_stderr() {
    for representation in DOCUMENT_ENCODINGS {
        let result = run_hooked(
            Ending::SummaryAndWarning,
            PostOutput::Replaces,
            representation,
        );

        result.assert_success();
        assert_eq!(result.stdout(), REPLACEMENT, "{representation:?}");
        assert!(
            result.stderr().contains(WARNING),
            "{representation:?}: the warning the hook's document dropped is prose again: {}",
            result.stderr()
        );
    }
}

#[test]
fn a_warning_a_post_output_hook_raises_reaches_the_document() {
    for representation in DOCUMENT_ENCODINGS {
        let result = run_hooked(Ending::Summary, PostOutput::Warns, representation);

        result.assert_success();
        let document = records(&result);
        let warning = document.last().expect("the document ends in the warning");
        assert_eq!(warning["type"], "diagnostic", "{representation:?}");
        assert_eq!(warning["summary"], HOOK_WARNING, "{representation:?}");
        assert_eq!(
            result.stderr(),
            "",
            "{representation:?}: a warning the document carries is not repeated as prose"
        );
    }
}

#[test]
fn a_post_output_hook_that_returns_the_document_unchanged_changes_nothing() {
    for representation in DOCUMENT_ENCODINGS {
        let hooked = run_hooked(
            Ending::SummaryAndWarning,
            PostOutput::Unchanged,
            representation,
        );
        let unhooked = run(Ending::SummaryAndWarning, representation);

        hooked.assert_success();
        assert_eq!(hooked.stdout(), unhooked.stdout(), "{representation:?}");
        assert_eq!(hooked.stderr(), "", "{representation:?}");
    }
}

/// The destination the run writes through, readable by the handler that is
/// still emitting into it.
#[derive(Clone, Default)]
struct Watched(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);

impl std::io::Write for Watched {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn nothing_is_written_before_the_command_completes() {
    for representation in DOCUMENT_ENCODINGS {
        let destination = Watched::default();
        let written = destination.0.clone();
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let watching = seen.clone();
        let app = App::builder()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "apply",
                EventsFnHandler::new(
                    move |_: &ArgMatches,
                          _: &CommandContext,
                          results: &mut Results<Value>|
                          -> HandlerResult<Value> {
                        for resource in RESOURCES {
                            results.emit(json!({"type": "apply_start", "resource": resource}))?;
                            watching.borrow_mut().push(written.borrow().len());
                        }
                        Ok(Output::Render(json!({ "add": RESOURCES.len() })))
                    },
                ),
                |cfg| cfg,
            )
            .unwrap()
            .build()
            .unwrap();

        let run = app.run_with_sink(
            command(),
            vec![
                "app".to_string(),
                format!("--output={representation:?}").to_lowercase(),
                "apply".to_string(),
            ],
            standout::TargetProperties::detect(),
            ColorPolicy::Never,
            standout::InputSources::from_process(),
            standout::cli::StreamSink::new(destination),
        );

        assert_eq!(
            *seen.borrow(),
            vec![0, 0],
            "{representation:?}: the destination is untouched while the handler emits"
        );
        assert!(run.output().unwrap().contains("apply_start"));
    }
}

#[test]
fn a_failure_after_events_delivers_the_diagnostic_in_place_of_the_array() {
    for representation in DOCUMENT_ENCODINGS {
        let result = run(Ending::Failure, representation);
        let diagnostic = result.expect_diagnostic();
        assert_eq!(diagnostic.summary, "db: refused", "{representation:?}");
        assert_eq!(
            result.error_kind(),
            Some(RunErrorKind::Handler),
            "{representation:?}"
        );
        assert!(
            !result.stdout().contains("apply_start"),
            "{representation:?}: nothing partial goes out: {}",
            result.stdout()
        );
    }
}

#[test]
fn a_silent_summary_leaves_the_events_as_the_whole_document() {
    for representation in DOCUMENT_ENCODINGS {
        let result = run(Ending::Silent, representation);
        result.assert_success();
        assert_eq!(records(&result).len(), 4, "{representation:?}");
        assert!(
            !result.stdout().contains("\"result\""),
            "{representation:?}: a silent summary has no record"
        );
    }
}

#[test]
fn the_output_file_receives_the_document_and_stdout_stays_empty() {
    for representation in DOCUMENT_ENCODINGS {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run.out");
        let result = run_with(
            Ending::SummaryAndWarning,
            representation,
            &["--output-file-path", path.to_str().unwrap()],
        );

        result.assert_success();
        assert_eq!(result.stdout(), "", "{representation:?}");
        assert_eq!(
            result.stderr(),
            "",
            "{representation:?}: the warning is in the file with the rest of the document"
        );
        let written = std::fs::read_to_string(&path).unwrap();
        let document: Vec<Value> = match representation {
            Representation::Yaml => serde_yaml::from_str(&written).unwrap(),
            _ => serde_json::from_str(&written).unwrap(),
        };
        assert_eq!(document.len(), 6, "{representation:?}");
        assert_eq!(document[5]["summary"], WARNING, "{representation:?}");
        assert_eq!(
            result.delivery().path(),
            Some(path.as_path()),
            "{representation:?}"
        );
    }
}

#[test]
fn a_final_write_that_fails_keeps_its_error_kind_and_status() {
    let dir = tempfile::tempdir().unwrap();
    let unwritable = dir.path().join("missing").join("run.json");
    let result = run_with(
        Ending::Summary,
        Representation::Json,
        &["--output-file-path", unwritable.to_str().unwrap()],
    );

    assert_eq!(
        result.error_kind(),
        Some(RunErrorKind::FinalWrite(standout::cli::OutputKind::Text))
    );
    assert_eq!(result.exit_status(), Some(ExitStatus::from(1)));
}

#[test]
fn a_summary_that_does_not_serialize_fails_the_run_before_any_document() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "apply",
            EventsFnHandler::new(
                |_: &ArgMatches,
                 _: &CommandContext,
                 results: &mut Results<Value>|
                 -> HandlerResult<std::collections::HashMap<(u8, u8), u8>> {
                    events(results)?;
                    Ok(Output::Render([((1u8, 2u8), 3u8)].into_iter().collect()))
                },
            ),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();

    let result = TestHarness::new()
        .color(ColorPolicy::Never)
        .output_mode(Representation::Json)
        .run(&app, command(), ["app", "apply"]);

    assert_eq!(result.error_kind(), Some(RunErrorKind::Render));
    assert!(
        !result.stdout().contains("apply_start"),
        "{}",
        result.stdout()
    );
}

#[test]
fn result_reports_the_same_events_and_summary_under_every_representation() {
    let human = run(Ending::Summary, Representation::Human);
    assert_eq!(human.results().len(), 5);
    for representation in [
        Representation::Ndjson,
        Representation::Json,
        Representation::Yaml,
    ] {
        let result = run(Ending::Summary, representation);
        assert_eq!(result.results(), human.results(), "{representation:?}");
        assert_eq!(
            result.result(),
            Some(&json!({ "add": 2 })),
            "{representation:?}"
        );
    }
}
