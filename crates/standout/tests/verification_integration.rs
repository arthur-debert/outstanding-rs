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
        .command_handler_with("test", my_verified_handler_Handler, |config| {
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
        .command_handler_with("test", my_verified_handler_Handler, |config| {
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
        .command_handler_with("test", my_verified_handler_Handler, |config| {
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
        .command_handler_with("db.migrate", nested_handler_Handler, |config| {
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
        .command_handler_with("db.migrate", nested_handler_Handler, |config| {
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
        .command_handler_with("test", my_verified_handler_Handler, |config| {
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
