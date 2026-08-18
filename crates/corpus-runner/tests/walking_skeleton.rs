//! The walking skeleton, end to end: blind workspace → implementation →
//! questionnaire → acceptance + invariant matrix → report, with the agent
//! seam filled by a script that installs the canned smoke solution
//! (`tests/fixtures/smoke-solution`) and answers the questionnaire.
//!
//! Ignored by default: the acceptance phase builds the produced app against
//! the crates.io `standout` pin, which needs the network and a full
//! dependency compile. The always-on hermetic twin (fake `cargo` on PATH,
//! no network) lives in `hermetic_loop.rs`; this test proves the same loop
//! against the real crates.io pin. Run it with:
//!
//! ```bash
//! cargo test -p corpus-runner --test walking_skeleton -- --ignored
//! ```

// Unix-only: the scripted agent is a `sh` script made executable via
// `PermissionsExt`; gating keeps the workspace buildable elsewhere.
#![cfg(unix)]

mod common;

use std::path::Path;

use corpus_runner::{run, RunConfig, Timeouts};

#[test]
#[ignore = "builds the produced app against crates.io (network + full compile)"]
fn smoke_archetype_completes_the_loop() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let solution = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/smoke-solution");
    let scratch = tempfile::tempdir().unwrap();

    // The scripted agent: install the canned solution into app/, then answer
    // the questionnaire in place (an answer line under each question tag),
    // and end with a stream-json result event so instrumentation has data.
    let agent = common::questionnaire_agent(
        scratch.path(),
        "agent.sh",
        &format!("cp -R \"{}/src/.\" app/src/", solution.display()),
        &[
            (
                "summary",
                "Implemented smoketable from SPEC.md; cargo build succeeds.",
            ),
            ("sources.docs", "docs/guides/minimal-single-crate.md"),
            ("sources.external", "none"),
            ("confidence", "high"),
        ],
        true,
    );

    let config = RunConfig {
        archetype: "smoke".to_string(),
        archetypes_dir: repo.join("corpus/archetypes"),
        runs_dir: scratch.path().join("runs"),
        docs_dir: repo.join("docs"),
        agent_cmd: format!("sh {}", agent.display()),
        framework_version: "8.1.1".to_string(),
        timeouts: Timeouts::default(),
    };

    let (report, run_dir) = run(&config).unwrap();

    // The report is on disk and complete.
    assert!(run_dir.join("report.json").is_file());
    assert!(run_dir.join(&report.session.transcript).is_file());
    assert_eq!(report.schema_version, 2);
    assert_eq!(report.archetype.name, "smoke");
    assert_eq!(report.pins.framework_version, "8.1.1");
    assert_ne!(report.pins.docs_commit, "unknown");

    // Session instrumentation flowed from the transcript.
    assert_eq!(report.session.exit_code, Some(0));
    assert_eq!(report.session.turns, Some(1));
    assert_eq!(report.session.output_tokens, Some(20));

    // Subjective: the questionnaire was collected, and its blindness record
    // landed in the blindness section.
    assert!(report.questionnaire.collected);
    assert_eq!(
        report.blindness.agent_reported_external_sources.as_deref(),
        Some("none")
    );

    // Objective: the produced binary built, and every acceptance check and
    // invariant cell passed.
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
