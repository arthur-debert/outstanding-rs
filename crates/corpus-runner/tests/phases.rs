// Per-phase tests of the corpus runner, hermetically. The whole loop end to
// end lives in `hermetic_loop.rs` and `walking_skeleton.rs`.
#![cfg(unix)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use common::script;
use corpus_runner::archetype::{Archetype, InvariantCommand, InvariantContract, Invariants};
use corpus_runner::report::{InvariantStatus, QuestionnaireReport, RunReport};
use corpus_runner::{acceptance, questionnaire, session, workspace};

const NO_TIMEOUT: Duration = Duration::from_secs(60);

// Every directory under `corpus/` holding committed run reports.
const COMMITTED_RUN_DIRS: &[&str] = &["pilot/runs", "rerun/runs", "completion/runs", "demo"];

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
}

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
    sheet = answer(&sheet, "confidence_reason", "Every assertion passes.");
    sheet
}

fn isolation(root: &Path) -> workspace::Isolation {
    workspace::Isolation::new(root, &Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap()
}

fn rendered(argv: &[&str]) -> InvariantCommand {
    InvariantCommand {
        argv: argv.iter().map(|s| (*s).to_string()).collect(),
        contract: InvariantContract::Rendered,
        equal_across_modes: true,
    }
}

fn opaque(argv: &[&str]) -> InvariantCommand {
    InvariantCommand {
        argv: argv.iter().map(|s| (*s).to_string()).collect(),
        contract: InvariantContract::OpaqueBytes,
        equal_across_modes: true,
    }
}

fn either(argv: &[&str]) -> InvariantCommand {
    InvariantCommand {
        argv: argv.iter().map(|s| (*s).to_string()).collect(),
        contract: InvariantContract::Either,
        equal_across_modes: true,
    }
}

fn rendered_content_varies_by_mode(argv: &[&str]) -> InvariantCommand {
    InvariantCommand {
        argv: argv.iter().map(|s| (*s).to_string()).collect(),
        contract: InvariantContract::Rendered,
        equal_across_modes: false,
    }
}

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
    assert!(!report.answers.contains_key("friction"));
}

// A rejected field (here, every required field simply unanswered) is a
// diagnostic on a sheet that was found, not the absence of one: `collected`
// stays true so a reader can tell "no self-report" apart from "a self-report
// with gaps" (#462).
#[test]
fn unanswered_required_field_is_a_collected_report_with_no_answers() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(questionnaire::SHEET_FILENAME),
        questionnaire::definition().render_answer_sheet(),
    )
    .unwrap();

    let report = questionnaire::collect(dir.path());
    assert!(report.collected, "{:?}", report.diagnostics);
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

// A sheet whose structure the parser cannot make sense of at all — as
// opposed to one that parses but has a rejected field — is the other state
// that means no self-report was collected (#462).
#[test]
fn unparseable_sheet_is_an_uncollected_report() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(questionnaire::SHEET_FILENAME), "garbage").unwrap();

    let report = questionnaire::collect(dir.path());
    assert!(!report.collected);
    assert!(!report.diagnostics.is_empty());
    assert!(report.answers.is_empty());
}

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

    for file in ["SPEC.md", "INSTRUCTIONS.md", "QUESTIONNAIRE.md"] {
        assert!(ws.root.join(file).is_file(), "missing {file}");
    }
    assert!(ws.root.join("docs/index.md").is_file());
    assert!(ws.root.join("docs/guides").is_dir());
    let crate_docs = ws.root.join("docs/crates/dispatch");
    assert!(crate_docs.is_dir());
    assert!(!fs::symlink_metadata(&crate_docs)
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(ws.app_dir.join("Cargo.toml").is_file());
    assert!(ws.app_dir.join("src/main.rs").is_file());

    assert!(!ws.root.join("docs/adr").exists());
    assert!(!ws.root.join("docs/spec").exists());
    assert!(!ws.root.join("docs/specs").exists());
    assert!(!ws.root.join("docs/proposals").exists());
    assert!(!ws.root.join("docs/dev").exists());
    assert!(!ws.root.join("crates").exists());

    let manifest = fs::read_to_string(ws.app_dir.join("Cargo.toml")).unwrap();
    assert!(manifest.contains("standout = \"=8.1.1\""));
    assert!(!manifest.contains("path"));
    assert!(!manifest.contains("git"));
    assert!(manifest.contains("[workspace]"));
}

