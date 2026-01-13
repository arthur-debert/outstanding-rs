//! Integration tests for the Dispatch derive macro.

use clap::{ArgMatches, CommandFactory, Parser, Subcommand};
use outstanding_clap::{CommandContext, CommandResult, Dispatch, GroupBuilder};
use serde_json::json;

// ============================================================================
// Test handlers module
// ============================================================================

mod handlers {
    use super::*;

    pub fn add(_m: &ArgMatches, _ctx: &CommandContext) -> CommandResult<serde_json::Value> {
        CommandResult::Ok(json!({"action": "add"}))
    }

    pub fn list(_m: &ArgMatches, _ctx: &CommandContext) -> CommandResult<serde_json::Value> {
        CommandResult::Ok(json!({"action": "list"}))
    }

    pub fn complete(_m: &ArgMatches, _ctx: &CommandContext) -> CommandResult<serde_json::Value> {
        CommandResult::Ok(json!({"action": "complete"}))
    }

    pub fn list_all(_m: &ArgMatches, _ctx: &CommandContext) -> CommandResult<serde_json::Value> {
        CommandResult::Ok(json!({"action": "list_all"}))
    }

    pub fn get_config(_m: &ArgMatches, _ctx: &CommandContext) -> CommandResult<serde_json::Value> {
        CommandResult::Ok(json!({"action": "get_config"}))
    }

    pub fn set_value(_m: &ArgMatches, _ctx: &CommandContext) -> CommandResult<serde_json::Value> {
        CommandResult::Ok(json!({"action": "set_value"}))
    }

    pub mod db {
        use super::*;

        pub fn migrate(_m: &ArgMatches, _ctx: &CommandContext) -> CommandResult<serde_json::Value> {
            CommandResult::Ok(json!({"action": "db_migrate"}))
        }

        pub fn backup(_m: &ArgMatches, _ctx: &CommandContext) -> CommandResult<serde_json::Value> {
            CommandResult::Ok(json!({"action": "db_backup"}))
        }
    }

    pub fn with_arg(_m: &ArgMatches, _ctx: &CommandContext) -> CommandResult<serde_json::Value> {
        CommandResult::Ok(json!({"action": "with_arg"}))
    }
}

mod custom {
    use super::*;

    pub fn list_handler(
        _m: &ArgMatches,
        _ctx: &CommandContext,
    ) -> CommandResult<serde_json::Value> {
        CommandResult::Ok(json!({"action": "custom_list"}))
    }
}

fn validate_hook(
    _m: &ArgMatches,
    _ctx: &CommandContext,
) -> Result<(), outstanding_clap::HookError> {
    Ok(())
}

// ============================================================================
// Simple enum test
// ============================================================================

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum SimpleCommands {
    /// Add a new item
    Add,

    /// List all items
    List,

    /// Complete an item
    Complete,
}

#[test]
fn test_simple_dispatch_config() {
    let config = SimpleCommands::dispatch_config();
    let builder = config(GroupBuilder::new());

    // Verify commands were registered
    assert!(builder.contains("add"));
    assert!(builder.contains("list"));
    assert!(builder.contains("complete"));
    assert_eq!(builder.len(), 3);
}

// ============================================================================
// Enum with explicit overrides
// ============================================================================

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum OverrideCommands {
    /// Add with custom template
    #[dispatch(template = "custom/add.j2")]
    Add,

    /// List with custom handler
    #[dispatch(handler = custom::list_handler)]
    List,

    /// Complete with hooks
    #[dispatch(pre_dispatch = validate_hook)]
    Complete,
}

#[test]
fn test_override_dispatch_config() {
    let config = OverrideCommands::dispatch_config();
    let builder = config(GroupBuilder::new());

    assert!(builder.contains("add"));
    assert!(builder.contains("list"));
    assert!(builder.contains("complete"));
}

// ============================================================================
// Enum with skip attribute
// ============================================================================

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum SkipCommands {
    Add,

    #[dispatch(skip)]
    Hidden,

    List,
}

#[test]
fn test_skip_dispatch_config() {
    let config = SkipCommands::dispatch_config();
    let builder = config(GroupBuilder::new());

    assert!(builder.contains("add"));
    assert!(builder.contains("list"));
    // Hidden should be skipped
    assert!(!builder.contains("hidden"));
    assert_eq!(builder.len(), 2);
}

// ============================================================================
// Integration with clap Parser
// ============================================================================

#[derive(Parser)]
#[command(name = "testapp")]
struct TestCli {
    #[command(subcommand)]
    command: TestCommands,
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum TestCommands {
    /// Add a task
    Add,
    /// List tasks
    List,
}

#[test]
fn test_with_clap_parser() {
    // Verify the CLI can be built
    let _cmd = TestCli::command();

    // Verify dispatch_config generates valid closure
    let config = TestCommands::dispatch_config();
    let builder = config(GroupBuilder::new());

    assert!(builder.contains("add"));
    assert!(builder.contains("list"));
}

// ============================================================================
// Snake case conversion
// ============================================================================

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum SnakeCaseCommands {
    ListAll,
    GetConfig,
    SetValue,
}

#[test]
fn test_snake_case_conversion() {
    let config = SnakeCaseCommands::dispatch_config();
    let builder = config(GroupBuilder::new());

    // Commands should be registered with snake_case names
    assert!(builder.contains("list_all"));
    assert!(builder.contains("get_config"));
    assert!(builder.contains("set_value"));
}

// ============================================================================
// Nested subcommands (group test)
// ============================================================================
// Note: Nested subcommand delegation requires the nested type to also derive Dispatch.
// For this test we verify the group is created for the nested command.

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers::db)]
enum DbCommands {
    Migrate,
    Backup,
}

#[test]
fn test_nested_db_commands() {
    let config = DbCommands::dispatch_config();
    let builder = config(GroupBuilder::new());

    assert!(builder.contains("migrate"));
    assert!(builder.contains("backup"));
}

// ============================================================================
// Tuple variant test (Regression test for incorrect nesting)
// ============================================================================

#[derive(clap::Args)]
struct TupleArgs {
    arg: String,
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum TupleVariantCommands {
    /// Should be treated as a leaf command, not nested
    WithArg(TupleArgs),
}

#[test]
fn test_tuple_variant_regression() {
    let config = TupleVariantCommands::dispatch_config();
    let builder = config(GroupBuilder::new());

    // Should be registered as "with_arg" using handlers::with_arg convention
    // But since handlers::with_arg doesn't exist in the shared module, let's use explicit handler
    // Actually, let's just use "add" handler via override to verify registration works

    // RETHINK: To test default convention, I need handlers::with_arg.
    // The shared handlers module doesn't have it.
    // So I will use explicit handler to avoid compilation error on the handler side,
    // while verifying the macro doesn't try to recurse into TupleArgs.
    assert!(builder.contains("with_arg"));
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum TupleVariantExplicit {
    #[dispatch(handler = handlers::add)]
    WithArg(TupleArgs),
}

#[test]
fn test_tuple_variant_explicit() {
    let config = TupleVariantExplicit::dispatch_config();
    let builder = config(GroupBuilder::new());
    assert!(builder.contains("with_arg"));
}
