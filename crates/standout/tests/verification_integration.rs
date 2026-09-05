#![allow(non_snake_case)] // Generated handler names use __handler suffix

use clap::{Arg, ArgAction, Command};
use serde::Serialize;
use standout::cli::{App, Output};
use standout::handler;

#[derive(Serialize)]
struct Empty;

#[handler]
#[allow(clippy::disallowed_names)] // tests below assert error messages reference the literal arg name "foo"
fn my_verified_handler(#[arg] foo: String) -> Result<standout::cli::Output<Empty>, anyhow::Error> {
    let _ = foo;
    Ok(Output::Render(Empty))
}

#[test]
fn test_verification_success() {
    let cmd_def =
        Command::new("app").subcommand(Command::new("test").arg(Arg::new("foo").required(true)));

    let app = App::builder()
        .command_with("test", my_verified_handler_Handler, |config| {
            config.structured_only()
        })
        .unwrap()
        .build()
        .unwrap();

    assert!(app.verify_command(&cmd_def).is_ok());
}

#[test]
fn test_verification_failure_missing_arg() {
    let cmd_def = Command::new("app").subcommand(Command::new("test"));

    let app = App::builder()
        .command_with("test", my_verified_handler_Handler, |config| {
            config.structured_only()
        })
        .unwrap()
        .build()
        .unwrap();

    let res = app.verify_command(&cmd_def);
    assert!(res.is_err());

    let err = res.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("verification failed"));
    assert!(msg.contains("foo"));
}

#[test]
fn test_verification_failure_wrong_type() {
    let cmd_def = Command::new("app")
        .subcommand(Command::new("test").arg(Arg::new("foo").action(clap::ArgAction::SetTrue)));

    let app = App::builder()
        .command_with("test", my_verified_handler_Handler, |config| {
            config.structured_only()
        })
        .unwrap()
        .build()
        .unwrap();

    let res = app.verify_command(&cmd_def);
    assert!(res.is_err());
    let msg = res.unwrap_err().to_string();
    assert!(msg.contains("verification failed"));
    assert!(msg.contains("foo"));
}

#[handler]
fn nested_handler(#[flag] verbose: bool) -> Result<standout::cli::Output<Empty>, anyhow::Error> {
    let _ = verbose;
    Ok(Output::Render(Empty))
}

#[test]
fn test_verification_nested_command_success() {
    let cmd_def = Command::new("app").subcommand(
        Command::new("db").subcommand(
            Command::new("migrate").arg(
                Arg::new("verbose")
                    .long("verbose")
                    .action(ArgAction::SetTrue),
            ),
        ),
    );

    let app = App::builder()
        .command_with("db.migrate", nested_handler_Handler, |config| {
            config.structured_only()
        })
        .unwrap()
        .build()
        .unwrap();

    assert!(app.verify_command(&cmd_def).is_ok());
}

#[test]
fn test_verification_nested_command_failure() {
    let cmd_def =
        Command::new("app").subcommand(Command::new("db").subcommand(Command::new("migrate")));

    let app = App::builder()
        .command_with("db.migrate", nested_handler_Handler, |config| {
            config.structured_only()
        })
        .unwrap()
        .build()
        .unwrap();

    let res = app.verify_command(&cmd_def);
    assert!(res.is_err());

    let msg = res.unwrap_err().to_string();
    assert!(msg.contains("verification failed"));
    assert!(msg.contains("verbose"));
}

#[test]
fn test_verification_preserves_structured_error() {
    let cmd_def = Command::new("app").subcommand(Command::new("test"));

    let app = App::builder()
        .command_with("test", my_verified_handler_Handler, |config| {
            config.structured_only()
        })
        .unwrap()
        .build()
        .unwrap();

    let err = app.verify_command(&cmd_def).unwrap_err();

    match err {
        standout::SetupError::VerificationFailed(mismatch_err) => {
            assert_eq!(mismatch_err.handler_name, "test");
            assert!(!mismatch_err.mismatches.is_empty());
        }
        _ => panic!("Expected VerificationFailed variant"),
    }
}

#[handler]
fn scoped_handler(
    #[arg] scope: Option<String>,
) -> Result<standout::cli::Output<Empty>, anyhow::Error> {
    let _ = scope;
    Ok(Output::Render(Empty))
}

