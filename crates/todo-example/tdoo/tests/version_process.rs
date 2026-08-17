//! Runs `tdoo --version` the way a user does — as a real process.
//!
//! The harness proves the builder setting reaches the parse; only the
//! compiled binary proves the answer arrives on stdout with status 0.

use standout_test::TestHarness;

#[test]
fn the_compiled_binary_prints_its_version_and_succeeds() {
    let result = TestHarness::new()
        // The binary loads its store before parsing; keep that off the
        // developer's real todo file.
        .fixture("todos.json", r#"{"todos":[],"next_id":1}"#)
        .env("TODO_FILE", "todos.json")
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["--version"]);

    result.assert_success();
    assert_eq!(
        result.stdout().trim(),
        format!("tdoo {}", env!("CARGO_PKG_VERSION"))
    );
    result.assert_stderr_empty();
}