#[test]
fn provisioning_records_the_docs_snapshot_digest() {
    let archetype = Archetype::load(&corpus_dir().join("archetypes"), "smoke").unwrap();
    let run_dir = tempfile::tempdir().unwrap();
    let docs_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");

    let ws = workspace::provision(run_dir.path(), &archetype, &docs_dir, "8.1.1").unwrap();

    assert_eq!(ws.docs_sha256.len(), 64);
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

#[test]
fn sandboxed_children_can_write_dev_null() {
    let dir = tempfile::tempdir().unwrap();
    let report = session::run_agent(
        dir.path(),
        &isolation(dir.path()),
        "echo probe > /dev/null",
        None,
        &dir.path().join("t.jsonl"),
        NO_TIMEOUT,
    )
    .unwrap();
    assert_eq!(report.exit_code, Some(0));
}

#[test]
fn session_scrubs_the_environment_and_writes_the_transcript() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("CORPUS_SECRET_CANARY", "leaked");

    let report = session::run_agent(
        dir.path(),
        &isolation(dir.path()),
        "env",
        None,
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
    assert_eq!(report.turns, None);
}

#[test]
fn overrunning_agent_is_killed_and_recorded_as_timed_out() {
    let dir = tempfile::tempdir().unwrap();
    let report = session::run_agent(
        dir.path(),
        &isolation(dir.path()),
        "sleep 30",
        None,
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
        None,
        &dir.path().join("t.jsonl"),
        NO_TIMEOUT,
    )
    .unwrap();
    assert_eq!(report.exit_code, Some(3));
}

// Every script below that accepts `--output` also answers `--help` with a
// line naming it, which is what the pre-matrix probe (#461) keys on.

// A binary that declines the framework's `--output` flag (#461): `--help`
// never mentions it.
const DECLINES_OUTPUT_FLAG: &str = r#"
if [ "$1" = "--help" ]; then
  echo 'Usage: fake [--json]'
  exit 0
fi
echo 'irrelevant'
"#;

// Honors `--output text|term|json`; term adds only ANSI bold.
const WELL_BEHAVED: &str = r#"
if [ "$1" = "--help" ]; then echo 'Usage: fake [--output <mode>]'; exit 0; fi
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

// A corrupt binary: term leaks an unresolved tag marker and drifts from text.
const CORRUPT: &str = r#"
if [ "$1" = "--help" ]; then echo 'Usage: fake [--output <mode>]'; exit 0; fi
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
if [ "$1" = "--help" ]; then echo 'Usage: fake [--output <mode>]'; exit 0; fi
printf '\001\002opaque\377\n'
"#;

// Ignores `--output` entirely and always prints the same bytes, the way an
// application that writes an `Artifact` to stdout does (#467): every mode's
// content is identical, so it satisfies `opaque-bytes` rather than
// `rendered` even though it never fails on an unknown flag.
const ARTIFACT_LIKE: &str = r#"
if [ "$1" = "--help" ]; then echo 'Usage: fake [--output <mode>]'; exit 0; fi
echo 'kind: Pod  name: web-0'
"#;

// Its content names the output mode, so term and text diverge by design
// (#465) even though every other identity in the cell holds: the marker and
// JSON checks on it pass.
const CONTENT_NAMES_THE_MODE: &str = r#"
if [ "$1" = "--help" ]; then echo 'Usage: fake [--output <mode>]'; exit 0; fi
mode=text
prev=""
for a in "$@"; do
  if [ "$prev" = "--output" ]; then mode="$a"; fi
  prev="$a"
done
case "$mode" in
  json) echo '{"term.output": "json"}' ;;
  term) printf 'term.output = term\n' ;;
  *) echo 'term.output = text' ;;
esac
"#;

// A binary whose help page advertises only `--output-file-path`, never
// `--output` as its own token (#461's substring trap: naive matching would
// read this as accepting `--output`).
const HELP_ADVERTISES_ONLY_OUTPUT_FILE_PATH: &str = r#"
if [ "$1" = "--help" ]; then echo 'Usage: fake [--output-file-path <path>]'; exit 0; fi
echo 'irrelevant'
"#;

// Well-behaved, but its `--help` names `--output` on stderr, not stdout
// (clap does write `--help` to stdout, but the probe must not assume that of
// every produced binary).
const HELP_MENTIONS_OUTPUT_ON_STDERR: &str = r#"
if [ "$1" = "--help" ]; then echo 'Usage: fake [--output <mode>]' 1>&2; exit 0; fi
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