#[test]
fn test_verification_reads_a_parent_global_arg() {
    let cmd_def = Command::new("app")
        .arg(Arg::new("scope").long("scope").global(true))
        .subcommand(Command::new("config").subcommand(Command::new("list")));

    let app = App::builder()
        .command_with("config.list", scoped_handler_Handler, |config| {
            config.structured_only()
        })
        .unwrap()
        .build()
        .unwrap();

    assert!(app.verify_command(&cmd_def).is_ok());
}

#[handler]
fn help_topic_handler() -> Result<standout::cli::Output<Empty>, anyhow::Error> {
    Ok(Output::Render(Empty))
}

#[test]
fn test_verification_rejects_a_registration_on_claps_generated_help() {
    let cmd_def = Command::new("app").subcommand(Command::new("run"));

    let app = App::builder()
        .help_handling(false)
        .command_with("help", help_topic_handler_Handler, |config| {
            config.structured_only()
        })
        .unwrap()
        .build()
        .unwrap();

    let msg = app.verify_command(&cmd_def).unwrap_err().to_string();
    assert!(msg.contains("No invocation reaches `help`"), "{msg}");
}

#[handler]
fn generated_names_handler(
    #[arg] help: Option<String>,
    #[arg] version: Option<String>,
) -> Result<standout::cli::Output<Empty>, anyhow::Error> {
    let _ = (help, version);
    Ok(Output::Render(Empty))
}

#[test]
fn test_verification_rejects_handler_args_named_after_claps_generated_flags() {
    let cmd_def = Command::new("app").version("1.0");

    let app = App::builder()
        .version("1.0")
        .command_with("", generated_names_handler_Handler, |config| {
            config.structured_only()
        })
        .unwrap()
        .build()
        .unwrap();

    let msg = app.verify_command(&cmd_def).unwrap_err().to_string();
    assert!(msg.contains("verification failed"), "{msg}");
    assert!(msg.contains("`help`"), "{msg}");
    assert!(msg.contains("`version`"), "{msg}");
}

#[test]
fn test_verification_reads_a_parent_global_arg_on_a_prebuilt_command() {
    let mut cmd_def = Command::new("app")
        .arg(Arg::new("scope").long("scope").global(true))
        .subcommand(Command::new("config").subcommand(Command::new("list")));
    cmd_def.build();

    let app = App::builder()
        .command_with("config.list", scoped_handler_Handler, |config| {
            config.structured_only()
        })
        .unwrap()
        .build()
        .unwrap();

    assert!(app.verify_command(&cmd_def).is_ok());
}

fn scoped_app() -> App {
    App::builder()
        .command_with("config.list", scoped_handler_Handler, |config| {
            config.structured_only()
        })
        .unwrap()
        .build()
        .unwrap()
}

fn verified_declared_and_clap_built(cmd: Command) -> Result<(), standout::SetupError> {
    let mut clap_built = cmd.clone();
    clap_built.build();

    let app = scoped_app();
    let declared = app.verify_command(&cmd);
    let built = app.verify_command(&clap_built);
    assert_eq!(
        declared.is_err(),
        built.is_err(),
        "declared: {declared:?}, clap-built: {built:?}"
    );
    declared
}

#[test]
fn propagation_matches_clap_for_a_global_on_an_intermediate_command() {
    let cmd = Command::new("app").subcommand(
        Command::new("config")
            .arg(Arg::new("scope").long("scope").global(true))
            .subcommand(Command::new("list")),
    );

    assert!(verified_declared_and_clap_built(cmd).is_ok());
}

#[test]
fn propagation_matches_clap_when_a_subcommand_redeclares_an_ancestor_global() {
    let cmd =
        Command::new("app")
            .arg(Arg::new("scope").long("scope").global(true))
            .subcommand(Command::new("config").subcommand(
                Command::new("list").arg(Arg::new("scope").long("scope").required(true)),
            ));

    assert!(verified_declared_and_clap_built(cmd).is_err());
}

#[test]
fn propagation_matches_clap_for_an_aliased_global() {
    let cmd = Command::new("app")
        .arg(
            Arg::new("scope")
                .long("scope-name")
                .alias("scope")
                .global(true),
        )
        .subcommand(Command::new("config").subcommand(Command::new("list")));

    assert!(verified_declared_and_clap_built(cmd).is_ok());
}
