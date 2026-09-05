//! What an incremental command writes under an encoding with no line framing:
//! one document at the end, or the diagnostic in its place.

use clap::{ArgMatches, Command};
use serde_json::{json, Value};
use standout::cli::hooks::TextOutput;
use standout::cli::{
    App, ArtifactOutput, CommandContext, CommandContextInput, DispatchResult, EventsFnHandler,
    ExitStatus, FnHandler, HandlerResult, Output, RenderedOutput, Results, RunErrorKind, Summary,
    SummaryResult,
};
use standout::tabular::{Column, Width};
use standout::{
    ColorPolicy, CsvProjection, EmbeddedTemplates, Representation, StructuredOutputProjection,
};
use standout_test::{TestHarness, TestResult};

const TEMPLATES: &[(&str, &str)] = &[
    ("apply", "{{ add }} added"),
    ("apply.event", "{{ event.type }} {{ event.resource }}"),
];

const RESOURCES: [&str; 2] = ["web", "db"];

const WARNING: &str = "a warning the run raised";

const HOOK_WARNING: &str = "a warning the post-output hook raised";

const REPLACEMENT: &str = "the hook's own document";

const PAYLOAD: &[u8] = b"the hook's own bytes";

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
    ChangesRawOnly,
    ReturnsBinary,
    ReturnsArtifact,
}

