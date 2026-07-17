use crate::handlers;
use anyhow::Result;
use clap::ArgMatches;
use serde_json::Value as JsonValue;
use standout::cli::{App, CommandContext, HookError};
use standout::input::{ArgSource, InputChain, StdinSource};
use standout::{embed_styles, embed_templates};
use todo_core::TodoStore;

/// Builds the complete shell application around the CLI-free core store.
///
/// Invocation policy is a shell concern, so it lives here rather than in
/// `todo-core`: a naked `tdoo` means "add" when something is piped in and
/// "list" at a terminal. The resolver only reads the terminal fact — the `add`
/// command's `InputChain` still owns actually reading stdin.
pub(crate) fn build(store: TodoStore) -> Result<App> {
    Ok(App::builder()
        .app_state(store)
        .default_command_with(|ctx| {
            Some(if ctx.stdin_is_piped() { "add" } else { "list" }.to_string())
        })
        .templates(embed_templates!("src/templates"))
        .styles(embed_styles!("src/styles"))
        .default_theme("todo")
        .command_with("add", handlers::add__handler, |config| {
            config
                .template("add.jinja")
                .input(
                    "title",
                    InputChain::<String>::new()
                        .try_source(ArgSource::new("title"))
                        .try_source(StdinSource::new())
                        .validate(|title| !title.trim().is_empty(), "title cannot be empty"),
                )
                .post_dispatch(audit_hook)
        })?
        .command_with("list", handlers::list__handler, |config| {
            config.template("list.jinja")
        })?
        .command_with("done", handlers::done__handler, |config| {
            config.template("done.jinja").post_dispatch(audit_hook)
        })?
        .build()?)
}

/// Observes mutation results after dispatch without coupling the core library
/// or handlers to shell-level audit configuration.
fn audit_hook(
    _matches: &ArgMatches,
    ctx: &CommandContext,
    value: JsonValue,
) -> std::result::Result<JsonValue, HookError> {
    if let Ok(path) = std::env::var("TODO_AUDIT_LOG") {
        let line = format!(
            "{}\t{}\n",
            ctx.command_path.join("."),
            value
                .get("todo")
                .and_then(|todo| todo.get("id"))
                .unwrap_or(&JsonValue::Null)
        );
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut file| std::io::Write::write_all(&mut file, line.as_bytes()));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli;
    use serial_test::serial;
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

        // The todo really landed in the core store, not just in the message.
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

        // Piped stdin would have meant `add`, but `list` was asked for.
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
