//! Per-phase tests of the corpus runner: each seam of the loop proven fast
//! and hermetically (no network, no real agent, no crates.io build). The
//! whole loop end to end lives in `hermetic_loop.rs` (fake build, always
//! on) and `walking_skeleton.rs` (real crates.io build, ignored).

// Unix-only: scripted agents rely on `sh` + `PermissionsExt`, and the
// symlink-refusal test builds its fixture with `std::os::unix::fs::symlink`;
// gating keeps the workspace buildable elsewhere.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use corpus_runner::archetype::{Archetype, InvariantCommand, InvariantContract, Invariants};
use corpus_runner::report::{InvariantStatus, QuestionnaireReport, RunReport};
use corpus_runner::{acceptance, questionnaire, session, workspace};

/// A generous per-process deadline for tests that must not time out.
const NO_TIMEOUT: Duration = Duration::from_secs(60);

/// The repo's real corpus directory, relative to this crate.
fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
}

/// Inserts `answer` directly below the question line tagged `id`.
fn answer(sheet: &str, id: &str, answer: &str) -> String {
    let tag = format!("<id:{id}>");
    let mut out = Vec::new();
    let mut found = false;
    for line in sheet.lines() {
        out.push(line.to_string());
        if !found && line.trim_end().ends_with(&tag) {
            found = true;
            out.push(answer.to_string());
        }
    }
    assert!(found, "no question line for {tag}");
    out.join("\n") + "\n"
}

/// A filled sheet answering every required field.
fn filled_sheet() -> String {
    let mut sheet = questionnaire::definition().render_answer_sheet();
    sheet = answer(&sheet, "summary", "Built the smoke CLI.");
    sheet = answer(
        &sheet,
        "sources.docs",
        "docs/guides/minimal-single-crate.md",
    );
    sheet = answer(&sheet, "sources.external", "none");
    sheet = answer(&sheet, "confidence", "high");
    sheet
}

/// Writes an executable script and returns its path.
fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn isolation(root: &Path) -> workspace::Isolation {
    workspace::Isolation::new(root, &Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap()
}

fn rendered(argv: &[&str]) -> InvariantCommand {
    InvariantCommand {
        argv: argv.iter().map(|s| (*s).to_string()).collect(),
        contract: InvariantContract::Rendered,
    }
}

fn opaque(argv: &[&str]) -> InvariantCommand {
    InvariantCommand {
        argv: argv.iter().map(|s| (*s).to_string()).collect(),
        contract: InvariantContract::OpaqueBytes,
    }
}

// ---------------------------------------------------------------------------
// Questionnaire
// ---------------------------------------------------------------------------

#[test]
fn field_ids_match_the_definition() {
    let ids: Vec<&str> = questionnaire::definition()
        .items()
        .iter()
        .map(|item| match item {
            standout_input::questionnaire::Item::Field(field) => field.id(),
            standout_input::questionnaire::Item::Group(group) => group.id(),
        })
        .map(|id| {
            questionnaire::FIELD_IDS
                .iter()
                .copied()
                .find(|known| *known == id)
                .expect("definition field missing from FIELD_IDS")
        })
        .collect();
    assert_eq!(ids.len(), questionnaire::FIELD_IDS.len());
}

#[test]
fn filled_sheet_collects_with_answers_and_blindness_record() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(questionnaire::SHEET_FILENAME),
        filled_sheet(),
    )
    .unwrap();

    let report = questionnaire::collect(dir.path());
    assert!(report.collected, "diagnostics: {:?}", report.diagnostics);
    assert_eq!(
        report.answers.get("sources.external").map(String::as_str),
        Some("none")
    );
    assert_eq!(
        report.answers.get("confidence").map(String::as_str),
        Some("high")
    );
    // Blank optional fields are omissions, not entries.
    assert!(!report.answers.contains_key("friction"));
}

#[test]
fn unanswered_required_field_is_an_uncollected_report_not_a_panic() {
    let dir = tempfile::tempdir().unwrap();
    // Blank sheet: required fields unanswered.
    fs::write(
        dir.path().join(questionnaire::SHEET_FILENAME),
        questionnaire::definition().render_answer_sheet(),
    )
    .unwrap();

    let report = questionnaire::collect(dir.path());
    assert!(!report.collected);
    assert!(report.diagnostics.iter().any(|d| d.contains("summary")));
    assert!(report.answers.is_empty());
}

