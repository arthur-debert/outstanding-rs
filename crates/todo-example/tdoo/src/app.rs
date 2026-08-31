use crate::cli::Commands;
use anyhow::Result;
use standout::cli::App;
use standout::{embed_styles, embed_templates};
use todo_core::TodoStore;

pub(crate) fn build(store: TodoStore) -> Result<App> {
    Ok(App::builder()
        .app_state(store)
        .version(env!("CARGO_PKG_VERSION"))
        .default_command_with(|ctx| {
            Some(if ctx.stdin_is_piped() { "add" } else { "list" }.to_string())
        })
        .templates(embed_templates!("src/templates"))
        .styles(embed_styles!("src/styles"))
        .default_theme("todo")
        .commands(Commands::dispatch_config())?
        .build()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli;
    use serde_json::Value as JsonValue;
    use serial_test::serial;
    use standout::OutputMode;
    use standout_test::TestHarness;
    use tempfile::TempDir;

    fn fresh_app() -> (App, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = TodoStore::load(dir.path().join("todos.json")).unwrap();
        (build(store).unwrap(), dir)
    }

    #[test]
    #[serial]
    fn empty_list_uses_the_command_template() {
        let (app, _dir) = fresh_app();

        let result = TestHarness::new()
            .no_color()
            .run(&app, cli::command(), ["tdoo", "list"]);

        result.assert_success();
        result.assert_stdout_contains("Nothing here yet");
    }

    #[test]
    #[serial]
    fn version_reports_the_binary_packages_version() {
        let (app, _dir) = fresh_app();

        let result = TestHarness::new()
            .no_color()
            .run(&app, cli::command(), ["tdoo", "--version"]);

        result.assert_success();
        assert_eq!(
            result.stdout().trim(),
            format!("tdoo {}", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    #[serial]
    fn add_reads_piped_stdin_and_list_can_serialize_json() {
        let (app, _dir) = fresh_app();

        TestHarness::new()
            .no_color()
            .piped_stdin("ship the docs\n")
            .run(&app, cli::command(), ["tdoo", "add"])
            .assert_success();
        let listed = TestHarness::new().no_color().run(
            &app,
            cli::command(),
            ["tdoo", "list", "--output", "json"],
        );

        listed.assert_success();
        let value: JsonValue = serde_json::from_str(listed.stdout()).unwrap();
        assert_eq!(value["total"], 1);
        assert_eq!(value["todos"][0]["title"], "ship the docs");
    }

    #[test]
    #[serial]
    fn naked_invocation_at_a_terminal_lists() {
        let (app, _dir) = fresh_app();

        let result =
            TestHarness::new()
                .no_color()
                .interactive_stdin()
                .run(&app, cli::command(), ["tdoo"]);

        result.assert_success();
        result.assert_stdout_contains("Nothing here yet");
    }

    #[test]
    #[serial]
    fn naked_invocation_with_piped_stdin_adds() {
        let (app, _dir) = fresh_app();

        let result = TestHarness::new()
            .no_color()
            .piped_stdin("ship the docs\n")
            .run(&app, cli::command(), ["tdoo"]);

        result.assert_success();
        result.assert_stdout_contains("ship the docs");

        let listed = TestHarness::new().no_color().run(
            &app,
            cli::command(),
            ["tdoo", "list", "--output", "json"],
        );
        let value: JsonValue = serde_json::from_str(listed.stdout()).unwrap();
        assert_eq!(value["todos"][0]["title"], "ship the docs");
    }

    #[test]
    #[serial]
    fn an_explicit_command_beats_the_invocation_policy() {
        let (app, _dir) = fresh_app();

        let result = TestHarness::new()
            .no_color()
            .piped_stdin("not a todo\n")
            .run(&app, cli::command(), ["tdoo", "list"]);

        result.assert_success();
        result.assert_stdout_contains("Nothing here yet");
    }

    #[test]
    #[serial]
    fn input_chain_rejects_an_empty_title_before_dispatch() {
        let (app, _dir) = fresh_app();

        let result = TestHarness::new().no_color().run(
            &app,
            cli::command(),
            ["tdoo", "add", "--title", "   "],
        );

        result.assert_error_contains("title cannot be empty");
    }

    #[test]
    #[serial]
    fn export_lets_standout_own_the_destination_and_the_success_report() {
        let (app, dir) = fresh_app();
        let destination = dir.path().join("todos.csv");

        TestHarness::new()
            .no_color()
            .run(&app, cli::command(), ["tdoo", "add", "--title", "buy milk"])
            .assert_success();

        let result = TestHarness::new().no_color().run(
            &app,
            cli::command(),
            [
                "tdoo",
                "export",
                "--output-file-path",
                destination.to_str().unwrap(),
            ],
        );

        result.assert_success();
        result.assert_artifact_suggested_destination("todos.csv");
        result.assert_artifact_written_to(&destination);
        result.assert_artifact_bytes(b"id,title,done\n1,buy milk,false\n");
        result.assert_artifact_report_contains("Exported 1 todos");
        result.assert_artifact_report_contains(&destination.display().to_string());
        assert_eq!(
            std::fs::read_to_string(&destination).unwrap(),
            "id,title,done\n1,buy milk,false\n"
        );
    }

    #[test]
    #[serial]
    fn export_keeps_core_warnings_typed_in_both_output_modes() {
        let (app, _dir) = fresh_app();

        TestHarness::new()
            .no_color()
            .run(&app, cli::command(), ["tdoo", "add", "--title", "buy milk"])
            .assert_success();
        TestHarness::new()
            .no_color()
            .run(&app, cli::command(), ["tdoo", "add", "--title", "ship it"])
            .assert_success();
        TestHarness::new()
            .no_color()
            .run(&app, cli::command(), ["tdoo", "done", "2"])
            .assert_success();

        let human =
            TestHarness::new()
                .no_color()
                .run(&app, cli::command(), ["tdoo", "export", "--stdout"]);
        human.assert_artifact_to_stdout();
        human.assert_artifact_report_contains("warning: 1 completed todo(s) omitted");

        let json = TestHarness::new()
            .no_color()
            .output_mode(OutputMode::Json)
            .run(&app, cli::command(), ["tdoo", "export", "--stdout"]);
        let value: JsonValue = serde_json::from_str(json.artifact_report().unwrap()).unwrap();
        assert_eq!(value["report"]["exported"], 1);
        assert_eq!(value["report"]["warnings"][0]["kind"], "completed_omitted");
        assert_eq!(value["receipt"]["destination"], "-");
        assert_eq!(value["receipt"]["stdout"], true);
    }

    #[test]
    #[serial]
    fn export_reports_a_failed_write_instead_of_a_false_success() {
        let (app, dir) = fresh_app();
        let unwritable = dir.path().join("missing").join("todos.csv");

        let result = TestHarness::new().no_color().run(
            &app,
            cli::command(),
            [
                "tdoo",
                "export",
                "--output-file-path",
                unwritable.to_str().unwrap(),
            ],
        );

        result.assert_error_contains("Error writing artifact");
        assert!(
            result.artifact().is_none(),
            "a failed write reports nothing"
        );
        assert!(!unwritable.exists());
    }

    #[test]
    #[serial]
    fn mutation_hook_writes_an_audit_entry() {
        let (app, dir) = fresh_app();
        let log_path = dir.path().join("audit.log");

        TestHarness::new()
            .no_color()
            .env("TODO_AUDIT_LOG", log_path.to_str().unwrap())
            .run(&app, cli::command(), ["tdoo", "add", "--title", "audited"])
            .assert_success();

        let log = std::fs::read_to_string(log_path).unwrap();
        assert!(log.contains("add\t1"), "unexpected audit log: {log}");
    }
}
