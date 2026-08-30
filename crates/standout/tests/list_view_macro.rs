use clap::{ArgMatches, Subcommand};
use serde::Serialize;
use standout::cli::{CommandContext, Dispatch, GroupBuilder, HandlerResult, Output};
use standout::views::list_view;
use standout::{Tabular, TabularRow};

#[derive(Serialize, Tabular, TabularRow, Clone)]
struct Task {
    #[col(width = 5)]
    id: u32,
    #[col(width = 20)]
    name: String,
}

mod handlers {
    use super::*;

    pub fn list(
        _matches: &ArgMatches,
        _ctx: &CommandContext,
    ) -> HandlerResult<standout::views::ListViewResult<Task>> {
        let tasks = vec![Task {
            id: 1,
            name: "Task 1".to_string(),
        }];
        Ok(Output::Render(list_view(tasks).build()))
    }
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum Commands {
    #[dispatch(list_view, item_type = "Task")]
    List,
}

#[test]
fn test_list_view_macro_injection() {
    let config = Commands::dispatch_config();
    let builder = config(GroupBuilder::new());

    assert!(builder.contains("list"));

    use standout::cli::App;
    let app = App::builder()
        .commands(Commands::dispatch_config())
        .expect("Failed to set commands")
        .build()
        .expect("Failed to build app");

    let cmd = clap::Command::new("test").subcommand(clap::Command::new("list"));
    let matches = cmd.try_get_matches_from(vec!["test", "list"]).unwrap();

    let result = app.dispatch(matches, standout::OutputMode::Json);

    assert!(result.is_handled());
    let output = result.output().expect("Expected output");

    println!("Output: {}", output);
    assert!(
        output.contains("\"tabular_spec\""),
        "Output should contain tabular_spec when list_view macro is used"
    );
    assert!(output.contains("\"width\": 5"));
}
