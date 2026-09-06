use clap::Command;
use console::Style;
use serde::Serialize;
use serde_json::{json, Value};
use standout::cli::{
    App, CompletedRun, EventsFnHandler, FnHandler, Output, Results, Summary, SummaryResult,
};
use standout::{
    AmbiguousWidth, ColorMode, ColorPolicy, EmbeddedTemplates, FormattedText, IconMode,
    InputSources, TargetProperties, Theme,
};

const ORDINARY: &str = "[accent]forged[/accent]\x1b[31mraw\x1b[0m";
const PLAIN: &str = "typed|[accent]forged[/accent]\\u{1b}[31mraw\\u{1b}[0m";
const TEMPLATES: &[(&str, &str)] = &[
    ("show", "{{ label }}|{{ ordinary }}"),
    ("stream", "{{ label }}|{{ ordinary }}"),
    ("stream.event", "{{ event.label }}|{{ event.ordinary }}"),
];

#[derive(Serialize)]
struct Row {
    label: FormattedText,
    ordinary: &'static str,
}

fn row() -> Row {
    Row {
        label: FormattedText::text("typed").styled("accent").unwrap(),
        ordinary: ORDINARY,
    }
}

#[derive(Serialize)]
struct Event {
    r#type: &'static str,
    #[serde(flatten)]
    row: Row,
}

