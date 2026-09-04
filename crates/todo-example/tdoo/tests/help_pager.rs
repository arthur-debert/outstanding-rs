mod common;

use standout_test::{ProcessResult, TestHarness};

/// A pager that marks every line it delivered, so a paged page and an unpaged
/// one are told apart by their bytes.
const MARKING: &str = "sed -e 's/^/PAGED /'";

/// The harness applies every removal after every set, so a variable this test
/// names is never also removed.
fn help(vars: &[(&str, &str)]) -> TestHarness {
    let mut harness = common::tdoo();
    for key in ["PAGER", "TDOO_PAGER"] {
        harness = match vars.iter().find(|(name, _)| *name == key) {
            Some((_, value)) => harness.env(key, *value),
            None => harness.env_remove(key),
        };
    }
    harness
}

fn assert_paged(result: &ProcessResult) {
    result.assert_success();
    result.assert_stdout_contains("PAGED ");
    result.assert_stdout_contains("USAGE");
}

fn assert_unpaged(result: &ProcessResult) {
    result.assert_success();
    result.assert_stdout_contains("USAGE");
    assert!(
        !result.stdout().contains("PAGED "),
        "expected an unpaged page, got:\n{}",
        result.stdout()
    );
}

#[cfg(unix)]
#[test]
fn help_on_a_terminal_goes_through_the_pager() {
    assert_paged(&help(&[("TDOO_PAGER", MARKING)]).run_pty(env!("CARGO_BIN_EXE_tdoo"), ["--help"]));
}

#[test]
fn help_through_a_pipe_never_pages() {
    assert_unpaged(
        &help(&[("TDOO_PAGER", MARKING)]).run_process(env!("CARGO_BIN_EXE_tdoo"), ["--help"]),
    );
}

#[cfg(unix)]
#[test]
fn the_application_variable_outranks_pager() {
    assert_paged(
        &help(&[("PAGER", "sed -e 's/^/PLAIN /'"), ("TDOO_PAGER", MARKING)])
            .run_pty(env!("CARGO_BIN_EXE_tdoo"), ["--help"]),
    );
}

#[cfg(unix)]
#[test]
fn an_empty_application_variable_pages_nothing() {
    assert_unpaged(
        &help(&[("PAGER", MARKING), ("TDOO_PAGER", "")])
            .run_pty(env!("CARGO_BIN_EXE_tdoo"), ["--help"]),
    );
}

#[cfg(unix)]
#[test]
fn neither_variable_set_pages_nothing() {
    assert_unpaged(&help(&[]).run_pty(env!("CARGO_BIN_EXE_tdoo"), ["--help"]));
}

#[cfg(unix)]
#[test]
fn no_pager_writes_the_page_straight_to_stdout() {
    assert_unpaged(
        &help(&[("TDOO_PAGER", MARKING)])
            .run_pty(env!("CARGO_BIN_EXE_tdoo"), ["--help", "--no-pager"]),
    );
}

#[cfg(unix)]
#[test]
fn a_named_output_file_takes_the_page_the_pager_would_have() {
    let dir = tempfile::tempdir().unwrap();

    for (asked, request) in [("--help", "flag"), ("help", "word")] {
        let page = dir.path().join(format!("{request}.txt"));
        let result = help(&[("TDOO_PAGER", MARKING)]).run_pty(
            env!("CARGO_BIN_EXE_tdoo"),
            ["--output-file-path", page.to_str().unwrap(), asked],
        );

        result.assert_success();
        let written = std::fs::read_to_string(&page).unwrap();
        assert!(
            written.contains("USAGE") && !written.contains("PAGED "),
            "expected the unpaged page in the file for the help {request}, got:\n{written}"
        );
        assert!(
            !result.stdout().contains("PAGED ") && !result.stdout().contains("USAGE"),
            "expected nothing on stdout for the help {request}, got:\n{}",
            result.stdout()
        );
    }
}

/// A file name starting with `-` is spelled `--output-file-path=<name>`, the
/// way clap itself requires it: the space form reads the next word as a flag.
#[cfg(unix)]
#[test]
fn a_file_named_like_a_flag_is_honored_in_the_joined_form() {
    let dir = tempfile::tempdir().unwrap();

    let result = help(&[("TDOO_PAGER", MARKING)]).cwd(dir.path()).run_pty(
        env!("CARGO_BIN_EXE_tdoo"),
        ["--output-file-path=-page", "--help"],
    );

    result.assert_success();
    let written = std::fs::read_to_string(dir.path().join("-page")).unwrap();
    assert!(
        written.contains("USAGE") && !written.contains("PAGED "),
        "expected the unpaged page in the file, got:\n{written}"
    );
}

#[cfg(unix)]
#[test]
fn a_pager_that_cannot_start_leaves_the_page_and_the_status_alone() {
    assert_unpaged(
        &help(&[("TDOO_PAGER", "/nonexistent/pager")])
            .run_pty(env!("CARGO_BIN_EXE_tdoo"), ["--help"]),
    );
}

#[cfg(unix)]
#[test]
fn a_pager_that_stops_reading_keeps_the_run_successful() {
    let result =
        help(&[("TDOO_PAGER", "head -c 1")]).run_pty(env!("CARGO_BIN_EXE_tdoo"), ["--help"]);
    result.assert_success();
}
