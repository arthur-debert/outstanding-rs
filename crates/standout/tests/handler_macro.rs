#![allow(non_snake_case)] // Generated handler names use __handler suffix

use clap::ArgMatches;
use standout::cli::handler::{
    CommandContext, Handler, NoEvents, Output, Results, RunRecorder, Summary,
};
use standout_macros::handler;

#[handler]
fn simple_flag(#[flag] verbose: bool) -> Result<bool, anyhow::Error> {
    Ok(verbose)
}

#[test]
fn test_simple_flag_true() {
    let matches = clap::Command::new("test")
        .arg(
            clap::Arg::new("verbose")
                .short('v')
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches_from(vec!["test", "-v"]);

    let ctx = CommandContext::default();
    let result = simple_flag__handler(&matches, &ctx);
    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[test]
fn test_simple_flag_false() {
    let matches = clap::Command::new("test")
        .arg(
            clap::Arg::new("verbose")
                .short('v')
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches_from(vec!["test"]);

    let ctx = CommandContext::default();
    let result = simple_flag__handler(&matches, &ctx);
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[handler]
fn flag_with_name(#[flag(name = "show-all")] all: bool) -> Result<bool, anyhow::Error> {
    Ok(all)
}

#[test]
fn test_flag_with_custom_name() {
    let matches = clap::Command::new("test")
        .arg(
            clap::Arg::new("show-all")
                .long("show-all")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches_from(vec!["test", "--show-all"]);

    let ctx = CommandContext::default();
    let result = flag_with_name__handler(&matches, &ctx);
    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[handler]
fn required_arg(#[arg] name: String) -> Result<String, anyhow::Error> {
    Ok(format!("Hello, {}!", name))
}

#[test]
fn test_required_arg() {
    let matches = clap::Command::new("test")
        .arg(clap::Arg::new("name").required(true))
        .get_matches_from(vec!["test", "world"]);

    let ctx = CommandContext::default();
    let result = required_arg__handler(&matches, &ctx);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Hello, world!");
}

#[handler]
fn optional_arg(#[arg] limit: Option<usize>) -> Result<String, anyhow::Error> {
    match limit {
        Some(n) => Ok(format!("Limit: {}", n)),
        None => Ok("No limit".to_string()),
    }
}

#[test]
fn test_optional_arg_present() {
    let matches = clap::Command::new("test")
        .arg(clap::Arg::new("limit").value_parser(clap::value_parser!(usize)))
        .get_matches_from(vec!["test", "10"]);

    let ctx = CommandContext::default();
    let result = optional_arg__handler(&matches, &ctx);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Limit: 10");
}

#[test]
fn test_optional_arg_missing() {
    let matches = clap::Command::new("test")
        .arg(clap::Arg::new("limit").value_parser(clap::value_parser!(usize)))
        .get_matches_from(vec!["test"]);

    let ctx = CommandContext::default();
    let result = optional_arg__handler(&matches, &ctx);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "No limit");
}

#[handler]
fn vec_arg(#[arg] tags: Vec<String>) -> Result<usize, anyhow::Error> {
    Ok(tags.len())
}

#[test]
fn test_vec_arg_multiple() {
    let matches = clap::Command::new("test")
        .arg(clap::Arg::new("tags").action(clap::ArgAction::Append))
        .get_matches_from(vec!["test", "foo", "bar", "baz"]);

    let ctx = CommandContext::default();
    let result = vec_arg__handler(&matches, &ctx);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 3);
}

#[test]
fn test_vec_arg_empty() {
    let matches = clap::Command::new("test")
        .arg(clap::Arg::new("tags").action(clap::ArgAction::Append))
        .get_matches_from(vec!["test"]);

    let ctx = CommandContext::default();
    let result = vec_arg__handler(&matches, &ctx);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 0);
}

#[handler]
fn arg_with_name(#[arg(name = "num")] count: usize) -> Result<usize, anyhow::Error> {
    Ok(count * 2)
}

#[test]
fn test_arg_with_custom_name() {
    let matches = clap::Command::new("test")
        .arg(
            clap::Arg::new("num")
                .required(true)
                .value_parser(clap::value_parser!(usize)),
        )
        .get_matches_from(vec!["test", "5"]);

    let ctx = CommandContext::default();
    let result = arg_with_name__handler(&matches, &ctx);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 10);
}

#[handler]
fn multiple_params(
    #[flag] verbose: bool,
    #[arg] name: String,
    #[arg] count: Option<usize>,
) -> Result<String, anyhow::Error> {
    let count_str = count.map(|c| c.to_string()).unwrap_or("none".to_string());
    Ok(format!(
        "verbose={}, name={}, count={}",
        verbose, name, count_str
    ))
}

#[test]
fn test_multiple_params() {
    let matches = clap::Command::new("test")
        .arg(
            clap::Arg::new("verbose")
                .short('v')
                .action(clap::ArgAction::SetTrue),
        )
        .arg(clap::Arg::new("name").required(true))
        .arg(clap::Arg::new("count").value_parser(clap::value_parser!(usize)))
        .get_matches_from(vec!["test", "-v", "alice", "42"]);

    let ctx = CommandContext::default();
    let result = multiple_params__handler(&matches, &ctx);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "verbose=true, name=alice, count=42");
}

#[handler]
fn with_context(#[ctx] ctx: &CommandContext) -> Result<usize, anyhow::Error> {
    Ok(ctx.command_path.len())
}

#[test]
fn test_with_context() {
    let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
    let ctx = CommandContext::default();

    let result = with_context__handler(&matches, &ctx);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 0);
}

