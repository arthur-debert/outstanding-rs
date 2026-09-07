use super::*;

#[test]
#[serial]
fn env_var_visible_to_handler() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "whoami",
            FnHandler::new(|_m, _ctx| {
                let v = InputChain::<String>::new()
                    .try_source(EnvSource::new("STANDOUT_TEST_USER"))
                    .default("anon".into())
                    .resolve(_m)
                    .unwrap();
                Ok(Output::Render(json!({ "user": v })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app").subcommand(Command::new("whoami"));
    let result = TestHarness::new().env("STANDOUT_TEST_USER", "arthur").run(
        &app,
        cmd,
        vec!["app", "whoami"],
    );
    result.assert_stdout_eq("arthur");
}
#[test]
#[serial]
fn env_remove_hides_existing_value() {
    std::env::set_var("STANDOUT_TEST_TOKEN", "real");
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "tok",
            FnHandler::new(|_m, _ctx| {
                let v = InputChain::<String>::new()
                    .try_source(EnvSource::new("STANDOUT_TEST_TOKEN"))
                    .default("missing".into())
                    .resolve(_m)
                    .unwrap();
                Ok(Output::Render(json!({ "tok": v })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app").subcommand(Command::new("tok"));
    {
        let result =
            TestHarness::new()
                .env_remove("STANDOUT_TEST_TOKEN")
                .run(&app, cmd, vec!["app", "tok"]);
        result.assert_stdout_eq("missing");
    }
    assert_eq!(std::env::var("STANDOUT_TEST_TOKEN").as_deref(), Ok("real"));
    std::env::remove_var("STANDOUT_TEST_TOKEN");
}
#[test]
#[serial]
fn fixture_files_are_materialized_in_cwd() {
    let app = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "cat",
            FnHandler::new(|m, _ctx| {
                let path = m.get_one::<String>("path").cloned().unwrap();
                let text = std::fs::read_to_string(path).unwrap();
                Ok(Output::Render(json!({ "text": text })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap();
    let cmd = Command::new("app")
        .subcommand(Command::new("cat").arg(clap::Arg::new("path").required(true).index(1)));
    let result = TestHarness::new()
        .fixture("notes/todo.txt", "- buy milk\n- write tests\n")
        .run(&app, cmd, vec!["app", "cat", "notes/todo.txt"]);
    result.assert_stdout_contains("buy milk");
    result.assert_stdout_contains("write tests");
}
#[test]
#[serial]
#[should_panic(expected = "absolute")]
fn fixture_rejects_absolute_path() {
    let _ = TestHarness::new().fixture("/etc/passwd", "nope");
}
#[test]
#[serial]
#[should_panic(expected = "..")]
fn fixture_rejects_parent_dir_escape() {
    let _ = TestHarness::new().fixture("../outside", "nope");
}
#[test]
#[serial]
#[should_panic(expected = "..")]
fn relative_cwd_rejects_parent_dir_escape() {
    let _ = TestHarness::new().cwd("../outside");
}
#[test]
#[serial]
#[should_panic(expected = "..")]
fn relative_cwd_rejects_nested_parent_dir_escape() {
    let _ = TestHarness::new().cwd("proj/../../outside");
}
#[test]
#[serial]
fn env_set_then_remove_restores_true_original() {
    std::env::set_var("STANDOUT_DOUBLE_PROBE", "original");
    let app = build_echo_app("echo");
    {
        let _result = TestHarness::new()
            .env("STANDOUT_DOUBLE_PROBE", "transient")
            .env_remove("STANDOUT_DOUBLE_PROBE")
            .run(&app, echo_command(), vec!["app", "echo", "x"]);
    }
    assert_eq!(
        std::env::var("STANDOUT_DOUBLE_PROBE").as_deref(),
        Ok("original")
    );
    std::env::remove_var("STANDOUT_DOUBLE_PROBE");
}
#[test]
#[serial]
fn overrides_are_restored_on_drop() {
    let original = std::env::var("STANDOUT_RESTORE_PROBE").ok();
    std::env::set_var("STANDOUT_RESTORE_PROBE", "before");
    {
        let app = build_echo_app("echo");
        let _result = TestHarness::new()
            .env("STANDOUT_RESTORE_PROBE", "during")
            .env("STANDOUT_BRAND_NEW", "new")
            .run(&app, echo_command(), vec!["app", "echo", "x"]);
    }
    assert_eq!(
        std::env::var("STANDOUT_RESTORE_PROBE").as_deref(),
        Ok("before")
    );
    assert!(std::env::var("STANDOUT_BRAND_NEW").is_err());
    std::env::remove_var("STANDOUT_RESTORE_PROBE");
    if let Some(v) = original {
        std::env::set_var("STANDOUT_RESTORE_PROBE", v);
    }
}
fn build_pwd_app() -> App {
    App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .command_with(
            "echo",
            FnHandler::new(|_m, _ctx| {
                let dir = std::env::current_dir().unwrap();
                Ok(Output::Render(json!({ "msg": dir.to_string_lossy() })))
            }),
            |cfg| cfg,
        )
        .unwrap()
        .build()
        .unwrap()
}
#[test]
#[serial]
fn relative_cwd_runs_inside_the_tempdir() {
    let app = build_pwd_app();
    let result =
        TestHarness::new()
            .cwd("proj/nested")
            .run(&app, echo_command(), vec!["app", "echo"]);
    let reported = std::path::PathBuf::from(result.stdout().trim())
        .canonicalize()
        .unwrap();
    let temp_root = std::env::temp_dir().canonicalize().unwrap();
    assert!(reported.starts_with(&temp_root), "{reported:?}");
    assert!(reported.ends_with("proj/nested"), "{reported:?}");
}
#[test]
#[serial]
fn relative_cwd_lands_beside_fixtures() {
    let app = build_pwd_app();
    let harness = TestHarness::new()
        .fixture("proj/todos.txt", "x\n")
        .cwd("proj");
    let expected = harness
        .tempdir()
        .unwrap()
        .canonicalize()
        .unwrap()
        .join("proj");
    let result = harness.run(&app, echo_command(), vec!["app", "echo"]);
    let reported = std::path::PathBuf::from(result.stdout().trim())
        .canonicalize()
        .unwrap();
    assert_eq!(reported, expected);
    assert!(reported.join("todos.txt").is_file());
}
#[test]
#[serial]
fn absolute_cwd_is_used_as_given() {
    let app = build_pwd_app();
    let dir = tempfile::tempdir().unwrap();
    let expected = dir.path().canonicalize().unwrap();
    let result = TestHarness::new()
        .cwd(dir.path())
        .run(&app, echo_command(), vec!["app", "echo"]);
    let reported = std::path::PathBuf::from(result.stdout().trim())
        .canonicalize()
        .unwrap();
    assert_eq!(reported, expected);
}
