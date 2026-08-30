use serde_json::json;
use standout::cli::{App, Output};
use standout_test::TestHarness;
#[test]
fn console_color_state_restores_to_the_pre_run_environment() {
    if std::env::var_os("CLICOLOR_FORCE").is_some() || std::env::var_os("CLICOLOR").is_some() {
        return;
    }
    let before = console::Term::stdout().features().colors_supported();
    {
        let app = App::builder()
            .command("say", |_m, _ctx| Ok(Output::Render(json!({}))), "hello")
            .unwrap()
            .build()
            .unwrap();
        let cmd = clap::Command::new("app").subcommand(clap::Command::new("say"));
        let result = TestHarness::new()
            .env("CLICOLOR_FORCE", "1")
            .no_color()
            .run(&app, cmd, ["app", "say"]);
        assert_eq!(result.stdout_plain().trim_end(), "hello");
    }
    assert_eq!(
        console::colors_enabled(),
        before,
        "the harness restored `console`'s color switch to a value its own temporary \
         CLICOLOR_FORCE produced, so every later test in this binary would inherit it"
    );
}
