// The scorecard script, checked against the figures the pilot scorecard
// published: it has to reproduce them from the committed reports, and say so
// when a row's pins or agent make it a different question.

#![cfg(unix)]

use std::path::Path;
use std::process::Command;

use serde_json::json;

fn repo() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// Synthetic stand-ins for `corpus/pilot/runs`: same acceptance, invariant
// and questionnaire data as the committed pilot reports, so the figures
// below still reproduce what the pilot scorecard published, without this
// test depending on committed run evidence.
const PILOT_RUNS: &str = "crates/corpus-runner/tests/fixtures/pilot/runs";

fn scorecard(sets: &[&str]) -> String {
    let output = Command::new("python3")
        .arg(repo().join("corpus/scorecard.py"))
        .args(sets)
        .current_dir(repo())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "scorecard.py failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

// A null provenance writes a report from before schema 4.
fn write_report(
    runs_dir: &Path,
    run_id: &str,
    archetype: &str,
    pins: serde_json::Value,
    provenance: serde_json::Value,
    agent_cmd: &str,
) {
    let run = runs_dir.join(run_id);
    std::fs::create_dir_all(&run).unwrap();
    let mut report = json!({
        "schema_version": 4,
        "run_id": run_id,
        "archetype": {"name": archetype, "spec_sha256": "spec"},
        "pins": pins,
        "session": {
            "wall_seconds": 1.0,
            "output_tokens": 2,
            "transcript": "t.jsonl",
            "agent_cmd": agent_cmd,
        },
        "acceptance": {"built": true, "cases": [{"name": "only", "outcome": "pass"}]},
        "invariants": [],
        "questionnaire": {"answers": {"workarounds": "", "friction": ""}},
    });
    if !provenance.is_null() {
        report["provenance"] = provenance;
    }
    std::fs::write(run.join("report.json"), report.to_string()).unwrap();
}

fn archetype_row<'a>(table: &'a str, archetype: &str) -> &'a str {
    table
        .lines()
        .find(|line| line.starts_with(&format!("| {archetype} |")))
        .unwrap_or_else(|| panic!("no row for {archetype} in:\n{table}"))
}

// Commands the runner's splitter and the scorecard's have to agree on, refusals included.
const SPLIT_ALIKE: &[&str] = &[
    "claude -p hello",
    "claude --model claude-opus-5 -p 'the cost is $5'",
    "claude -p 'a | pipe and a * glob' --setting-sources ''",
    r#"claude -p "a \"quoted\" word, a \$ and a literal \\""#,
    "/opt/agents/claude --dangerously-skip-permissions -p go",
    "claude\t-p\tgo",
    // Refused by both.
    "printf x | claude -p go",
    "claude -p go > out.txt",
    "claude -p $HOME",
    "claude -p *.rs",
    "~/bin/claude -p go",
    "claude -p 'unclosed",
    "claude -p go\\",
    "   ",
];

#[test]
fn the_scorecard_splits_a_recorded_command_the_way_the_runner_does() {
    const PROGRAM: &str = "import json, sys; sys.path.insert(0, 'corpus'); \
                           import scorecard; \
                           print(json.dumps([scorecard.direct_argv(c) for c in sys.argv[1:]]))";
    let output = Command::new("python3")
        .arg("-c")
        .arg(PROGRAM)
        .args(SPLIT_ALIKE)
        .current_dir(repo())
        // Otherwise a `__pycache__` lands beside the committed evidence.
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let scorecard_side: Vec<Vec<String>> = serde_json::from_slice(&output.stdout).unwrap();

    for (command, scorecard_argv) in SPLIT_ALIKE.iter().zip(&scorecard_side) {
        let runner_argv = corpus_runner::session::direct_argv(command).unwrap_or_default();
        assert_eq!(&runner_argv, scorecard_argv, "disagreed on {command:?}");
    }
}

// As `corpus/pilot/scorecard.md` published them; `gitlike`'s fifth item is the
// deliberate direct-write path counted as "4 workarounds plus one".
const PILOT_FIGURES: &[(&str, &str, &str, &str)] = &[
    (
        "formlike",
        "4/11 (36.4%); 7 fail",
        "12/14 (85.7%) applicable; 30 planned: 12 pass, 2 fail, 16 N/A",
        "| 3 |",
    ),
    (
        "ghlike",
        "18/18 (100.0%)",
        "70/70 (100.0%) applicable; 150 planned: 70 pass, 80 N/A",
        "| 6 |",
    ),
    (
        "gitlike",
        "15/19 (78.9%); 4 unexpected-pass",
        "48/48 (100.0%) applicable; 120 planned: 48 pass, 72 N/A",
        "| 5 |",
    ),
    (
        "systemdlike",
        "17/18 (94.4%); 1 fail",
        "56/56 (100.0%) applicable; 120 planned: 56 pass, 64 N/A",
        "| 6 |",
    ),
];