fn payload_hook(hook: PostOutput) -> RenderedOutput {
    match hook {
        PostOutput::ReturnsBinary => RenderedOutput::Binary(PAYLOAD.to_vec(), "run.bin".into()),
        PostOutput::ReturnsArtifact => RenderedOutput::Artifact(ArtifactOutput {
            bytes: PAYLOAD.to_vec(),
            suggested_destination: None,
            stdout_allowed: true,
            report: None,
        }),
        _ => unreachable!("only the payload hooks build a payload"),
    }
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
                      -> SummaryResult<Value> {
                    events(results)?;
                    let summary = Summary::Render(json!({ "add": RESOURCES.len() }));
                    match ending {
                        Ending::Summary => Ok(summary),
                        Ending::Silent => Ok(Summary::Silent),
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
                PostOutput::ChangesRawOnly => cfg.post_output(|_, _, output| {
                    Ok(match output {
                        RenderedOutput::Text(text) => RenderedOutput::Text(TextOutput::new(
                            text.formatted,
                            format!("{REPLACEMENT}{}", text.raw),
                        )),
                        output => output,
                    })
                }),
                PostOutput::ReturnsBinary | PostOutput::ReturnsArtifact => {
                    cfg.post_output(move |_, _, _| Ok(payload_hook(hook)))
                }
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
        .rendering(representation, ColorPolicy::Never)
        .run(&app(ending, PostOutput::None), command(), argv)
}

fn run(ending: Ending, representation: Representation) -> TestResult {
    run_with(ending, representation, &[])
}

fn run_hooked(ending: Ending, hook: PostOutput, representation: Representation) -> TestResult {
    TestHarness::new()
        .rendering(representation, ColorPolicy::Never)
        .run(&app(ending, hook), command(), ["app", "apply"])
}

/// `ndjson` is parsed line by line, every other encoding as one array.
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

fn stream_records() -> Vec<Value> {
    vec![
        json!({"type": "apply_start", "resource": "web"}),
        json!({"type": "apply_complete", "resource": "web"}),
        json!({"type": "apply_start", "resource": "db"}),
        json!({"type": "apply_complete", "resource": "db"}),
        json!({"type": "result", "data": {"add": 2}}),
    ]
}

#[test]
fn the_document_is_what_jq_s_makes_of_the_ndjson_run() {
    let framed = records(&run(Ending::Summary, Representation::Ndjson));
    assert_eq!(framed, stream_records());
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

#[test]
fn a_post_output_hook_that_changes_only_raw_keeps_its_output() {
    for representation in DOCUMENT_ENCODINGS {
        let hooked = run_hooked(
            Ending::SummaryAndWarning,
            PostOutput::ChangesRawOnly,
            representation,
        );
        let unwarned = run(Ending::Summary, representation);

        hooked.assert_success();
        assert_eq!(
            hooked.stdout(),
            unwarned.stdout(),
            "{representation:?}: the framework does not append to a document the hook changed"
        );
        assert!(
            hooked.stderr().contains(WARNING),
            "{representation:?}: the warning is prose again: {}",
            hooked.stderr()
        );
    }
}

/// Readable by the handler that is still emitting into it.
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
    for representation in [
        Representation::Json,
        Representation::Yaml,
        Representation::Csv,
    ] {
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
                          -> SummaryResult<Value> {
                        for resource in RESOURCES {
                            results.emit(json!({"type": "apply_start", "resource": resource}))?;
                            watching.borrow_mut().push(written.borrow().len());
                        }
                        Ok(Summary::Render(json!({ "add": RESOURCES.len() })))
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
                 -> SummaryResult<std::collections::HashMap<(u8, u8), u8>> {
                    events(results)?;
                    Ok(Summary::Render([((1u8, 2u8), 3u8)].into_iter().collect()))
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

const EVENT_ROWS: &str = "type,resource\n\
                          apply_start,web\n\
                          apply_complete,web\n\
                          apply_start,db\n\
                          apply_complete,db\n";

#[test]
fn csv_writes_the_events_as_rows_and_does_not_encode_the_summary() {
    let result = run(Ending::Summary, Representation::Csv);
    result.assert_success();
    assert_eq!(result.stdout(), EVENT_ROWS);
    assert_eq!(result.result(), Some(&json!({ "add": 2 })));
}

#[test]
fn a_silent_summary_leaves_the_csv_rows_unchanged() {
    let result = run(Ending::Silent, Representation::Csv);
    result.assert_success();
    assert_eq!(result.stdout(), EVENT_ROWS);
}

#[test]
fn a_warning_under_csv_is_prose_on_stderr_and_not_a_row() {
    let result = run(Ending::SummaryAndWarning, Representation::Csv);
    result.assert_success();
    assert_eq!(result.stdout(), EVENT_ROWS);
    assert!(result.stderr().contains(WARNING), "{}", result.stderr());
}

#[test]
fn a_failure_after_events_delivers_the_diagnostic_in_place_of_the_rows() {
    let result = run(Ending::Failure, Representation::Csv);
    assert_eq!(result.expect_diagnostic().summary, "db: refused");
    assert_eq!(result.error_kind(), Some(RunErrorKind::Handler));
    assert!(
        !result.stdout().contains("apply_start"),
        "nothing partial goes out: {}",
        result.stdout()
    );
}

#[test]
fn the_output_file_receives_the_csv_document_and_stdout_stays_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("run.csv");
    let result = run_with(
        Ending::Summary,
        Representation::Csv,
        &["--output-file-path", path.to_str().unwrap()],
    );

    result.assert_success();
    assert_eq!(result.stdout(), "");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), EVENT_ROWS);
    assert_eq!(result.delivery().path(), Some(path.as_path()));
}

fn nested_event_app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "apply",
            EventsFnHandler::new(
                |_: &ArgMatches,
                 _: &CommandContext,
                 results: &mut Results<Value>|
                 -> SummaryResult<Value> {
                    results.emit(json!({ "type": "apply_start", "at": { "line": 2 } }))?;
                    Ok(Summary::Render(json!({ "add": 1 })))
                },
            ),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap()
}

#[test]
fn a_nested_event_under_csv_is_the_render_error_a_nested_value_is() {
    let result = TestHarness::new()
        .color(ColorPolicy::Never)
        .output_mode(Representation::Csv)
        .run(&nested_event_app(), command(), ["app", "apply"]);

    assert_eq!(result.error_kind(), Some(RunErrorKind::Render));
    let summary = result.expect_diagnostic().summary;
    assert!(summary.contains("CsvProjection"), "{summary}");
    assert!(
        !result.stdout().contains("apply_start"),
        "{}",
        result.stdout()
    );
}

#[test]
fn a_csv_projection_declared_on_the_command_takes_the_events_as_its_rows() {
    let projection = StructuredOutputProjection::csv(
        CsvProjection::builder(".")
            .column(
                Column::new(Width::default())
                    .key("resource")
                    .header("RESOURCE"),
            )
            .derived_column(
                Column::new(Width::default()).key("phase").header("PHASE"),
                |row, _root| json!(row["type"].as_str().unwrap_or("").replace("apply_", "")),
            )
            .build(),
    );
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "apply",
            EventsFnHandler::new(
                |_: &ArgMatches,
                 _: &CommandContext,
                 results: &mut Results<Value>|
                 -> SummaryResult<Value> {
                    events(results)?;
                    Ok(Summary::Render(json!({ "add": 2 })))
                },
            ),
            move |cfg| cfg.structured_output_projection(projection.clone()),
        )
        .unwrap()
        .build()
        .unwrap();

    let result = TestHarness::new()
        .color(ColorPolicy::Never)
        .output_mode(Representation::Csv)
        .run(&app, command(), ["app", "apply"]);

    result.assert_success();
    assert_eq!(
        result.stdout(),
        "RESOURCE,PHASE\nweb,start\nweb,complete\ndb,start\ndb,complete\n"
    );
}

#[test]
fn result_reports_the_same_events_and_summary_under_csv() {
    let human = run(Ending::Summary, Representation::Human);
    let csv = run(Ending::Summary, Representation::Csv);
    assert_eq!(csv.results(), human.results());
    assert_eq!(csv.result(), Some(&json!({ "add": 2 })));
}

#[derive(Clone, Copy)]
enum NoSummaryTemplate {
    Silent,
    Binary,
}

fn summaryless_app(absence: NoSummaryTemplate, renders_summary: bool) -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "apply",
            EventsFnHandler::new(
                move |_: &ArgMatches,
                      _: &CommandContext,
                      results: &mut Results<Value>|
                      -> SummaryResult<Value> {
                    events(results)?;
                    if renders_summary {
                        Ok(Summary::Render(json!({ "add": RESOURCES.len() })))
                    } else {
                        Ok(Summary::Silent)
                    }
                },
            ),
            move |cfg| match absence {
                NoSummaryTemplate::Silent => cfg.silent(),
                NoSummaryTemplate::Binary => cfg.binary(),
            },
        )
        .unwrap()
        .build()
        .unwrap()
}

