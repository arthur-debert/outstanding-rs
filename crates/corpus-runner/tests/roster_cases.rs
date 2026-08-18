//! The roster case schema, executed for real: loading, the documented run
//! semantics (scrubbed baseline env, sandbox files and cwd, pty attachment,
//! scripted stdin, per-case timeout), the assertion vocabulary, and the
//! expected-fail mapping — all against scripted stand-in binaries, the same
//! way `phases.rs` covers the check schema.

// Unix-only: the stand-in binaries are `sh` scripts made executable via
// `PermissionsExt`, and pty attachment is a Unix object.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use corpus_runner::archetype::{Archetype, Suite};
use corpus_runner::cases::run_cases;
use corpus_runner::report::{CaseOutcome, CaseResult};

/// Writes an executable script and returns its path.
fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// Loads `toml` as archetype `fake`'s acceptance suite and runs its cases
/// against `binary_body`, returning the per-case results.
fn run_suite(toml: &str, binary_body: &str) -> Vec<CaseResult> {
    let dir = tempfile::tempdir().unwrap();
    let binary = script(dir.path(), "fake", binary_body);
    let archetype_dir = dir.path().join("archetypes/fake");
    fs::create_dir_all(&archetype_dir).unwrap();
    fs::write(archetype_dir.join("spec.md"), "spec").unwrap();
    fs::write(
        archetype_dir.join("acceptance.toml"),
        format!("schema = 1\narchetype = \"fake\"\n{toml}"),
    )
    .unwrap();
    let archetype = Archetype::load(&dir.path().join("archetypes"), "fake").unwrap();
    let Suite::Cases(suite) = &archetype.suite else {
        panic!("fixture carries the roster case schema");
    };
    let report = run_cases(&binary, &suite.cases, &dir.path().join("cases"));
    assert!(report.built);
    assert!(report.checks.is_empty());
    report.cases
}

fn one(toml: &str, binary_body: &str) -> CaseResult {
    let mut results = run_suite(toml, binary_body);
    assert_eq!(results.len(), 1);
    results.remove(0)
}

// ---------------------------------------------------------------------------
// Loading and validation
// ---------------------------------------------------------------------------

#[test]
fn pilot_roster_suites_load_as_case_suites() {
    let archetypes = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/archetypes");
    for name in [
        "gitlike",
        "systemdlike",
        "formlike",
        "ghlike",
        "tflike",
        "jjlike",
    ] {
        let archetype = Archetype::load(&archetypes, name).unwrap();
        assert_eq!(archetype.binary(), name, "roster names double as binaries");
        let Suite::Cases(suite) = &archetype.suite else {
            panic!("{name} must load as the roster case schema");
        };
        assert!(!suite.cases.is_empty());
    }
}

#[test]
fn pilot_archetypes_carry_invariant_commands() {
    let archetypes = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/archetypes");
    for name in ["gitlike", "systemdlike", "formlike", "ghlike"] {
        let archetype = Archetype::load(&archetypes, name).unwrap();
        assert!(
            !archetype.invariants().commands.is_empty(),
            "{name} must name ROB01 invariant-matrix commands"
        );
    }
}

#[test]
fn gap_marker_without_reason_is_a_load_error() {
    let dir = tempfile::tempdir().unwrap();
    let archetype_dir = dir.path().join("archetypes/fake");
    fs::create_dir_all(&archetype_dir).unwrap();
    fs::write(archetype_dir.join("spec.md"), "spec").unwrap();
    fs::write(
        archetype_dir.join("acceptance.toml"),
        r#"
schema = 1
archetype = "fake"

[[case]]
name = "gap-without-reason"
stresses = "validation"
expected = "fail"
gap = "PAR01"
[case.run]
argv = []
timeout_seconds = 5
[case.expect]
exit_code = 0
"#,
    )
    .unwrap();
    let err = Archetype::load(&dir.path().join("archetypes"), "fake").unwrap_err();
    assert!(err.to_string().contains("gap+reason"), "{err:#}");
}