#[test]
fn the_script_reproduces_the_pilot_scorecards_published_figures() {
    let table = scorecard(&[&format!("pilot={PILOT_RUNS}")]);
    for (archetype, acceptance, invariants, workarounds) in PILOT_FIGURES {
        let row = table
            .lines()
            .find(|line| line.starts_with(&format!("| {archetype} |")))
            .unwrap_or_else(|| panic!("no row for {archetype} in:\n{table}"));
        assert!(row.contains(acceptance), "{archetype}: {row}");
        assert!(row.contains(invariants), "{archetype}: {row}");
        assert!(row.contains(workarounds), "{archetype}: {row}");
    }
}

#[test]
fn a_report_without_a_provenance_block_names_its_agent_from_the_run_record() {
    let table = scorecard(&[&format!("pilot={PILOT_RUNS}")]);
    for line in table.lines().filter(|line| line.starts_with("| gitlike |")) {
        assert!(
            line.contains("claude 2.1.234, claude-opus-5[1m] (recovered)"),
            "{line}"
        );
    }
}

// Not the fixtures: the real committed pilot reports, whose transcripts are
// deleted (D28). Their `recovered_provenance` block has to carry what a
// transcript read would have, or this documented command
// (`pilot=corpus/pilot/runs`) degrades silently to "version unstated, model
// unstated" the moment the fixtures stop standing in for it.
#[test]
fn the_real_committed_pilot_reports_stay_self_sufficient_without_their_deleted_transcripts() {
    let real_pilot_runs = repo().join("corpus/pilot/runs");
    let table = scorecard(&[&format!("pilot={}", real_pilot_runs.display())]);
    for archetype in ["formlike", "ghlike", "gitlike", "systemdlike"] {
        let row = archetype_row(&table, archetype);
        assert!(
            row.contains("claude 2.1.234, claude-opus-5[1m] (recovered)"),
            "{archetype}: {row}"
        );
        assert!(
            row.contains("| single run |"),
            "{archetype} unexpected comparable column: {row}"
        );
    }
    let row = archetype_row(&table, "validity");
    assert!(
        row.contains("claude 2.1.252, claude-opus-5[1m] (recovered)"),
        "{row}"
    );
}

#[test]
fn recovery_reads_the_prompt_and_settings_from_the_recorded_command() {
    let rows: serde_json::Value =
        serde_json::from_str(&scorecard(&[&format!("pilot={PILOT_RUNS}"), "--json"])).unwrap();
    let row = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["archetype"] == "gitlike")
        .unwrap();
    assert_eq!(row["provenance_recovered"], true, "{row}");
    let provenance = &row["provenance"];
    assert_eq!(provenance["backend"], "claude", "{row}");
    assert!(
        provenance["prompt"]
            .as_str()
            .unwrap()
            .starts_with("Read INSTRUCTIONS.md"),
        "{row}"
    );
    assert!(
        provenance["settings"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("--dangerously-skip-permissions")),
        "{row}"
    );
    // No `--model`: the backend's default, not a model asked for and unrecorded.
    assert_eq!(
        provenance["model_requested"],
        serde_json::Value::Null,
        "{row}"
    );
}

#[test]
fn recovery_reads_the_transcripts_head_rather_than_the_whole_session() {
    let temp = tempfile::tempdir().unwrap();
    let run = temp.path().join("headlike-1");
    std::fs::create_dir_all(&run).unwrap();
    write_report(
        temp.path(),
        "headlike-1",
        "headlike",
        json!({"framework_version": "9.0.0"}),
        json!(null),
        "claude -p go",
    );
    // An init record larger than the head budget, then a second announcement a
    // bounded read must never reach.
    let init = format!(
        concat!(
            r#"{{"type":"system","subtype":"init","model":"claude-opus-5[1m]","#,
            r#""claude_code_version":"2.1.252","tools":"{}"}}"#
        ),
        "x".repeat(300 * 1024)
    );
    let beyond =
        r#"{"type":"system","subtype":"init","model":"never-read","claude_code_version":"0.0.0"}"#;
    std::fs::write(
        run.join("t.jsonl"),
        format!("{init}\n{}\n{beyond}\n", "y".repeat(400 * 1024)),
    )
    .unwrap();

    let table = scorecard(&[&format!("t={}", temp.path().display())]);
    let row = archetype_row(&table, "headlike");
    assert!(
        row.contains("claude 2.1.252, claude-opus-5[1m] (recovered)"),
        "{row}"
    );
    assert!(!row.contains("never-read"), "{row}");
}

#[test]
fn a_run_that_states_no_agent_is_reported_unrecorded() {
    let temp = tempfile::tempdir().unwrap();
    write_report(
        temp.path(),
        "quietlike-1",
        "quietlike",
        json!({"framework_version": "9.0.0"}),
        json!(null),
        "printf x | claude -p go",
    );

    let table = scorecard(&[&format!("t={}", temp.path().display())]);
    let row = archetype_row(&table, "quietlike");
    assert!(row.contains("| unrecorded |"), "{row}");
}

