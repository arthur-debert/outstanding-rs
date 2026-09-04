//! The derive path a standout-only downstream sees, driven end to end.
//!
//! The fixture crate is the compile-time half (it depends on `standout` alone
//! and denies warnings); these run the app it wires.

use clap::Command;
use standout::cli::FnHandler;
use standout::cli::{App, CommandContext, GroupBuilder, HandlerResult, Output};
use standout::input::questionnaire::QuestionnaireInput;
use standout::ColorPolicy;
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
    let result = TestHarness::new().color(ColorPolicy::Never).run(
        &app(),
        command(),
        ["unitctl", "list-units", "--all"],
    );
    result.assert_success();
    assert_eq!(result.stdout(), "ssh, cron");
}

#[test]
fn handler_function_runs_under_a_renamed_variant() {
    let result = TestHarness::new().color(ColorPolicy::Never).run(
        &app(),
        command(),
        ["unitctl", "about-this"],
    );
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
        .color(ColorPolicy::Never)
        .fixture("answers.txt", answer_sheet("db-1"))
        .run(
            &app(),
            command(),
            ["unitctl", "provision", "--answers", "answers.txt", "--yes"],
        );
    result.assert_success();
    assert_eq!(result.stdout(), "db-1:basic");
}

#[test]
fn handler_function_runs_under_a_keyword_named_variant() {
    let builder = Commands::dispatch_config()(GroupBuilder::new());
    assert!(builder.contains("move"));

    let result = TestHarness::new().color(ColorPolicy::Never).run(
        &app(),
        command(),
        ["unitctl", "move", "--type"],
    );
    result.assert_success();
    assert_eq!(result.stdout(), "typed");
}

fn show_all(_matches: &clap::ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
    Ok(Output::Silent)
}

#[test]
fn a_command_registered_under_another_spelling_is_an_error() {
    let app = App::builder()
        .command_with("show_all", FnHandler::new(show_all), |cfg| cfg.silent())
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

#[test]
fn a_registration_the_cli_never_declares_is_an_error() {
    let app = App::builder()
        .command_with("provision", FnHandler::new(show_all), |cfg| cfg.silent())
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app").subcommand(Command::new("prepare"));
    let result = TestHarness::new().run(&app, cmd.clone(), ["app", "prepare"]);
    let error = result.stderr();
    assert!(error.contains("provision"), "{error}");
    assert!(
        App::builder()
            .command_with("provision", FnHandler::new(show_all), |cfg| cfg.silent())
            .unwrap()
            .build()
            .unwrap()
            .verify_command(&cmd)
            .is_err(),
        "verify_command should report the same unreachable registration"
    );
}

#[test]
fn a_flat_app_registers_the_root_command_reachably() {
    let app = App::builder()
        .command_with("", FnHandler::new(show_all), |cfg| cfg.silent())
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app");
    assert!(app.verify_command(&cmd).is_ok());
    TestHarness::new().run(&app, cmd, ["app"]).assert_success();
}

#[test]
fn a_registration_path_with_a_blank_command_name_is_an_error() {
    for path in ["list.", ".list", "parent..child"] {
        let app = App::builder()
            .command_with(path, FnHandler::new(show_all), |cfg| cfg.silent())
            .unwrap()
            .build()
            .unwrap();
        let cmd = Command::new("app").subcommand(
            Command::new("parent")
                .subcommand(Command::new("child"))
                .subcommand(Command::new("list")),
        );
        let error = app.verify_command(&cmd).unwrap_err().to_string();
        assert!(
            error.contains(&format!(
                "Registration path `{path}` has a blank command name"
            )),
            "{path}: {error}"
        );

        let result = TestHarness::new().run(&app, cmd, ["app", "parent", "child"]);
        assert!(
            result.stderr().contains(path),
            "{path}: {}",
            result.stderr()
        );
    }
}

#[test]
fn a_cli_command_with_no_registration_still_hands_off() {
    let app = App::builder()
        .command_with("list-units", FnHandler::new(show_all), |cfg| cfg.silent())
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app")
        .subcommand(Command::new("list-units"))
        .subcommand(Command::new("legacy"));
    assert!(app.verify_command(&cmd).is_ok());
    assert!(matches!(
        app.run_with(
            cmd,
            ["app", "legacy"],
            standout::TargetProperties::detect(),
            standout::InputSources::from_process()
        )
        .into_outcome(),
        standout::cli::DispatchResult::NoMatch(_)
    ));
}

#[test]
fn a_command_registered_under_a_clap_alias_is_an_error() {
    let cmd = Command::new("app").subcommand(Command::new("list").alias("ls"));
    let app = App::builder()
        .command_with("ls", FnHandler::new(show_all), |cfg| cfg.silent())
        .unwrap()
        .build()
        .unwrap();
    let error = app.verify_command(&cmd).unwrap_err().to_string();
    assert!(error.contains("No invocation reaches `ls`"), "{error}");
    assert!(
        error.contains("The CLI declares `list` and accepts `ls` as an alias for it"),
        "{error}"
    );
    assert!(
        error.contains("register the handler under `list`"),
        "{error}"
    );

    let result = TestHarness::new().run(&app, cmd, ["app", "ls"]);
    assert!(result.stderr().contains("ls"), "{}", result.stderr());
}

#[test]
fn an_alias_invokes_the_handler_registered_under_the_declared_name() {
    let cmd = Command::new("app").subcommand(Command::new("list").alias("ls"));
    let app = App::builder()
        .command_with("list", FnHandler::new(show_all), |cfg| cfg.silent())
        .unwrap()
        .build()
        .unwrap();
    assert!(app.verify_command(&cmd).is_ok());
    TestHarness::new()
        .run(&app, cmd.clone(), ["app", "ls"])
        .assert_success();
    TestHarness::new()
        .run(&app, cmd, ["app", "list"])
        .assert_success();
}
