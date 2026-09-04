//! `tflike` acceptance suite, progress milestone: black-box against the binary
//! named by `CORPUS_TFLIKE_BIN`. Behavior under test is
//! `corpus/archetypes/tflike/spec.md`; the gate is closed in `gaps.toml`, so
//! every assertion is a plain requirement.

use std::path::Path;

use corpus_gap_suites::{parse_ndjson, reject_ansi, required_binary, run, Output};

const BIN: &str = "CORPUS_TFLIKE_BIN";

/// `state` is written at the default `main.tfl.state` path; the tempdir is returned to inspect.
fn apply_in_tempdir(
    binary: &Path,
    config: &str,
    state: Option<&str>,
) -> Result<(tempfile::TempDir, Output), String> {
    let dir = tempfile::tempdir().expect("suite broken: creating tempdir");
    std::fs::write(dir.path().join("main.tfl"), config)
        .unwrap_or_else(|err| panic!("suite broken: writing fixture: {err}"));
    if let Some(state) = state {
        std::fs::write(dir.path().join("main.tfl.state"), state)
            .unwrap_or_else(|err| panic!("suite broken: writing state fixture: {err}"));
    }
    let out = run(
        binary,
        &["apply", "--config", "main.tfl", "--output", "ndjson"],
        dir.path(),
    )?;
    Ok((dir, out))
}

fn apply_two_changes(binary: &Path) -> Result<(tempfile::TempDir, Output), String> {
    apply_in_tempdir(binary, "resource web present\nresource db present\n", None)
}

fn state_resources(dir: &Path) -> Result<Vec<String>, String> {
    let contents = std::fs::read_to_string(dir.join("main.tfl.state"))
        .map_err(|err| format!("apply left no readable state file: {err}"))?;
    Ok(contents.lines().map(str::to_string).collect())
}

fn position(entries: &[serde_json::Value], entry_type: &str, resource: &str) -> Option<usize> {
    entries
        .iter()
        .position(|e| e["type"] == entry_type && e["resource"] == resource)
}

/// Exactly one `apply_start`/`apply_complete` pair, in that order; duplicates are a mismatch.
fn assert_lifecycle_pair(entries: &[serde_json::Value], resource: &str) -> Result<(), String> {
    for entry_type in ["apply_start", "apply_complete"] {
        let count = entries
            .iter()
            .filter(|e| e["type"] == entry_type && e["resource"] == resource)
            .count();
        if count != 1 {
            return Err(format!(
                "expected exactly one {entry_type} event for {resource}, found {count}"
            ));
        }
    }
    let start = position(entries, "apply_start", resource).expect("count checked");
    let complete = position(entries, "apply_complete", resource).expect("count checked");
    if complete < start {
        return Err(format!(
            "apply_complete for {resource} precedes its apply_start"
        ));
    }
    Ok(())
}

fn assert_terminal_summary(
    entries: &[serde_json::Value],
    add: i64,
    remove: i64,
) -> Result<(), String> {
    let last = entries.last().ok_or("the stream was empty")?;
    if last["type"] != "change_summary" {
        return Err(format!(
            "the terminal stream entry must be a change_summary, was {last}"
        ));
    }
    if last["add"] != add || last["remove"] != remove {
        return Err(format!(
            "change_summary should count add:{add}/remove:{remove}, was {last}"
        ));
    }
    Ok(())
}

#[test]
fn apply_lifecycle_events_ride_the_stream_and_state_is_rewritten() {
    let binary = required_binary(BIN);
    let (dir, out) = apply_two_changes(&binary).unwrap();
    assert_eq!(out.code, Some(0), "a successful apply should exit 0");
    let entries = parse_ndjson(&out.stdout).unwrap();
    for resource in ["web", "db"] {
        assert_lifecycle_pair(&entries, resource).unwrap();
    }
    assert_terminal_summary(&entries, 2, 0).unwrap();
    // Plausible events without applying anything may not pass.
    let mut state = state_resources(dir.path()).unwrap();
    state.sort();
    assert_eq!(
        state,
        ["db", "web"],
        "the state file should record exactly web and db"
    );
}

#[test]
fn apply_deletion_emits_lifecycle_and_rewrites_state() {
    let binary = required_binary(BIN);
    let (dir, out) = apply_in_tempdir(&binary, "resource web absent\n", Some("web\n")).unwrap();
    assert_eq!(out.code, Some(0), "a successful apply should exit 0");
    let entries = parse_ndjson(&out.stdout).unwrap();
    assert_lifecycle_pair(&entries, "web").unwrap();
    assert_terminal_summary(&entries, 0, 1).unwrap();
    let state = state_resources(dir.path()).unwrap();
    assert!(
        state.is_empty(),
        "the state file should be empty after the deletion, holds {state:?}"
    );
}

#[test]
fn progress_is_suppressed_under_structured_mode() {
    let binary = required_binary(BIN);
    let (_dir, out) = apply_two_changes(&binary).unwrap();
    assert_eq!(out.code, Some(0), "a successful apply should exit 0");
    parse_ndjson(&out.stdout).unwrap();
    reject_ansi(&out.stdout, "stdout").unwrap();
    // A known-success structured invocation has no legitimate stderr traffic:
    // plain prose progress is as much a mismatch as a spinner redraw.
    assert_eq!(
        out.stderr, "",
        "structured mode must silence stderr entirely"
    );
}
