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
    let scratch = tempfile::tempdir().unwrap();

    // The agent script and its canned solution both need to be readable by
    // the sandboxed agent phase, which admits only the run workspace (whose
    // name is claimed inside `run()`, so it can't be staged into ahead of
    // time), system roots, and PATH directories. `tools_dir` is prepended to
    // PATH (like `install_fake_cargo`'s `bin_dir`), which makes it — and
    // everything staged beneath it — an explicitly admitted read root on
    // both the macOS Seatbelt and Linux Landlock backends; this test runs
    // alone in its own binary because prepending to PATH is process-wide
    // state (see `common::install_fake_cargo`'s doc comment).
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

    // The canned solution is staged into the PATH-admitted tools directory
    // host-side first: the agent runs under the kernel sandbox, which denies
    // reads under the source checkout, so the script cannot copy the fixture
    // from the repo.
    let solution = tools_dir.join("smoke-solution");
    common::stage_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/smoke-solution"),
        &solution,
    );

    // The scripted agent: install the canned solution into app/, then answer
    // the questionnaire in place (an answer line under each question tag),
    // and end with a stream-json result event so instrumentation has data.
    // Placed in `tools_dir` and invoked by bare name (resolved via PATH,
    // like `hermetic_loop.rs`'s `agent_cmd`) rather than an absolute path,
    // so the sandboxed `sh` that reads it also stays within an admitted root.
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
        framework_version: "8.1.1".to_string(),
        timeouts: Timeouts::default(),
    };

    let (report, run_dir) = run(&config).unwrap();

    // The report is on disk and complete.
    assert!(run_dir.join("report.json").is_file());
    assert!(run_dir.join(&report.session.transcript).is_file());
    assert_eq!(report.schema_version, corpus_runner::report::SCHEMA_VERSION);
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

    // Objective: the produced binary built, and every acceptance case and
    // invariant cell passed.
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
