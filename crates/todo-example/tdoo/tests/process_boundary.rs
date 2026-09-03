mod common;

#[test]
fn a_usage_error_goes_to_stderr_and_leaves_stdout_clean() {
    let result = common::tdoo().run_process(env!("CARGO_BIN_EXE_tdoo"), ["bogus-command"]);

    result.assert_exit_code(2);
    result.assert_stdout_empty();
    result.assert_stderr_contains("unexpected argument 'bogus-command'");
}

#[test]
fn the_child_reads_the_fixture_through_its_environment_and_cwd() {
    let result = common::tdoo().run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);

    result.assert_success();
    result.assert_stdout_contains("buy milk");
    result.assert_stderr_empty();
}

#[test]
fn a_forced_output_mode_reaches_the_child_as_a_flag() {
    let result = common::tdoo()
        .output_mode(standout::OutputMode::Json)
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);

    result.assert_success();
    let value: serde_json::Value =
        serde_json::from_str(result.stdout()).expect("--output=json must produce JSON");
    assert_eq!(value["todos"][0]["title"], "buy milk");
}

#[test]
fn the_store_the_child_wrote_survives_the_run() {
    let result =
        common::tdoo().run_process(env!("CARGO_BIN_EXE_tdoo"), ["add", "--title", "walk dog"]);

    result.assert_success();
    let store = std::fs::read_to_string(
        result
            .tempdir()
            .expect("a fixture allocates the tempdir")
            .join("todos.json"),
    )
    .expect("the child must have written the store back");
    assert!(store.contains("walk dog"), "{store}");
    assert!(
        store.contains("buy milk"),
        "the child must not lose the existing todo: {store}"
    );
}