#[test]
fn assertion_free_case_is_a_load_error() {
    let dir = tempfile::tempdir().unwrap();
    let archetype_dir = dir.path().join("archetypes/fake");
    fs::create_dir_all(&archetype_dir).unwrap();
    fs::write(archetype_dir.join("spec.md"), "spec").unwrap();
    fs::write(
        archetype_dir.join("acceptance.toml"),
        r#"
schema = 1
archetype = "fake"

[[case]]
name = "asserts-nothing"
stresses = "validation"
expected = "pass"
[case.run]
argv = []
timeout_seconds = 5
[case.expect]
"#,
    )
    .unwrap();
    let err = Archetype::load(&dir.path().join("archetypes"), "fake").unwrap_err();
    assert!(err.to_string().contains("asserts nothing"), "{err:#}");
}

// ---------------------------------------------------------------------------
// Assertion vocabulary
// ---------------------------------------------------------------------------

#[test]
fn exact_stream_and_exit_assertions_pass_and_fail() {
    let results = run_suite(
        r#"
[[case]]
name = "exact-match"
stresses = "exact bytes"
expected = "pass"
[case.run]
argv = ["greet"]
timeout_seconds = 5
[case.expect]
exit_code = 0
stdout = "hello\n"
stderr = ""

[[case]]
name = "exact-mismatch"
stresses = "exact bytes"
expected = "pass"
[case.run]
argv = ["greet"]
timeout_seconds = 5
[case.expect]
exit_code = 0
stdout = "goodbye\n"
"#,
        "echo hello",
    );
    assert_eq!(results[0].outcome, CaseOutcome::Pass);
    assert!(results[0].detail.is_none());
    assert_eq!(results[1].outcome, CaseOutcome::Fail);
    let detail = results[1].detail.as_deref().unwrap();
    assert!(detail.contains("stdout differs"), "{detail}");
    assert!(detail.contains("--- stdout ---"), "{detail}");
}

#[test]
fn json_assertion_is_semantic_not_byte() {
    let result = one(
        r#"
[[case]]
name = "json-semantic"
stresses = "machine output"
expected = "pass"
[case.run]
argv = []
timeout_seconds = 5
[case.expect]
stdout_json = '{"b": 2, "a": 1}'
"#,
        r#"printf '{ "a" : 1, "b" : 2 }\n'"#,
    );
    assert_eq!(result.outcome, CaseOutcome::Pass, "{:?}", result.detail);
}

#[test]
fn contains_and_not_contains_families_apply_per_stream() {
    let result = one(
        r#"
[[case]]
name = "contains"
stresses = "substring vocabulary"
expected = "pass"
[case.run]
argv = []
timeout_seconds = 5
[case.expect]
stdout_contains = ["plain"]
stdout_not_contains = ["\u001b["]
stderr_contains = ["warned"]
stderr_not_contains = ["fatal"]
"#,
        "echo plain; echo warned >&2",
    );
    assert_eq!(result.outcome, CaseOutcome::Pass, "{:?}", result.detail);
}

// ---------------------------------------------------------------------------
// Run semantics: environment, sandbox, stdin, tty, timeout
// ---------------------------------------------------------------------------

#[test]
fn baseline_env_is_scrubbed_and_case_env_is_complete() {
    std::env::set_var("CORPUS_CASE_CANARY", "leaked");
    std::env::set_var("TERM", "xterm-256color");
    let result = one(
        r#"
[[case]]
name = "env-discipline"
stresses = "scrubbed baseline"
expected = "pass"
[case.run]
argv = []
env = { CASE_VAR = "present" }
timeout_seconds = 5
[case.expect]
exit_code = 0
stdout = "canary=unset\nterm=unset\ncase=present\nlang=C.UTF-8\n"
"#,
        // `sh` invents TERM for itself when it is unset, so the child's real
        // environment is probed with printenv rather than shell expansion.
        r#"printf 'canary=%s\nterm=%s\ncase=%s\nlang=%s\n' "$(printenv CORPUS_CASE_CANARY || echo unset)" "$(printenv TERM || echo unset)" "$CASE_VAR" "$LANG""#,
    );
    assert_eq!(result.outcome, CaseOutcome::Pass, "{:?}", result.detail);
}

