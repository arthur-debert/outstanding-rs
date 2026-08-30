use clap::Command;
use console::Style;
use serde_json::json;
use standout::cli::{App, Output};
use standout::dispatch;
use standout::Theme;

#[test]
fn test_late_binding_theme_sequencing() {
    let style = Style::new().cyan().force_styling(true);
    let theme = Theme::new().add("issue_89_style", style);

    let app = App::builder()
        .command(
            "late_bind",
            |_m, _ctx| Ok(Output::Render("late_content".to_string())),
            "[issue_89_style]late_content[/issue_89_style]",
        )
        .unwrap()
        .theme(theme) // Theme set AFTER command registration
        .build()
        .expect("Failed to build app");

    let cmd = Command::new("app").subcommand(Command::new("late_bind"));

    let result = app.run_to_string(cmd, ["app", "--output=term", "late_bind"]);

    match result.outcome() {
        standout::cli::DispatchResult::Handled(output) => {
            assert!(
                output.contains("\x1b[36m"),
                "Output should contain Cyan ANSI code for late bound theme, but got: {:?}",
                output
            );
        }
        _ => panic!("Expected handled result, got {:?}", result),
    }
}

#[test]
fn test_late_binding_with_dispatch_macro() {
    let style = Style::new().magenta().force_styling(true);
    let theme = Theme::new().add("macro_style", style);

    let app = App::builder()
        .commands(dispatch! {
            macro_cmd => {
                handler: |_m, _ctx| Ok(Output::Render(json!({"val": "macro_test"}))),
                template: "[macro_style]{{ val }}[/macro_style]",
            }
        })
        .unwrap()
        .theme(theme) // Theme set AFTER dispatch! macro
        .build()
        .expect("Failed to build app");

    let cmd = Command::new("app").subcommand(Command::new("macro_cmd"));
    let result = app.run_to_string(cmd, ["app", "--output=term", "macro_cmd"]);

    match result.outcome() {
        standout::cli::DispatchResult::Handled(output) => {
            assert!(
                output.contains("\x1b[35m"),
                "dispatch! macro should use late-bound theme, but got: {:?}",
                output
            );
            assert!(
                !output.contains("[macro_style?]"),
                "Style should be found, but got unknown style marker: {:?}",
                output
            );
        }
        _ => panic!("Expected handled result, got {:?}", result),
    }
}

#[test]
fn test_late_binding_with_nested_groups() {
    let style = Style::new().green().force_styling(true);
    let theme = Theme::new().add("nested_style", style);

    let app = App::builder()
        .group("db", |g| {
            g.command_with(
                "migrate",
                |_m, _ctx| Ok(Output::Render(json!({"status": "migrated"}))),
                |c| c.structured_only(),
            )
        })
        .unwrap()
        .group("app", |g| {
            g.group("config", |g| {
                g.command_with(
                    "get",
                    |_m, _ctx| Ok(Output::Render(json!({"key": "value"}))),
                    |c| c.template("[nested_style]{{ key }}[/nested_style]"),
                )
            })
        })
        .unwrap()
        .theme(theme) // Theme set AFTER all group registrations
        .build()
        .expect("Failed to build app");

    let cmd = Command::new("test")
        .subcommand(Command::new("db").subcommand(Command::new("migrate")))
        .subcommand(
            Command::new("app").subcommand(Command::new("config").subcommand(Command::new("get"))),
        );

    let result = app.run_to_string(cmd, ["test", "--output=term", "app", "config", "get"]);

    match result.outcome() {
        standout::cli::DispatchResult::Handled(output) => {
            assert!(
                output.contains("\x1b[32m"),
                "Nested group command should use late-bound theme, but got: {:?}",
                output
            );
            assert!(
                !output.contains("[nested_style?]"),
                "Style should be found in nested group, but got unknown style marker: {:?}",
                output
            );
        }
        _ => panic!("Expected handled result, got {:?}", result),
    }
}

#[test]
fn test_unknown_style_degrades_to_unstyled_text() {
    let app = App::builder()
        .command(
            "test",
            |_m, _ctx| Ok(Output::Render("content".to_string())),
            "[unknown_style]content[/unknown_style]",
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let cmd = Command::new("app").subcommand(Command::new("test"));
    let result = app.run_to_string(cmd, ["app", "--output=term", "test"]);

    match result.outcome() {
        standout::cli::DispatchResult::Handled(output) => {
            assert_eq!(output, "content");
            assert!(!output.contains("?]"));
        }
        _ => panic!("Expected handled result, got {:?}", result),
    }
}

#[test]
fn test_defined_style_does_not_render_as_tag_question_mark() {
    let style = Style::new().yellow().force_styling(true);
    let theme = Theme::new().add("defined_style", style);

    let app = App::builder()
        .command(
            "test",
            |_m, _ctx| Ok(Output::Render("content".to_string())),
            "[defined_style]content[/defined_style]",
        )
        .unwrap()
        .theme(theme)
        .build()
        .expect("Failed to build app");

    let cmd = Command::new("app").subcommand(Command::new("test"));
    let result = app.run_to_string(cmd, ["app", "--output=term", "test"]);

    match result.outcome() {
        standout::cli::DispatchResult::Handled(output) => {
            assert!(
                !output.contains("[defined_style?]"),
                "Defined style should NOT show ? marker, but got: {:?}",
                output
            );
            assert!(
                output.contains("\x1b[33m"),
                "Defined style should apply yellow color, but got: {:?}",
                output
            );
        }
        _ => panic!("Expected handled result, got {:?}", result),
    }
}
