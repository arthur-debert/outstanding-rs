// The scorecard script, checked against the figures the ROB03 pilot
// scorecard published. Scorecard v2 compares the re-run with the pilot, and
// that comparison is only worth reading if both sides are counted the same
// way — so the script that counts them has to reproduce the pilot's numbers
// from the pilot's own committed reports.

#![cfg(unix)]

use std::path::Path;
use std::process::Command;

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
/// what ran them from their committed transcripts — and says that it did,
/// rather than presenting a recovered fact as a recorded one.
#[test]
fn a_report_without_a_provenance_block_names_its_agent_from_the_transcript() {
    let table = scorecard(&["pilot=corpus/pilot/runs"]);
    for line in table.lines().filter(|line| line.starts_with("| gitlike |")) {
        assert!(
            line.contains("claude 2.1.234, claude-opus-5[1m] (from transcript)"),
            "{line}"
        );
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
