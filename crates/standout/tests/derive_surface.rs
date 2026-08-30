//! The derive path a standout-only downstream sees, driven end to end.
//!
//! The fixture crate is the compile-time half (it depends on `standout` alone
//! and denies warnings); these run the app it wires.

use clap::Command;
use standout::cli::{App, CommandContext, GroupBuilder, HandlerResult, Output};
use standout::input::questionnaire::QuestionnaireInput;
use standout_fixtures::derive_surface::{app, command, Commands, ProvisionAnswers};
use standout_test::TestHarness;

fn answer_sheet(host: &str) -> String {
    ProvisionAnswers::questionnaire()
        .unwrap()
        .render_answer_sheet()
        .replace("\nlocalhost\n", &format!("\n{host}\n"))
}

#[test]
fn derive_registers_kebab_case_and_renamed_commands() {
    let builder = Commands::dispatch_config()(GroupBuilder::new());
    assert!(builder.contains("list-units"));
    assert!(!builder.contains("list_units"));
    assert!(builder.contains("about-this"));
    assert!(!builder.contains("about"));
    assert_eq!(builder.get_default_command(), Some("list-units"));
}

#[test]
fn handler_function_runs_under_the_derive() {
    let result =
        TestHarness::new()
            .text_output()
            .run(&app(), command(), ["unitctl", "list-units", "--all"]);
    result.assert_success();
    assert_eq!(result.stdout(), "ssh, cron");
}

#[test]
fn handler_function_runs_under_a_renamed_variant() {
    let result = TestHarness::new()
        .text_output()
        .run(&app(), command(), ["unitctl", "about-this"]);
    result.assert_success();
    assert_eq!(result.stdout(), "unitctl");
}

#[test]
fn handler_function_runs_under_a_silent_variant() {
    let result = TestHarness::new().run(&app(), command(), ["unitctl", "reload"]);
    result.assert_success();
    assert_eq!(result.stdout(), "");
}

#[test]
#[serial_test::serial(questionnaire)]
fn handler_function_drives_a_questionnaire_command() {
    let result = TestHarness::new()
        .text_output()
        .fixture("answers.txt", answer_sheet("db-1"))
        .run(
            &app(),
            command(),
            ["unitctl", "provision", "--answers", "answers.txt", "--yes"],
        );
    result.assert_success();
    assert_eq!(result.stdout(), "db-1:basic");
}

fn show_all(_matches: &clap::ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
    Ok(Output::Silent)
}

#[test]
fn a_command_registered_under_another_spelling_is_an_error() {
    let app = App::builder()
        .command_with("show_all", show_all, |cfg| cfg.silent())
        .unwrap()
        .build()
        .unwrap();
    let result = TestHarness::new().run(
        &app,
        Command::new("app").subcommand(Command::new("show-all")),
        ["app", "show-all"],
    );
    let error = result.stderr();
    assert!(error.contains("show-all"), "{error}");
    assert!(error.contains("show_all"), "{error}");
}
