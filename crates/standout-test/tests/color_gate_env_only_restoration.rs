use serde_json::json;
use standout::cli::FnHandler;
use standout::cli::{App, Output};
use standout::EmbeddedTemplates;
use standout_test::TestHarness;

const TEMPLATES: &[(&str, &str)] = &[("say", "hello")];
#[test]
fn console_color_state_survives_a_run_with_no_color_knob() {
    if std::env::var_os("CLICOLOR_FORCE").is_some() || std::env::var_os("CLICOLOR").is_some() {
        return;
    }
    let before = console::Term::stdout().features().colors_supported();
    {
        let app = App::builder()
            .templates(EmbeddedTemplates::new(TEMPLATES, ""))
            .command_with(
                "say",
                FnHandler::new(|_m, _ctx| {
                    let _ = console::Style::new().red().apply_to("hello").to_string();
                    Ok(Output::Render(json!({})))
                }),
                |cfg| cfg,
            )
            .unwrap()
            .build()
            .unwrap();
        let cmd = clap::Command::new("app").subcommand(clap::Command::new("say"));
        let result = TestHarness::new()
            .env("CLICOLOR_FORCE", "1")
            .run(&app, cmd, ["app", "say"]);
        assert_eq!(result.stdout_plain().trim_end(), "hello");
    }
    assert_eq!(
        console::colors_enabled(),
        before,
        "a run with no color knob let its own temporary CLICOLOR_FORCE initialize \
         `console`'s color switch, and nothing restores it — every later test in this \
         binary would inherit it"
    );
}
