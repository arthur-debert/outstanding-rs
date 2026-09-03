use clap::{Arg, Command};
use serde_json::json;
use standout::cli::{
    App, DispatchResult, EventsFnHandler, FnHandler, HandlerResult, Output, Results, StreamCapture,
    StreamSink,
};
use standout::ColorPolicy;
use standout::{
    AmbiguousWidth, ColorMode, EmbeddedTemplates, IconMode, InputSources, Representation,
    TargetProperties, TemplateRef,
};

const TEMPLATES: &[(&str, &str)] = &[
    ("stream", "{{ applied }} applied"),
    ("stream.event", "{{ event.type }} {{ event.resource }}"),
];

fn command() -> Command {
    Command::new("app").subcommand(Command::new("stream"))
}

fn command_with_output_flag() -> Command {
    command().arg(
        Arg::new("_output_mode")
            .long("output")
            .global(true)
            .value_parser(["auto", "json", "ndjson"])
            .default_value("auto"),
    )
}

fn stream_handler(
    _: &clap::ArgMatches,
    _ctx: &standout::cli::CommandContext,
    results: &mut Results<serde_json::Value>,
) -> HandlerResult<serde_json::Value> {
    results.emit(json!({ "type": "apply_start", "resource": "web" }))?;
    results.emit(json!({ "type": "apply_complete", "resource": "web" }))?;
    Ok(Output::Render(json!({ "applied": 1 })))
}

fn app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with("stream", EventsFnHandler::new(stream_handler), |cfg| cfg)
        .unwrap()
        .build()
        .unwrap()
}

fn target() -> TargetProperties {
    TargetProperties {
        width: None,
        stdout_is_terminal: false,
        stderr_is_terminal: false,
        stdout_color_capability: false,
        stderr_color_capability: false,
        color_scheme: ColorMode::Dark,
        icon_mode: IconMode::Classic,
        ambiguous_width: AmbiguousWidth::Narrow,
    }
}

const ENTRIES: &str = "{\"type\":\"apply_start\",\"resource\":\"web\"}\n\
                       {\"type\":\"apply_complete\",\"resource\":\"web\"}\n";

#[test]
fn run_with_captures_the_entries_beside_the_result() {
    let run = app().run_with(
        command(),
        ["app", "--output=ndjson", "stream"],
        target(),
        InputSources::from_process(),
    );
    assert_eq!(run.entries(), ENTRIES);
    assert_eq!(
        run.output(),
        Some("{\"type\":\"result\",\"data\":{\"applied\":1}}")
    );

    let json = app().run_with(
        command(),
        ["app", "--output=json", "stream"],
        target(),
        InputSources::from_process(),
    );
    assert_eq!(json.entries(), "");
    assert!(
        matches!(json.outcome(), DispatchResult::Error(_)),
        "an encoding that buffers a command's events carries none yet"
    );
}

#[test]
fn dispatch_captures_the_entries_beside_the_result() {
    let matches = command().try_get_matches_from(["app", "stream"]).unwrap();
    let run = app().dispatch(matches, Representation::Ndjson);
    assert_eq!(run.entries(), ENTRIES);
    assert_eq!(
        run.output(),
        Some("{\"type\":\"result\",\"data\":{\"applied\":1}}")
    );
}

#[test]
fn run_command_writes_the_events_to_the_sink_it_is_given() {
    let matches = command_with_output_flag()
        .try_get_matches_from(["app", "--output=ndjson", "stream"])
        .unwrap();
    let sub = matches.subcommand_matches("stream").unwrap();
    let capture = StreamCapture::default();
    let output = app()
        .run_command(
            "stream",
            sub,
            EventsFnHandler::new(stream_handler),
            TemplateRef::Named("stream".to_string()),
            ColorPolicy::Auto,
            StreamSink::new(capture.clone()),
        )
        .unwrap();
    assert_eq!(String::from_utf8(capture.take()).unwrap(), ENTRIES);
    assert_eq!(
        output.as_text(),
        Some("{\"type\":\"result\",\"data\":{\"applied\":1}}")
    );

    let matches = command_with_output_flag()
        .try_get_matches_from(["app", "--output=json", "stream"])
        .unwrap();
    let sub = matches.subcommand_matches("stream").unwrap();
    let capture = StreamCapture::default();
    let error = app()
        .run_command(
            "stream",
            sub,
            EventsFnHandler::new(stream_handler),
            TemplateRef::Named("stream".to_string()),
            ColorPolicy::Auto,
            StreamSink::new(capture.clone()),
        )
        .unwrap_err();
    assert!(capture.take().is_empty(), "{error}");
}

#[test]
fn run_command_rejects_binary_output_under_ndjson() {
    let matches = command_with_output_flag()
        .try_get_matches_from(["app", "--output=ndjson", "stream"])
        .unwrap();
    let sub = matches.subcommand_matches("stream").unwrap();
    let error = app()
        .run_command(
            "stream",
            sub,
            FnHandler::new(|_, _| -> HandlerResult<()> {
                Ok(Output::Binary {
                    data: vec![0, 1, 2],
                    filename: "out.bin".into(),
                })
            }),
            TemplateRef::Absent,
            ColorPolicy::Auto,
            StreamSink::new(Vec::new()),
        )
        .unwrap_err();
    let source = std::error::Error::source(&error)
        .map(ToString::to_string)
        .unwrap_or_default();
    assert!(
        source.contains("binary output was produced under ndjson"),
        "{error}: {source}"
    );
}
