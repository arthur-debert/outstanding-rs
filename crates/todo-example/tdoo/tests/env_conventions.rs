use standout::OutputMode;
use standout_test::TestHarness;

const ESC: char = '\u{1b}';

const STORE: &str = r#"{"todos":[{"id":1,"title":"buy milk","done":false}],"next_id":1}"#;

const CONVENTION_VARS: [&str; 4] = ["NO_COLOR", "CLICOLOR", "CLICOLOR_FORCE", "TERM"];

fn conventions(vars: &[(&str, &str)]) -> TestHarness {
    let mut harness = TestHarness::new()
        .fixture("todos.json", STORE)
        .env("TDOO__STORE", "todos.json");
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
    let result = conventions(&[])
        .output_mode(OutputMode::Auto)
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_plain(&result);
}

#[test]
fn no_color_keeps_a_piped_run_plain() {
    let result = conventions(&[("NO_COLOR", "1")])
        .output_mode(OutputMode::Auto)
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_plain(&result);
}

#[test]
fn term_dumb_keeps_a_piped_run_plain() {
    let result = conventions(&[("TERM", "dumb")])
        .output_mode(OutputMode::Auto)
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_plain(&result);
}

#[cfg(unix)]
#[test]
fn auto_on_a_pty_renders_with_ansi() {
    let result = conventions(&[("TERM", "xterm-256color")])
        .output_mode(OutputMode::Auto)
        .run_pty(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_ansi(&result);
}

#[cfg(unix)]
#[test]
fn no_color_suppresses_ansi_on_a_pty() {
    let result = conventions(&[("TERM", "xterm-256color"), ("NO_COLOR", "1")])
        .output_mode(OutputMode::Auto)
        .run_pty(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_plain(&result);
}

#[cfg(unix)]
#[test]
fn term_dumb_suppresses_ansi_on_a_pty() {
    let result = conventions(&[("TERM", "dumb")])
        .output_mode(OutputMode::Auto)
        .run_pty(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_plain(&result);
}

#[test]
fn explicit_term_through_a_pipe_emits_ansi() {
    let result = conventions(&[])
        .output_mode(OutputMode::Term)
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_ansi(&result);
}

#[test]
fn explicit_term_overrides_no_color() {
    let result = conventions(&[("NO_COLOR", "1")])
        .output_mode(OutputMode::Term)
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_ansi(&result);
}

#[test]
fn explicit_text_overrides_clicolor_force() {
    let result = conventions(&[("CLICOLOR_FORCE", "1")])
        .output_mode(OutputMode::Text)
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_plain(&result);
}

#[test]
fn clicolor_force_reaches_term_mode_through_consoles_gate() {
    let result = conventions(&[("CLICOLOR_FORCE", "1")])
        .output_mode(OutputMode::Term)
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_ansi(&result);
}

#[test]
fn no_color_does_not_reach_the_force_path() {
    let result = conventions(&[("CLICOLOR_FORCE", "1"), ("NO_COLOR", "1")])
        .output_mode(OutputMode::Term)
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_ansi(&result);
}

#[test]
fn the_force_path_does_not_reach_auto_mode() {
    let result = conventions(&[("CLICOLOR_FORCE", "1")])
        .output_mode(OutputMode::Auto)
        .run_process(env!("CARGO_BIN_EXE_tdoo"), ["list"]);
    assert_plain(&result);
}