#[handler]
fn with_matches(#[matches] m: &ArgMatches) -> Result<bool, anyhow::Error> {
    Ok(m.get_flag("verbose"))
}

#[test]
fn test_with_matches() {
    let matches = clap::Command::new("test")
        .arg(
            clap::Arg::new("verbose")
                .short('v')
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches_from(vec!["test", "-v"]);

    let ctx = CommandContext::default();
    let result = with_matches__handler(&matches, &ctx);
    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[handler]
fn silent_handler(#[arg] path: String) -> Result<(), anyhow::Error> {
    let _ = path;
    Ok(())
}

#[test]
fn test_silent_handler() {
    let matches = clap::Command::new("test")
        .arg(clap::Arg::new("path").required(true))
        .get_matches_from(vec!["test", "/tmp/foo"]);

    let ctx = CommandContext::default();
    let result = silent_handler__handler(&matches, &ctx);
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), Output::Silent));
}

#[test]
fn test_original_function_preserved() {
    let result = simple_flag(true);
    assert!(result.is_ok());
    assert!(result.unwrap());

    let result = required_arg("test".to_string());
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Hello, test!");
}

#[handler]
fn mixed_params(
    #[flag] verbose: bool,
    #[ctx] ctx: &CommandContext,
    #[arg] limit: Option<usize>,
) -> Result<String, anyhow::Error> {
    Ok(format!(
        "verbose={}, path_len={}, limit={:?}",
        verbose,
        ctx.command_path.len(),
        limit
    ))
}

#[test]
fn test_mixed_params() {
    let matches = clap::Command::new("test")
        .arg(
            clap::Arg::new("verbose")
                .short('v')
                .action(clap::ArgAction::SetTrue),
        )
        .arg(clap::Arg::new("limit").value_parser(clap::value_parser!(usize)))
        .get_matches_from(vec!["test", "-v", "5"]);
    let ctx = CommandContext::default();

    let result = mixed_params__handler(&matches, &ctx);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "verbose=true, path_len=0, limit=Some(5)");
}

use standout_dispatch::verify::{verify_handler_args, ArgKind, ExpectedArg};

#[test]
fn test_expected_args_generated_for_flag() {
    let expected = simple_flag__expected_args();
    assert_eq!(expected.len(), 1);
    assert_eq!(expected[0].cli_name, "verbose");
    assert_eq!(expected[0].rust_name, "verbose");
    assert_eq!(expected[0].kind, ArgKind::Flag);
}

