// The full `run()` orchestration over a fixture archetype, hermetically,
// exercising the expected-fail mapping (the same fake-`cargo` seam as
// `hermetic_loop.rs`). Runs in its own test binary because it prepends to
// the process-wide PATH.

#![cfg(unix)]

mod common;

use std::fs;
use std::path::Path;

use corpus_runner::report::CaseOutcome;
use corpus_runner::{run, RunConfig, Timeouts};

const CASELIKE: &str = r#"cmd="$1"
mode=text
prev=""
for a in "$@"; do
  if [ "$prev" = "--output" ]; then mode="$a"; fi
  prev="$a"
done
case "$cmd" in
  greet)
    case "$mode" in
      json) echo '{"greeting":"hello"}' ;;
      *) echo 'hello' ;;
    esac
    ;;
  home) printf '%s\n' "$HOME" ;;
  *) echo "caselike: unknown command" >&2; exit 2 ;;
esac
"#;

const ACCEPTANCE: &str = r#"
schema = 1
archetype = "caselike"

[[case]]
name = "greet-exact"
stresses = "exact piped bytes"
expected = "pass"
[case.run]
argv = ["greet"]
timeout_seconds = 5
[case.expect]
exit_code = 0
stdout = "hello\n"
stderr = ""

[[case]]
name = "greet-wrong-expectation"
stresses = "a failing case is a finding, not a runner error"
expected = "pass"
[case.run]
argv = ["greet"]
timeout_seconds = 5
[case.expect]
stdout = "goodbye\n"

[[case]]
name = "specced-past-capability"
stresses = "gap cases stay expected-fail"
expected = "fail"
gap = "PARXX"
reason = "the fixture cannot do this on purpose"
[case.run]
argv = ["future-feature"]
timeout_seconds = 5
[case.expect]
exit_code = 0

[invariants]
modes = ["text", "term", "json"]
colors = ["off", "on"]
[[invariants.theme]]
name = "application"
[[invariants.command]]
argv = ["greet"]
contract = "rendered"
"#;

#[test]
fn roster_archetype_completes_the_loop_with_case_results() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let scratch = tempfile::tempdir().unwrap();

    let archetypes = scratch.path().join("archetypes");
    let archetype_dir = archetypes.join("caselike");
    fs::create_dir_all(&archetype_dir).unwrap();
    fs::write(archetype_dir.join("spec.md"), "# caselike\n").unwrap();
    fs::write(archetype_dir.join("acceptance.toml"), ACCEPTANCE).unwrap();

    let bin_dir = scratch.path().join("bin");
    common::install_fake_cargo(&bin_dir, "caselike", CASELIKE);

    common::questionnaire_agent(
        &bin_dir,
        "agent.sh",
        "",
        &[
            ("summary", "Implemented caselike from SPEC.md."),
            ("sources.docs", "docs/index.md"),
            ("sources.external", "none"),
            ("confidence", "high"),
        ],
        false,
    );

    let config = RunConfig {
        archetype: "caselike".to_string(),
        archetypes_dir: archetypes,
        runs_dir: scratch.path().join("runs"),
        docs_dir: repo.join("docs"),
        agent_cmd: "agent.sh".to_string(),
        framework_version: "8.1.1".to_string(),
        timeouts: Timeouts::default(),
    };

    let (report, run_dir) = run(&config).unwrap();

    assert!(run_dir.join("report.json").is_file());
    assert!(
        report.acceptance.built,
        "{:?}",
        report.acceptance.build_detail
    );

    let outcomes: Vec<CaseOutcome> = report.acceptance.cases.iter().map(|c| c.outcome).collect();
    assert_eq!(
        outcomes,
        vec![
            CaseOutcome::Pass,
            CaseOutcome::Fail,
            CaseOutcome::ExpectedFail
        ]
    );

    assert!(run_dir.join("cases/greet-exact").is_dir());

    assert!(!report.invariants.is_empty());
    let failed: Vec<_> = report
        .invariants
        .iter()
        .filter(|c| c.status == corpus_runner::report::InvariantStatus::Fail)
        .collect();
    assert!(failed.is_empty(), "{failed:?}");

    let json = fs::read_to_string(run_dir.join("report.json")).unwrap();
    assert!(json.contains("\"expected-fail\""), "cases serialized");
}