#[test]
fn missing_sheet_is_an_uncollected_report() {
    let dir = tempfile::tempdir().unwrap();
    let report = questionnaire::collect(dir.path());
    assert!(!report.collected);
    assert!(!report.diagnostics.is_empty());
}

// ---------------------------------------------------------------------------
// Archetype + provisioning
// ---------------------------------------------------------------------------

#[test]
fn smoke_archetype_loads_from_the_repo_corpus() {
    let archetype = Archetype::load(&corpus_dir().join("archetypes"), "smoke").unwrap();
    assert_eq!(
        archetype.binary(),
        "smoke",
        "roster names double as binaries"
    );
    assert!(!archetype.suite.cases.is_empty());
    assert!(!archetype.invariants().commands.is_empty());
    assert_eq!(archetype.spec_sha256().len(), 64);
}

#[test]
fn provisioned_workspace_is_blind() {
    let archetype = Archetype::load(&corpus_dir().join("archetypes"), "smoke").unwrap();
    let run_dir = tempfile::tempdir().unwrap();
    let docs_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");

    let ws = workspace::provision(run_dir.path(), &archetype, &docs_dir, "8.1.1").unwrap();

    // What the agent must see.
    for file in ["SPEC.md", "INSTRUCTIONS.md", "QUESTIONNAIRE.md"] {
        assert!(ws.root.join(file).is_file(), "missing {file}");
    }
    assert!(ws.root.join("docs/index.md").is_file());
    assert!(ws.root.join("docs/guides").is_dir());
    // The crate-docs mounts (symlinks in the source tree) arrive as real
    // dereferenced content — never as links back into the checkout.
    let crate_docs = ws.root.join("docs/crates/dispatch");
    assert!(crate_docs.is_dir());
    assert!(!fs::symlink_metadata(&crate_docs)
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(ws.app_dir.join("Cargo.toml").is_file());
    assert!(ws.app_dir.join("src/main.rs").is_file());

    // What it must not: framework source and internal docs.
    assert!(!ws.root.join("docs/adr").exists());
    assert!(!ws.root.join("docs/spec").exists());
    assert!(!ws.root.join("docs/specs").exists());
    assert!(!ws.root.join("docs/proposals").exists());
    assert!(!ws.root.join("docs/dev").exists());
    assert!(!ws.root.join("crates").exists());

    // The scaffold pins crates.io exactly and carries no path/git deps.
    let manifest = fs::read_to_string(ws.app_dir.join("Cargo.toml")).unwrap();
    assert!(manifest.contains("standout = \"=8.1.1\""));
    assert!(!manifest.contains("path"));
    assert!(!manifest.contains("git"));
    // The scaffold is its own cargo workspace, so cargo never walks up into
    // whatever checkout the run directory happens to live under.
    assert!(manifest.contains("[workspace]"));
}

#[test]
fn provisioning_records_the_docs_snapshot_digest() {
    let archetype = Archetype::load(&corpus_dir().join("archetypes"), "smoke").unwrap();
    let run_dir = tempfile::tempdir().unwrap();
    let docs_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");

    let ws = workspace::provision(run_dir.path(), &archetype, &docs_dir, "8.1.1").unwrap();

    assert_eq!(ws.docs_sha256.len(), 64);
    // The digest is over the copied bytes: recomputing from the snapshot
    // reproduces it.
    assert_eq!(
        ws.docs_sha256,
        workspace::docs_digest(&ws.root.join("docs")).unwrap()
    );
}

#[test]
fn provisioning_refuses_symlinks_in_the_docs_source() {
    let archetype = Archetype::load(&corpus_dir().join("archetypes"), "smoke").unwrap();
    let run_dir = tempfile::tempdir().unwrap();
    let scratch = tempfile::tempdir().unwrap();

    // A docs tree whose published set hides a symlink pointing outside it.
    let outside = scratch.path().join("outside.md");
    fs::write(&outside, "framework internals").unwrap();
    let docs_dir = scratch.path().join("docs");
    fs::create_dir_all(docs_dir.join("guides")).unwrap();
    fs::write(docs_dir.join("index.md"), "index").unwrap();
    std::os::unix::fs::symlink(&outside, docs_dir.join("guides/leak.md")).unwrap();

    let err = workspace::provision(run_dir.path(), &archetype, &docs_dir, "8.1.1").unwrap_err();
    assert!(format!("{err:#}").contains("symlink"), "{err:#}");
}

#[test]
fn provisioning_refuses_symlinks_from_a_published_root_into_internal_docs() {
    let archetype = Archetype::load(&corpus_dir().join("archetypes"), "smoke").unwrap();
    let run_dir = tempfile::tempdir().unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let docs_dir = scratch.path().join("docs");
    fs::create_dir_all(docs_dir.join("guides")).unwrap();
    fs::create_dir_all(docs_dir.join("spec")).unwrap();
    fs::write(docs_dir.join("index.md"), "index").unwrap();
    fs::write(docs_dir.join("spec/internal.md"), "framework internals").unwrap();
    std::os::unix::fs::symlink(
        docs_dir.join("spec/internal.md"),
        docs_dir.join("guides/leak.md"),
    )
    .unwrap();

    let err = workspace::provision(run_dir.path(), &archetype, &docs_dir, "8.1.1").unwrap_err();
    assert!(format!("{err:#}").contains("symlink"), "{err:#}");
}

#[test]
fn kernel_boundary_blocks_an_actual_checkout_file_open() {
    let workspace_root = tempfile::tempdir().unwrap();
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let isolation = workspace::Isolation::new(workspace_root.path(), &source_root).unwrap();

    isolation.verify_boundary(&source_root).unwrap();
}

// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

#[test]
fn session_scrubs_the_environment_and_writes_the_transcript() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("CORPUS_SECRET_CANARY", "leaked");

    let report = session::run_agent(
        dir.path(),
        &isolation(dir.path()),
        "env",
        &dir.path().join("t.jsonl"),
        NO_TIMEOUT,
    )
    .unwrap();

    let transcript = fs::read_to_string(dir.path().join("t.jsonl")).unwrap();
    assert!(!transcript.contains("CORPUS_SECRET_CANARY"), "{transcript}");
    assert!(transcript.contains("PATH="));
    assert_eq!(report.exit_code, Some(0));
    assert!(report.wall_seconds >= 0.0);
    assert_eq!(report.transcript, session::TRANSCRIPT_FILENAME);
    assert!(!report.timed_out);
    // A non-stream-json transcript yields no turn/token instrumentation.
    assert_eq!(report.turns, None);
}

