//! Integration tests for the #[handler] proc macro.

#![allow(non_snake_case)] // Generated handler names use __handler suffix

use clap::ArgMatches;
use standout::cli::handler::{CommandContext, Output};
use standout_macros::handler;

// =============================================================================
// Basic flag extraction
// =============================================================================

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

// =============================================================================
// Flag with custom name
// =============================================================================

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

// =============================================================================
// Required argument
// =============================================================================

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

// =============================================================================
// Optional argument
// =============================================================================

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

// =============================================================================
// Vec argument
// =============================================================================

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

// =============================================================================
// Argument with custom name
// =============================================================================

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

// =============================================================================
// Multiple parameters
// =============================================================================

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

// =============================================================================
// Context access
// =============================================================================

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

// =============================================================================
// Raw matches access
// =============================================================================

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

// =============================================================================
// Unit result (silent output)
// =============================================================================

#[handler]
fn silent_handler(#[arg] path: String) -> Result<(), anyhow::Error> {
    // In real code, this would do something with path
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

// =============================================================================
// Original function is preserved for direct testing
// =============================================================================

#[test]
fn test_original_function_preserved() {
    // Can call the original function directly
    let result = simple_flag(true);
    assert!(result.is_ok());
    assert!(result.unwrap());

    let result = required_arg("test".to_string());
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Hello, test!");
}

// =============================================================================
// Mixed context and args
// =============================================================================

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
