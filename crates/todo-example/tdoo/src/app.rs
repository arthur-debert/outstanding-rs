use crate::cli::Commands;
use crate::config::{self, TdooConfig};
use anyhow::Result;
use clapfig::SearchPath;
use standout::cli::App;
use standout::{embed_styles, embed_templates};

pub(crate) fn build(user_scope: SearchPath) -> Result<App> {
    Ok(App::builder()
        .version(env!("CARGO_PKG_VERSION"))
        .default_command_with(|ctx| {
            Some(if ctx.stdin_is_piped() { "add" } else { "list" }.to_string())
        })
        .templates(embed_templates!("src/templates"))
        .styles(embed_styles!("src/styles"))
        .default_theme("todo")
        .config(config::builder(user_scope))
        .term_settings(|config: &TdooConfig| &config.term)
        .commands(Commands::dispatch_config())?
        .build()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli;
    use serde_json::{json, Value as JsonValue};
    use serial_test::serial;
    use standout::OutputMode;
    use standout_test::{TestHarness, TestResult};
    use tempfile::TempDir;

    const PROJECT: &str = "store = \"todos.json\"\n";

    struct Tdoo {
        app: App,
        dir: TempDir,
    }

    impl Tdoo {
        fn new() -> Self {
            Self::with_files(PROJECT, "")
        }

        fn with_files(project: &str, user: &str) -> Self {
            let dir = TempDir::new().unwrap();
            let user_dir = dir.path().join("user");
            std::fs::create_dir_all(&user_dir).unwrap();
            std::fs::write(dir.path().join("tdoo.toml"), project).unwrap();
            if !user.is_empty() {
                std::fs::write(user_dir.join("tdoo.toml"), user).unwrap();
            }
            let app = build(SearchPath::Path(user_dir)).unwrap();
            Self { app, dir }
        }

        fn harness(&self) -> TestHarness {
            TestHarness::new()
                .no_color()
                .cwd(self.dir.path().to_path_buf())
        }

        fn run<const N: usize>(&self, args: [&str; N]) -> TestResult {
            self.harness().run(&self.app, cli::command(), args)
        }

        fn run_with<const N: usize>(&self, harness: TestHarness, args: [&str; N]) -> TestResult {
            harness
                .cwd(self.dir.path().to_path_buf())
                .run(&self.app, cli::command(), args)
        }

        fn add(&self, title: &str) {
            self.run(["tdoo", "add", "--title", title]).assert_success();
        }

        fn titles<const N: usize>(&self, args: [&str; N]) -> Vec<String> {
            let listed = self.run(args);
            listed.assert_success();
            let value: JsonValue = serde_json::from_str(listed.stdout()).unwrap();
            value["todos"]
                .as_array()
                .unwrap()
                .iter()
                .map(|todo| todo["title"].as_str().unwrap().to_string())
                .collect()
        }

        fn project_file(&self) -> String {
            std::fs::read_to_string(self.dir.path().join("tdoo.toml")).unwrap()
        }

        fn user_file(&self) -> String {
            std::fs::read_to_string(self.dir.path().join("user").join("tdoo.toml")).unwrap()
        }
    }

    #[test]
    #[serial]
    fn empty_list_uses_the_command_template() {
        let tdoo = Tdoo::new();

        let result = tdoo.run(["tdoo", "list"]);

        result.assert_success();
        result.assert_stdout_contains("Nothing here yet");
    }

    #[test]
    #[serial]
    fn version_reports_the_binary_packages_version() {
        let tdoo = Tdoo::new();

        let result = tdoo.run(["tdoo", "--version"]);

        result.assert_success();
        assert_eq!(
            result.stdout().trim(),
            format!("tdoo {}", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    #[serial]
    fn add_reads_piped_stdin_and_list_can_serialize_json() {
        let tdoo = Tdoo::new();

        tdoo.run_with(
            TestHarness::new().no_color().piped_stdin("ship the docs\n"),
            ["tdoo", "add"],
        )
        .assert_success();
        let listed = tdoo.run(["tdoo", "list", "--output", "json"]);

        listed.assert_success();
        let value: JsonValue = serde_json::from_str(listed.stdout()).unwrap();
        assert_eq!(value["total"], 1);
        assert_eq!(value["todos"][0]["title"], "ship the docs");
    }

    #[test]
    #[serial]
    fn the_list_document_keeps_its_schema() {
        let tdoo = Tdoo::new();
        tdoo.add("ship the docs");

        let listed = tdoo.run(["tdoo", "list", "--output", "json"]);

        listed.assert_success();
        listed.assert_schema_snapshot("list.json");
    }

    #[test]
    #[serial]
    fn naked_invocation_at_a_terminal_lists() {
        let tdoo = Tdoo::new();

        let result = tdoo.run_with(TestHarness::new().no_color().interactive_stdin(), ["tdoo"]);

        result.assert_success();
        result.assert_stdout_contains("Nothing here yet");
    }

    #[test]
    #[serial]
    fn naked_invocation_with_piped_stdin_adds() {
        let tdoo = Tdoo::new();

        let result = tdoo.run_with(
            TestHarness::new().no_color().piped_stdin("ship the docs\n"),
            ["tdoo"],
        );

        result.assert_success();
        result.assert_stdout_contains("ship the docs");
        assert_eq!(
            tdoo.titles(["tdoo", "list", "--output", "json"]),
            ["ship the docs"]
        );
    }

    #[test]
    #[serial]
    fn an_explicit_command_beats_the_invocation_policy() {
        let tdoo = Tdoo::new();

        let result = tdoo.run_with(
            TestHarness::new().no_color().piped_stdin("not a todo\n"),
            ["tdoo", "list"],
        );

        result.assert_success();
        result.assert_stdout_contains("Nothing here yet");
    }

    #[test]
    #[serial]
    fn input_chain_rejects_an_empty_title_before_dispatch() {
        let tdoo = Tdoo::new();

        let result = tdoo.run(["tdoo", "add", "--title", "   "]);

        result.assert_error_contains("title cannot be empty");
    }

    #[test]
    #[serial]
    fn the_reverse_flag_beats_the_config_value_which_applies_when_the_flag_is_absent() {
        let configured_off = Tdoo::with_files("store = \"todos.json\"\nreverse = false\n", "");
        configured_off.add("first");
        configured_off.add("second");
        assert_eq!(
            configured_off.titles(["tdoo", "list", "--output", "json"]),
            ["first", "second"]
        );
        assert_eq!(
            configured_off.titles(["tdoo", "list", "--reverse", "--output", "json"]),
            ["second", "first"]
        );

        let configured_on = Tdoo::with_files("store = \"todos.json\"\nreverse = true\n", "");
        configured_on.add("first");
        configured_on.add("second");
        assert_eq!(
            configured_on.titles(["tdoo", "list", "--output", "json"]),
            ["second", "first"]
        );
    }

    #[test]
    #[serial]
    fn a_typed_reverse_false_turns_off_a_configured_reverse() {
        let tdoo = Tdoo::with_files("store = \"todos.json\"\nreverse = true\n", "");
        tdoo.add("first");
        tdoo.add("second");
        assert_eq!(
            tdoo.titles(["tdoo", "list", "--reverse=false", "--output", "json"]),
            ["first", "second"]
        );
        assert_eq!(
            tdoo.titles(["tdoo", "list", "--reverse", "--output", "json"]),
            ["second", "first"]
        );
    }

    #[test]
    #[serial]
    fn a_project_file_and_a_user_file_both_feed_the_struct() {
        let tdoo = Tdoo::with_files(PROJECT, "reverse = true\n");
        tdoo.add("first");
        tdoo.add("second");

        assert!(tdoo.dir.path().join("todos.json").is_file());
        assert_eq!(
            tdoo.titles(["tdoo", "list", "--output", "json"]),
            ["second", "first"]
        );
    }

    #[test]
    #[serial]
    fn config_set_with_scope_global_writes_the_user_file() {
        let tdoo = Tdoo::with_files(PROJECT, "reverse = true\n");
        tdoo.add("first");
        tdoo.add("second");

        tdoo.run([
            "tdoo", "config", "set", "reverse", "false", "--scope", "global",
        ])
        .assert_success();

        assert!(
            tdoo.user_file().contains("reverse = false"),
            "{}",
            tdoo.user_file()
        );
        assert_eq!(tdoo.project_file(), PROJECT);
        assert_eq!(
            tdoo.titles(["tdoo", "list", "--output", "json"]),
            ["first", "second"]
        );
    }

    #[test]
    #[serial]
    fn term_output_in_the_file_makes_a_bare_list_emit_json() {
        let tdoo = Tdoo::with_files("store = \"todos.json\"\n\n[term]\noutput = \"json\"\n", "");
        tdoo.add("ship the docs");

        let result = tdoo.run(["tdoo", "list"]);

        result.assert_success();
        assert_eq!(result.output_mode(), OutputMode::Json);
        let value: JsonValue = serde_json::from_str(result.stdout()).unwrap();
        assert_eq!(value["todos"][0]["title"], "ship the docs");
    }

    #[test]
    #[serial]
    fn config_list_as_json_returns_typed_values() {
        let tdoo = Tdoo::with_files("store = \"todos.json\"\nreverse = true\n", "");

        let result = tdoo.run(["tdoo", "config", "list", "--output", "json"]);

        result.assert_success();
        let value: JsonValue = serde_json::from_str(result.stdout()).unwrap();
        assert_eq!(value["store"], json!("todos.json"));
        assert_eq!(value["reverse"], json!(true));
    }

    #[test]
    #[serial]
    fn export_lets_standout_own_the_destination_and_the_success_report() {
        let tdoo = Tdoo::new();
        let destination = tdoo.dir.path().join("todos.csv");
        tdoo.add("buy milk");

        let result = tdoo.run([
            "tdoo",
            "export",
            "--output-file-path",
            destination.to_str().unwrap(),
        ]);

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
        let tdoo = Tdoo::new();
        tdoo.add("buy milk");
        tdoo.add("ship it");
        tdoo.run(["tdoo", "done", "2"]).assert_success();

        let human = tdoo.run(["tdoo", "export", "--stdout"]);
        human.assert_artifact_to_stdout();
        human.assert_artifact_report_contains("warning: 1 completed todo(s) omitted");

        let json = tdoo.run_with(
            TestHarness::new().no_color().output_mode(OutputMode::Json),
            ["tdoo", "export", "--stdout"],
        );
        let value: JsonValue = serde_json::from_str(json.artifact_report().unwrap()).unwrap();
        assert_eq!(value["report"]["exported"], 1);
        assert_eq!(value["report"]["warnings"][0]["kind"], "completed_omitted");
        assert_eq!(value["receipt"]["destination"], "-");
        assert_eq!(value["receipt"]["stdout"], true);
    }

    #[test]
    #[serial]
    fn export_reports_a_failed_write_instead_of_a_false_success() {
        let tdoo = Tdoo::new();
        let unwritable = tdoo.dir.path().join("missing").join("todos.csv");

        let result = tdoo.run([
            "tdoo",
            "export",
            "--output-file-path",
            unwritable.to_str().unwrap(),
        ]);

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
        let tdoo = Tdoo::new();
        let log_path = tdoo.dir.path().join("audit.log");

        tdoo.run_with(
            TestHarness::new()
                .no_color()
                .env("TODO_AUDIT_LOG", log_path.to_str().unwrap()),
            ["tdoo", "add", "--title", "audited"],
        )
        .assert_success();

        let log = std::fs::read_to_string(log_path).unwrap();
        assert!(log.contains("add\t1"), "unexpected audit log: {log}");
    }
}