#[test]
fn test_expected_args_generated_for_custom_name_flag() {
    let expected = flag_with_name__expected_args();
    assert_eq!(expected.len(), 1);
    assert_eq!(expected[0].cli_name, "show-all");
    assert_eq!(expected[0].rust_name, "all");
    assert_eq!(expected[0].kind, ArgKind::Flag);
}

#[test]
fn test_expected_args_generated_for_required_arg() {
    let expected = required_arg__expected_args();
    assert_eq!(expected.len(), 1);
    assert_eq!(expected[0].cli_name, "name");
    assert_eq!(expected[0].rust_name, "name");
    assert_eq!(expected[0].kind, ArgKind::RequiredArg);
}

#[test]
fn test_expected_args_generated_for_optional_arg() {
    let expected = optional_arg__expected_args();
    assert_eq!(expected.len(), 1);
    assert_eq!(expected[0].cli_name, "limit");
    assert_eq!(expected[0].rust_name, "limit");
    assert_eq!(expected[0].kind, ArgKind::OptionalArg);
}

#[test]
fn test_expected_args_generated_for_vec_arg() {
    let expected = vec_arg__expected_args();
    assert_eq!(expected.len(), 1);
    assert_eq!(expected[0].cli_name, "tags");
    assert_eq!(expected[0].rust_name, "tags");
    assert_eq!(expected[0].kind, ArgKind::VecArg);
}

#[test]
fn test_expected_args_excludes_ctx_and_matches() {
    let expected = mixed_params__expected_args();
    assert_eq!(expected.len(), 2);
    assert_eq!(expected[0].cli_name, "verbose");
    assert_eq!(expected[0].kind, ArgKind::Flag);
    assert_eq!(expected[1].cli_name, "limit");
    assert_eq!(expected[1].kind, ArgKind::OptionalArg);
}

#[test]
fn test_verification_passes_for_matching_command() {
    let command = clap::Command::new("test").arg(
        clap::Arg::new("verbose")
            .short('v')
            .action(clap::ArgAction::SetTrue),
    );

    let expected = simple_flag__expected_args();
    assert!(verify_handler_args(&command, "simple_flag", &expected).is_ok());
}

#[test]
fn test_verification_fails_for_missing_arg() {
    let command = clap::Command::new("test");

    let expected = simple_flag__expected_args();
    let err = verify_handler_args(&command, "simple_flag", &expected).unwrap_err();

    assert_eq!(err.handler_name, "simple_flag");
    assert_eq!(err.mismatches.len(), 1);
    let msg = err.to_string();
    assert!(msg.contains("verbose"));
    assert!(msg.contains("argument not defined"));
}

#[test]
fn test_verification_fails_for_wrong_action() {
    let command = clap::Command::new("test").arg(
        clap::Arg::new("verbose")
            .long("verbose")
            .action(clap::ArgAction::Set),
    );

    let expected = simple_flag__expected_args();
    let err = verify_handler_args(&command, "simple_flag", &expected).unwrap_err();

    assert_eq!(err.mismatches.len(), 1);
    let msg = err.to_string();
    assert!(msg.contains("boolean flag"));
    assert!(msg.contains("ArgAction::Set"));
    assert!(msg.contains("SetTrue"));
}

#[test]
fn test_verification_error_message_is_helpful() {
    let command = clap::Command::new("list").arg(
        clap::Arg::new("verbose")
            .long("verbose")
            .action(clap::ArgAction::Set),
    );

    let expected = vec![ExpectedArg::flag("verbose", "verbose")];
    let err = verify_handler_args(&command, "list_handler", &expected).unwrap_err();

    let msg = err.to_string();

    assert!(msg.contains("Handler `list_handler`"));
    assert!(msg.contains("command `list`"));

    assert!(msg.contains("Handler expects: boolean flag"));
    assert!(msg.contains("Command defines:"));

    assert!(msg.contains("Fix:"));
    assert!(msg.contains("ArgAction::SetTrue"));
}

