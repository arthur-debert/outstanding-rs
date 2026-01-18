//! Integration tests for the Dispatch derive macro.
//!
//! These tests verify that the `#[derive(Dispatch)]` macro generates correct
//! dispatch configuration for clap Subcommand enums.

use clap::Subcommand;
use standout::cli::{CommandContext, Dispatch, GroupBuilder, HandlerResult, Output};

// =============================================================================
// Test handlers module
// =============================================================================

mod handlers {
    use super::*;
    use clap::ArgMatches;

    pub fn list(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
        Ok(Output::Silent)
    }

    pub fn add(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
        Ok(Output::Silent)
    }

    pub fn show_all(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
        Ok(Output::Silent)
    }
}

// =============================================================================
// Basic dispatch tests
// =============================================================================

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum BasicCommands {
    List,
    Add,
}

#[test]
fn test_basic_dispatch_compiles() {
    // This test verifies that dispatch_config() returns the correct type
    let config: fn(GroupBuilder) -> GroupBuilder =
        |builder| BasicCommands::dispatch_config()(builder);
    let _ = config;
}

// =============================================================================
// Snake case conversion tests
// =============================================================================

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum SnakeCaseCommands {
    ShowAll,
}

#[test]
fn test_snake_case_dispatch_compiles() {
    // Verifies that ShowAll -> show_all conversion works
    let _ = SnakeCaseCommands::dispatch_config();
}

// =============================================================================
// Explicit handler override tests
// =============================================================================

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum OverrideCommands {
    #[dispatch(handler = handlers::list)]
    Custom,
}

#[test]
fn test_handler_override_compiles() {
    let _ = OverrideCommands::dispatch_config();
}

// =============================================================================
// Template override tests
// =============================================================================

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum TemplateCommands {
    #[dispatch(template = "custom.j2")]
    List,
}

#[test]
fn test_template_override_compiles() {
    let _ = TemplateCommands::dispatch_config();
}

// =============================================================================
// Skip attribute tests
// =============================================================================

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum SkipCommands {
    List,
    #[dispatch(skip)]
    Hidden,
}

#[test]
fn test_skip_attribute_compiles() {
    let _ = SkipCommands::dispatch_config();
}

// =============================================================================
// Default command tests
// =============================================================================

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum DefaultCommands {
    #[dispatch(default)]
    List,
    Add,
}

#[test]
fn test_default_command_compiles() {
    let _ = DefaultCommands::dispatch_config();
}

#[test]
fn test_default_command_sets_default() {
    let builder = DefaultCommands::dispatch_config()(GroupBuilder::new());
    assert_eq!(builder.get_default_command(), Some("list"));
}

#[test]
fn test_default_command_registers_commands() {
    let builder = DefaultCommands::dispatch_config()(GroupBuilder::new());
    // Both commands should be registered
    assert!(builder.contains("list"));
    assert!(builder.contains("add"));
}
