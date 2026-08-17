//! `tdoo` run the way a user runs it: as a real process.
//!
//! These assert the facts an in-process `TestHarness::run` cannot reach. It
//! reconstructs the two text channels from one `RunResult` by mirroring
//! `App::run`'s writer seam — a faithful model, but a model: it cannot show
//! that the compiled binary wrote the diagnostic to the *OS's* stderr, left
//! the *OS's* stdout untouched, and handed the shell a real exit code. Only
//! a fork shows that.
//!
//! `tdoo` is the workspace's realistic app and has a binary to run, which is
//! why the process seam's end-to-end coverage lives here rather than in
//! `standout-test`, whose own tests pin the settings the seam refuses.

use standout_test::TestHarness;

/// The store `tdoo` loads before parsing, as a fixture file. Every test
/// declares one so nothing reaches the developer's real todo file.
const STORE: &str = r#"{"todos":[{"id":1,"title":"buy milk","done":false}],"next_id":1}"#;

/// Stream separation and the exit code, all three at once: a usage error is
/// a diagnostic on stderr, an *empty* stdout, and a non-zero status. The
/// empty-stdout half is the one that matters — a program that helpfully
/// prints its usage error to stdout corrupts every pipeline it is used in,
/// and an in-process run, which rebuilds both channels from one captured
/// outcome, cannot tell the two apart at the OS level.
#[test]
fn a_usage_error_goes_to_stderr_and_leaves_stdout_clean() {
    let result = TestHarness::new()
        .fixture("todos.json", STORE)
        .env("TODO_FILE", "todos.json")
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["bogus-command"]);

    result.assert_exit_code(2);
    result.assert_stdout_empty();
    result.assert_stderr_contains("unexpected argument 'bogus-command'");
}

/// The settings that do cross the boundary: the fixture file is written, the
/// child runs in the tempdir that holds it, and the environment variable
/// naming it (relatively, so it only resolves from that cwd) reaches the
/// child's own `std::env::var_os`.
#[test]
fn the_child_reads_the_fixture_through_its_environment_and_cwd() {
    let result = TestHarness::new()
        .fixture("todos.json", STORE)
        .env("TODO_FILE", "todos.json")
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);

    result.assert_success();
    result.assert_stdout_contains("buy milk");
    result.assert_stderr_empty();
}

/// A forced output mode is an argv edit, which survives the boundary — so
/// the same `output_mode()` call works on both runners. Piping also proves
/// the shape of the JSON is not decided by whether a terminal is attached.
#[test]
fn a_forced_output_mode_reaches_the_child_as_a_flag() {
    let result = TestHarness::new()
        .fixture("todos.json", STORE)
        .env("TODO_FILE", "todos.json")
        .output_mode(standout::OutputMode::Json)
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);

    result.assert_success();
    let value: serde_json::Value =
        serde_json::from_str(result.stdout()).expect("--output=json must produce JSON");
    assert_eq!(value["todos"][0]["title"], "buy milk");
}

/// What the child writes to disk stays readable: the tempdir outlives the
/// run, so a mutating command can be asserted on its effect, not just on its
/// output.
#[test]
fn the_store_the_child_wrote_survives_the_run() {
    let result = TestHarness::new()
        .fixture("todos.json", STORE)
        .env("TODO_FILE", "todos.json")
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["add", "--title", "walk dog"]);

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
