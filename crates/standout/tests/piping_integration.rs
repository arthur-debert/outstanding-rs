//! Integration tests for output piping functionality.

use clap::Command;
use serde_json::json;
use standout::cli::{App, Output, RunResult, ThreadSafe};
use std::time::Duration;

/// Test basic pipe_to (passthrough mode) - output is preserved
#[test]
fn test_pipe_to_passthrough() {
    let app = App::<ThreadSafe>::builder()
        .commands(|g| {
            g.command_with(
                "list",
                |_m, _ctx| Ok(Output::Render(json!({"items": ["foo", "bar", "baz"]}))),
                |cfg| {
                    cfg.template("{{ items | join(\", \") }}")
                        // Passthrough: runs cat but returns original output
                        .pipe_to(if cfg!(windows) { "more" } else { "cat" })
                },
            )
        })
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("test").subcommand(Command::new("list"));
    let result = app.run_to_string(cmd, vec!["test", "list"]);

    if let RunResult::Handled(output) = result {
        // Passthrough returns original input
        assert_eq!(output, "foo, bar, baz");
    } else {
        panic!("Expected RunResult::Handled, got {:?}", result);
    }
}

/// Test pipe_through (capture mode) - uses command's stdout as new output
#[test]
fn test_pipe_through_capture() {
    let app = App::<ThreadSafe>::builder()
        .commands(|g| {
            g.command_with(
                "filter",
                |_m, _ctx| Ok(Output::Render(json!({"lines": "foo\nbar\nbaz"}))),
                |cfg| {
                    cfg.template("{{ lines }}")
                        // Capture: grep's output becomes the new output
                        .pipe_through(if cfg!(windows) {
                            "findstr foo"
                        } else {
                            "grep foo"
                        })
                },
            )
        })
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("test").subcommand(Command::new("filter"));
    let result = app.run_to_string(cmd, vec!["test", "filter"]);

    if let RunResult::Handled(output) = result {
        // Capture returns grep's output (only the line containing "foo")
        assert_eq!(output.trim(), "foo");
    } else {
        panic!("Expected RunResult::Handled, got {:?}", result);
    }
}

/// Test chaining multiple pipes
#[test]
fn test_pipe_chaining() {
    let app = App::<ThreadSafe>::builder()
        .commands(|g| {
            g.command_with(
                "chain",
                |_m, _ctx| Ok(Output::Render(json!({"data": "hello world"}))),
                |cfg| {
                    cfg.template("{{ data }}")
                        // First pipe: capture (transforms output)
                        .pipe_through(if cfg!(windows) {
                            "findstr hello"
                        } else {
                            "grep hello"
                        })
                        // Second pipe: passthrough (side effect, preserves output)
                        .pipe_to(if cfg!(windows) { "more" } else { "cat" })
                },
            )
        })
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("test").subcommand(Command::new("chain"));
    let result = app.run_to_string(cmd, vec!["test", "chain"]);

    if let RunResult::Handled(output) = result {
        assert!(output.contains("hello"));
    } else {
        panic!("Expected RunResult::Handled, got {:?}", result);
    }
}

/// Test pipe_to_with_timeout
#[test]
fn test_pipe_with_custom_timeout() {
    let app = App::<ThreadSafe>::builder()
        .commands(|g| {
            g.command_with(
                "slow",
                |_m, _ctx| Ok(Output::Render(json!({"msg": "done"}))),
                |cfg| {
                    cfg.template("{{ msg }}")
                        .pipe_to_with_timeout(
                            if cfg!(windows) { "more" } else { "cat" },
                            Duration::from_secs(60),
                        )
                },
            )
        })
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("test").subcommand(Command::new("slow"));
    let result = app.run_to_string(cmd, vec!["test", "slow"]);

    if let RunResult::Handled(output) = result {
        assert_eq!(output, "done");
    } else {
        panic!("Expected RunResult::Handled, got {:?}", result);
    }
}

/// Test pipe_through_with_timeout
#[test]
fn test_pipe_through_with_custom_timeout() {
    let app = App::<ThreadSafe>::builder()
        .commands(|g| {
            g.command_with(
                "process",
                |_m, _ctx| Ok(Output::Render(json!({"text": "abc\ndef"}))),
                |cfg| {
                    cfg.template("{{ text }}")
                        .pipe_through_with_timeout(
                            if cfg!(windows) {
                                "findstr abc"
                            } else {
                                "grep abc"
                            },
                            Duration::from_secs(60),
                        )
                },
            )
        })
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("test").subcommand(Command::new("process"));
    let result = app.run_to_string(cmd, vec!["test", "process"]);

    if let RunResult::Handled(output) = result {
        assert_eq!(output.trim(), "abc");
    } else {
        panic!("Expected RunResult::Handled, got {:?}", result);
    }
}

/// Test pipe_with custom PipeTarget
#[test]
fn test_pipe_with_custom_target() {
    use standout_pipe::{PipeError, PipeTarget};

    struct UppercasePipe;

    impl PipeTarget for UppercasePipe {
        fn pipe(&self, input: &str) -> Result<String, PipeError> {
            Ok(input.to_uppercase())
        }
    }

    let app = App::<ThreadSafe>::builder()
        .commands(|g| {
            g.command_with(
                "upper",
                |_m, _ctx| Ok(Output::Render(json!({"text": "hello"}))),
                |cfg| cfg.template("{{ text }}").pipe_with(UppercasePipe),
            )
        })
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("test").subcommand(Command::new("upper"));
    let result = app.run_to_string(cmd, vec!["test", "upper"]);

    if let RunResult::Handled(output) = result {
        assert_eq!(output, "HELLO");
    } else {
        panic!("Expected RunResult::Handled, got {:?}", result);
    }
}

/// Test that piping a failed command propagates the error
#[test]
fn test_pipe_command_failure() {
    let app = App::<ThreadSafe>::builder()
        .commands(|g| {
            g.command_with(
                "fail",
                |_m, _ctx| Ok(Output::Render(json!({"text": "test"}))),
                |cfg| {
                    cfg.template("{{ text }}")
                        .pipe_through("exit 1") // This command will fail
                },
            )
        })
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("test").subcommand(Command::new("fail"));
    let result = app.run_to_string(cmd, vec!["test", "fail"]);

    // Hook error should produce a Handled result with error message
    match result {
        RunResult::Handled(output) => {
            // Error message should contain info about the failed command
            assert!(
                output.contains("exit 1") || output.contains("failed"),
                "Expected error message about failed command, got: {}",
                output
            );
        }
        _ => panic!("Expected RunResult::Handled with error, got {:?}", result),
    }
}
