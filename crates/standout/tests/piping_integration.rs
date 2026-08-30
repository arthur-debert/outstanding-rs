use clap::Command;
use console::Style;
use serde_json::json;
use standout::cli::{App, DispatchResult, Output};
use standout::Theme;
use std::sync::Arc;
use std::time::Duration;

#[test]
fn test_pipe_to_passthrough() {
    let app = App::builder()
        .commands(|g| {
            g.command_with(
                "list",
                |_m, _ctx| Ok(Output::Render(json!({"items": ["foo", "bar", "baz"]}))),
                |cfg| {
                    cfg.template("{{ items | join(\", \") }}")
                        .pipe_to(if cfg!(windows) { "more" } else { "cat" })
                },
            )
        })
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("test").subcommand(Command::new("list"));
    let result = app.run_to_string(cmd, vec!["test", "list"]);

    if let DispatchResult::Handled(output) = result.outcome() {
        assert_eq!(output, "foo, bar, baz");
    } else {
        panic!("Expected DispatchResult::Handled, got {:?}", result);
    }
}

#[test]
fn test_pipe_through_capture() {
    let app = App::builder()
        .commands(|g| {
            g.command_with(
                "filter",
                |_m, _ctx| Ok(Output::Render(json!({"lines": "foo\nbar\nbaz"}))),
                |cfg| {
                    cfg.template("{{ lines }}").pipe_through(if cfg!(windows) {
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

    if let DispatchResult::Handled(output) = result.outcome() {
        assert_eq!(output.trim(), "foo");
    } else {
        panic!("Expected DispatchResult::Handled, got {:?}", result);
    }
}

#[test]
fn test_pipe_chaining() {
    let app = App::builder()
        .commands(|g| {
            g.command_with(
                "chain",
                |_m, _ctx| Ok(Output::Render(json!({"data": "hello world"}))),
                |cfg| {
                    cfg.template("{{ data }}")
                        .pipe_through(if cfg!(windows) {
                            "findstr hello"
                        } else {
                            "grep hello"
                        })
                        .pipe_to(if cfg!(windows) { "more" } else { "cat" })
                },
            )
        })
        .unwrap()
        .build()
        .unwrap();

    let cmd = Command::new("test").subcommand(Command::new("chain"));
    let result = app.run_to_string(cmd, vec!["test", "chain"]);

    if let DispatchResult::Handled(output) = result.outcome() {
        assert!(output.contains("hello"));
    } else {
        panic!("Expected DispatchResult::Handled, got {:?}", result);
    }
}

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

    if let DispatchResult::Handled(output) = result.outcome() {
        assert_eq!(output, "done");
    } else {
        panic!("Expected DispatchResult::Handled, got {:?}", result);
    }
}

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

    if let DispatchResult::Handled(output) = result.outcome() {
        assert_eq!(output.trim(), "abc");
    } else {
        panic!("Expected DispatchResult::Handled, got {:?}", result);
    }
}

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

    if let DispatchResult::Handled(output) = result.outcome() {
        assert_eq!(output, "HELLO");
    } else {
        panic!("Expected DispatchResult::Handled, got {:?}", result);
    }
}

#[test]
fn test_pipe_command_failure() {
    use standout::cli::{ExitStatus, HookPhase, RunErrorKind};
    let app = App::builder()
        .commands(|g| {
            g.command_with(
                "fail",
                |_m, _ctx| Ok(Output::Render(json!({"text": "test"}))),
                |cfg| cfg.template("{{ text }}").pipe_through("exit 1"),
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

    match result.outcome() {
        DispatchResult::Error(msg) => {
            assert!(
                msg.contains("exit 1") || msg.contains("failed") || msg.contains("Broken pipe"),
                "Expected error message about failed command, got: {}",
                msg
            );
        }
        _ => panic!("Expected DispatchResult::Error, got {:?}", result),
    }
}

#[test]
fn test_pipe_strips_ansi_codes() {
    use standout_pipe::{PipeError, PipeTarget};

    struct CapturePipe(Arc<std::sync::Mutex<String>>);

    impl PipeTarget for CapturePipe {
        fn pipe(&self, input: &str) -> Result<String, PipeError> {
            *self.0.lock().unwrap() = input.to_string();
            Ok(input.to_string())
        }
    }

    let captured = Arc::new(std::sync::Mutex::new(String::new()));
    let capture_clone = captured.clone();

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

#[test]
fn test_pipe_preserves_terminal_formatting_in_passthrough() {
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

    if let DispatchResult::Handled(terminal_output) = result.outcome() {
        assert!(
            terminal_output.contains("\x1b[") || terminal_output == "world",
            "Terminal output should have ANSI codes (or be plain if not a TTY), got: {:?}",
            terminal_output
        );
    } else {
        panic!("Expected DispatchResult::Handled");
    }
}

#[test]
fn test_clipboard_receives_plain_text() {
    use standout_pipe::{PipeError, PipeTarget};

    let copied = Arc::new(std::sync::Mutex::new(String::new()));
    let copied_clone = copied.clone();

    struct MockClipboard(Arc<std::sync::Mutex<String>>);

    impl PipeTarget for MockClipboard {
        fn pipe(&self, input: &str) -> Result<String, PipeError> {
            *self.0.lock().unwrap() = input.to_string();
            Ok(String::new())
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