#[test]
fn overrunning_agent_is_killed_and_recorded_as_timed_out() {
    let dir = tempfile::tempdir().unwrap();
    let report = session::run_agent(
        dir.path(),
        &isolation(dir.path()),
        "sleep 30",
        &dir.path().join("t.jsonl"),
        Duration::from_millis(200),
    )
    .unwrap();
    assert!(report.timed_out);
    assert_eq!(report.exit_code, None);
    assert!(report.wall_seconds < 10.0);
}

#[test]
fn stream_json_result_event_yields_turns_and_tokens() {
    let transcript = concat!(
        "{\"type\":\"system\",\"subtype\":\"init\"}\n",
        "not json at all\n",
        "{\"type\":\"result\",\"num_turns\":7,\"usage\":{\"input_tokens\":123,\"output_tokens\":45}}\n",
    );
    let stats = session::stream_json_stats(transcript);
    assert_eq!(stats.turns, Some(7));
    assert_eq!(stats.input_tokens, Some(123));
    assert_eq!(stats.output_tokens, Some(45));
}

#[test]
fn failing_agent_is_recorded_not_fatal() {
    let dir = tempfile::tempdir().unwrap();
    let report = session::run_agent(
        dir.path(),
        &isolation(dir.path()),
        "exit 3",
        &dir.path().join("t.jsonl"),
        NO_TIMEOUT,
    )
    .unwrap();
    assert_eq!(report.exit_code, Some(3));
}

// ---------------------------------------------------------------------------
// Acceptance + invariants
// ---------------------------------------------------------------------------

/// A fake produced binary honoring `--output text|term|json`, with identical
/// term/text content (term adds only ANSI bold).
const WELL_BEHAVED: &str = r#"
mode=text
prev=""
for a in "$@"; do
  if [ "$prev" = "--output" ]; then mode="$a"; fi
  prev="$a"
done
case "$mode" in
  json) echo '{"ok": true}' ;;
  term) printf '\033[1mHello\033[0m table\n' ;;
  *) echo 'Hello table' ;;
esac
"#;

/// A corrupt binary: term leaks an unresolved tag marker and drifts from text.
const CORRUPT: &str = r#"
mode=text
prev=""
for a in "$@"; do
  if [ "$prev" = "--output" ]; then mode="$a"; fi
  prev="$a"
