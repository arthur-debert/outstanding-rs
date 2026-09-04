use clap::Command;
use serde_json::json;
use standout::cli::FnHandler;
use standout::cli::{App, HandlerResult, Output};
use standout::AmbiguousWidth;
use standout::EmbeddedTemplates;
use std::cell::RefCell;
use std::rc::Rc;

const TEMPLATES: &[(&str, &str)] = &[
    ("test", "{{ msg }}"),
    ("width", "{{ indicator | display_width }}"),
    ("inc", "{{ count }}"),
    ("add", "{{ val }}"),
];

#[test]
fn test_app_integration() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "test",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"msg": "success"})))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("test").subcommand(Command::new("test"));
    let result = app.run_with(
        cmd,
        vec!["test", "test"],
        standout::TargetProperties::detect(),
        standout::InputSources::from_process(),
    );
    if let standout::cli::DispatchResult::Handled(output) = result.outcome() {
        assert_eq!(output, "success");
    } else {
        panic!("Expected DispatchResult::Handled, got {:?}", result);
    }
}

#[test]
fn app_builder_wide_policy_reaches_dispatch_rendering() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .ambiguous_width(AmbiguousWidth::Wide)
        .command_with(
            "width",
            FnHandler::new(|_m, _ctx| Ok(Output::Render(json!({"indicator": "↦≈Δ"})))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("test").subcommand(Command::new("width"));
    let matches = cmd.try_get_matches_from(["test", "width"]).unwrap();
    let result = app.dispatch(matches, standout::Representation::Human);
    assert_eq!(result.output(), Some("5"));
}

#[test]
fn test_app_with_mutable_state() {
    let counter = Rc::new(RefCell::new(0));
    let counter_clone = counter.clone();

    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "inc",
            FnHandler::new(move |_m, _ctx| {
                *counter_clone.borrow_mut() += 1;
                Ok(Output::Render(json!({"count": *counter_clone.borrow()})))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("test").subcommand(Command::new("inc"));
    let result = app.run_with(
        cmd,
        vec!["test", "inc"],
        standout::TargetProperties::detect(),
        standout::InputSources::from_process(),
    );

    if let standout::cli::DispatchResult::Handled(output) = result.outcome() {
        assert_eq!(output, "1");
    } else {
        panic!("Expected DispatchResult::Handled, got {:?}", result);
    }
    assert_eq!(*counter.borrow(), 1);
}

#[test]
fn test_struct_handler_with_state() {
    struct StatefulHandler {
        count: i32,
    }

    impl standout::cli::Handler for StatefulHandler {
        type Event = standout::cli::NoEvents;
        type Output = serde_json::Value;
        type Outcome = standout::cli::Output<serde_json::Value>;

        fn handle(
            &mut self,
            _m: &clap::ArgMatches,
            _ctx: &standout::cli::CommandContext,
            _results: &mut standout::cli::Results<Self::Event>,
        ) -> HandlerResult<serde_json::Value> {
            self.count += 10;
            Ok(Output::Render(json!({"val": self.count})))
        }
    }

    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with("add", StatefulHandler { count: 0 }, |cfg| cfg)
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("test").subcommand(Command::new("add"));
    let result1 = app.run_with(
        cmd.clone(),
        vec!["test", "add"],
        standout::TargetProperties::detect(),
        standout::InputSources::from_process(),
    );
    if let standout::cli::DispatchResult::Handled(output) = result1.outcome() {
        assert_eq!(output, "10");
    } else {
        panic!("Expected DispatchResult::Handled, got {:?}", result1);
    }

    let result2 = app.run_with(
        cmd,
        vec!["test", "add"],
        standout::TargetProperties::detect(),
        standout::InputSources::from_process(),
    );
    if let standout::cli::DispatchResult::Handled(output) = result2.outcome() {
        assert_eq!(output, "20");
    } else {
        panic!("Expected DispatchResult::Handled, got {:?}", result2);
    }
}
