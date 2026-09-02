// The roster case schema, executed for real against scripted stand-in
// binaries.

#![cfg(unix)]

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use common::script;
use corpus_runner::archetype::Archetype;
use corpus_runner::cases::run_cases;
use corpus_runner::manifest::{Evidence, GapEntry};
use corpus_runner::report::{CaseOutcome, CaseResult};
use corpus_runner::workspace::Isolation;

fn corpus_archetypes_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/archetypes")
}

fn run_suite(toml: &str, binary_body: &str) -> Vec<CaseResult> {
    run_suite_with_evidence(toml, binary_body, &BTreeMap::new(), Ok(""))
}

fn run_suite_with_evidence(
    toml: &str,
    binary_body: &str,
    gaps: &BTreeMap<String, GapEntry>,
    app_cargo_toml: Result<&str, &str>,
) -> Vec<CaseResult> {
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
    let isolation = Isolation::new(
        dir.path(),
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
    )
    .unwrap();
    let report = run_cases(
        &binary,
        &archetype.suite.cases,
        &dir.path().join("cases"),
        &isolation,
        gaps,
        app_cargo_toml,
    );
    assert!(report.built);
    report.cases
}

fn one(toml: &str, binary_body: &str) -> CaseResult {
    let mut results = run_suite(toml, binary_body);
    assert_eq!(results.len(), 1);
    results.remove(0)
}

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
        assert!(!archetype.suite.cases.is_empty());
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
fn json_subset_admits_extra_keys_but_not_a_wrong_value() {
    let expect = r#"
[[case]]
name = "json-subset"
stresses = "an envelope whose payload another spec defines"
expected = "pass"
[case.run]
argv = []
timeout_seconds = 5
[case.expect]
stdout_json_subset = '{"schema_version": 1}'
"#;
    let carried = one(
        expect,
        r#"printf '{"schema_version": 1, "data": ["anything"]}\n'"#,
    );
    assert_eq!(carried.outcome, CaseOutcome::Pass, "{:?}", carried.detail);

    let as_a_string = one(expect, r#"printf '{"schema_version": "1"}\n'"#);
    assert_eq!(as_a_string.outcome, CaseOutcome::Fail);

    let named_but_not_a_field = one(expect, r#"printf '{"flags": ["schema_version"]}\n'"#);
    assert_eq!(named_but_not_a_field.outcome, CaseOutcome::Fail);

    let array = expect.replace(r#"{"schema_version": 1}"#, r#"{"items": [1, 2]}"#);
    assert_eq!(
        one(&array, r#"printf '{"items": [1, 2], "extra": true}\n'"#).outcome,
        CaseOutcome::Pass
    );
    assert_eq!(
        one(&array, r#"printf '{"items": [1, 2, 3]}\n'"#).outcome,
        CaseOutcome::Fail,
        "a longer array is not a superset"
    );
    assert_eq!(
        one(&array, r#"printf '{"items": [1, 9]}\n'"#).outcome,
        CaseOutcome::Fail,
        "elements must match positionally"
    );
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

#[test]
fn line_suffix_assertion_rejects_duplicate_or_embedded_answer_sheet_tags() {
    let results = run_suite(
        r#"
[[case]]
name = "one-tag-per-line"
stresses = "answer-sheet structure"
expected = "pass"
[case.run]
argv = ["good"]
timeout_seconds = 5
[case.expect]
stdout_lines_end_with_once = ["<id:name>", "<id:region>"]

[[case]]
name = "duplicate-tag"
stresses = "answer-sheet structure"
expected = "pass"
[case.run]
argv = ["duplicate"]
timeout_seconds = 5
[case.expect]
stdout_lines_end_with_once = ["<id:name>", "<id:region>"]

[[case]]
name = "embedded-tag"
stresses = "answer-sheet structure"
expected = "pass"
[case.run]
argv = ["embedded"]
timeout_seconds = 5
[case.expect]
stdout_lines_end_with_once = ["<id:name>", "<id:region>"]
"#,
        r#"case "$1" in
  good) printf 'Name <id:name>\nRegion <id:region>\n' ;;
  duplicate) printf 'Name <id:name>\nAgain <id:name>\nRegion <id:region>\n' ;;
  embedded) printf 'Name <id:name> trailing\nRegion <id:region>\n' ;;
esac"#,
    );
    assert_eq!(
        results[0].outcome,
        CaseOutcome::Pass,
        "{:?}",
        results[0].detail
    );
    assert_eq!(results[1].outcome, CaseOutcome::Fail);
    assert!(results[1]
        .detail
        .as_deref()
        .unwrap()
        .contains("exactly one"));
    assert_eq!(results[2].outcome, CaseOutcome::Fail);
}

const ROWS: &str = r#"
mode=text
prev=""
for a in "$@"; do
  if [ "$prev" = "--output" ]; then mode="$a"; fi
  prev="$a"
done
case "$mode" in
  json) echo '{"stars":[{"name":"Aldebaran","constellation":"Taurus","magnitude":0.86},{"name":"Rigel","constellation":"Orion","magnitude":0.13}]}' ;;
  *) printf 'Stars\nAldebaran  Taurus  0.86\nRigel  Orion  0.13\n' ;;
esac
"#;

#[test]
fn row_assertions_bind_values_to_one_row() {
    let results = run_suite(
        r#"
[[case]]
name = "text-rows-are-associated"
stresses = "row association in rendered output"
expected = "pass"
[case.run]
argv = ["list", "--output", "text"]
timeout_seconds = 5
[case.expect]
stdout_row_contains = [["Aldebaran", "Taurus", "0.86"], ["Rigel", "Orion", "0.13"]]

[[case]]
name = "cross-row-bag-of-substrings-fails"
stresses = "row association in rendered output"
expected = "pass"
[case.run]
argv = ["list", "--output", "text"]
timeout_seconds = 5
[case.expect]
stdout_row_contains = [["Aldebaran", "Orion"]]

[[case]]
name = "json-rows-are-associated"
stresses = "row association in machine output"
expected = "pass"
[case.run]
argv = ["list", "--output", "json"]
timeout_seconds = 5
[case.expect]
stdout_json_rows = [["Aldebaran", "Taurus", "0.86"], ["Rigel", "Orion", "0.13"]]

[[case]]
name = "json-cross-row-group-fails"
stresses = "row association in machine output"
expected = "pass"
[case.run]
argv = ["list", "--output", "json"]
timeout_seconds = 5
[case.expect]
stdout_json_rows = [["Aldebaran", "0.13"]]

[[case]]
name = "json-rows-on-non-json-output-fails"
stresses = "row association in machine output"
expected = "pass"
[case.run]
argv = ["list", "--output", "text"]
timeout_seconds = 5
[case.expect]
stdout_json_rows = [["Aldebaran", "Taurus", "0.86"]]
"#,
        ROWS,
    );
    let outcomes: Vec<CaseOutcome> = results.iter().map(|r| r.outcome).collect();
    assert_eq!(
        outcomes,
        vec![
            CaseOutcome::Pass,
            CaseOutcome::Fail,
            CaseOutcome::Pass,
            CaseOutcome::Fail,
            CaseOutcome::Fail
        ],
        "{results:?}"
    );
    assert!(results[1]
        .detail
        .as_deref()
        .unwrap()
        .contains("no single stdout line"));
    assert!(results[3]
        .detail
        .as_deref()
        .unwrap()
        .contains("no single JSON element"));
    assert!(results[4]
        .detail
        .as_deref()
        .unwrap()
        .contains("not valid JSON"));
}

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
        // `sh` invents TERM when it is unset, so printenv rather than shell expansion.
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

#[test]
fn files_assertion_reads_the_sandbox_after_the_run() {
    let results = run_suite(
        r#"
[[case]]
name = "writes-expected-content"
stresses = "post-run sandbox assertion"
expected = "pass"
[case.run]
argv = ["alpha"]
timeout_seconds = 5
[case.expect]
exit_code = 0
[case.expect.files]
"conf/config_default" = "[core]\nproject = alpha\n"

[[case]]
name = "content-mismatch"
stresses = "post-run sandbox assertion"
expected = "pass"
[case.run]
argv = ["alpha"]
timeout_seconds = 5
[case.expect]
exit_code = 0
[case.expect.files]
"conf/config_default" = "[core]\nproject = wrong\n"

[[case]]
name = "missing-file"
stresses = "post-run sandbox assertion"
expected = "pass"
[case.run]
argv = ["alpha"]
timeout_seconds = 5
[case.expect]
exit_code = 0
[case.expect.files]
"conf/never-written" = "anything\n"
"#,
        r#"mkdir -p conf; printf '[core]\nproject = %s\n' "$1" > conf/config_default"#,
    );
    assert_eq!(
        results[0].outcome,
        CaseOutcome::Pass,
        "{:?}",
        results[0].detail
    );
    assert_eq!(results[1].outcome, CaseOutcome::Fail);
    assert!(
        results[1]
            .detail
            .as_deref()
            .unwrap()
            .contains("content differs"),
        "{:?}",
        results[1].detail
    );
    assert_eq!(results[2].outcome, CaseOutcome::Fail);
    assert!(
        results[2]
            .detail
            .as_deref()
            .unwrap()
            .contains("does not exist"),
        "{:?}",
        results[2].detail
    );
}

#[test]
fn files_absent_fails_when_the_path_exists() {
    let results = run_suite(
        r#"
[[case]]
name = "absent-as-expected"
stresses = "post-run sandbox assertion"
expected = "pass"
[case.run]
argv = ["skip"]
timeout_seconds = 5
[case.expect]
exit_code = 0
files_absent = ["conf/configurations/config_staging"]

[[case]]
name = "unexpectedly-present"
stresses = "post-run sandbox assertion"
expected = "pass"
[case.run]
argv = ["write"]
timeout_seconds = 5
[case.expect]
exit_code = 0
files_absent = ["conf/configurations/config_staging"]
"#,
        r#"if [ "$1" = "write" ]; then mkdir -p conf/configurations; touch conf/configurations/config_staging; fi"#,
    );
    assert_eq!(
        results[0].outcome,
        CaseOutcome::Pass,
        "{:?}",
        results[0].detail
    );
    assert_eq!(results[1].outcome, CaseOutcome::Fail);
    assert!(
        results[1]
            .detail
            .as_deref()
            .unwrap()
            .contains("must not exist"),
        "{:?}",
        results[1].detail
    );
}

#[test]
fn files_assertion_refuses_a_symlink_at_the_leaf() {
    let results = run_suite(
        r#"
[[case]]
name = "symlinked-target"
stresses = "post-run sandbox assertion"
expected = "pass"
[case.run]
argv = ["alpha"]
timeout_seconds = 5
[case.expect]
exit_code = 0
[case.expect.files]
"conf/config_default" = "irrelevant\n"
"#,
        r#"mkdir -p conf; echo elsewhere > conf/elsewhere; ln -s elsewhere conf/config_default"#,
    );
    assert_eq!(results[0].outcome, CaseOutcome::Fail);
    assert!(
        results[0]
            .detail
            .as_deref()
            .unwrap()
            .contains("not a regular file"),
        "{:?}",
        results[0].detail
    );
}

#[test]
fn files_assertion_refuses_a_symlinked_parent_directory() {
    let results = run_suite(
        r#"
[[case]]
name = "symlinked-parent"
stresses = "post-run sandbox assertion"
expected = "pass"
[case.run]
argv = ["alpha"]
timeout_seconds = 5
[case.expect]
exit_code = 0
[case.expect.files]
"conf/config_default" = "irrelevant\n"
"#,
        r#"mkdir -p elsewhere; echo elsewhere > elsewhere/config_default; ln -s elsewhere conf"#,
    );
    assert_eq!(results[0].outcome, CaseOutcome::Fail);
    // The inventory records the symlinked `conf` itself and never descends
    // into it, so nothing under it is ever recorded: the same "does not
    // exist" a produced app that simply never wrote the file would get.
    assert!(
        results[0]
            .detail
            .as_deref()
            .unwrap()
            .contains("does not exist"),
        "{:?}",
        results[0].detail
    );
}

#[test]
fn files_assertion_refuses_a_fifo() {
    let results = run_suite(
        r#"
[[case]]
name = "fifo-target"
stresses = "post-run sandbox assertion"
expected = "pass"
[case.run]
argv = ["alpha"]
timeout_seconds = 5
[case.expect]
exit_code = 0
[case.expect.files]
"conf/config_default" = "irrelevant\n"
"#,
        r#"mkdir -p conf; mkfifo conf/config_default"#,
    );
    assert_eq!(results[0].outcome, CaseOutcome::Fail);
    assert!(
        results[0]
            .detail
            .as_deref()
            .unwrap()
            .contains("not a regular file"),
        "{:?}",
        results[0].detail
    );
}

#[test]
fn files_absent_with_a_dangling_symlink_present_fails() {
    let results = run_suite(
        r#"
[[case]]
name = "dangling-symlink"
stresses = "post-run sandbox assertion"
expected = "pass"
[case.run]
argv = ["alpha"]
timeout_seconds = 5
[case.expect]
exit_code = 0
files_absent = ["conf/config_default"]
"#,
        r#"mkdir -p conf; ln -s /nonexistent/target conf/config_default"#,
    );
    assert_eq!(results[0].outcome, CaseOutcome::Fail);
    assert!(
        results[0]
            .detail
            .as_deref()
            .unwrap()
            .contains("must not exist"),
        "{:?}",
        results[0].detail
    );
}

#[test]
fn files_absent_ignores_content_behind_a_symlinked_directory() {
    // A naive symlink-following walk would find `conf/secret.txt` (via
    // `elsewhere/secret.txt`) and fail this files_absent; the inventory
    // records the symlinked `conf` as `Other` and never descends into it,
    // so the case passes.
    let results = run_suite(
        r#"
[[case]]
name = "symlinked-parent-hides-content"
stresses = "post-run sandbox assertion"
expected = "pass"
[case.run]
argv = ["alpha"]
timeout_seconds = 5
[case.expect]
exit_code = 0
files_absent = ["conf/secret.txt"]
"#,
        r#"mkdir -p elsewhere; echo leaked > elsewhere/secret.txt; ln -s elsewhere conf"#,
    );
    assert_eq!(
        results[0].outcome,
        CaseOutcome::Pass,
        "{:?}",
        results[0].detail
    );
}

#[test]
fn files_assertion_accepts_a_crlf_file_normalizing_to_the_expectation() {
    let results = run_suite(
        r#"
[[case]]
name = "crlf-target"
stresses = "post-run sandbox assertion"
expected = "pass"
[case.run]
argv = ["alpha"]
timeout_seconds = 5
[case.expect]
exit_code = 0
[case.expect.files]
"greeting.txt" = "hello\n"
"#,
        r#"printf 'hello\r\n' > greeting.txt"#,
    );
    assert_eq!(
        results[0].outcome,
        CaseOutcome::Pass,
        "{:?}",
        results[0].detail
    );
}

#[test]
fn an_over_budget_sandbox_is_a_case_error() {
    let results = run_suite(
        r#"
[[case]]
name = "oversized-sandbox"
stresses = "post-run sandbox assertion"
expected = "pass"
[case.run]
argv = ["alpha"]
timeout_seconds = 5
[case.expect]
exit_code = 0
[case.expect.files]
"conf/config_default" = "short\n"
"#,
        r#"mkdir -p conf; head -c 2000000 /dev/zero | tr '\0' 'x' > conf/config_default"#,
    );
    assert_eq!(results[0].outcome, CaseOutcome::Fail);
    assert!(
        results[0]
            .detail
            .as_deref()
            .unwrap()
            .contains("inventory budget"),
        "{:?}",
        results[0].detail
    );
}

#[test]
fn a_backgrounded_descendant_is_killed_before_assertions_run() {
    // Inlined rather than run_suite: the sandbox (`dir`) must survive past
    // the run to prove the write never lands even after the descendant's
    // sleep would have elapsed, not just that the assertion beat it there.
    let dir = tempfile::tempdir().unwrap();
    let binary = script(
        dir.path(),
        "fake",
        // Redirected away from the captured pipes: inherited, they would
        // stay open until the backgrounded job exits on its own, which
        // would delay case completion until well past its write.
        r#"(sleep 0.3; echo written > race.txt) >/dev/null 2>&1 &
exit 0"#,
    );
    let archetype_dir = dir.path().join("archetypes/fake");
    fs::create_dir_all(&archetype_dir).unwrap();
    fs::write(archetype_dir.join("spec.md"), "spec").unwrap();
    fs::write(
        archetype_dir.join("acceptance.toml"),
        r#"schema = 1
archetype = "fake"

[[case]]
name = "background-writer"
stresses = "process-group reaping before assertions"
expected = "pass"
[case.run]
argv = []
timeout_seconds = 5
[case.expect]
exit_code = 0
files_absent = ["race.txt"]
"#,
    )
    .unwrap();
    let archetype = Archetype::load(&dir.path().join("archetypes"), "fake").unwrap();
    let isolation = Isolation::new(
        dir.path(),
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
    )
    .unwrap();
    let report = run_cases(
        &binary,
        &archetype.suite.cases,
        &dir.path().join("cases"),
        &isolation,
        &BTreeMap::new(),
        Ok(""),
    );
    assert!(report.built);
    assert_eq!(
        report.cases[0].outcome,
        CaseOutcome::Pass,
        "{:?}",
        report.cases[0].detail
    );

    // Prove the descendant was actually killed, not just outrun by a fast
    // assertion: wait past its sleep and confirm the write never lands.
    std::thread::sleep(std::time::Duration::from_millis(600));
    assert!(
        !dir.path().join("cases/background-writer/race.txt").exists(),
        "the backgrounded writer was not killed before it could write"
    );
}

#[test]
fn evidence_absent_reports_hand_rolled_pass() {
    let mut gaps = BTreeMap::new();
    gaps.insert(
        "PAR01".to_string(),
        GapEntry::Evidenced {
            text: "named sets are specced past current capability".to_string(),
            evidence: "uses-crate:clapfig".to_string(),
        },
    );
    let toml = r#"
[[case]]
name = "gap-passes-without-the-crate"
stresses = "hand-rolled pass detection"
expected = "fail"
gap = "PAR01"
reason = "named sets not built yet"
[case.run]
argv = []
timeout_seconds = 5
[case.expect]
exit_code = 0
stdout = "hello\n"
"#;

    let without_crate = run_suite_with_evidence(
        toml,
        "echo hello",
        &gaps,
        Ok("[dependencies]\nserde = \"1\"\n"),
    );
    assert_eq!(without_crate[0].outcome, CaseOutcome::HandRolledPass);
    assert!(!without_crate[0].outcome.is_expected());

    let with_crate = run_suite_with_evidence(
        toml,
        "echo hello",
        &gaps,
        Ok("[dependencies]\nclapfig = \"0.24\"\n"),
    );
    assert_eq!(with_crate[0].outcome, CaseOutcome::UnexpectedPass);

    // Unreadable, not merely absent: the evidence claim could not be
    // checked at all, so this must not read as either a framework win
    // (unexpected-pass) or a hand-rolled one — it's a case error.
    let unreadable_cargo_toml =
        run_suite_with_evidence(toml, "echo hello", &gaps, Err("permission denied"));
    assert_eq!(unreadable_cargo_toml[0].outcome, CaseOutcome::Fail);
    assert!(
        unreadable_cargo_toml[0]
            .detail
            .as_deref()
            .unwrap()
            .contains("could not be checked"),
        "{:?}",
        unreadable_cargo_toml[0].detail
    );

    let without_evidence = run_suite(toml, "echo hello");
    assert_eq!(without_evidence[0].outcome, CaseOutcome::UnexpectedPass);
}

#[test]
fn config_layering_gaps_declare_the_clapfig_evidence() {
    for (archetype_name, gap) in [
        ("gitlike", "PAR01"),
        ("cargolike", "PAR01"),
        ("gcloudlike", "PAR05"),
    ] {
        let archetype = Archetype::load(&corpus_archetypes_dir(), archetype_name).unwrap();
        assert_eq!(
            archetype.gap_evidence(gap),
            Some(Evidence::UsesCrate("clapfig")),
            "{archetype_name}'s {gap} gap must declare uses-crate:clapfig evidence"
        );

        // The evidence check itself: a passing gap case in a workspace
        // without clapfig reports hand-rolled-pass, not unexpected-pass.
        let mut gaps = BTreeMap::new();
        gaps.insert(
            gap.to_string(),
            GapEntry::Evidenced {
                text: "irrelevant for this check".to_string(),
                evidence: "uses-crate:clapfig".to_string(),
            },
        );
        let toml = format!(
            r#"
[[case]]
name = "gap-passes-without-clapfig"
stresses = "hand-rolled pass detection"
expected = "fail"
gap = "{gap}"
reason = "named sets not built yet"
[case.run]
argv = []
timeout_seconds = 5
[case.expect]
exit_code = 0
stdout = "hello\n"
"#
        );
        let without_clapfig = run_suite_with_evidence(
            &toml,
            "echo hello",
            &gaps,
            Ok("[dependencies]\nserde = \"1\"\n"),
        );
        assert_eq!(
            without_clapfig[0].outcome,
            CaseOutcome::HandRolledPass,
            "{archetype_name}'s {gap} gap case must report hand-rolled-pass without clapfig"
        );

        let with_clapfig = run_suite_with_evidence(
            &toml,
            "echo hello",
            &gaps,
            Ok("[dependencies]\nclapfig = \"0.24\"\n"),
        );
        assert_eq!(with_clapfig[0].outcome, CaseOutcome::UnexpectedPass);
    }
}