#[test]
fn home_points_into_the_sandbox_and_files_seed_it() {
    let result = one(
        r#"
[[case]]
name = "sandbox-home"
stresses = "HOME in sandbox + seeded files + cwd"
expected = "pass"
[case.run]
argv = []
cwd = "project/sub"
timeout_seconds = 5
[case.run.files]
"project/sub/marker.txt" = "found\n"
".config.toml" = "root\n"
[case.expect]
exit_code = 0
stdout = "found\nroot\n"
"#,
        // cwd is project/sub; HOME is the sandbox root holding .config.toml.
        r#"cat marker.txt; cat "$HOME/.config.toml""#,
    );
    assert_eq!(result.outcome, CaseOutcome::Pass, "{:?}", result.detail);
}

#[test]
fn sandbox_escaping_file_paths_fail_the_case_not_the_host() {
    let result = one(
        r#"
[[case]]
name = "escape-attempt"
stresses = "sandbox boundary"
expected = "pass"
[case.run]
argv = []
timeout_seconds = 5
[case.run.files]
"../outside.txt" = "nope\n"
[case.expect]
exit_code = 0
"#,
        "true",
    );
    assert_eq!(result.outcome, CaseOutcome::Fail);
    assert!(
        result.detail.as_deref().unwrap().contains("escapes"),
        "{:?}",
        result.detail
    );
}

#[test]
fn omitted_stdin_is_piped_and_at_eof() {
    let result = one(
        r#"
[[case]]
name = "stdin-eof"
stresses = "adversarial non-interactive default"
expected = "pass"
[case.run]
argv = []
timeout_seconds = 5
[case.expect]
exit_code = 0
stdout = "lines=0\n"
"#,
        r#"n=0; while read -r line; do n=$((n+1)); done; printf 'lines=%s\n' "$n""#,
    );
    assert_eq!(result.outcome, CaseOutcome::Pass, "{:?}", result.detail);
}

#[test]
fn omitted_stdin_stays_piped_when_an_output_stream_uses_a_pty() {
    let result = one(
        r#"
[[case]]
name = "mixed-pty-stdin-eof"
stresses = "piped stdin with terminal stdout"
expected = "pass"
[case.run]
argv = []
tty = ["stdout"]
timeout_seconds = 5
[case.expect]
exit_code = 0
stdout = "stdin=pipe\nlines=0\n"
"#,
        r#"if [ -p /dev/stdin ]; then kind=pipe; else kind=other; fi
n=0
while read -r line; do n=$((n+1)); done
printf 'stdin=%s\nlines=%s\n' "$kind" "$n""#,
    );
    assert_eq!(result.outcome, CaseOutcome::Pass, "{:?}", result.detail);
}

#[test]
fn stdin_string_on_a_pipe_is_content_then_eof() {
    let result = one(
        r#"
[[case]]
name = "stdin-piped-content"
stresses = "scripted piped input"
expected = "pass"
[case.run]
argv = []
stdin = "alpha\nbeta\n"
timeout_seconds = 5
[case.expect]
exit_code = 0
stdout = "alpha\nbeta\n"
"#,
        "cat",
    );
    assert_eq!(result.outcome, CaseOutcome::Pass, "{:?}", result.detail);
}

#[test]
fn tty_stdout_is_a_terminal_and_pipes_are_not() {
    let results = run_suite(
        r#"
[[case]]
name = "attended-stdout"
stresses = "pty attachment"
expected = "pass"
[case.run]
argv = []
tty = ["stdout"]
timeout_seconds = 5
[case.expect]
exit_code = 0
stdout = "tty=yes\n"

[[case]]
name = "piped-stdout"
stresses = "pipe default"
expected = "pass"
[case.run]
argv = []
timeout_seconds = 5
[case.expect]
exit_code = 0
stdout = "tty=no\n"
"#,
        r#"if [ -t 1 ]; then printf 'tty=yes\n'; else printf 'tty=no\n'; fi"#,
    );
    assert_eq!(
        results[0].outcome,
        CaseOutcome::Pass,
        "{:?}",
        results[0].detail
    );
    assert_eq!(
        results[1].outcome,
        CaseOutcome::Pass,
        "{:?}",
        results[1].detail
    );
}