done
case "$mode" in
  json) echo 'not json' ;;
  term) echo '[title?]Hello broken' ;;
  *) echo 'Hello table' ;;
esac
"#;

const OPAQUE: &str = r#"
printf '\001\002opaque\377\n'
"#;

#[test]
fn invariant_matrix_passes_a_well_behaved_binary() {
    let dir = tempfile::tempdir().unwrap();
    let binary = script(dir.path(), "fake", WELL_BEHAVED);
    let invariants = Invariants {
        commands: vec![rendered(&["greet"])],
        ..Invariants::default()
    };

    let cells = acceptance::run_invariants(
        &binary,
        &invariants,
        NO_TIMEOUT,
        &isolation(dir.path()),
        &dir.path().join("matrix"),
    );
    assert!(!cells.is_empty());
    let failures: Vec<_> = cells
        .iter()
        .filter(|c| c.status == InvariantStatus::Fail)
        .collect();
    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(cells.len(), 30);
    assert_eq!(
        cells
            .iter()
            .filter(|c| c.status == InvariantStatus::Pass)
            .count(),
        14
    );
    assert_eq!(
        cells
            .iter()
            .filter(|c| c.status == InvariantStatus::NotApplicable)
            .count(),
        16
    );
}

#[test]
fn opaque_matrix_preserves_bytes_without_json_or_render_checks() {
    let dir = tempfile::tempdir().unwrap();
    let binary = script(dir.path(), "fake", OPAQUE);
    let invariants = Invariants {
        commands: vec![opaque(&["cat-object", "deadbeef"])],
        ..Invariants::default()
    };

    let cells = acceptance::run_invariants(
        &binary,
        &invariants,
        NO_TIMEOUT,
        &isolation(dir.path()),
        &dir.path().join("matrix"),
    );
    assert_eq!(cells.len(), 30);
    assert_eq!(
        cells
            .iter()
            .filter(|c| c.status == InvariantStatus::Pass)
            .count(),
        10
    );
    assert_eq!(
        cells
            .iter()
            .filter(|c| c.status == InvariantStatus::NotApplicable)
            .count(),
        20
    );
    assert!(cells
        .iter()
        .all(|c| c.status != InvariantStatus::Fail && c.status != InvariantStatus::NotRun));
    assert!(cells
        .iter()
        .filter(|c| c.check == "stdout parses as JSON")
        .all(|c| c.status == InvariantStatus::NotApplicable));
}

#[test]
fn matrix_keeps_stable_identities_when_the_build_does_not_run() {
    let invariants = Invariants {
        commands: vec![rendered(&["greet"])],
        ..Invariants::default()
    };
    let cells = acceptance::not_run_invariants(&invariants, "build failed");
    assert_eq!(cells.len(), 30);
    assert_eq!(
        cells
            .iter()
            .filter(|c| c.status == InvariantStatus::NotRun)
            .count(),
        14
    );
    assert_eq!(
        cells
            .iter()
            .filter(|c| c.status == InvariantStatus::NotApplicable)
            .count(),
        16
    );
}

#[test]
fn invariant_matrix_catches_markers_layout_drift_and_bad_json() {
    let dir = tempfile::tempdir().unwrap();
    let binary = script(dir.path(), "fake", CORRUPT);
    let invariants = Invariants {
        commands: vec![rendered(&["greet"])],
        ..Invariants::default()
    };

    let cells = acceptance::run_invariants(
        &binary,
        &invariants,
        NO_TIMEOUT,
        &isolation(dir.path()),
        &dir.path().join("matrix"),
    );
    let failed: Vec<&str> = cells
        .iter()
        .filter(|c| c.status == InvariantStatus::Fail)
        .map(|c| c.check.as_str())
        .collect();
    assert!(failed.contains(&"no unresolved tag markers"), "{failed:?}");
    assert!(
        failed.contains(&"styling preserves text layout"),
        "{failed:?}"
    );
    assert!(failed.contains(&"stdout parses as JSON"), "{failed:?}");
}

