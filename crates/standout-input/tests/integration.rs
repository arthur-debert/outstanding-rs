use clap::{Arg, Command};
use standout_input::{
    ArgSource, ClipboardSource, EnvSource, FlagSource, InputChain, InputError, InputSourceKind,
    MockClipboard, MockEnv, MockStdin, StdinSource,
};

fn create_test_command() -> Command {
    Command::new("test")
        .arg(Arg::new("message").long("message").short('m'))
        .arg(Arg::new("body").long("body").short('b'))
        .arg(
            Arg::new("yes")
                .long("yes")
                .short('y')
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-editor")
                .long("no-editor")
                .action(clap::ArgAction::SetTrue),
        )
}

#[test]
fn gh_pattern_arg_provided() {
    let matches = create_test_command()
        .try_get_matches_from(["test", "--body", "from argument"])
        .unwrap();

    let chain = InputChain::<String>::new()
        .try_source(ArgSource::new("body"))
        .try_source(StdinSource::with_reader(MockStdin::terminal()))
        .default("from default".to_string());

    let result = chain.resolve_with_source(&matches).unwrap();
    assert_eq!(result.value, "from argument");
    assert_eq!(result.source, InputSourceKind::Arg);
}

#[test]
fn gh_pattern_stdin_piped() {
    let matches = create_test_command()
        .try_get_matches_from(["test"])
        .unwrap();

    let chain = InputChain::<String>::new()
        .try_source(ArgSource::new("body"))
        .try_source(StdinSource::with_reader(MockStdin::piped("from stdin")))
        .default("from default".to_string());

    let result = chain.resolve_with_source(&matches).unwrap();
    assert_eq!(result.value, "from stdin");
    assert_eq!(result.source, InputSourceKind::Stdin);
}

#[test]
fn gh_pattern_falls_through_to_default() {
    let matches = create_test_command()
        .try_get_matches_from(["test"])
        .unwrap();

    let chain = InputChain::<String>::new()
        .try_source(ArgSource::new("body"))
        .try_source(StdinSource::with_reader(MockStdin::terminal()))
        .default("from default".to_string());

    let result = chain.resolve_with_source(&matches).unwrap();
    assert_eq!(result.value, "from default");
    assert_eq!(result.source, InputSourceKind::Default);
}

#[test]
fn confirmation_with_yes_flag() {
    let matches = create_test_command()
        .try_get_matches_from(["test", "--yes"])
        .unwrap();

    let chain = InputChain::<bool>::new()
        .try_source(FlagSource::new("yes"))
        .default(false);

    let result = chain.resolve(&matches).unwrap();
    assert!(result);
}

#[test]
fn confirmation_without_flag_uses_default() {
    let matches = create_test_command()
        .try_get_matches_from(["test"])
        .unwrap();

    let chain = InputChain::<bool>::new()
        .try_source(FlagSource::new("yes"))
        .default(false);

    let result = chain.resolve(&matches).unwrap();
    assert!(!result);
}

#[test]
fn inverted_flag_for_no_editor() {
    let matches = create_test_command()
        .try_get_matches_from(["test", "--no-editor"])
        .unwrap();

    let chain = InputChain::<bool>::new()
        .try_source(FlagSource::new("no-editor").inverted())
        .default(true);

    let result = chain.resolve(&matches).unwrap();
    assert!(!result);
}

#[test]
fn env_var_priority_over_default() {
    let matches = create_test_command()
        .try_get_matches_from(["test"])
        .unwrap();

    let env = MockEnv::new().with_var("MY_TOKEN", "secret-from-env");

    let chain = InputChain::<String>::new()
        .try_source(ArgSource::new("message"))
        .try_source(EnvSource::with_reader("MY_TOKEN", env))
        .default("no-token".to_string());

    let result = chain.resolve_with_source(&matches).unwrap();
    assert_eq!(result.value, "secret-from-env");
    assert_eq!(result.source, InputSourceKind::Env);
}

