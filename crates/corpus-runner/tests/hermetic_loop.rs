// The full `run()` orchestration, hermetically (fake `cargo` on PATH, no
// network). The crates.io-backed twin lives in `walking_skeleton.rs`.
// Runs in its own test binary because it prepends to the process-wide PATH.

#![cfg(unix)]

mod common;

use std::path::Path;

use corpus_runner::{run, RunConfig, Timeouts};

const SMOKE: &str = r#"cmd="$1"
mode=text
prev=""
for a in "$@"; do
  if [ "$prev" = "--output" ]; then mode="$a"; fi
  prev="$a"
done
if [ "$cmd" = "about" ]; then
  case "$mode" in
    json) echo '{"name":"smoke","purpose":"a tiny fixed star catalog"}' ;;
    *) echo 'smoke — a tiny fixed star catalog' ;;
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

    let bin_dir = scratch.path().join("bin");
    common::install_fake_cargo(&bin_dir, "smoke", SMOKE);

    common::questionnaire_agent(
        &bin_dir,
        "agent.sh",
        "",
        &[
            ("summary", "Implemented smoke from SPEC.md."),
            ("sources.docs", "docs/guides/minimal-single-crate.md"),
            ("sources.external", "none"),
            ("confidence", "high"),
            ("confidence_reason", "Every case passes."),
        ],
        true,
    );

    let config = RunConfig {
        archetype: "smoke".to_string(),
        archetypes_dir: repo.join("corpus/archetypes"),
        runs_dir: scratch.path().join("runs"),
        docs_dir: repo.join("docs"),
        agent_cmd: "agent.sh".to_string(),
        broker: None,
        framework_version: "8.1.1".to_string(),
        timeouts: Timeouts::default(),
    };

    let (report, run_dir) = run(&config).unwrap();

    assert!(run_dir.join("report.json").is_file());
    assert!(run_dir.join(&report.session.transcript).is_file());
    assert_eq!(report.archetype.name, "smoke");
    assert_eq!(report.pins.docs_sha256.len(), 64);
    assert!(!report.session.timed_out);
    assert_eq!(report.session.turns, Some(1));
    assert!(report.questionnaire.collected);

    assert!(
        report.acceptance.built,
        "{:?}",
        report.acceptance.build_detail
    );
    assert!(!report.acceptance.cases.is_empty());
    let failed: Vec<_> = report
        .acceptance
        .cases
        .iter()
        .filter(|c| !c.outcome.is_expected())
        .collect();
    assert!(failed.is_empty(), "{failed:?}");
    let failed: Vec<_> = report
        .invariants
        .iter()
        .filter(|c| c.status == corpus_runner::report::InvariantStatus::Fail)
        .collect();
    assert!(failed.is_empty(), "{failed:?}");
}