// `--help` itself fails (nonzero exit, no mention of `--output` on either
// stream) even though the binary otherwise accepts and honors `--output`.
const HELP_FAILS_BUT_BINARY_ACCEPTS_OUTPUT: &str = r#"
if [ "$1" = "--help" ]; then echo 'error: help unavailable' 1>&2; exit 1; fi
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

// Artifact-like (#467's opaque-bytes shape), but hangs under `NO_COLOR`
// (i.e. `ColorState::Off`, the matrix's first color) so that color's cell
// never completes. `either` must not lock in a contract from that cell.
const ARTIFACT_LIKE_TIMES_OUT_WHEN_COLOR_OFF: &str = r#"
if [ "$1" = "--help" ]; then echo 'Usage: fake [--output <mode>]'; exit 0; fi
if [ -n "$NO_COLOR" ]; then sleep 30; fi
echo 'kind: Pod  name: web-0'
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

// #461: an app that declines `--output` (`--help` never names it) reads
// `not-applicable` on every mode cell instead of failing an unknown-flag
// error on all of them.
#[test]
fn binary_declining_the_output_flag_reads_not_applicable_not_failed() {
    let dir = tempfile::tempdir().unwrap();
    let binary = script(dir.path(), "fake", DECLINES_OUTPUT_FLAG);
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
    assert_eq!(cells.len(), 30);
    assert!(cells
        .iter()
        .all(|c| c.status == InvariantStatus::NotApplicable));
    assert!(cells
        .iter()
        .all(|c| c.detail.as_deref() == Some("no output flag")));
}

