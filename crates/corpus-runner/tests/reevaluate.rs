// Typed re-evaluation of preserved historical runs via `report::HistoricalRun`.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use corpus_runner::{reevaluate, ReevaluationConfig, Timeouts};

// `docs_dir` is the real one: a tempdir would sit under the admitted `/private/var` on macOS.
struct Fixture {
    _scratch: tempfile::TempDir,
    archetypes_dir: PathBuf,
    docs_dir: PathBuf,
    workspace_root: PathBuf,
    source_report: PathBuf,
    output_report: PathBuf,
    binary: PathBuf,
}

fn fixture(report_json: &str) -> Fixture {
    let scratch = tempfile::tempdir().unwrap();

    let archetype_dir = scratch.path().join("archetypes/fake");
    fs::create_dir_all(&archetype_dir).unwrap();
    fs::write(archetype_dir.join("spec.md"), "spec").unwrap();
    fs::write(
        archetype_dir.join("acceptance.toml"),
        r#"
schema = 1
archetype = "fake"

[[case]]
name = "greets"
stresses = "greeting output"
expected = "pass"
[case.run]
argv = []
timeout_seconds = 5
[case.expect]
stdout_contains = ["hello"]
"#,
    )
    .unwrap();

    let workspace_root = scratch.path().join("ws");
    fs::create_dir_all(&workspace_root).unwrap();

    // The evaluation sandbox admits only the workspace and system roots.
    let binary = workspace_root.join("fake");
    fs::write(&binary, "#!/bin/sh\necho hello\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();

    let source_report = scratch.path().join("source-report.json");
    fs::write(&source_report, report_json).unwrap();

    Fixture {
        archetypes_dir: scratch.path().join("archetypes"),
        docs_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs"),
        workspace_root,
        source_report,
        output_report: scratch.path().join("report.json"),
        binary,
        _scratch: scratch,
    }
}

fn config(fixture: &Fixture) -> ReevaluationConfig {
    ReevaluationConfig {
        archetype: "fake".to_string(),
        archetypes_dir: fixture.archetypes_dir.clone(),
        docs_dir: fixture.docs_dir.clone(),
        workspace_root: fixture.workspace_root.clone(),
        source_report: fixture.source_report.clone(),
        output_report: fixture.output_report.clone(),
        produced_binary: Some(fixture.binary.clone()),
        timeouts: Timeouts::default(),
    }
}

fn historical_v2_report(archetype: &str) -> String {
    format!(
        r#"{{
  "schema_version": 2,
  "run_id": "{archetype}-1700000000",
  "archetype": {{ "name": "{archetype}", "spec_sha256": "{a}" }},
  "pins": {{
    "framework_version": "8.1.1",
    "docs_commit": "cafecafe",
    "docs_sha256": "{b}",
    "acceptance_sha256": "{c}",
    "questionnaire_fingerprint": "fp-1"
  }},
  "evaluation": {{
    "origin": "full-run",
    "isolation_backend": "macos-seatbelt",
    "binary_sha256": null
  }},
  "blindness": {{
    "policy": "historical policy line",
    "env_allowlist": ["PATH", "HOME"],
    "framework_source_excluded": true,
    "isolation_backend": "macos-seatbelt",
    "credential_exceptions": [],
    "agent_reported_docs": "docs/guides/minimal.md",
    "agent_reported_external_sources": "none"
  }},
  "session": {{
    "agent_cmd": "sh agent.sh",
    "wall_seconds": 12.5,
    "exit_code": 0,
    "timed_out": false,
    "attempts": 1,
    "turns": 3,
    "input_tokens": 10,
    "output_tokens": 20,
    "transcript": "transcript.jsonl"
  }},
  "acceptance": {{ "built": true, "build_detail": null }},
  "invariants": [],
  "questionnaire": {{
    "collected": true,
    "diagnostics": [],
    "answers": {{ "summary": "built it" }}
  }}
}}"#,
        a = "ab".repeat(32),
        b = "cd".repeat(32),
        c = "ef".repeat(32),
    )
}

