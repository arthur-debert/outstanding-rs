use super::*;
use crate::cli::builder::test_support::EXECUTION_TEMPLATES as TEMPLATES;
use crate::cli::handler::{EventsFnHandler, StreamSink, Summary as HandlerSummary};
use crate::ColorPolicy;
use crate::EmbeddedTemplates;
use clap::Command;

#[test]
fn a_summary_only_recorder_writes_every_event_and_returns_only_the_summary() {
    #[derive(serde::Serialize)]
    struct Started {
        resource: String,
    }

    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "apply",
            EventsFnHandler::new(
                |_m, _ctx, results: &mut crate::cli::handler::Results<Started>| {
                    for n in 0..64 {
                        results.emit(Started {
                            resource: format!("r{n}"),
                        })?;
                    }
                    Ok::<_, anyhow::Error>(HandlerSummary::Render(serde_json::json!({"done": 64})))
                },
            ),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();

    let capture = StreamCapture::default();
    let run = app.run_recording(
        Command::new("app").subcommand(Command::new("apply")),
        ["app", "apply"],
        TargetProperties::detect(),
        ColorPolicy::Never,
        InputSources::from_process(),
        StreamSink::new(capture.clone()),
        RunRecorder::summary_only(),
    );

    let written = String::from_utf8(capture.take()).unwrap();
    assert_eq!(written.lines().count(), 64, "{written}");
    assert_eq!(run.results(), [serde_json::json!({"done": 64})]);
}