// #467: `contract = "either"` resolves to `rendered` when the binary's json
// mode actually parses as JSON, and scores exactly as a `rendered`
// declaration would.
#[test]
fn either_contract_resolves_to_rendered_for_a_well_behaved_binary() {
    let dir = tempfile::tempdir().unwrap();
    let binary = script(dir.path(), "fake", WELL_BEHAVED);
    let invariants = Invariants {
        commands: vec![either(&["greet"])],
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
    assert!(cells.iter().all(|c| c.status != InvariantStatus::Fail));
    assert_eq!(
        cells
            .iter()
            .filter(|c| c.status == InvariantStatus::Pass)
            .count(),
        14
    );
}

// #467: kubelike's shape — an app that writes an `Artifact` to stdout, whose
// bytes are identical whatever `--output` says. Declared `rendered`
// spec-first, `contract = "either"` resolves it to `opaque-bytes` instead of
// failing `stdout parses as JSON` on every cell.
#[test]
fn either_contract_resolves_to_opaque_bytes_for_an_artifact_style_binary() {
    let dir = tempfile::tempdir().unwrap();
    let binary = script(dir.path(), "fake", ARTIFACT_LIKE);
    let invariants = Invariants {
        commands: vec![either(&["get", "pods"])],
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
    assert!(cells.iter().all(|c| c.status != InvariantStatus::Fail));
    assert!(cells
        .iter()
        .filter(|c| c.check == "stdout parses as JSON")
        .all(|c| c.status == InvariantStatus::NotApplicable));
}

// #465: cargolike's shape — `config list` names the resolved output mode as
// part of its content, so term and text diverge by design. Default
// (`equal_across_modes = true`) reports the divergence as a failure.
#[test]
fn content_that_names_the_mode_fails_the_cross_mode_check_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let binary = script(dir.path(), "fake", CONTENT_NAMES_THE_MODE);
    let invariants = Invariants {
        commands: vec![rendered(&["config", "list"])],
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
    assert_eq!(
        failed,
        vec![
            "styling preserves text layout",
            "styling preserves text layout"
        ]
    );
}

// #465: `equal_across_modes = false` reads that same divergence as
// `not-applicable` instead — the identity stays present with a reason,
// nothing fails, and every other check on the command still runs.
#[test]
fn equal_across_modes_false_skips_the_cross_mode_check() {
    let dir = tempfile::tempdir().unwrap();
    let binary = script(dir.path(), "fake", CONTENT_NAMES_THE_MODE);
    let invariants = Invariants {
        commands: vec![rendered_content_varies_by_mode(&["config", "list"])],
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
    assert!(cells.iter().all(|c| c.status != InvariantStatus::Fail));
    assert_eq!(
        cells
            .iter()
            .filter(|c| c.status == InvariantStatus::Pass)
            .count(),
        12
    );
    let styling: Vec<_> = cells
        .iter()
        .filter(|c| c.check == "styling preserves text layout")
        .collect();
    assert!(styling
        .iter()
        .all(|c| c.status == InvariantStatus::NotApplicable));
    // Only the `term`-mode cells were kept from running by
    // `equal_across_modes`; `text`/`json`-mode cells never applied to this
    // check regardless, so they carry the generic mode-mismatch reason, not
    // the mode-variance one.
    assert!(styling
        .iter()
        .filter(|c| c.mode == "term")
        .all(|c| c.detail.as_deref() == Some("command's content varies by output mode")));
    assert!(styling
        .iter()
        .filter(|c| c.mode != "term")
        .all(
            |c| c.detail.as_deref() == Some("check does not apply to text mode")
                || c.detail.as_deref() == Some("check does not apply to json mode")
        ));
}

// #461: a help page that only ever names `--output-file-path` does not
// satisfy the probe for `--output` as its own flag: a substring match would
// wrongly treat this as accepting `--output`.
#[test]
fn help_page_advertising_only_output_file_path_reads_no_output_flag() {
    let dir = tempfile::tempdir().unwrap();
    let binary = script(dir.path(), "fake", HELP_ADVERTISES_ONLY_OUTPUT_FILE_PATH);
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
    assert_eq!(cells.len(), 30);
    assert!(cells
        .iter()
        .all(|c| c.status == InvariantStatus::NotApplicable));
    assert!(cells
        .iter()
        .all(|c| c.detail.as_deref() == Some("no output flag")));
}

// #461: the probe reads stderr too — clap prints `--help` to stdout, but the
// probe must not assume every produced binary does, or it would misclassify
// a binary that documents `--output` on stderr as declining it.
#[test]
fn help_page_naming_output_on_stderr_is_detected() {
    let dir = tempfile::tempdir().unwrap();
    let binary = script(dir.path(), "fake", HELP_MENTIONS_OUTPUT_ON_STDERR);
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
    assert_eq!(cells.len(), 30);
    assert!(cells.iter().all(|c| c.status != InvariantStatus::Fail));
    assert!(cells
        .iter()
        .filter(|c| c.check == "exits 0")
        .all(|c| c.status == InvariantStatus::Pass));
}

// #461: a `--help` invocation that itself fails (nonzero exit, no mention of
// `--output` on either stream) must not be read as the binary declining
// `--output` — it lets the matrix run so the real invocations report
// whatever that failure actually is.
#[test]
fn failing_help_probe_lets_the_matrix_run() {
    let dir = tempfile::tempdir().unwrap();
    let binary = script(dir.path(), "fake", HELP_FAILS_BUT_BINARY_ACCEPTS_OUTPUT);
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
    assert_eq!(cells.len(), 30);
    assert!(cells
        .iter()
        .any(|c| c.status != InvariantStatus::NotApplicable));
    assert!(cells.iter().all(|c| c.status != InvariantStatus::Fail));
    assert_eq!(
        cells
            .iter()
            .filter(|c| c.status == InvariantStatus::Pass)
            .count(),
        14
    );
}

// The `equal_across_modes` reason is only correct when the contract and
// mode would otherwise have let the check run: a rendered command's
// "opaque output preserves text bytes" cells never apply to it, regardless
// of `equal_across_modes`, so the reason must name the contract mismatch,
// not claim the content varies by mode.
#[test]
fn applicability_reason_names_the_contract_mismatch_not_mode_variance() {
    let dir = tempfile::tempdir().unwrap();
    let binary = script(dir.path(), "fake", CONTENT_NAMES_THE_MODE);
    let invariants = Invariants {
        commands: vec![rendered_content_varies_by_mode(&["config", "list"])],
        ..Invariants::default()
    };

    let cells = acceptance::run_invariants(
        &binary,
        &invariants,
        NO_TIMEOUT,
        &isolation(dir.path()),
        &dir.path().join("matrix"),
    );
    let opaque_bytes_checks: Vec<_> = cells
        .iter()
        .filter(|c| c.check == "opaque output preserves text bytes")
        .collect();
    assert!(!opaque_bytes_checks.is_empty());
    assert!(opaque_bytes_checks
        .iter()
        .all(|c| c.status == InvariantStatus::NotApplicable));
    assert!(opaque_bytes_checks
        .iter()
        .all(|c| c.detail.as_deref() == Some("rendered command uses render invariants")));
}

// #467: `either` resolves from the first cell whose runs actually executed.
// The matrix's first color (`off`, via `NO_COLOR`) times out on this binary
// and locks in nothing; the second color (`on`) runs normally and reveals
// the artifact-like, opaque-bytes shape. A resolver that locked in from the
// failed first cell would default to `rendered` and fail "stdout parses as
// JSON" on the `on` cells instead.
#[test]
fn either_contract_skips_a_first_cell_that_never_ran() {
    let dir = tempfile::tempdir().unwrap();
    let binary = script(dir.path(), "fake", ARTIFACT_LIKE_TIMES_OUT_WHEN_COLOR_OFF);
    let invariants = Invariants {
        commands: vec![either(&["get", "pods"])],
        ..Invariants::default()
    };

    let cells = acceptance::run_invariants(
        &binary,
        &invariants,
        Duration::from_millis(200),
        &isolation(dir.path()),
        &dir.path().join("matrix"),
    );
    let on_cells: Vec<_> = cells.iter().filter(|c| c.color == "on").collect();
    assert!(!on_cells.is_empty());
    assert!(on_cells.iter().all(|c| c.status != InvariantStatus::Fail));
    assert!(on_cells
        .iter()
        .filter(|c| c.check == "stdout parses as JSON")
        .all(|c| c.status == InvariantStatus::NotApplicable));
}

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
        provenance: corpus_runner::provenance::recorded(
            "claude --model claude-opus-5 -p 'do the thing'",
        ),
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
        restored.provenance.model_requested.as_deref(),
        Some("claude-opus-5")
    );
    assert_eq!(restored.provenance.prompt.as_deref(), Some("do the thing"));
    assert_eq!(
        restored.questionnaire.answers.get("summary").unwrap(),
        "did it"
    );
}

#[test]
fn committed_historical_reports_still_deserialize() {
    use corpus_runner::report::{HistoricalRun, HISTORICAL_SCHEMA_MIN, SCHEMA_VERSION};

    let mut reports = Vec::new();
    for dir in COMMITTED_RUN_DIRS {
        for entry in fs::read_dir(corpus_dir().join(dir)).unwrap() {
            let path = entry.unwrap().path().join("report.json");
            if path.is_file() {
                reports.push(path);
            }
        }
    }
    assert!(!reports.is_empty(), "no committed reports found");
    let mut versions = std::collections::BTreeSet::new();
    for path in reports {
        let report: HistoricalRun = serde_json::from_str(&fs::read_to_string(&path).unwrap())
            .unwrap_or_else(|err| panic!("{} must deserialize: {err}", path.display()));
        assert!(
            (HISTORICAL_SCHEMA_MIN..=SCHEMA_VERSION).contains(&report.schema_version),
            "{} carries schema version {} outside the supported historical range",
            path.display(),
            report.schema_version
        );
        versions.insert(report.schema_version);
    }
    // The pilot runs are schema 2 and the validity run schema 3; both are still read.
    for older in [2, 3] {
        assert!(
            versions.contains(&older),
            "committed evidence in schema {older} is gone; the historical path's \
             claim to read it is no longer tested (found {versions:?})"
        );
    }
}

#[test]
fn a_provenance_block_reads_back_without_the_fields_it_does_not_state() {
    use corpus_runner::report::AgentProvenance;

    let stated_nothing: AgentProvenance = serde_json::from_str("{}").unwrap();
    assert_eq!(stated_nothing, AgentProvenance::default());

    let backend_only: AgentProvenance = serde_json::from_str(r#"{"backend":"claude"}"#).unwrap();
    assert_eq!(backend_only.backend.as_deref(), Some("claude"));
    assert!(backend_only.settings.is_empty(), "{backend_only:?}");
    assert_eq!(backend_only.prompt, None);
}

#[test]
fn the_historical_path_reads_a_recorded_agent_provenance() {
    use corpus_runner::report::HistoricalRun;

    let mut reports: Vec<_> = Vec::new();
    for dir in COMMITTED_RUN_DIRS {
        for entry in fs::read_dir(corpus_dir().join(dir)).unwrap() {
            let path = entry.unwrap().path().join("report.json");
            if path.is_file() {
                let text = fs::read_to_string(&path).unwrap();
                reports.push((path, serde_json::from_str::<HistoricalRun>(&text).unwrap()));
            }
        }
    }
    for (path, report) in &reports {
        if report.schema_version < 4 {
            assert!(
                report.provenance.is_none(),
                "{} predates the provenance block",
                path.display()
            );
        } else {
            assert!(
                report.provenance.is_some(),
                "{} is schema {} and must state its agent provenance",
                path.display(),
                report.schema_version
            );
        }
    }
}