fn app() -> App {
    App::builder()
        .theme(Theme::new().add("accent", Style::new().bold()))
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "show",
            FnHandler::new(|_, _| Ok(Output::Render(row()))),
            |cfg| {
                cfg.post_dispatch(|_, _, data| {
                    assert!(matches!(data["label"], standout::RenderData::Formatted(_)));
                    Ok(data)
                })
            },
        )
        .unwrap()
        .command_with(
            "stream",
            EventsFnHandler::new(|_, _, results: &mut Results<Event>| -> SummaryResult<Row> {
                for r#type in ["started", "finished"] {
                    results.emit(Event { r#type, row: row() })?;
                }
                Ok(Summary::Render(row()))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap()
}

fn run(command: &str, output: Option<&str>, color: ColorPolicy) -> CompletedRun {
    let mut args = vec!["app", command];
    if let Some(output) = output {
        args.extend(["--output", output]);
    }
    let run = app().run_with_color(
        Command::new("app")
            .subcommand(Command::new("show"))
            .subcommand(Command::new("stream")),
        args,
        TargetProperties {
            width: Some(80),
            stdout_is_terminal: true,
            stderr_is_terminal: true,
            stdout_color_capability: true,
            stderr_color_capability: true,
            color_scheme: ColorMode::Dark,
            icon_mode: IconMode::Classic,
            ambiguous_width: AmbiguousWidth::Narrow,
        },
        color,
        InputSources::from_process(),
    );
    assert!(run.error().is_none(), "{:?}", run.outcome());
    assert!(run.warnings().is_empty(), "{:?}", run.warnings());
    run
}

fn plain_row() -> Value {
    json!({ "label": "typed", "ordinary": ORDINARY })
}

fn event_rows() -> Vec<Value> {
    ["started", "finished"]
        .into_iter()
        .map(|kind| json!({ "type": kind, "label": "typed", "ordinary": ORDINARY }))
        .collect()
}

fn recorded_rows() -> Vec<Value> {
    let mut rows = event_rows();
    rows.push(plain_row());
    rows
}

fn framed_rows() -> Vec<Value> {
    let mut rows = event_rows();
    rows.push(json!({ "type": "result", "data": plain_row() }));
    rows
}

#[test]
fn a_typed_handler_keeps_styling_through_hooks_and_escapes_ordinary_text() {
    let plain = run("show", None, ColorPolicy::Never);
    assert_eq!(plain.output(), Some(PLAIN));
    assert_eq!(plain.results(), [plain_row()]);

    let styled = run("show", None, ColorPolicy::Always);
    let output = styled.output().unwrap();
    assert_eq!(console::strip_ansi_codes(output), PLAIN);
    assert!(output.starts_with("\x1b[1mtyped\x1b[0m|"), "{output:?}");
    assert_eq!(output.matches('\x1b').count(), 2);
    assert_eq!(styled.results(), plain.results());
}

#[test]
fn structured_encodings_use_plain_formatted_text_and_preserve_ordinary_strings() {
    for format in ["json", "yaml", "csv", "ndjson"] {
        let result = run("show", Some(format), ColorPolicy::Always);
        assert_eq!(result.results(), [plain_row()], "{format}");
        let output = result.output().unwrap();
        match format {
            "json" => assert_eq!(serde_json::from_str::<Value>(output).unwrap(), plain_row()),
            "yaml" => assert_eq!(serde_yaml::from_str::<Value>(output).unwrap(), plain_row()),
            "ndjson" => assert_eq!(
                serde_json::from_str::<Value>(output).unwrap(),
                json!({ "type": "result", "data": plain_row() })
            ),
            "csv" => {
                let mut reader = csv::Reader::from_reader(output.as_bytes());
                assert_eq!(
                    reader.headers().unwrap(),
                    &csv::StringRecord::from(vec!["label", "ordinary"])
                );
                let rows = reader.records().collect::<Result<Vec<_>, _>>().unwrap();
                assert_eq!(rows, [csv::StringRecord::from(vec!["typed", ORDINARY])]);
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn incremental_human_output_preserves_formatted_event_and_summary_fields() {
    for color in [ColorPolicy::Never, ColorPolicy::Always] {
        let result = run("stream", None, color);
        assert_eq!(
            console::strip_ansi_codes(result.entries()),
            format!("{PLAIN}\n{PLAIN}\n")
        );
        assert_eq!(console::strip_ansi_codes(result.output().unwrap()), PLAIN);
        assert_eq!(result.results(), recorded_rows());
        if color == ColorPolicy::Always {
            assert_eq!(result.entries().matches("\x1b[1mtyped\x1b[0m").count(), 2);
            assert_eq!(
                result
                    .output()
                    .unwrap()
                    .matches("\x1b[1mtyped\x1b[0m")
                    .count(),
                1
            );
        } else {
            assert!(!result.entries().contains('\x1b'));
            assert!(!result.output().unwrap().contains('\x1b'));
        }
    }
}

#[test]
fn incremental_structured_formats_preserve_event_and_summary_plain_projections() {
    for format in ["json", "yaml", "csv", "ndjson"] {
        let result = run("stream", Some(format), ColorPolicy::Always);
        assert_eq!(result.results(), recorded_rows(), "{format}");
        let output = result.output().unwrap();
        match format {
            "json" => assert_eq!(
                serde_json::from_str::<Vec<Value>>(output).unwrap(),
                framed_rows()
            ),
            "yaml" => assert_eq!(
                serde_yaml::from_str::<Vec<Value>>(output).unwrap(),
                framed_rows()
            ),
            "ndjson" => {
                let entries = result
                    .entries()
                    .lines()
                    .map(|line| serde_json::from_str::<Value>(line).unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(entries, event_rows());
                assert_eq!(
                    serde_json::from_str::<Value>(output).unwrap(),
                    json!({ "type": "result", "data": plain_row() })
                );
            }
            "csv" => {
                let mut reader = csv::Reader::from_reader(output.as_bytes());
                assert_eq!(
                    reader.headers().unwrap(),
                    &csv::StringRecord::from(vec!["type", "label", "ordinary"])
                );
                let rows = reader.records().collect::<Result<Vec<_>, _>>().unwrap();
                assert_eq!(
                    rows,
                    [
                        csv::StringRecord::from(vec!["started", "typed", ORDINARY]),
                        csv::StringRecord::from(vec!["finished", "typed", ORDINARY]),
                    ]
                );
            }
            _ => unreachable!(),
        }
    }
}
