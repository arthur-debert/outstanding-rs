//! The full `run()` orchestration, hermetically: blind workspace → scripted
//! agent → questionnaire → build → acceptance + invariants → report, with no
//! network and no crates.io compile. The agent seam is a script, and the
//! build seam needs no seam at all — PATH is one: a fake `cargo` ahead of
//! the real one installs a canned shell "binary" into the `--target-dir`
//! the runner passes. This keeps the normal test path proving phase
//! ordering and path integration; the crates.io-backed twin lives in
//! `walking_skeleton.rs` (ignored: network + full compile).
//!
//! Runs in its own test binary because it prepends to the process-wide
//! PATH.

// Unix-only: the fake `cargo` and scripted agent are `sh` scripts made
// executable via `PermissionsExt`; gating keeps the workspace buildable
// elsewhere.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use corpus_runner::{run, RunConfig, Timeouts};

/// A canned `smoketable` implementation as a shell script: the star table
/// in text/term, the catalog as JSON rows, and an about line — enough to
/// satisfy the smoke archetype's full acceptance suite and invariant
/// matrix.
const SMOKETABLE: &str = r#"#!/bin/sh
cmd="$1"
mode=text
prev=""
for a in "$@"; do
  if [ "$prev" = "--output" ]; then mode="$a"; fi
  prev="$a"
done
if [ "$cmd" = "about" ]; then
  case "$mode" in
    json) echo '{"name":"smoketable","purpose":"a tiny fixed star catalog"}' ;;
    *) echo 'smoketable — a tiny fixed star catalog' ;;
  esac
else
  case "$mode" in
    json) echo '{"stars":[{"name":"Aldebaran","constellation":"Taurus","magnitude":0.86},{"name":"Rigel","constellation":"Orion","magnitude":0.13},{"name":"Vega","constellation":"Lyra","magnitude":0.03}]}' ;;
    *) printf 'Star Catalog\nAldebaran  Taurus  0.86\nRigel      Orion   0.13\nVega       Lyra    0.03\n' ;;
  esac
fi
"#;

#[test]
fn full_loop_completes_hermetically_with_a_fake_build() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let scratch = tempfile::tempdir().unwrap();

    // The fake toolchain: `cargo` installs the canned binary into the
    // --target-dir the runner passes (also proving that plumbing).
    let bin_dir = scratch.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let impl_path = bin_dir.join("smoketable-impl");
    fs::write(&impl_path, SMOKETABLE).unwrap();
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
cp "{impl_path}" "$td/debug/smoketable"
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

    // The scripted agent: answer the questionnaire in place and emit a
    // stream-json result event (the fake cargo makes app/ contents moot).
    let agent = bin_dir.join("agent.sh");
    fs::write(
        &agent,
        r#"#!/bin/sh
set -e
awk '{ print }
/<id:summary>$/ { print "Implemented smoketable from SPEC.md." }
/<id:sources.docs>$/ { print "docs/guides/minimal-single-crate.md" }
/<id:sources.external>$/ { print "none" }
/<id:confidence>$/ { print "high" }' QUESTIONNAIRE.md > QUESTIONNAIRE.md.filled
mv QUESTIONNAIRE.md.filled QUESTIONNAIRE.md
echo '{"type":"result","num_turns":1,"usage":{"input_tokens":10,"output_tokens":20}}'
"#,
    )
    .unwrap();
    fs::set_permissions(&agent, fs::Permissions::from_mode(0o755)).unwrap();

    let config = RunConfig {
        archetype: "smoke".to_string(),
        archetypes_dir: repo.join("corpus/archetypes"),
        runs_dir: scratch.path().join("runs"),
        docs_dir: repo.join("docs"),
        agent_cmd: "agent.sh".to_string(),
        framework_version: "8.1.1".to_string(),
        timeouts: Timeouts::default(),
    };

    let (report, run_dir) = run(&config).unwrap();

    // The report is on disk and complete, with the full pin set.
    assert!(run_dir.join("report.json").is_file());
    assert!(run_dir.join(&report.session.transcript).is_file());
    assert_eq!(report.archetype.name, "smoke");
    assert_eq!(report.pins.docs_sha256.len(), 64);
    assert!(!report.session.timed_out);
    assert_eq!(report.session.turns, Some(1));
    assert!(report.questionnaire.collected);

    // Objective: the fake build produced the binary where the runner looks,
    // and the strengthened acceptance suite (row + JSON-row association)
    // passes end to end, as does the invariant matrix.
    assert!(
        report.acceptance.built,
        "{:?}",
        report.acceptance.build_detail
    );
    let failed: Vec<_> = report
        .acceptance
        .checks
        .iter()
        .filter(|c| !c.passed)
        .collect();
    assert!(failed.is_empty(), "{failed:?}");
    let failed: Vec<_> = report
        .invariants
        .iter()
        .filter(|c| c.status == corpus_runner::report::InvariantStatus::Fail)
        .collect();
    assert!(failed.is_empty(), "{failed:?}");
}