#[test]
fn a_row_judged_by_a_different_suite_is_marked_not_comparable() {
    let temp = tempfile::tempdir().unwrap();
    let agent = json!({
        "backend": "claude",
        "executable_version": "2.1.252",
        "model_observed": "claude-opus-5[1m]",
    });
    for (set, suite) in [
        (
            "v1",
            "e87a2b0580000000000000000000000000000000000000000000000000000000",
        ),
        (
            "v2",
            "f3db09300000000000000000000000000000000000000000000000000000000000",
        ),
    ] {
        write_report(
            &temp.path().join(set),
            "suitelike-1",
            "suitelike",
            json!({"framework_version": "9.0.0", "acceptance_sha256": suite}),
            agent.clone(),
            "claude -p go",
        );
    }

    let table = scorecard(&[
        &format!("v1={}", temp.path().join("v1").display()),
        &format!("v2={}", temp.path().join("v2").display()),
    ]);
    let rows: Vec<&str> = table
        .lines()
        .filter(|line| line.starts_with("| suitelike |"))
        .collect();
    assert!(rows[0].contains("| baseline |"), "{table}");
    assert!(rows[1].contains("| no: suite |"), "{table}");
    assert!(
        table.contains("the acceptance suite: e87a2b05… → f3db0930…"),
        "{table}"
    );
}

#[test]
fn an_agent_that_differs_only_in_unprinted_fields_is_still_a_difference() {
    let temp = tempfile::tempdir().unwrap();
    let pins = json!({"framework_version": "9.0.0", "acceptance_sha256": "same"});
    let shown = ("claude", "2.1.252", "claude-opus-5[1m]");
    for (set, requested, prompt, setting) in [
        ("v1", "claude-opus-5", "do the thing", "--verbose"),
        ("v2", "claude-opus-5[1m]", "do the other thing", "--quiet"),
    ] {
        write_report(
            &temp.path().join(set),
            "agentlike-1",
            "agentlike",
            pins.clone(),
            json!({
                "backend": shown.0,
                "executable_version": shown.1,
                "model_observed": shown.2,
                "model_requested": requested,
                "prompt": prompt,
                "settings": [setting],
            }),
            "claude -p go",
        );
    }

    let table = scorecard(&[
        &format!("v1={}", temp.path().join("v1").display()),
        &format!("v2={}", temp.path().join("v2").display()),
    ]);
    let rows: Vec<&str> = table
        .lines()
        .filter(|line| line.starts_with("| agentlike |"))
        .collect();
    assert!(
        rows[0].contains("claude 2.1.252, claude-opus-5[1m]"),
        "{table}"
    );
    assert!(
        rows[1].contains("claude 2.1.252, claude-opus-5[1m]"),
        "{table}"
    );
    assert!(rows[1].contains("| no: agent |"), "{table}");
    for stated in [
        "model requested: claude-opus-5 → claude-opus-5[1m]",
        "prompt: do the thing → do the other thing",
        "settings: --verbose → --quiet",
    ] {
        assert!(table.contains(stated), "{stated} missing from:\n{table}");
    }
}

// Indented lines are continuations, not items.
#[test]
fn listed_items_are_counted_in_every_form_an_agent_has_used() {
    let temp = tempfile::tempdir().unwrap();
    let run = temp.path().join("listlike-1");
    std::fs::create_dir_all(&run).unwrap();
    let answer = "1. numbered\n\
                  2) parenthesized\n\
                  a) lettered\n\
                  - dashed\n\
                  * starred\n\
                  **bold lead-in.** and its sentence\n\
                  \x20  1. an indented continuation, not an item\n\
                  plain prose, not an item\n";
    let report = serde_json::json!({
        "schema_version": 4,
        "run_id": "listlike-1",
        "archetype": {"name": "listlike"},
        "pins": {"framework_version": "9.0.0"},
        "session": {"wall_seconds": 1.0, "output_tokens": 2, "transcript": "t.jsonl"},
        "provenance": {"backend": "claude", "executable_version": "1", "model_observed": "m"},
        "acceptance": {"built": true, "cases": []},
        "invariants": [],
        "questionnaire": {"answers": {"workarounds": answer, "friction": answer}},
    });
    std::fs::write(run.join("report.json"), report.to_string()).unwrap();

    let table = scorecard(&[&format!("t={}", temp.path().display())]);
    let row = table
        .lines()
        .find(|line| line.starts_with("| listlike |"))
        .unwrap_or_else(|| panic!("{table}"));
    assert!(row.contains("| 6 | 6 |"), "{row}");
}

#[test]
fn runs_from_two_sets_sit_beside_each_other_under_their_archetype() {
    let table = scorecard(&[
        &format!("pilot={PILOT_RUNS}"),
        &format!("again={PILOT_RUNS}"),
    ]);
    let rows: Vec<&str> = table
        .lines()
        .filter(|line| line.starts_with("| gitlike |"))
        .collect();
    assert_eq!(rows.len(), 2, "{table}");
    assert!(rows[0].contains("| again |"), "{}", rows[0]);
    assert!(rows[1].contains("| pilot |"), "{}", rows[1]);
}