#[test]
fn reevaluation_preserves_history_and_regenerates_objective_sections() {
    let fixture = fixture(&historical_v2_report("fake"));

    let report = reevaluate(&config(&fixture)).unwrap();

    assert_eq!(report.schema_version, corpus_runner::report::SCHEMA_VERSION);
    assert_eq!(report.evaluation.origin, "isolated-re-evaluation");
    assert_eq!(
        report.blindness.isolation.network,
        corpus_runner::report::NetworkEnforcement::NotEnforced
    );
    #[cfg(target_os = "macos")]
    assert_eq!(
        report.evaluation.isolation.network,
        corpus_runner::report::NetworkEnforcement::Denied
    );
    #[cfg(target_os = "linux")]
    assert_eq!(
        report.evaluation.isolation.network,
        corpus_runner::report::NetworkEnforcement::DenialRequestedButUnsupported
    );
    assert_eq!(report.blindness.isolation.backend, "historical-partial");
    assert!(!report.blindness.framework_source_excluded);
    assert!(!report.blindness.credential_exceptions.is_empty());
    assert_ne!(report.pins.acceptance_sha256, "ef".repeat(32));
    assert_eq!(report.pins.acceptance_sha256.len(), 64);

    assert_eq!(report.run_id, "fake-1700000000");
    assert_eq!(report.pins.framework_version, "8.1.1");
    assert_eq!(report.pins.docs_commit, "cafecafe");
    assert_eq!(
        report.pins.docs_source,
        corpus_runner::report::DocsSource::Checkout
    );
    assert_eq!(report.session.wall_seconds, 12.5);
    assert_eq!(report.session.turns, Some(3));
    assert_eq!(report.blindness.env_allowlist, vec!["PATH", "HOME"]);
    assert_eq!(
        report.blindness.agent_reported_docs.as_deref(),
        Some("docs/guides/minimal.md")
    );
    assert_eq!(
        report.questionnaire.answers.get("summary").unwrap(),
        "built it"
    );

    assert!(report.acceptance.built);
    assert_eq!(report.acceptance.cases.len(), 1);
    assert_eq!(
        report.acceptance.cases[0].outcome,
        corpus_runner::report::CaseOutcome::Pass
    );
    assert_eq!(
        report.evaluation.binary_sha256.as_deref().map(str::len),
        Some(64)
    );

    let restored: corpus_runner::report::RunReport =
        serde_json::from_str(&fs::read_to_string(&fixture.output_report).unwrap()).unwrap();
    assert_eq!(
        restored.schema_version,
        corpus_runner::report::SCHEMA_VERSION
    );
}

fn with_schema_version(report: &str, version: Option<u64>) -> String {
    let mut value: serde_json::Value = serde_json::from_str(report).unwrap();
    let object = value.as_object_mut().unwrap();
    match version {
        Some(version) => {
            object.insert("schema_version".into(), version.into());
        }
        None => {
            object.remove("schema_version");
        }
    }
    serde_json::to_string(&value).unwrap()
}

#[test]
fn schema_versions_outside_the_historical_range_are_refused() {
    for version in [1, 99] {
        let fixture = fixture(&with_schema_version(
            &historical_v2_report("fake"),
            Some(version),
        ));

        let err = reevaluate(&config(&fixture)).unwrap_err();

        let chain = format!("{err:#}");
        assert!(
            chain.contains(&format!("records schema_version {version}")),
            "{chain}"
        );
        assert!(
            chain.contains(&fixture.source_report.display().to_string()),
            "{chain}"
        );
        assert!(!fixture.output_report.exists());
    }
}

#[test]
fn missing_schema_version_is_a_deserialization_diagnostic() {
    let fixture = fixture(&with_schema_version(&historical_v2_report("fake"), None));

    let err = reevaluate(&config(&fixture)).unwrap_err();

    let chain = format!("{err:#}");
    assert!(
        chain.contains("does not deserialize as a run report"),
        "{chain}"
    );
    assert!(chain.contains("schema_version"), "{chain}");
    assert!(!fixture.output_report.exists());
}

#[test]
fn out_of_workspace_binary_override_is_a_loud_finding() {
    let fixture = fixture(&historical_v2_report("fake"));
    let outside = fixture
        .workspace_root
        .parent()
        .unwrap()
        .join("outside-fake");
    fs::write(&outside, "#!/bin/sh\necho hello\n").unwrap();
    fs::set_permissions(&outside, fs::Permissions::from_mode(0o755)).unwrap();
    let mut config = config(&fixture);
    config.produced_binary = Some(outside);

    let report = reevaluate(&config).unwrap();

    assert!(!report.acceptance.built);
    assert!(report
        .acceptance
        .build_detail
        .as_deref()
        .unwrap()
        .contains("outside the preserved workspace"));
}

#[test]
fn off_shape_source_report_is_a_diagnostic_error_not_a_panic() {
    let fixture = fixture(r#"{ "schema_version": 2, "run_id": 42 }"#);

    let err = reevaluate(&config(&fixture)).unwrap_err();

    let chain = format!("{err:#}");
    assert!(
        chain.contains("does not deserialize as a run report"),
        "{chain}"
    );
    assert!(
        chain.contains(&fixture.source_report.display().to_string()),
        "{chain}"
    );
    assert!(!fixture.output_report.exists());
}

#[test]
fn archetype_mismatch_is_refused_with_both_names() {
    let fixture = fixture(&historical_v2_report("other"));

    let err = reevaluate(&config(&fixture)).unwrap_err();

    let chain = format!("{err:#}");
    assert!(chain.contains("records archetype"), "{chain}");
    assert!(chain.contains("other") && chain.contains("fake"), "{chain}");
    assert!(!fixture.output_report.exists());
}
