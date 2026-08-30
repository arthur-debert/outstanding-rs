use clap::{Arg, ArgAction, Command};
use standout_dispatch::verify::{verify_handler_args, ExpectedArg};

#[test]
fn test_repro_default_value_false_positive() {
    let command = Command::new("test").arg(Arg::new("mode").long("mode").default_value("fast"));

    let expected = vec![ExpectedArg::required_arg("mode", "mode")];

    let result = verify_handler_args(&command, "handler", &expected);
    assert!(
        result.is_ok(),
        "Verification failed: {}",
        result.unwrap_err()
    );
}

#[test]
fn test_repro_count_false_positive() {
    let command = Command::new("test").arg(Arg::new("verbose").short('v').action(ArgAction::Count));

    let expected = vec![ExpectedArg::required_arg("verbose", "verbose")];

    let result = verify_handler_args(&command, "handler", &expected);
    assert!(
        result.is_ok(),
        "Verification failed: {}",
        result.unwrap_err()
    );
}
