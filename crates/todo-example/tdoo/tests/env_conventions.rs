mod common;

use standout_test::TestHarness;

const ESC: char = '\u{1b}';

const CONVENTION_VARS: [&str; 4] = ["NO_COLOR", "CLICOLOR", "CLICOLOR_FORCE", "TERM"];

fn conventions(vars: &[(&str, &str)]) -> TestHarness {
    let mut harness = common::tdoo();
    for key in CONVENTION_VARS {
        harness = match vars.iter().find(|(name, _)| *name == key) {
            Some((_, value)) => harness.env(key, *value),
            None => harness.env_remove(key),
        };
    }
    harness
}

fn assert_plain(result: &standout_test::ProcessResult) {
    result.assert_success();
    assert!(
        !result.stdout().contains(ESC),
        "expected a plain page, got ANSI:\n{:?}",
        result.stdout()
    );
    result.assert_stdout_contains("buy milk");
}

fn assert_ansi(result: &standout_test::ProcessResult) {
    result.assert_success();
    assert!(
        result.stdout().contains(ESC),
        "expected ANSI escapes, got a plain page:\n{:?}",
        result.stdout()
    );
}

#[test]
fn auto_through_a_pipe_renders_plain() {
    let result = conventions(&[]).run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_plain(&result);
}

#[test]
fn no_color_keeps_a_piped_run_plain() {
    let result =
        conventions(&[("NO_COLOR", "1")]).run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_plain(&result);
}

#[test]
fn term_dumb_keeps_a_piped_run_plain() {
    let result = conventions(&[("TERM", "dumb")]).run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_plain(&result);
}

#[cfg(unix)]
#[test]
fn auto_on_a_pty_renders_with_ansi() {
    let result =
        conventions(&[("TERM", "xterm-256color")]).run_pty(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_ansi(&result);
}

#[cfg(unix)]
#[test]
fn no_color_suppresses_ansi_on_a_pty() {
    let result = conventions(&[("TERM", "xterm-256color"), ("NO_COLOR", "1")])
        .run_pty(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_plain(&result);
}

#[cfg(unix)]
#[test]
fn term_dumb_suppresses_ansi_on_a_pty() {
    let result = conventions(&[("TERM", "dumb")]).run_pty(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_plain(&result);
}

#[test]
fn the_force_path_does_not_reach_auto_mode() {
    let result =
        conventions(&[("CLICOLOR_FORCE", "1")]).run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_plain(&result);
}

#[test]
fn color_always_through_a_pipe_emits_ansi() {
    let result =
        conventions(&[]).run_process(env!("CARGO_BIN_EXE_tdoo"), ["list", "--color", "always"]);
    assert_ansi(&result);
}

#[test]
fn color_always_overrides_no_color() {
    let result = conventions(&[("NO_COLOR", "1")])
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list", "--color", "always"]);
    assert_ansi(&result);
}

#[test]
fn color_never_overrides_clicolor_force() {
    let result = conventions(&[("CLICOLOR_FORCE", "1")])
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list", "--color", "never"]);
    assert_plain(&result);
}

#[test]
fn the_term_color_key_is_read_from_its_environment_spelling() {
    let result = conventions(&[])
        .env("TDOO__TERM__COLOR", "always")
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_ansi(&result);
}

/// The configured key at the process edge, where `NO_COLOR` outranks it.
#[test]
fn the_term_color_key_is_read_from_the_file() {
    let configured = conventions(&[])
        .fixture("tdoo.toml", "[term]\ncolor = \"always\"\n")
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_ansi(&configured);

    let vetoed = conventions(&[("NO_COLOR", "1")])
        .fixture("tdoo.toml", "[term]\ncolor = \"always\"\n")
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_plain(&vetoed);
}