#[test]
fn hanging_binary_times_out_as_a_finding() {
    let dir = tempfile::tempdir().unwrap();
    let binary = script(dir.path(), "fake", "sleep 30");
    let invariants = Invariants {
        commands: vec![rendered(&["greet"])],
        ..Invariants::default()
    };

    let cells = acceptance::run_invariants(
        &binary,
        &invariants,
        Duration::from_millis(200),
        &isolation(dir.path()),
        &dir.path().join("matrix"),
    );
    assert!(!cells.is_empty());
    let exit_cells: Vec<_> = cells.iter().filter(|c| c.check == "exits 0").collect();
    assert!(exit_cells.iter().all(|c| c.status == InvariantStatus::Fail));
    assert!(exit_cells
        .iter()
        .all(|c| c.detail.as_deref().unwrap().contains("timed out")));
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

fn test_isolation_record() -> corpus_runner::report::IsolationRecord {
    corpus_runner::report::IsolationRecord {
        backend: "test".into(),
        filesystem: "test".into(),
        network: corpus_runner::report::NetworkEnforcement::NotEnforced,
    }
}

#[test]
fn report_round_trips_through_json() {
    let report = RunReport {
        schema_version: corpus_runner::report::SCHEMA_VERSION,
        run_id: "smoke-0".into(),
        archetype: corpus_runner::report::ArchetypeStamp {
            name: "smoke".into(),
            spec_sha256: "ab".repeat(32),
        },
        pins: corpus_runner::report::Pins {
            framework_version: "8.1.1".into(),
            docs_commit: "deadbeef".into(),
            docs_sha256: "cd".repeat(32),
            acceptance_sha256: "ef".repeat(32),
            questionnaire_fingerprint: questionnaire::definition().fingerprint().into(),
        },
        evaluation: corpus_runner::report::EvaluationStamp {
            origin: "full-run".into(),
            isolation: test_isolation_record(),
            binary_sha256: None,
        },
        blindness: corpus_runner::report::Blindness {
            policy: "p".into(),
            env_allowlist: vec!["PATH".into()],
            framework_source_excluded: true,
            isolation: test_isolation_record(),
            credential_exceptions: vec![],
            agent_reported_docs: None,
            agent_reported_external_sources: Some("none".into()),
        },
        session: corpus_runner::report::SessionReport {
            agent_cmd: "true".into(),
            wall_seconds: 1.5,
            exit_code: Some(0),
            timed_out: false,
            turns: Some(3),
            input_tokens: None,
            output_tokens: None,
            transcript: "transcript.jsonl".into(),
        },
        acceptance: corpus_runner::report::AcceptanceReport {
            built: true,
            build_detail: None,
            cases: vec![corpus_runner::report::CaseResult {
                name: "c".into(),
                group: None,
                stresses: "round-trip".into(),
                expected: "pass".into(),
                outcome: corpus_runner::report::CaseOutcome::Pass,
                gap: None,
                reason: None,
                detail: None,
            }],
        },
        invariants: vec![],
        questionnaire: QuestionnaireReport {
            collected: true,
            diagnostics: vec![],
            answers: [("summary".to_string(), "did it".to_string())].into(),
        },
    };

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.json");
    report.write(&path).unwrap();
    let restored: RunReport = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        restored.schema_version,
        corpus_runner::report::SCHEMA_VERSION
    );
    assert_eq!(restored.run_id, report.run_id);
    assert_eq!(restored.session.turns, Some(3));
    assert_eq!(
        restored.questionnaire.answers.get("summary").unwrap(),
        "did it"
    );
}

/// Every committed historical report — the pilot runs (schema 2, case
/// results) and the demo smoke run (schema 2, whose `checks` vector belongs
/// to the retired check schema) — must keep loading through the typed
/// historical path re-evaluation uses ([`HistoricalRun`]), with retired
/// keys (`checks`, `session.attempts`, string `isolation_backend`s) ignored
/// and every version inside the supported historical range.
#[test]
fn committed_historical_reports_still_deserialize() {
    use corpus_runner::report::{HistoricalRun, HISTORICAL_SCHEMA_MIN, SCHEMA_VERSION};

    let mut reports = Vec::new();
    for dir in ["pilot/runs", "demo"] {
        for entry in fs::read_dir(corpus_dir().join(dir)).unwrap() {
            let path = entry.unwrap().path().join("report.json");
            if path.is_file() {
                reports.push(path);
            }
        }
    }
    assert!(!reports.is_empty(), "no committed reports found");
    for path in reports {
        let report: HistoricalRun = serde_json::from_str(&fs::read_to_string(&path).unwrap())
            .unwrap_or_else(|err| panic!("{} must deserialize: {err}", path.display()));
        assert!(
            (HISTORICAL_SCHEMA_MIN..=SCHEMA_VERSION).contains(&report.schema_version),
            "{} carries schema version {} outside the supported historical range",
            path.display(),
            report.schema_version
        );
    }
}
