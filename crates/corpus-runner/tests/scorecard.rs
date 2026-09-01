// The scorecard script, checked against the figures the pilot scorecard
// published. Scorecard v2 compares the re-run with the pilot, and that
// comparison is only worth reading if both sides are counted the same way and
// were asked the same question — so the script that counts them has to
// reproduce the pilot's numbers from the pilot's own committed reports, and
// has to say so when a row's pins or agent make it something else.

#![cfg(unix)]

use std::path::Path;
use std::process::Command;

use serde_json::json;

fn repo() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

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

/// One committed run as the script reads it: the fields every row needs, plus
/// the pins, the provenance block and the recorded command a test varies. A
/// null provenance writes a report from before schema 4, which states none.
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

/// Commands whose splitting the two implementations have to agree on,
/// including the ones both refuse. Quoting is the whole point of most of
/// them: a `$`, a pipe or a glob inside quotes is an ordinary character to a
/// shell and to the runner, and only an unquoted one refuses the command.
const SPLIT_ALIKE: &[&str] = &[
    "claude -p hello",
    "claude --model claude-opus-5 -p 'the cost is $5'",
    "claude -p 'a | pipe and a * glob' --setting-sources ''",
    r#"claude -p "a \"quoted\" word, a \$ and a literal \\""#,
    "/opt/agents/claude --dangerously-skip-permissions -p go",
    "claude\t-p\tgo",
    // Refused by both: a pipeline, a redirection, an expansion, an unquoted
    // glob, a word-leading tilde, an unclosed quote, a dangling escape, and
    // a command with no program at all.
    "printf x | claude -p go",
    "claude -p go > out.txt",
    "claude -p $HOME",
    "claude -p *.rs",
    "~/bin/claude -p go",
    "claude -p 'unclosed",
    "claude -p go\\",
    "   ",
];

/// The runner splits the command it spawns; the scorecard splits the command
/// a report recorded, to recover the provenance a pre-schema-4 run states
/// nowhere else. Two splitters, so one table goes through both: a command
/// the runner would have refused never ran, and reading an agent out of it
/// would invent one, while a command the runner accepts has to recover
/// rather than read as a run that stated no agent.
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
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let scorecard_side: Vec<Vec<String>> = serde_json::from_slice(&output.stdout).unwrap();

    for (command, scorecard_argv) in SPLIT_ALIKE.iter().zip(&scorecard_side) {
        // The runner refuses with an error where the scorecard recovers
        // nothing; both mean the same thing about the same command.
        let runner_argv = corpus_runner::session::direct_argv(command).unwrap_or_default();
        assert_eq!(&runner_argv, scorecard_argv, "disagreed on {command:?}");
    }
}

/// Each row as `corpus/pilot/scorecard.md` published it: the acceptance
/// ratio, the invariant ratio with its full planned breakdown, and the
/// workaround count its prose reports (`gitlike`'s fifth listed item is the
/// deliberate direct-write path v1 counts as "4 workarounds plus one").
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
    let table = scorecard(&["pilot=corpus/pilot/runs"]);
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

/// The pilot reports predate the provenance block, so the script recovers
/// what ran them from the run's own two sources — and says that it did,
/// rather than presenting a recovered fact as a recorded one.
#[test]
fn a_report_without_a_provenance_block_names_its_agent_from_the_run_record() {
    let table = scorecard(&["pilot=corpus/pilot/runs"]);
    for line in table.lines().filter(|line| line.starts_with("| gitlike |")) {
        assert!(
            line.contains("claude 2.1.234, claude-opus-5[1m] (recovered)"),
            "{line}"
        );
    }
}

/// The recovery reaches every provenance field, not only the three the cell
/// prints: the pilot's recorded command states the prompt, the settings and
/// whether a model was asked for, and the comparison needs them to say the
/// re-run asked the same question.
#[test]
fn recovery_reads_the_prompt_and_settings_from_the_recorded_command() {
    let rows: serde_json::Value =
        serde_json::from_str(&scorecard(&["pilot=corpus/pilot/runs", "--json"])).unwrap();
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
    // No `--model` in the command: the run took the backend's default, which
    // is not the same fact as a model asked for and unrecorded.
    assert_eq!(
        provenance["model_requested"],
        serde_json::Value::Null,
        "{row}"
    );
}

/// A run that states no provenance, records no command the script can split,
/// and has no transcript beside it names no agent — the cell says so instead
/// of presenting the runner's usual backend as this run's.
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

/// The suite that judged a run is what its acceptance figures mean. A re-run
/// against an edited suite is measuring a different question, and a scorecard
/// that prints the two ratios side by side has to say so — the improvement
/// otherwise reads as the framework's.
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

/// The agent comparison is the whole provenance block, not the three fields
/// the cell prints. A run asked for a different model, or handed a different
/// prompt or setting, is not the same question even when what answered — and
/// what the table shows — is identical.
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
    // The printed cell is identical on both rows, and the rows are still not
    // the same question.
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

/// Agents list their workarounds in whichever markdown form they reach for,
/// and the count has to be of items rather than of one favoured syntax — the
/// pilot numbered them, the re-run used bullets, bold lead-ins and letters.
/// Continuation lines are indented, and indented lines are not items.
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

/// Two run sets land in one table, grouped by archetype: that side-by-side
/// is what a scorecard comparison reads.
#[test]
fn runs_from_two_sets_sit_beside_each_other_under_their_archetype() {
    let table = scorecard(&["pilot=corpus/pilot/runs", "again=corpus/pilot/runs"]);
    let rows: Vec<&str> = table
        .lines()
        .filter(|line| line.starts_with("| gitlike |"))
        .collect();
    assert_eq!(rows.len(), 2, "{table}");
    assert!(rows[0].contains("| again |"), "{}", rows[0]);
    assert!(rows[1].contains("| pilot |"), "{}", rows[1]);
}
