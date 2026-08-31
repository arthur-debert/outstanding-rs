use clap::Command;
use console::Style;
use standout::cli::FnHandler;
use standout::cli::{App, Output};
use standout::EmbeddedTemplates;
use standout::Theme;

const TEMPLATES: &[(&str, &str)] = &[("test", "[custom_error]my_content[/custom_error]")];

#[test]
fn test_theme_preservation_bug() {
    let style = Style::new().red().force_styling(true);
    let theme = Theme::new().add("custom_error", style);

    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .theme(theme)
        .command_with(
            "test",
            FnHandler::new(|_m, _ctx| Ok(Output::Render("my_content".to_string()))),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .expect("Failed to build app");

    let cmd = Command::new("app").subcommand(Command::new("test"));

    let result = app.run_with(
        cmd,
        ["app", "--output=term", "test"],
        standout::TargetProperties::detect(),
        standout::InputSources::from_process(),
    );

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
