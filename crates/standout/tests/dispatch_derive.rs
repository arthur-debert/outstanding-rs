use clap::Subcommand;
use serde_json::json;
use standout::cli::{App, CommandContext, Dispatch, GroupBuilder, HandlerResult, Output};
use standout::{EmbeddedSource, OutputMode, TemplateResource};
use standout_test::TestHarness;

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

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum BasicCommands {
    List,
    Add,
}

#[test]
fn test_basic_dispatch_compiles() {
    let config: fn(GroupBuilder) -> GroupBuilder =
        |builder| BasicCommands::dispatch_config()(builder);
    let _ = config;
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum MultiWordCommands {
    ShowAll,
}

#[test]
fn multi_word_variant_registers_the_kebab_case_name() {
    let builder = MultiWordCommands::dispatch_config()(GroupBuilder::new());
    assert!(builder.contains("show-all"));
    assert!(!builder.contains("show_all"));
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum RenamedCommands {
    #[dispatch(name = "ls", handler = handlers::list)]
    List,
    #[dispatch(name = "show", default)]
    ShowAll,
}

#[test]
fn variant_rename_replaces_the_derived_name() {
    let builder = RenamedCommands::dispatch_config()(GroupBuilder::new());
    assert!(builder.contains("ls"));
    assert!(builder.contains("show"));
    assert!(!builder.contains("list"));
    assert!(!builder.contains("show-all"));
    assert_eq!(builder.get_default_command(), Some("show"));
}

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
            .subcommand(clap::Command::new("show-all"))
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
            .run(&app, command(), ["app", "show-all"]);
    structured.assert_success();
    assert_eq!(structured.stdout(), "{\n  \"name\": \"Ada\"\n}");
}

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
    assert!(builder.contains("list"));
    assert!(builder.contains("add"));
}