#[derive(serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Step {
    Started { name: String },
}

#[handler]
fn incremental(
    #[flag] verbose: bool,
    results: &mut Results<Step>,
) -> Result<Summary<bool>, anyhow::Error> {
    results.emit(Step::Started {
        name: "one".to_string(),
    })?;
    Ok(Summary::Render(verbose))
}

fn verbose_matches(args: &[&str]) -> ArgMatches {
    clap::Command::new("test")
        .arg(
            clap::Arg::new("verbose")
                .short('v')
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches_from(args)
}

#[test]
fn a_results_parameter_makes_the_generated_handler_carry_the_event_type() {
    let matches = verbose_matches(&["test", "-v"]);
    let ctx = CommandContext::default();
    let recorder = RunRecorder::new();
    let mut results = Results::<Step>::recording(recorder.clone());

    let summary = incremental__handler(&matches, &ctx, &mut results).unwrap();

    assert!(matches!(summary, Summary::Render(true)));
    assert_eq!(
        recorder.records(),
        vec![serde_json::json!({ "type": "started", "name": "one" })]
    );
}

#[test]
fn the_generated_struct_handler_emits_through_the_channel_it_is_given() {
    let matches = verbose_matches(&["test"]);
    let ctx = CommandContext::default();
    let recorder = RunRecorder::new();
    let mut results = Results::recording(recorder.clone());

    let summary = incremental_Handler
        .handle(&matches, &ctx, &mut results)
        .unwrap();

    assert!(matches!(summary, Summary::Render(false)));
    assert_eq!(recorder.records().len(), 1);
}

// The signature docs/topics/incremental-commands.md shows. It writes
// `Result<Summary<T>, E>` out because the macro reads the return type by name,
// and neither `SummaryResult<T>` nor the batch `HandlerResult<T>` is `Result`.
#[handler]
fn apply(
    #[matches] _matches: &ArgMatches,
    #[ctx] _ctx: &CommandContext,
    results: &mut Results<Step>,
) -> Result<Summary<usize>, anyhow::Error> {
    results.emit(Step::Started {
        name: "web".to_string(),
    })?;
    Ok(Summary::Render(1))
}

#[test]
fn the_documented_incremental_signature_compiles_under_the_macro() {
    let matches = verbose_matches(&["test"]);
    let ctx = CommandContext::default();
    let recorder = RunRecorder::new();
    let mut results = Results::recording(recorder.clone());

    let summary = apply_Handler.handle(&matches, &ctx, &mut results).unwrap();

    assert!(matches!(summary, Summary::Render(1)));
    assert_eq!(recorder.records().len(), 1);
}

#[handler]
fn batch(#[flag] verbose: bool) -> Result<bool, anyhow::Error> {
    Ok(verbose)
}

#[test]
fn a_handler_without_a_results_parameter_declares_no_events() {
    fn assert_no_events<H: Handler<Event = NoEvents>>(_: &H) {}
    assert_no_events(&batch_Handler);
}

mod domain {
    #[derive(Debug, PartialEq, serde::Serialize)]
    pub struct Summary<T> {
        pub rows: Vec<T>,
    }
}

#[handler]
fn report(#[flag] verbose: bool) -> Result<domain::Summary<bool>, anyhow::Error> {
    Ok(domain::Summary {
        rows: vec![verbose],
    })
}

#[test]
fn a_batch_handler_keeps_an_application_type_named_summary_as_its_output() {
    fn assert_output<H: Handler<Output = domain::Summary<bool>>>(_: &H) {}
    assert_output(&report_Handler);

    let matches = verbose_matches(&["test", "-v"]);
    let ctx = CommandContext::default();
    let mut results = Results::<NoEvents>::recording(RunRecorder::new());

    let output = report_Handler.handle(&matches, &ctx, &mut results).unwrap();

    assert!(matches!(
        output,
        Output::Render(domain::Summary { ref rows }) if rows == &[true]
    ));
}
