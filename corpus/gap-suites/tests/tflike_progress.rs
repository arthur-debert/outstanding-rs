//! `tflike` acceptance suite — **progress milestone**, gating **PAR03** (terminal
//! citizenship, `docs/spec/parity-terminal-citizenship.md`). PAR03 is done when this
//! group turns green; the diagnostic milestone in `tflike_diagnostic.rs` belongs to
//! PAR02, not here. Behavior under test: `corpus/archetypes/tflike/SPEC.md`
//! (apply-lifecycle events in the stream; progress suppression under structured
//! modes). Every assertion is black-box against the binary named by
//! `CORPUS_TFLIKE_BIN` and runs with expected-fail semantics
//! (`corpus_gap_suites::expect_gap`).

use std::path::Path;

use corpus_gap_suites::{expect_gap, parse_ndjson, reject_ansi, run, Output};

/// Milestone group and owning epic, printed with every outcome.
const GATE: &str = "tflike/progress -> PAR03 (terminal citizenship)";
/// Env var locating the produced archetype binary.
const BIN: &str = "CORPUS_TFLIKE_BIN";

/// Applies a two-resource config against empty state in a fresh tempdir: two changes,
/// so the stream must carry lifecycle events for `web` and `db`.
fn apply_two_changes(binary: &Path) -> Result<Output, String> {
    let dir = tempfile::tempdir().expect("suite broken: creating tempdir");
    std::fs::write(
        dir.path().join("main.tfl"),
        "resource web present\nresource db present\n",
    )
    .unwrap_or_else(|err| panic!("suite broken: writing fixture: {err}"));
    run(
        binary,
        &["apply", "--config", "main.tfl", "--output", "ndjson"],
        dir.path(),
    )
}

/// Index of the first stream entry with this `type` and `resource`, if any.
fn position(entries: &[serde_json::Value], entry_type: &str, resource: &str) -> Option<usize> {
    entries
        .iter()
        .position(|e| e["type"] == entry_type && e["resource"] == resource)
}

#[test]
fn expected_fail_apply_lifecycle_events_ride_the_stream() {
    expect_gap(
        GATE,
        BIN,
        "no progress seam emits lifecycle events",
        |binary| {
            let out = apply_two_changes(binary)?;
            let entries = parse_ndjson(&out.stdout)?;
            for resource in ["web", "db"] {
                let start = position(&entries, "apply_start", resource)
                    .ok_or_else(|| format!("no apply_start event for {resource}"))?;
                let complete = position(&entries, "apply_complete", resource)
                    .ok_or_else(|| format!("no apply_complete event for {resource}"))?;
                if complete < start {
                    return Err(format!(
                        "apply_complete for {resource} precedes its apply_start"
                    ));
                }
            }
            if !entries.iter().any(|e| e["type"] == "change_summary") {
                return Err("no terminal change_summary entry".into());
            }
            Ok(())
        },
    );
}

#[test]
fn expected_fail_progress_is_suppressed_under_structured_mode() {
    expect_gap(
        GATE,
        BIN,
        "no mode-aware progress suppression exists",
        |binary| {
            let out = apply_two_changes(binary)?;
            // Machine stdout: nothing but parseable stream entries, no escapes.
            parse_ndjson(&out.stdout)?;
            reject_ansi(&out.stdout, "stdout")?;
            // No spinner redraws or color on stderr either while the stream is on.
            reject_ansi(&out.stderr, "stderr")?;
            if out.stderr.contains('\r') {
                return Err("stderr carries carriage-return progress redraws".into());
            }
            Ok(())
        },
    );
}
