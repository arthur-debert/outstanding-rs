use clap::Command;
use console::Style;
use serde_json::json;
use standout::cli::{App, Output};
use standout::dispatch;
use standout::Theme;

#[test]
fn test_late_binding_theme_sequencing() {
    // 1. Create a theme with a custom style
    // Note: force_styling(true) is required in tests because there's no TTY
    let style = Style::new().cyan().force_styling(true);
    let theme = Theme::new().add("issue_89_style", style);

    // 2. Build the app with OUT-OF-ORDER configuration:
    //    Command is registered BEFORE the theme is set.
    //    Prior to the fix, this would capture the default (empty) theme.
    //    With Late Binding, it should use the theme provided at runtime.
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

    // 3. Run to string
    let cmd = Command::new("app").subcommand(Command::new("late_bind"));

    // We simulate passing "--output=term" to force terminal output with colors
    let result = app.run_to_string(cmd, ["app", "--output=term", "late_bind"]);

    match result {
        standout::cli::RunResult::Handled(output) => {
            // 4. Verification: If theme works, output should contain ANSI cyan code: \x1b[36m
            assert!(
                output.contains("\x1b[36m"),
                "Output should contain Cyan ANSI code for late bound theme, but got: {:?}",
                output
            );
        }
        _ => panic!("Expected handled result, got {:?}", result),
    }
}

/// Test that the dispatch! macro works correctly with late binding.
/// The theme is set AFTER the dispatch! macro, which should still work.
#[test]
fn test_late_binding_with_dispatch_macro() {
    let style = Style::new().magenta().force_styling(true);
    let theme = Theme::new().add("macro_style", style);

    // dispatch! macro is called BEFORE .theme()
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

    match result {
        standout::cli::RunResult::Handled(output) => {
            // Magenta ANSI code: \x1b[35m
            assert!(
                output.contains("\x1b[35m"),
                "dispatch! macro should use late-bound theme, but got: {:?}",
                output
            );
            // Also verify the style tag wasn't rendered as [macro_style?]
            assert!(
                !output.contains("[macro_style?]"),
                "Style should be found, but got unknown style marker: {:?}",
                output
            );
        }
        _ => panic!("Expected handled result, got {:?}", result),
    }
}

/// Test that nested groups work correctly with late binding.
/// Commands in nested groups should also receive the late-bound theme.
#[test]
fn test_late_binding_with_nested_groups() {
    let style = Style::new().green().force_styling(true);
    let theme = Theme::new().add("nested_style", style);

    // Nested group commands registered BEFORE .theme()
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

    match result {
        standout::cli::RunResult::Handled(output) => {
            // Green ANSI code: \x1b[32m
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

// ============================================================================
// Invariant Tests
// ============================================================================

/// Test the invariant: unknown style tags render as [tag?].
///
/// This is the symptom of the original bug - when a theme is not applied,
/// style tags render with a "?" suffix to indicate the style was not found.
/// This test documents this behavior as a regression detection mechanism.
#[test]
fn test_unknown_style_renders_as_tag_question_mark() {
    // Build WITHOUT any theme - styles should render as [tag?]
    let app = App::builder()
        .command(
            "test",
            |_m, _ctx| Ok(Output::Render("content".to_string())),
            "[unknown_style]content[/unknown_style]",
        )
        .unwrap()
        // No .theme() call - intentionally missing
        .build()
        .expect("Failed to build app");

    let cmd = Command::new("app").subcommand(Command::new("test"));
    let result = app.run_to_string(cmd, ["app", "--output=term", "test"]);

    match result {
        standout::cli::RunResult::Handled(output) => {
            // Unknown styles should render as [tag?] to indicate missing style
            assert!(
                output.contains("[unknown_style?]"),
                "Unknown style should render as [unknown_style?], but got: {:?}",
                output
            );
        }
        _ => panic!("Expected handled result, got {:?}", result),
    }
}

/// Test that a defined style does NOT render as [tag?].
///
/// This is the complement to the previous test - when a style IS defined,
/// it should be applied and NOT show the "?" suffix.
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

    match result {
        standout::cli::RunResult::Handled(output) => {
            // Defined style should NOT render as [tag?]
            assert!(
                !output.contains("[defined_style?]"),
                "Defined style should NOT show ? marker, but got: {:?}",
                output
            );
            // Should contain yellow ANSI code: \x1b[33m
            assert!(
                output.contains("\x1b[33m"),
                "Defined style should apply yellow color, but got: {:?}",
                output
            );
        }
        _ => panic!("Expected handled result, got {:?}", result),
    }
}
