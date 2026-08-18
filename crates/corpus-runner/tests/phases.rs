//! Per-phase tests of the corpus runner: each seam of the loop proven fast
//! and hermetically (no network, no real agent, no crates.io build). The
//! whole loop end to end — blind workspace → scripted agent → questionnaire
//! → acceptance → report — lives in `walking_skeleton.rs`.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use corpus_runner::archetype::{Archetype, Invariants};
use corpus_runner::report::{QuestionnaireReport, RunReport};
use corpus_runner::{acceptance, questionnaire, session, workspace};

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
    sheet = answer(&sheet, "summary", "Built the smoketable CLI.");
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
    assert_eq!(archetype.acceptance.binary, "smoketable");
    assert!(!archetype.acceptance.checks.is_empty());
    assert!(!archetype.acceptance.invariants.commands.is_empty());
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

// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

#[test]
fn session_scrubs_the_environment_and_writes_the_transcript() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("CORPUS_SECRET_CANARY", "leaked");

    let report = session::run_agent(dir.path(), "env", &dir.path().join("t.jsonl")).unwrap();

    let transcript = fs::read_to_string(dir.path().join("t.jsonl")).unwrap();
    assert!(!transcript.contains("CORPUS_SECRET_CANARY"), "{transcript}");
    assert!(transcript.contains("PATH="));
    assert_eq!(report.exit_code, Some(0));
    assert_eq!(report.attempts, 1);
    assert!(report.wall_seconds >= 0.0);
    assert_eq!(report.transcript, session::TRANSCRIPT_FILENAME);
    // A non-stream-json transcript yields no turn/token instrumentation.
    assert_eq!(report.turns, None);
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
    let report = session::run_agent(dir.path(), "exit 3", &dir.path().join("t.jsonl")).unwrap();
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

#[test]
fn checks_pass_and_fail_against_real_process_output() {
    let dir = tempfile::tempdir().unwrap();
    let binary = script(dir.path(), "fake", WELL_BEHAVED);
    let archetype_dir = dir.path().join("archetypes/fake");
    fs::create_dir_all(&archetype_dir).unwrap();
    fs::write(archetype_dir.join("spec.md"), "spec").unwrap();
    fs::write(
        archetype_dir.join("acceptance.toml"),
        r#"
binary = "fake"

[[check]]
name = "greets"
args = ["greet", "--output", "text"]
stdout_contains = ["Hello"]

[[check]]
name = "emits json"
args = ["greet", "--output", "json"]
stdout_is_json = true

[[check]]
name = "expected to fail"
args = ["greet", "--output", "text"]
stdout_contains = ["absent needle"]
"#,
    )
    .unwrap();
    let archetype = Archetype::load(&dir.path().join("archetypes"), "fake").unwrap();

    let report = acceptance::run_checks(&binary, &archetype.acceptance.checks);
    assert!(report.built);
    let outcomes: Vec<bool> = report.checks.iter().map(|c| c.passed).collect();
    assert_eq!(outcomes, vec![true, true, false]);
    let failure = &report.checks[2];
    assert!(failure.detail.as_deref().unwrap().contains("absent needle"));
}

#[test]
fn invariant_matrix_passes_a_well_behaved_binary() {
    let dir = tempfile::tempdir().unwrap();
    let binary = script(dir.path(), "fake", WELL_BEHAVED);
    let invariants = Invariants {
        commands: vec![vec!["greet".to_string()]],
    };

    let cells = acceptance::run_invariants(&binary, &invariants);
    assert!(!cells.is_empty());
    let failures: Vec<_> = cells.iter().filter(|c| !c.passed).collect();
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn invariant_matrix_catches_markers_layout_drift_and_bad_json() {
    let dir = tempfile::tempdir().unwrap();
    let binary = script(dir.path(), "fake", CORRUPT);
    let invariants = Invariants {
        commands: vec![vec!["greet".to_string()]],
    };

    let cells = acceptance::run_invariants(&binary, &invariants);
    let failed: Vec<&str> = cells
        .iter()
        .filter(|c| !c.passed)
        .map(|c| c.check.as_str())
        .collect();
    assert!(
        failed.contains(&"term: no unresolved tag markers"),
        "{failed:?}"
    );
    assert!(
        failed.contains(&"term vs text: styling preserves layout"),
        "{failed:?}"
    );
    assert!(
        failed.contains(&"json: stdout parses as JSON"),
        "{failed:?}"
    );
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

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
            questionnaire_fingerprint: questionnaire::definition().fingerprint().into(),
        },
        blindness: corpus_runner::report::Blindness {
            policy: "p".into(),
            env_allowlist: vec!["PATH".into()],
            framework_source_excluded: true,
            agent_reported_docs: None,
            agent_reported_external_sources: Some("none".into()),
        },
        session: corpus_runner::report::SessionReport {
            agent_cmd: "true".into(),
            wall_seconds: 1.5,
            exit_code: Some(0),
            attempts: 1,
            turns: Some(3),
            input_tokens: None,
            output_tokens: None,
            transcript: "transcript.jsonl".into(),
        },
        acceptance: corpus_runner::report::AcceptanceReport {
            built: true,
            build_detail: None,
            checks: vec![corpus_runner::report::CheckResult {
                name: "c".into(),
                passed: true,
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
    assert_eq!(restored.schema_version, 1);
    assert_eq!(restored.run_id, report.run_id);
    assert_eq!(restored.session.turns, Some(3));
    assert_eq!(
        restored.questionnaire.answers.get("summary").unwrap(),
        "did it"
    );
}