#[test]
fn a_command_with_no_summary_template_still_has_its_csv_rows() {
    for absence in [NoSummaryTemplate::Silent, NoSummaryTemplate::Binary] {
        for renders_summary in [false, true] {
            let result = TestHarness::new()
                .color(ColorPolicy::Never)
                .output_mode(Representation::Csv)
                .run(
                    &summaryless_app(absence, renders_summary),
                    command(),
                    ["app", "apply"],
                );

            result.assert_success();
            assert_eq!(
                result.stdout(),
                EVENT_ROWS,
                "renders_summary={renders_summary}"
            );
        }
    }
}

const HUMAN_LINES: &str = "apply_start web\n\
                           apply_complete web\n\
                           apply_start db\n\
                           apply_complete db\n\
                           2 added";

const EVERY_REPRESENTATION: [Representation; 5] = [
    Representation::Human,
    Representation::Ndjson,
    Representation::Json,
    Representation::Yaml,
    Representation::Csv,
];

#[test]
fn a_successful_run_puts_its_results_on_stdout_and_writes_nothing_to_stderr() {
    for representation in EVERY_REPRESENTATION {
        let result = run(Ending::Summary, representation);

        result.assert_success();
        assert_eq!(result.stderr(), "", "{representation:?}");
        assert!(
            !result.stdout().contains('\u{1b}'),
            "{representation:?} put escape sequences on stdout: {:?}",
            result.stdout()
        );
        match representation {
            Representation::Human => assert_eq!(result.stdout(), HUMAN_LINES),
            Representation::Csv => assert_eq!(result.stdout(), EVENT_ROWS),
            _ => assert_eq!(records(&result), stream_records(), "{representation:?}"),
        }
    }
}

const PAYLOAD_ENCODINGS: [Representation; 4] = [
    Representation::Human,
    Representation::Csv,
    Representation::Json,
    Representation::Yaml,
];

#[test]
fn a_post_output_hook_cannot_turn_an_emitting_run_into_a_payload() {
    for (hook, payload) in [
        (PostOutput::ReturnsBinary, "binary"),
        (PostOutput::ReturnsArtifact, "artifact"),
    ] {
        for representation in PAYLOAD_ENCODINGS {
            let result = run_hooked(Ending::Summary, hook, representation);

            let DispatchResult::Error(error) = result.outcome() else {
                panic!(
                    "{payload} {representation:?}: expected a render error, got {}",
                    result.stdout()
                );
            };
            assert_eq!(
                error.kind(),
                RunErrorKind::Render,
                "{payload} {representation:?}"
            );
            assert!(
                error.as_str().contains(&format!(
                    "{payload} output was produced by the post_output hook of a command that \
                     emits events"
                )),
                "{payload} {representation:?}: {}",
                error.as_str()
            );
        }
    }
}

fn batch_app(hook: PostOutput) -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "apply",
            FnHandler::new(
                |_: &ArgMatches, _: &CommandContext| -> HandlerResult<Value> {
                    Ok(Output::Render(json!({ "add": RESOURCES.len() })))
                },
            ),
            move |cfg| cfg.post_output(move |_, _, _| Ok(payload_hook(hook))),
        )
        .unwrap()
        .build()
        .unwrap()
}

#[test]
fn the_same_hook_on_a_command_that_emits_nothing_still_delivers_its_payload() {
    for hook in [PostOutput::ReturnsBinary, PostOutput::ReturnsArtifact] {
        let result = TestHarness::new()
            .color(ColorPolicy::Never)
            .output_mode(Representation::Human)
            .run(&batch_app(hook), command(), ["app", "apply"]);

        result.assert_success();
    }
}