#[test]
fn arg_overrides_env_var() {
    let matches = create_test_command()
        .try_get_matches_from(["test", "--message", "from-arg"])
        .unwrap();

    let env = MockEnv::new().with_var("MY_TOKEN", "secret-from-env");

    let chain = InputChain::<String>::new()
        .try_source(ArgSource::new("message"))
        .try_source(EnvSource::with_reader("MY_TOKEN", env))
        .default("no-token".to_string());

    let result = chain.resolve_with_source(&matches).unwrap();
    assert_eq!(result.value, "from-arg");
    assert_eq!(result.source, InputSourceKind::Arg);
}

#[test]
fn clipboard_as_fallback() {
    let matches = create_test_command()
        .try_get_matches_from(["test"])
        .unwrap();

    let chain = InputChain::<String>::new()
        .try_source(ArgSource::new("message"))
        .try_source(StdinSource::with_reader(MockStdin::terminal()))
        .try_source(ClipboardSource::with_reader(MockClipboard::with_content(
            "clipboard content",
        )));

    let result = chain.resolve_with_source(&matches).unwrap();
    assert_eq!(result.value, "clipboard content");
    assert_eq!(result.source, InputSourceKind::Clipboard);
}

#[test]
fn empty_clipboard_falls_through() {
    let matches = create_test_command()
        .try_get_matches_from(["test"])
        .unwrap();

    let chain = InputChain::<String>::new()
        .try_source(ArgSource::new("message"))
        .try_source(ClipboardSource::with_reader(MockClipboard::empty()))
        .default("fallback".to_string());

    let result = chain.resolve_with_source(&matches).unwrap();
    assert_eq!(result.value, "fallback");
    assert_eq!(result.source, InputSourceKind::Default);
}

#[test]
fn validation_passes() {
    let matches = create_test_command()
        .try_get_matches_from(["test", "--message", "user@example.com"])
        .unwrap();

    let chain = InputChain::<String>::new()
        .try_source(ArgSource::new("message"))
        .validate(|s| s.contains('@'), "Must be an email");

    let result = chain.resolve(&matches);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "user@example.com");
}

#[test]
fn validation_fails_with_error() {
    let matches = create_test_command()
        .try_get_matches_from(["test", "--message", "not-an-email"])
        .unwrap();

    let chain = InputChain::<String>::new()
        .try_source(ArgSource::new("message"))
        .validate(|s| s.contains('@'), "Must be an email");

    let result = chain.resolve(&matches);
    assert!(matches!(result, Err(InputError::ValidationFailed(_))));
}

#[test]
fn multiple_validations_all_pass() {
    let matches = create_test_command()
        .try_get_matches_from(["test", "--message", "hello@world.com"])
        .unwrap();

    let chain = InputChain::<String>::new()
        .try_source(ArgSource::new("message"))
        .validate(|s| !s.is_empty(), "Cannot be empty")
        .validate(|s| s.contains('@'), "Must contain @")
        .validate(|s| s.len() >= 5, "Must be at least 5 chars");

    let result = chain.resolve(&matches);
    assert!(result.is_ok());
}

#[test]
fn multiple_validations_first_fails() {
    let matches = create_test_command()
        .try_get_matches_from(["test", "--message", ""])
        .unwrap();

    let chain = InputChain::<String>::new()
        .try_source(ArgSource::new("message"))
        .try_source(StdinSource::with_reader(MockStdin::piped("")))
        .default("".to_string())
        .validate(|s| !s.is_empty(), "Cannot be empty");

    let result = chain.resolve(&matches);
    assert!(matches!(result, Err(InputError::ValidationFailed(_))));
}

#[test]
fn no_input_returns_error() {
    let matches = create_test_command()
        .try_get_matches_from(["test"])
        .unwrap();

    let chain = InputChain::<String>::new()
        .try_source(ArgSource::new("message"))
        .try_source(StdinSource::with_reader(MockStdin::terminal()))
        .try_source(EnvSource::with_reader("MISSING", MockEnv::new()));

    let result = chain.resolve(&matches);
    assert!(matches!(result, Err(InputError::NoInput)));
}

