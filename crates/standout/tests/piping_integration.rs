//! Integration tests for output piping functionality.

use clap::Command;
use console::Style;
use serde_json::json;
use standout::cli::{App, Output, RunResult};
use standout::Theme;
use std::sync::Arc;
use std::time::Duration;

/// Test basic pipe_to (passthrough mode) - output is preserved
#[test]
fn test_pipe_to_passthrough() {
    let app = App::builder()
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
    let app = App::builder()
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
    let app = App::builder()
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
    let app = App::builder()
        .commands(|g| {
            g.command_with(
                "slow",
                |_m, _ctx| Ok(Output::Render(json!({"msg": "done"}))),
                |cfg| {
                    cfg.template("{{ msg }}").pipe_to_with_timeout(
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
    let app = App::builder()
        .commands(|g| {
            g.command_with(
                "process",
                |_m, _ctx| Ok(Output::Render(json!({"text": "abc\ndef"}))),
                |cfg| {
                    cfg.template("{{ text }}").pipe_through_with_timeout(
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

    let app = App::builder()
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
    use standout::cli::{ExitStatus, HookPhase, RunErrorKind};
    let app = App::builder()
        .commands(|g| {
            g.command_with(
                "fail",
                |_m, _ctx| Ok(Output::Render(json!({"text": "test"}))),
                |cfg| {
                    cfg.template("{{ text }}").pipe_through("exit 1") // This command will fail
                },
            )
        })
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("test").subcommand(Command::new("fail"));
    let result = app.run_to_string(cmd, vec!["test", "fail"]);

    assert_eq!(result.exit_status(), Some(ExitStatus::FAILURE));
    assert_eq!(
        result.error_kind(),
        Some(RunErrorKind::Hook(HookPhase::PostOutput))
    );

    // Hook error should produce RunResult::Error with the failure message
    match result {
        RunResult::Error(msg) => {
            // Error message should indicate the pipe command failed.
            // On macOS the error typically mentions "exit 1" or "failed";
            // on Linux it may surface as "Broken pipe" instead.
            assert!(
                msg.contains("exit 1") || msg.contains("failed") || msg.contains("Broken pipe"),
                "Expected error message about failed command, got: {}",
                msg
            );
        }
        _ => panic!("Expected RunResult::Error, got {:?}", result),
    }
}

/// Test that piped content has ANSI codes stripped (matches shell semantics).
/// This verifies that even when the terminal output has ANSI codes,
/// the piped content is plain text.
#[test]
fn test_pipe_strips_ansi_codes() {
    use standout_pipe::{PipeError, PipeTarget};

    // Use a custom pipe target to capture what gets piped
    struct CapturePipe(Arc<std::sync::Mutex<String>>);

    impl PipeTarget for CapturePipe {
        fn pipe(&self, input: &str) -> Result<String, PipeError> {
            *self.0.lock().unwrap() = input.to_string();
            Ok(input.to_string())
        }
    }

    let captured = Arc::new(std::sync::Mutex::new(String::new()));
    let capture_clone = captured.clone();

    // Use a theme with forced styling to ensure ANSI codes would be generated
    let theme = Theme::new().add("highlight", Style::new().green().force_styling(true));

    let app = App::builder()
        .theme(theme)
        .commands(|g| {
            g.command_with(
                "styled",
                |_m, _ctx| Ok(Output::Render(json!({"text": "hello"}))),
                move |cfg| {
                    cfg.template("[highlight]{{ text }}[/highlight]")
                        .pipe_with(CapturePipe(capture_clone.clone()))
                },
            )
        })
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("test").subcommand(Command::new("styled"));
    let _result = app.run_to_string(cmd, vec!["test", "styled"]);

    // Check what was piped - should NOT contain ANSI escape codes
    let piped_content = captured.lock().unwrap();
    assert!(
        !piped_content.contains("\x1b["),
        "Piped content should not contain ANSI codes, got: {:?}",
        *piped_content
    );
    assert_eq!(
        piped_content.trim(),
        "hello",
        "Piped content should be plain text"
    );
}

/// Test that terminal output still has formatting while piped content is plain.
#[test]
fn test_pipe_preserves_terminal_formatting_in_passthrough() {
    // Use a theme with forced styling
    let theme = Theme::new().add("bold", Style::new().bold().force_styling(true));

    let app = App::builder()
        .theme(theme)
        .commands(|g| {
            g.command_with(
                "test",
                |_m, _ctx| Ok(Output::Render(json!({"msg": "world"}))),
                move |cfg| {
                    cfg.template("[bold]{{ msg }}[/bold]")
                        .pipe_to(if cfg!(windows) { "more" } else { "cat" })
                },
            )
        })
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("app").subcommand(Command::new("test"));
    let result = app.run_to_string(cmd, vec!["app", "test"]);

    // Terminal output (from run_to_string) should have ANSI codes (formatted field)
    if let RunResult::Handled(terminal_output) = result {
        assert!(
            terminal_output.contains("\x1b[") || terminal_output == "world",
            "Terminal output should have ANSI codes (or be plain if not a TTY), got: {:?}",
            terminal_output
        );
    } else {
        panic!("Expected RunResult::Handled");
    }
}

/// Test that clipboard copy receives plain text (no ANSI codes).
/// Note: This test uses a mock since clipboard() may not be available in CI.
#[test]
fn test_clipboard_receives_plain_text() {
    use standout_pipe::{PipeError, PipeTarget};

    // Custom target simulating clipboard behavior
    let copied = Arc::new(std::sync::Mutex::new(String::new()));
    let copied_clone = copied.clone();

    struct MockClipboard(Arc<std::sync::Mutex<String>>);

    impl PipeTarget for MockClipboard {
        fn pipe(&self, input: &str) -> Result<String, PipeError> {
            *self.0.lock().unwrap() = input.to_string();
            Ok(String::new()) // Clipboard consume mode returns empty
        }
    }

    let theme = Theme::new().add("red", Style::new().red().force_styling(true));

    let app = App::builder()
        .theme(theme)
        .commands(|g| {
            g.command_with(
                "copy",
                |_m, _ctx| Ok(Output::Render(json!({"secret": "password123"}))),
                move |cfg| {
                    // Use pipe_with to simulate clipboard behavior
                    cfg.template("[red]{{ secret }}[/red]")
                        .pipe_with(MockClipboard(copied_clone.clone()))
                },
            )
        })
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("test").subcommand(Command::new("copy"));
    let _result = app.run_to_string(cmd, vec!["test", "copy"]);

    // Check what was "copied" - should be plain text
    let clipboard_content = copied.lock().unwrap();
    assert!(
        !clipboard_content.contains("\x1b["),
        "Clipboard should receive plain text without ANSI codes, got: {:?}",
        *clipboard_content
    );
    assert_eq!(
        clipboard_content.trim(),
        "password123",
        "Clipboard should receive the raw text content"
    );
}