#[test]
fn tty_stdin_delivers_keystrokes() {
    let result = one(
        r#"
[[case]]
name = "attended-stdin"
stresses = "keystroke transport"
expected = "pass"
[case.run]
argv = []
tty = ["stdin"]
stdin = "first\nsecond\n"
timeout_seconds = 5
[case.expect]
exit_code = 0
stdout = "got first\ngot second\n"
"#,
        r#"[ -t 0 ] || exit 9; read -r a; read -r b; printf 'got %s\ngot %s\n' "$a" "$b""#,
    );
    assert_eq!(result.outcome, CaseOutcome::Pass, "{:?}", result.detail);
}

#[test]
fn tty_stdin_sends_eof_while_an_output_stream_keeps_the_pty_open() {
    let result = one(
        r#"
[[case]]
name = "attended-stdin-eof"
stresses = "scripted pty input followed by terminal EOF"
expected = "pass"
[case.run]
argv = []
tty = ["stdin", "stderr"]
stdin = "alpha\nbeta\n"
timeout_seconds = 2
[case.expect]
exit_code = 0
stdout = "alpha\nbeta\n"
"#,
        "cat",
    );
    assert_eq!(result.outcome, CaseOutcome::Pass, "{:?}", result.detail);
}

#[test]
fn exceeding_timeout_seconds_fails_the_case() {
    let result = one(
        r#"
[[case]]
name = "never-hangs"
stresses = "the never-hang bound"
expected = "pass"
[case.run]
argv = []
timeout_seconds = 1
[case.expect]
exit_code = 0
"#,
        "sleep 30",
    );
    assert_eq!(result.outcome, CaseOutcome::Fail);
    assert!(
        result.detail.as_deref().unwrap().contains("timed out"),
        "{:?}",
        result.detail
    );
}

// ---------------------------------------------------------------------------
// Expected-fail mapping
// ---------------------------------------------------------------------------

#[test]
fn gap_cases_report_expected_fail_and_unexpected_pass() {
    let results = run_suite(
        r#"
[[case]]
name = "open-gap"
stresses = "capability specced past the framework"
expected = "fail"
gap = "PAR01"
reason = "not built yet"
[case.run]
argv = ["missing-feature"]
timeout_seconds = 5
[case.expect]
exit_code = 0
stdout = "impossible today\n"

[[case]]
name = "silently-closed-gap"
stresses = "a gap case that passes is news"
expected = "fail"
gap = "PAR01"
reason = "supposedly not built yet"
[case.run]
argv = []
timeout_seconds = 5
[case.expect]
exit_code = 0
stdout = "hello\n"
"#,
        "echo hello",
    );
    assert_eq!(results[0].outcome, CaseOutcome::ExpectedFail);
    assert_eq!(results[0].gap.as_deref(), Some("PAR01"));
    assert!(results[0].outcome.is_expected());
    assert_eq!(results[1].outcome, CaseOutcome::UnexpectedPass);
    assert!(!results[1].outcome.is_expected());
}

#[test]
fn expected_fail_does_not_hide_case_execution_errors() {
    let result = one(
        r#"
[[case]]
name = "broken-gap"
stresses = "runner errors stay visible"
expected = "fail"
gap = "PAR01"
reason = "a known behavioral gap"
[case.run]
argv = []
timeout_seconds = 5
[case.run.files]
"../outside.txt" = "must not be written\n"
[case.expect]
exit_code = 0
"#,
        "true",
    );
    assert_eq!(result.outcome, CaseOutcome::Fail);
    assert!(!result.outcome.is_expected());
    assert!(
        result
            .detail
            .as_deref()
            .unwrap()
            .contains("case execution error"),
        "{:?}",
        result.detail
    );
}
