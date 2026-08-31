use clap::Command;
use serde_json::json;
use standout::cli::FnHandler;
use standout::cli::{App, ExitStatus, HelpResult, Output, RunErrorKind, SuccessKind};
use standout::EmbeddedTemplates;

const TEMPLATES: &[(&str, &str)] = &[("go", "{{ message }}")];

fn versionless_command() -> Command {
    Command::new("app").subcommand(Command::new("go"))
}

fn app_with(version: Option<&str>) -> App {
    let builder = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "go",
            FnHandler::new(|_matches, _ctx| Ok(Output::Render(json!({ "message": "ok" })))),
            |cfg| cfg,
        )
        .unwrap();
    match version {
        Some(version) => builder.version(version),
        None => builder,
    }
    .build()
    .unwrap()
}

fn app_with_owned_version(version: String) -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "go",
            FnHandler::new(|_matches, _ctx| Ok(Output::Render(json!({ "message": "ok" })))),
            |cfg| cfg,
        )
        .unwrap()
        .version(version)
        .build()
        .unwrap()
}

#[test]
fn a_configured_version_is_answered_as_a_clap_display_success() {
    let result = app_with(Some("9.9.9")).run_with(
        versionless_command(),
        ["app", "--version"],
        standout::TargetProperties::detect(),
        standout::InputSources::from_process(),
    );

    assert_eq!(result.exit_status(), Some(ExitStatus::SUCCESS));
    assert_eq!(result.success_kind(), Some(SuccessKind::ClapVersion));
    assert_eq!(result.output().unwrap().trim(), "app 9.9.9");
}

#[test]
fn an_owned_string_version_is_accepted() {
    let result = app_with_owned_version(String::from("8.8.8")).run_with(
        versionless_command(),
        ["app", "--version"],
        standout::TargetProperties::detect(),
        standout::InputSources::from_process(),
    );

    assert_eq!(result.exit_status(), Some(ExitStatus::SUCCESS));
    assert_eq!(result.success_kind(), Some(SuccessKind::ClapVersion));
    assert_eq!(result.output().unwrap().trim(), "app 8.8.8");
}

#[test]
fn the_parse_only_path_sees_the_same_version() {
    let result = app_with(Some("9.9.9")).get_matches_from(
        versionless_command(),
        ["app", "--version"],
        &standout::InputSources::from_process(),
    );

    match result {
        HelpResult::Error(error) => {
            assert_eq!(error.kind(), clap::error::ErrorKind::DisplayVersion);
            assert!(
                error.to_string().contains("9.9.9"),
                "unexpected version display: {error}"
            );
        }
        other => panic!("expected clap's version display, got {other:?}"),
    }
}

#[test]
fn an_unset_version_leaves_a_version_configured_on_the_command() {
    let command = versionless_command().version("1.2.3");
    let result = app_with(None).run_with(
        command,
        ["app", "--version"],
        standout::TargetProperties::detect(),
        standout::InputSources::from_process(),
    );

    assert_eq!(result.success_kind(), Some(SuccessKind::ClapVersion));
    assert_eq!(result.output().unwrap().trim(), "app 1.2.3");
}

#[test]
fn an_unset_version_leaves_a_versionless_command_versionless() {
    let result = app_with(None).run_with(
        versionless_command(),
        ["app", "--version"],
        standout::TargetProperties::detect(),
        standout::InputSources::from_process(),
    );

    assert_eq!(result.error_kind(), Some(RunErrorKind::ClapUsage));
    assert!(
        result.error().unwrap().contains("unexpected argument"),
        "a versionless command has no --version: {}",
        result.error().unwrap()
    );
}

#[test]
fn a_configured_version_wins_over_one_set_on_the_command() {
    let command = versionless_command().version("1.2.3");
    let result = app_with(Some("9.9.9")).run_with(
        command,
        ["app", "--version"],
        standout::TargetProperties::detect(),
        standout::InputSources::from_process(),
    );

    assert_eq!(result.output().unwrap().trim(), "app 9.9.9");
}

#[test]
fn a_configured_version_leaves_dispatch_alone() {
    let result = app_with(Some("9.9.9")).run_with(
        versionless_command(),
        ["app", "go"],
        standout::TargetProperties::detect(),
        standout::InputSources::from_process(),
    );

    assert_eq!(result.exit_status(), Some(ExitStatus::SUCCESS));
    assert_eq!(result.output(), Some("ok"));
}
