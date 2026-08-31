// The walking skeleton, end to end, against the real crates.io `standout`
// pin (network + full compile). Ignored by default; the always-on hermetic
// twin lives in `hermetic_loop.rs`. Run with:
//
//   cargo test -p corpus-runner --test walking_skeleton -- --ignored

#![cfg(unix)]

mod common;

use std::path::Path;

use corpus_runner::{run, RunConfig, Timeouts};

#[test]
#[ignore = "builds the produced app against crates.io (network + full compile)"]
fn smoke_archetype_completes_the_loop() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let scratch = tempfile::tempdir().unwrap();

    // The sandbox admits only the workspace, system roots, and PATH
    // directories, so the agent script and solution are staged here instead
    // of referenced by an absolute path into the checkout.
    let tools_dir = scratch.path().join("tools");
    std::fs::create_dir_all(&tools_dir).unwrap();
    std::env::set_var(
        "PATH",
        format!(
            "{}:{}",
            tools_dir.display(),
            std::env::var("PATH").unwrap_or_default()
        ),
    );

    let solution = tools_dir.join("smoke-solution");
    common::stage_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/smoke-solution"),
        &solution,
    );

    common::questionnaire_agent(
        &tools_dir,
        "agent.sh",
        &format!("cp -R \"{}/src/.\" app/src/", solution.display()),
        &[
            (
                "summary",
                "Implemented smoke from SPEC.md; cargo build succeeds.",
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
        agent_cmd: "agent.sh".to_string(),
        broker: None,
        framework_version: "8.1.1".to_string(),
        timeouts: Timeouts::default(),
    };

    let (report, run_dir) = run(&config).unwrap();

    assert!(run_dir.join("report.json").is_file());
    assert!(run_dir.join(&report.session.transcript).is_file());
    assert_eq!(report.schema_version, corpus_runner::report::SCHEMA_VERSION);
    assert_eq!(report.archetype.name, "smoke");
    assert_eq!(report.pins.framework_version, "8.1.1");
    assert_ne!(report.pins.docs_commit, "unknown");

    assert_eq!(report.session.exit_code, Some(0));
    assert_eq!(report.session.turns, Some(1));
    assert_eq!(report.session.output_tokens, Some(20));

    assert!(report.questionnaire.collected);
    assert_eq!(
        report.blindness.agent_reported_external_sources.as_deref(),
        Some("none")
    );

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
