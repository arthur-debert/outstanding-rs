use clap::Command;
use console::Style;
use standout::cli::{App, Output};
use standout::Theme;

#[test]
fn test_theme_preservation_bug() {
    let style = Style::new().red().force_styling(true);
    let theme = Theme::new().add("custom_error", style);

    let app = App::builder()
        .theme(theme)
        .command(
            "test",
            |_m, _ctx| Ok(Output::Render("my_content".to_string())),
            "[custom_error]my_content[/custom_error]",
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let cmd = Command::new("app").subcommand(Command::new("test"));

    let result = app.run_to_string(cmd, ["app", "--output=term", "test"]);

    match result.into_outcome() {
        standout::cli::DispatchResult::Handled(output) => {
            assert!(
                output.contains("\x1b[31m"),
                "Output should contain Red ANSI code, but got: {:?}",
                output
            );
        }
        _ => panic!("Expected handled result"),
    }
}
