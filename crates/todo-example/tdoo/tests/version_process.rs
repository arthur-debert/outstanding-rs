use standout_test::TestHarness;

#[test]
fn the_compiled_binary_prints_its_version_and_succeeds() {
    let result = TestHarness::new()
        .fixture("todos.json", r#"{"todos":[],"next_id":1}"#)
        .env("TDOO__STORE", "todos.json")
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["--version"]);

    result.assert_success();
    assert_eq!(
        result.stdout().trim(),
        format!("tdoo {}", env!("CARGO_PKG_VERSION"))
    );
    result.assert_stderr_empty();
}
