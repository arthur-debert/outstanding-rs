#![cfg(feature = "clap")]
use clap::Command;
use console::Style;
use standout::cli::{LocalApp, Output};
use standout::Theme;

#[test]
fn test_theme_preservation_bug() {
    // 1. Create a theme with a custom style
    // Note: force_styling(true) is required in tests because there's no TTY
    let style = Style::new().red().force_styling(true);
    let theme = Theme::new().add("custom_error", style);

    // 2. Build the app
    let app = LocalApp::builder()
        .theme(theme)
        .command(
            "test",
            |_m, _ctx| Ok(Output::Render("my_content".to_string())),
            "[custom_error]my_content[/custom_error]",
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    // 3. Run to string (verify output mode handling too)
    // We register the subcommand "test" so clap parses it correctly.
    let cmd = Command::new("app").subcommand(Command::new("test"));

    // We simulate passing "--output=term" to force terminal output with colors
    let result = app.run_to_string(cmd, ["app", "--output=term", "test"]);

    match result {
        standout::cli::RunResult::Handled(output) => {
            // 4. Verification: If theme works, output should contain ANSI red code: \x1b[31m
            assert!(
                output.contains("\x1b[31m"),
                "Output should contain Red ANSI code, but got: {:?}",
                output
            );
        }
        _ => panic!("Expected handled result"),
    }
}
