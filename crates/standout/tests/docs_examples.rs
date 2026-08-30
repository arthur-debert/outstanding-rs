use clap::{ArgMatches, CommandFactory, Parser, Subcommand};
use serde::Serialize;
use standout::cli::FnHandler;
use standout::cli::{App, CommandContext, Dispatch, HandlerResult, Output};
use standout::{embed_styles, embed_templates};
use standout_test::{serial, TestHarness};

#[derive(Parser)]
#[command(name = "app")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum Commands {
    List,
}

#[derive(Serialize)]
struct TodoResult {
    name: String,
    todos: Vec<String>,
}

mod handlers {
    use super::*;

    pub fn list(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<TodoResult> {
        Ok(Output::Render(TodoResult {
            name: "Ada".to_string(),
            todos: vec!["one".to_string()],
        }))
    }
}

fn list_handler(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<TodoResult> {
    Ok(Output::Render(TodoResult {
        name: "Ada".to_string(),
        todos: vec!["one".to_string()],
    }))
}

#[test]
#[serial]
fn readme_and_index_builder_order_builds_and_runs() -> Result<(), Box<dyn std::error::Error>> {
    let app = App::builder()
        .templates(embed_templates!("tests/fixtures/templates"))
        .styles(embed_styles!("tests/fixtures/styles"))
        .default_theme("default")
        .commands(Commands::dispatch_config())?
        .build()?;

    let result = TestHarness::new()
        .text_output()
        .run(&app, Cli::command(), ["app", "list"]);

    result.assert_success();
    assert_eq!(result.stdout(), "Hello Ada!");
    Ok(())
}

#[test]
#[serial]
fn app_configuration_complete_example_builds() -> Result<(), Box<dyn std::error::Error>> {
    let app = App::builder()
        .templates(embed_templates!("tests/fixtures/templates"))
        .styles(embed_styles!("tests/fixtures/styles"))
        .default_theme("default")
        .version(env!("CARGO_PKG_VERSION"))
        .context("version", env!("CARGO_PKG_VERSION").into())
        .command_with("list", FnHandler::new(list_handler), |config| {
            config.template_name("list")
        })?
        .topics_dir("../../docs/topics")?
        .help_handling(true)
        .build()?;

    let result = TestHarness::new()
        .text_output()
        .run(&app, Cli::command(), ["app", "list"]);

    result.assert_success();
    assert_eq!(result.stdout(), "Hello Ada!");
    Ok(())
}
