use clap::Command;
use serde_json::json;
use standout::cli::{App, HandlerResult, Output};
use standout::AmbiguousWidth;
use std::cell::RefCell;
use std::rc::Rc;

// Test App with closure handlers
#[test]
fn test_app_integration() {
    let app = App::builder()
        .command(
            "test",
            |_m, _ctx| Ok(Output::Render(json!({"msg": "success"}))),
            "{{ msg }}",
        )
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("test").subcommand(Command::new("test"));
    let result = app.run_to_string(cmd, vec!["test", "test"]);
    if let standout::cli::RunResult::Handled(output) = result {
        assert_eq!(output, "success");
    } else {
        panic!("Expected RunResult::Handled, got {:?}", result);
    }
}

#[test]
fn app_builder_wide_policy_reaches_dispatch_rendering() {
    let app = App::builder()
        .ambiguous_width(AmbiguousWidth::Wide)
        .command(
            "width",
            |_m, _ctx| Ok(Output::Render(json!({"indicator": "↦≈Δ"}))),
            "{{ indicator | display_width }}",
        )
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("test").subcommand(Command::new("width"));
    let result = app.run_to_string(cmd, ["test", "width"]);
    assert_eq!(result.output(), Some("5"));
}

// Test App with mutable state (FnMut closures)
#[test]
fn test_app_with_mutable_state() {
    let counter = Rc::new(RefCell::new(0));
    let counter_clone = counter.clone();

    let app = App::builder()
        .command(
            "inc",
            move |_m, _ctx| {
                *counter_clone.borrow_mut() += 1;
                Ok(Output::Render(json!({"count": *counter_clone.borrow()})))
            },
            "{{ count }}",
        )
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("test").subcommand(Command::new("inc"));
    let result = app.run_to_string(cmd, vec!["test", "inc"]);

    if let standout::cli::RunResult::Handled(output) = result {
        assert_eq!(output, "1");
    } else {
        panic!("Expected RunResult::Handled, got {:?}", result);
    }
    assert_eq!(*counter.borrow(), 1);
}

// Test stateful struct handler
#[test]
fn test_struct_handler_with_state() {
    struct StatefulHandler {
        count: i32,
    }

    impl standout::cli::Handler for StatefulHandler {
        type Output = serde_json::Value;

        fn handle(
            &mut self,
            _m: &clap::ArgMatches,
            _ctx: &standout::cli::CommandContext,
        ) -> HandlerResult<serde_json::Value> {
            self.count += 10;
            Ok(Output::Render(json!({"val": self.count})))
        }
    }

    let app = App::builder()
        .command_handler("add", StatefulHandler { count: 0 }, "{{ val }}")
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("test").subcommand(Command::new("add"));
    // First run
    let result1 = app.run_to_string(cmd.clone(), vec!["test", "add"]);
    if let standout::cli::RunResult::Handled(output) = result1 {
        assert_eq!(output, "10");
    } else {
        panic!("Expected RunResult::Handled, got {:?}", result1);
    }

    // State persists across calls because handlers are stored in Rc<RefCell>
    let result2 = app.run_to_string(cmd, vec!["test", "add"]);
    if let standout::cli::RunResult::Handled(output) = result2 {
        assert_eq!(output, "20");
    } else {
        panic!("Expected RunResult::Handled, got {:?}", result2);
    }
}
