use clap::{ArgMatches, Command};
use clapfig::{Clapfig, SearchPath};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use standout::cli::{
    App, CommandContext, CommandContextInput, FnHandler, HandlerResult, Output, RunErrorKind,
};
use standout::Representation;
use standout_test::{serial, TestHarness, TestResult};

/// A window-title sequence: the shape an archive entry uses to paint a terminal.
const PAINT: &str = "\u{1b}]0;pwned\u{7}";

const ESCAPED: &str = "\\u{1b}]0;pwned\\u{7}";

#[derive(Debug, Clone, Serialize, Deserialize, clapfig::Schema)]
struct ArchiveConfig {
    #[clapfig(default = "/archives")]
    root: String,
}

fn app() -> App {
    App::builder()
        .command_with(
            "warn",
            FnHandler::new(
                |_: &ArgMatches, ctx: &CommandContext| -> HandlerResult<Value> {
                    ctx.warn(format!("skipped entry {PAINT}"));
                    Ok(Output::Silent)
                },
            ),
            |cfg| cfg.silent(),
        )
        .unwrap()
        .command_with(
            "read",
            FnHandler::new(
                |_: &ArgMatches, _: &CommandContext| -> HandlerResult<Value> {
                    Err(anyhow::anyhow!("cannot read entry {PAINT}"))
                },
            ),
            |cfg| cfg.silent(),
        )
        .unwrap()
        .config(
            Clapfig::typed::<ArchiveConfig>()
                .app_name("archiver")
                .search_paths(vec![SearchPath::Cwd]),
        )
        .config_override_flag("set")
        .build()
        .unwrap()
}

fn command() -> Command {
    Command::new("archiver")
        .subcommand(Command::new("warn"))
        .subcommand(Command::new("read"))
}

fn run(args: &[&str]) -> TestResult {
    TestHarness::new().run(&app(), command(), args)
}

#[test]
#[serial]
fn a_handler_diagnostic_reaches_stderr_with_its_escape_sequence_defused() {
    let result = run(&["archiver", "read"]);
    result.assert_error_kind(RunErrorKind::Handler);
    assert!(!result.stderr().contains('\u{1b}'), "{:?}", result.stderr());
    result.assert_stderr_contains(&format!("cannot read entry {ESCAPED}"));
}

#[test]
#[serial]
fn the_diagnostic_document_carries_the_escaped_summary_too() {
    let result = run(&["archiver", "read", "--output", "json"]);
    assert_eq!(result.output_mode(), Representation::Json);
    assert_eq!(
        result.expect_diagnostic().summary,
        format!("cannot read entry {ESCAPED}")
    );
}

#[test]
#[serial]
fn a_warning_reaches_stderr_with_its_escape_sequence_defused() {
    let result = run(&["archiver", "warn"]);
    result.assert_success();
    assert_eq!(result.warnings(), [format!("skipped entry {ESCAPED}")]);
    assert!(!result.stderr().contains('\u{1b}'), "{:?}", result.stderr());
    result.assert_stderr_contains(&format!("skipped entry {ESCAPED}"));
}

#[test]
#[serial]
fn a_styled_warning_block_keeps_its_own_ansi_around_the_escaped_text() {
    let result =
        TestHarness::new()
            .color_capable_terminal()
            .run(&app(), command(), ["archiver", "warn"]);
    result.assert_success();
    assert!(result.stderr().contains("\u{1b}["), "{:?}", result.stderr());
    assert!(
        !result.stderr().contains("\u{1b}]0;"),
        "{:?}",
        result.stderr()
    );
    result.assert_stderr_contains(&format!("skipped entry {ESCAPED}"));
}

#[test]
#[serial]
fn usage_prose_that_quotes_argv_reaches_stderr_with_its_escape_sequence_defused() {
    let result = run(&["archiver", "warn", "--set", PAINT]);
    result.assert_error_kind(RunErrorKind::ClapUsage);
    assert!(!result.stderr().contains('\u{1b}'), "{:?}", result.stderr());
    result.assert_stderr_contains(&format!("expected KEY=VALUE, got `{ESCAPED}`"));
}
