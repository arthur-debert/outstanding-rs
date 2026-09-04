//! `tdoo list` is the application's pageable command, run as a process so the
//! pager is really started and the bytes really cross a terminal.

mod common;

use standout_test::{ProcessResult, TestHarness};

/// Marks every line it delivered, so a paged run shows in the bytes.
const MARKING: &str = "sed -e 's/^/PAGED /'";

/// The harness applies every removal after every set.
fn listing(vars: &[(&str, &str)]) -> TestHarness {
    let mut harness = common::tdoo();
    for key in ["PAGER", "TDOO_PAGER"] {
        harness = match vars.iter().find(|(name, _)| *name == key) {
            Some((_, value)) => harness.env(key, *value),
            None => harness.env_remove(key),
        };
    }
    harness
}

fn assert_listed(result: &ProcessResult) {
    result.assert_success();
    result.assert_stdout_contains("buy milk");
}

fn assert_unpaged(result: &ProcessResult) {
    assert_listed(result);
    assert!(
        !result.stdout().contains("PAGED "),
        "expected an unpaged listing, got:\n{}",
        result.stdout()
    );
}

#[cfg(unix)]
#[test]
fn a_pageable_command_on_a_terminal_goes_through_the_pager() {
    let result = listing(&[("TDOO_PAGER", MARKING)]).run_pty(env!("CARGO_BIN_EXE_tdoo"), ["list"]);

    assert_listed(&result);
    result.assert_stdout_contains("PAGED ");
}

#[cfg(unix)]
#[test]
fn the_pager_the_environment_names_filters_the_page() {
    let result =
        listing(&[("TDOO_PAGER", "sed -n 1p")]).run_pty(env!("CARGO_BIN_EXE_tdoo"), ["list"]);

    result.assert_success();
    result.assert_stdout_contains("Your Todos");
    assert!(
        !result.stdout().contains("buy milk"),
        "expected only the first line, got:\n{}",
        result.stdout()
    );
}

#[test]
fn a_pipe_never_starts_the_pager() {
    assert_unpaged(
        &listing(&[("TDOO_PAGER", MARKING)]).run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]),
    );
}

#[cfg(unix)]
#[test]
fn no_pager_writes_the_listing_straight_to_stdout() {
    assert_unpaged(
        &listing(&[("TDOO_PAGER", MARKING)])
            .run_pty(env!("CARGO_BIN_EXE_tdoo"), ["list", "--no-pager"]),
    );
}

#[cfg(unix)]
#[test]
fn a_structured_encoding_never_starts_the_pager() {
    let result = listing(&[("TDOO_PAGER", MARKING)])
        .run_pty(env!("CARGO_BIN_EXE_tdoo"), ["list", "--output", "json"]);

    result.assert_success();
    assert!(
        !result.stdout().contains("PAGED "),
        "expected an unpaged document, got:\n{}",
        result.stdout()
    );
    let value: serde_json::Value = serde_json::from_str(result.stdout()).unwrap();
    assert_eq!(value["todos"][0]["title"], "buy milk");
}

#[cfg(unix)]
#[test]
fn a_pager_that_cannot_start_writes_the_bytes_it_would_have_paged() {
    let fell_back = listing(&[("TDOO_PAGER", "/nonexistent/pager")])
        .run_pty(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    let unpaged = listing(&[("TDOO_PAGER", MARKING)])
        .run_pty(env!("CARGO_BIN_EXE_tdoo"), ["list", "--no-pager"]);

    fell_back.assert_success();
    assert_eq!(fell_back.status().code(), Some(0));
    assert_eq!(fell_back.stdout_bytes(), unpaged.stdout_bytes());
}

#[cfg(unix)]
#[test]
fn a_pager_that_is_not_executable_writes_the_bytes_it_would_have_paged() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let pager = dir.path().join("pager");
    std::fs::write(&pager, "#!/bin/sh\ncat\n").unwrap();
    std::fs::set_permissions(&pager, std::fs::Permissions::from_mode(0o644)).unwrap();

    let fell_back = listing(&[("TDOO_PAGER", pager.to_str().unwrap())])
        .run_pty(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    let unpaged = listing(&[("TDOO_PAGER", MARKING)])
        .run_pty(env!("CARGO_BIN_EXE_tdoo"), ["list", "--no-pager"]);

    fell_back.assert_success();
    assert_eq!(fell_back.status().code(), Some(0));
    assert_eq!(fell_back.stdout_bytes(), unpaged.stdout_bytes());
}

#[cfg(unix)]
#[test]
fn a_pager_that_stops_reading_keeps_the_run_successful() {
    let result =
        listing(&[("TDOO_PAGER", "head -1")]).run_pty(env!("CARGO_BIN_EXE_tdoo"), ["list"]);

    result.assert_success();
    assert_eq!(result.status().code(), Some(0));
}
