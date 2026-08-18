//! Integration tests for the Dispatch derive macro.
//!
//! These tests verify that the `#[derive(Dispatch)]` macro generates correct
//! dispatch configuration for clap Subcommand enums.

use clap::Subcommand;
use serde_json::json;
use standout::cli::{App, CommandContext, Dispatch, GroupBuilder, HandlerResult, Output};
use standout::{EmbeddedSource, OutputMode, TemplateResource};
use standout_test::TestHarness;

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

    pub fn export(
        _matches: &ArgMatches,
        _ctx: &CommandContext,
    ) -> HandlerResult<serde_json::Value> {
        Ok(Output::Render(json!({"name": "Ada"})))
    }

    pub fn download(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
        Ok(Output::Binary {
            data: vec![1, 2, 3],
            filename: "data.bin".to_string(),
        })
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
    #[dispatch(template_name = "custom.j2")]
    List,
}

#[test]
fn test_template_override_compiles() {
    let _ = TemplateCommands::dispatch_config();
}

// =============================================================================
// Template absence tests
// =============================================================================

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum TemplateAbsenceCommands {
    #[dispatch(template_name = "list")]
    Export,
    #[dispatch(silent)]
    Add,
    #[dispatch(binary, handler = handlers::download)]
    Download,
    #[dispatch(structured_only, handler = handlers::export)]
    ShowAll,
}

#[test]
fn test_template_absence_attributes_build_mixed_apps() {
    let app = App::builder()
        .templates(EmbeddedSource::<TemplateResource>::new(
            &[("list.jinja", "Hello {{ name }}")],
            "/path/that/does/not/exist",
        ))
        .commands(TemplateAbsenceCommands::dispatch_config())
        .unwrap()
        .build()
        .unwrap();
    let command = || {
        clap::Command::new("app")
            .subcommand(clap::Command::new("export"))
            .subcommand(clap::Command::new("add"))
            .subcommand(clap::Command::new("download"))
            .subcommand(clap::Command::new("show_all"))
    };

    let rendered = TestHarness::new()
        .text_output()
        .run(&app, command(), ["app", "export"]);
    rendered.assert_success();
    assert_eq!(rendered.stdout(), "Hello Ada");

    let silent = TestHarness::new().run(&app, command(), ["app", "add"]);
    silent.assert_success();
    assert_eq!(silent.stdout(), "");

    let binary = TestHarness::new().run(&app, command(), ["app", "download"]);
    binary.assert_success();
    assert_eq!(binary.binary(), Some((&[1, 2, 3][..], "data.bin")));

    let structured =
        TestHarness::new()
            .output_mode(OutputMode::Json)
            .run(&app, command(), ["app", "show_all"]);
    structured.assert_success();
    assert_eq!(structured.stdout(), "{\n  \"name\": \"Ada\"\n}");
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