#[test]
fn complex_chain_priority() {
    let matches = create_test_command()
        .try_get_matches_from(["test", "--message", "from-arg"])
        .unwrap();

    let chain = build_complex_chain("env-value", "clipboard-value");
    assert_eq!(chain.resolve(&matches).unwrap(), "from-arg");

    let matches = create_test_command()
        .try_get_matches_from(["test"])
        .unwrap();

    let chain = InputChain::<String>::new()
        .try_source(ArgSource::new("message"))
        .try_source(StdinSource::with_reader(MockStdin::piped("from-stdin")))
        .try_source(EnvSource::with_reader(
            "MY_VAR",
            MockEnv::new().with_var("MY_VAR", "env-value"),
        ))
        .try_source(ClipboardSource::with_reader(MockClipboard::with_content(
            "clipboard-value",
        )))
        .default("default-value".to_string());

    assert_eq!(chain.resolve(&matches).unwrap(), "from-stdin");

    let chain = InputChain::<String>::new()
        .try_source(ArgSource::new("message"))
        .try_source(StdinSource::with_reader(MockStdin::terminal()))
        .try_source(EnvSource::with_reader(
            "MY_VAR",
            MockEnv::new().with_var("MY_VAR", "env-value"),
        ))
        .try_source(ClipboardSource::with_reader(MockClipboard::with_content(
            "clipboard-value",
        )))
        .default("default-value".to_string());

    assert_eq!(chain.resolve(&matches).unwrap(), "env-value");

    let chain = InputChain::<String>::new()
        .try_source(ArgSource::new("message"))
        .try_source(StdinSource::with_reader(MockStdin::terminal()))
        .try_source(EnvSource::with_reader("MY_VAR", MockEnv::new()))
        .try_source(ClipboardSource::with_reader(MockClipboard::with_content(
            "clipboard-value",
        )))
        .default("default-value".to_string());

    assert_eq!(chain.resolve(&matches).unwrap(), "clipboard-value");

    let chain = InputChain::<String>::new()
        .try_source(ArgSource::new("message"))
        .try_source(StdinSource::with_reader(MockStdin::terminal()))
        .try_source(EnvSource::with_reader("MY_VAR", MockEnv::new()))
        .try_source(ClipboardSource::with_reader(MockClipboard::empty()))
        .default("default-value".to_string());

    assert_eq!(chain.resolve(&matches).unwrap(), "default-value");
}

fn build_complex_chain(env_value: &str, clipboard_value: &str) -> InputChain<String> {
    InputChain::<String>::new()
        .try_source(ArgSource::new("message"))
        .try_source(StdinSource::with_reader(MockStdin::terminal()))
        .try_source(EnvSource::with_reader(
            "MY_VAR",
            MockEnv::new().with_var("MY_VAR", env_value),
        ))
        .try_source(ClipboardSource::with_reader(MockClipboard::with_content(
            clipboard_value,
        )))
        .default("default-value".to_string())
}

#[test]
fn mock_ensures_consistent_behavior_in_ci() {
    let matches = create_test_command()
        .try_get_matches_from(["test"])
        .unwrap();

    let ci_stdin = MockStdin::terminal();
    let chain = InputChain::<String>::new()
        .try_source(StdinSource::with_reader(ci_stdin))
        .default("ci-default".to_string());

    assert_eq!(chain.resolve(&matches).unwrap(), "ci-default");

    let piped_stdin = MockStdin::piped("piped-content");
    let chain = InputChain::<String>::new()
        .try_source(StdinSource::with_reader(piped_stdin))
        .default("ci-default".to_string());

    assert_eq!(chain.resolve(&matches).unwrap(), "piped-content");
}

#[test]
fn mock_stdin_preserves_whitespace_when_configured() {
    let matches = create_test_command()
        .try_get_matches_from(["test"])
        .unwrap();

    let chain = InputChain::<String>::new()
        .try_source(StdinSource::with_reader(MockStdin::piped("  hello  \n")));
    assert_eq!(chain.resolve(&matches).unwrap(), "hello");

    let chain = InputChain::<String>::new()
        .try_source(StdinSource::with_reader(MockStdin::piped("  hello  \n")).trim(false));
    assert_eq!(chain.resolve(&matches).unwrap(), "  hello  \n");
}
