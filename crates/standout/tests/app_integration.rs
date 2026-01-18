use clap::Command;
use serde_json::json;
use standout::cli::{App, HandlerResult, LocalApp, Output};
use std::cell::RefCell;
use std::rc::Rc;

// Test App<ThreadSafe> (default)
#[test]
fn test_app_thread_safe_integration() {
    let app = App::<standout::cli::ThreadSafe>::builder()
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

// Test App<Local> via LocalApp alias
#[test]
fn test_local_app_integration() {
    let counter = Rc::new(RefCell::new(0));
    let counter_clone = counter.clone();

    let app = LocalApp::builder()
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

// Test stateful struct handler with LocalApp
#[test]
fn test_local_struct_handler() {
    struct StatefulHandler {
        count: i32,
    }

    impl standout::cli::LocalHandler for StatefulHandler {
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

    let app = LocalApp::builder()
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

    // Note: In the current architecture for LocalApp, handlers are closures in Rc<RefCell>.
    // Each run re-uses the SAME closure instance because we build the App once.
    // So state should persist across calls if we reuse the App instance.

    let result2 = app.run_to_string(cmd, vec!["test", "add"]);
    if let standout::cli::RunResult::Handled(output) = result2 {
        assert_eq!(output, "20");
    } else {
        panic!("Expected RunResult::Handled, got {:?}", result2);
    }
}
