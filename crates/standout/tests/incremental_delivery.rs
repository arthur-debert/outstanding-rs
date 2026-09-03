//! When an emitted event reaches the destination, and what a reader that walks
//! away does to the rest of the run.

use clap::Command;
use serde::Serialize;
use serde_json::json;
use standout::cli::{
    App, DispatchResult, EventsFnHandler, ExitStatus, HandlerResult, Output, Results, StreamSink,
};
use standout::{
    AmbiguousWidth, ColorMode, ColorPolicy, EmbeddedTemplates, IconMode, InputSources,
    Representation, TargetProperties,
};
use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

const TEMPLATES: &[(&str, &str)] = &[
    ("apply", "{{ add }} added"),
    ("apply.event", "starting {{ event.resource }}"),
];

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Event<'a> {
    ApplyStart { resource: &'a str },
}

const RESOURCES: [&str; 3] = ["web", "db", "cache"];

fn command() -> Command {
    Command::new("app").subcommand(Command::new("apply"))
}

/// The handler reads the destination between emits; `seen` holds what had
/// arrived when each `emit` returned.
fn app(seen: Rc<RefCell<Vec<String>>>, written: Rc<RefCell<Vec<u8>>>) -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "apply",
            EventsFnHandler::new(
                move |_, _ctx, results: &mut Results<Event>| -> HandlerResult<serde_json::Value> {
                    for resource in RESOURCES {
                        results.emit(Event::ApplyStart { resource })?;
                        seen.borrow_mut()
                            .push(String::from_utf8_lossy(&written.borrow()).into_owned());
                    }
                    Ok(Output::Render(json!({ "add": RESOURCES.len() })))
                },
            ),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap()
}

fn target() -> TargetProperties {
    target_that(false)
}

fn target_that(is_terminal: bool) -> TargetProperties {
    TargetProperties {
        width: None,
        stdout_is_terminal: is_terminal,
        stderr_is_terminal: is_terminal,
        stdout_color_capability: false,
        stderr_color_capability: false,
        color_scheme: ColorMode::Dark,
        icon_mode: IconMode::Classic,
        ambiguous_width: AmbiguousWidth::Narrow,
    }
}

#[derive(Clone, Default)]
struct Shared(Rc<RefCell<Vec<u8>>>);

impl Write for Shared {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn run_watching(representation: Representation) -> Vec<String> {
    run_watching_on(representation, target())
}

fn run_watching_on(representation: Representation, target: TargetProperties) -> Vec<String> {
    let destination = Shared::default();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let app = app(seen.clone(), destination.0.clone());
    let args: Vec<String> = match representation {
        Representation::Ndjson => vec!["app".into(), "--output=ndjson".into(), "apply".into()],
        _ => vec!["app".into(), "apply".into()],
    };
    let run = app.run_with_sink(
        command(),
        args,
        target,
        ColorPolicy::Never,
        InputSources::from_process(),
        StreamSink::new(destination.clone()),
    );
    assert!(matches!(run.outcome(), DispatchResult::Handled(_)));
    let seen = seen.borrow().clone();
    seen
}

#[test]
fn each_rendered_event_is_written_before_the_handler_produces_the_next() {
    let seen = run_watching(Representation::Human);
    assert_eq!(
        seen,
        vec![
            "starting web\n".to_string(),
            "starting web\nstarting db\n".to_string(),
            "starting web\nstarting db\nstarting cache\n".to_string(),
        ]
    );
}

#[test]
fn a_terminal_destination_writes_each_event_at_the_same_point_a_pipe_does() {
    assert_eq!(
        run_watching_on(Representation::Human, target_that(true)),
        run_watching(Representation::Human),
    );
}

#[test]
fn each_framed_event_is_written_before_the_handler_produces_the_next() {
    let seen = run_watching(Representation::Ndjson);
    assert_eq!(seen.len(), 3);
    assert_eq!(seen[0], "{\"type\":\"apply_start\",\"resource\":\"web\"}\n");
    assert_eq!(seen[2].lines().count(), 3, "{}", seen[2]);
}

/// Accepts one write, then reports the pipe the way `head -1` leaves it.
struct ReaderLeft(Rc<RefCell<usize>>);

impl Write for ReaderLeft {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut writes = self.0.borrow_mut();
        *writes += 1;
        if *writes > 1 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "reader left",
            ));
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_reader_that_leaves_lets_the_handler_finish_and_keeps_the_command_s_status() {
    let reached = Rc::new(RefCell::new(Vec::new()));
    let handler_reached = reached.clone();
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "apply",
            EventsFnHandler::new(
                move |_, _ctx, results: &mut Results<Event>| -> HandlerResult<serde_json::Value> {
                    for resource in RESOURCES {
                        results.emit(Event::ApplyStart { resource })?;
                        handler_reached.borrow_mut().push(resource);
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
        ["app", "apply"],
        target(),
        ColorPolicy::Never,
        InputSources::from_process(),
        StreamSink::new(ReaderLeft(Rc::new(RefCell::new(0)))),
    );

    assert_eq!(
        *reached.borrow(),
        RESOURCES,
        "the handler ran to completion"
    );
    assert!(
        matches!(run.outcome(), DispatchResult::Handled(_)),
        "a reader that left is not the command's failure"
    );
    assert_eq!(run.exit_status(), Some(ExitStatus::SUCCESS));
    assert_eq!(
        run.results().len(),
        RESOURCES.len() + 1,
        "the values the run produced stand whether or not anyone read them"
    );
}
