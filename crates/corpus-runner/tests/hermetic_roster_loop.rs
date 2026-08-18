//! The full `run()` orchestration over a roster-schema archetype,
//! hermetically: the same fake-`cargo` seam as `hermetic_loop.rs`, but the
//! acceptance phase dispatches to the case executor — proving the
//! `Suite::Cases` path end to end: case results (pass, fail, expected-fail)
//! in the report, per-case sandboxes under the run directory, and the
//! invariant matrix swept under the cases' scrubbed baseline env.
//!
//! Runs in its own test binary because it prepends to the process-wide
//! PATH.

// Unix-only: the fake `cargo` and scripted agent are `sh` scripts made
// executable via `PermissionsExt`.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use corpus_runner::report::CaseOutcome;
use corpus_runner::{run, RunConfig, Timeouts};

/// A canned `caselike` implementation: `greet` prints a line (JSON under
/// `--output json`), `home` prints `$HOME`, and everything else exits 2.
const CASELIKE: &str = r#"#!/bin/sh
cmd="$1"
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

    // A roster-schema fixture archetype.
    let archetypes = scratch.path().join("archetypes");
    let archetype_dir = archetypes.join("caselike");
    fs::create_dir_all(&archetype_dir).unwrap();
    fs::write(archetype_dir.join("spec.md"), "# caselike\n").unwrap();
    fs::write(archetype_dir.join("acceptance.toml"), ACCEPTANCE).unwrap();

    // The fake toolchain installs the canned binary as `caselike` — the
    // roster rule that archetype names double as binary names.
    let bin_dir = scratch.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let impl_path = bin_dir.join("caselike-impl");
    fs::write(&impl_path, CASELIKE).unwrap();
    fs::set_permissions(&impl_path, fs::Permissions::from_mode(0o755)).unwrap();
    let cargo = bin_dir.join("cargo");
    fs::write(
        &cargo,
        format!(
            r#"#!/bin/sh
td=""
prev=""
for a in "$@"; do
  if [ "$prev" = "--target-dir" ]; then td="$a"; fi
  prev="$a"
done
[ -n "$td" ] || {{ echo "no --target-dir passed" >&2; exit 1; }}
mkdir -p "$td/debug"
cp "{impl_path}" "$td/debug/caselike"
"#,
            impl_path = impl_path.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();
    std::env::set_var(
        "PATH",
        format!(
            "{}:{}",
            bin_dir.display(),
            std::env::var("PATH").unwrap_or_default()
        ),
    );

    let agent = bin_dir.join("agent.sh");
    fs::write(
        &agent,
        r#"#!/bin/sh
set -e
awk '{ print }
/<id:summary>$/ { print "Implemented caselike from SPEC.md." }
/<id:sources.docs>$/ { print "docs/index.md" }
/<id:sources.external>$/ { print "none" }
/<id:confidence>$/ { print "high" }' QUESTIONNAIRE.md > QUESTIONNAIRE.md.filled
mv QUESTIONNAIRE.md.filled QUESTIONNAIRE.md
"#,
    )
    .unwrap();
    fs::set_permissions(&agent, fs::Permissions::from_mode(0o755)).unwrap();

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
    assert!(report.acceptance.checks.is_empty());

    // One result per case, expected-fail mapping applied.
    let outcomes: Vec<CaseOutcome> = report.acceptance.cases.iter().map(|c| c.outcome).collect();
    assert_eq!(
        outcomes,
        vec![
            CaseOutcome::Pass,
            CaseOutcome::Fail,
            CaseOutcome::ExpectedFail
        ]
    );

    // Case sandboxes are part of the run directory's inspectable residue.
    assert!(run_dir.join("cases/greet-exact").is_dir());

    // The invariant matrix ran under the case baseline and all cells pass
    // for the well-behaved fixture.
    assert!(!report.invariants.is_empty());
    let failed: Vec<_> = report
        .invariants
        .iter()
        .filter(|c| c.status == corpus_runner::report::InvariantStatus::Fail)
        .collect();
    assert!(failed.is_empty(), "{failed:?}");

    // The report round-trips with its cases section.
    let json = fs::read_to_string(run_dir.join("report.json")).unwrap();
    assert!(json.contains("\"expected-fail\""), "cases serialized");
}
