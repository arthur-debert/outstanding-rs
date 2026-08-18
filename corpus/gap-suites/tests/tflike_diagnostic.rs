//! `tflike` acceptance suite — **diagnostic milestone**, gating **PAR02** (the machine
//! contract, `docs/spec/parity-machine-contract.md`). PAR02 is done when this group
//! turns green; the progress milestone in `tflike_progress.rs` belongs to PAR03, not
//! here. Behavior under test: `corpus/archetypes/tflike/spec.md`. Every assertion is
//! black-box against the binary named by `CORPUS_TFLIKE_BIN` and runs with
//! expected-fail semantics (`corpus_gap_suites::expect_gap`).

use std::path::Path;

use corpus_gap_suites::{expect_gap, parse_ndjson, run, Output};

/// Milestone group and owning epic, printed with every outcome.
const GATE: &str = "tflike/diagnostic -> PAR02 (machine contract)";
/// Env var locating the produced archetype binary.
const BIN: &str = "CORPUS_TFLIKE_BIN";

/// A config whose two resources are absent from (missing) state: a two-change plan.
const CONFIG_TWO_CHANGES: &str = "resource web present\nresource db present\n";
/// A config whose second line breaks the `resource <name> <state>` grammar.
const CONFIG_LINE_2_BROKEN: &str = "resource web present\nresurce db present\n";

/// Writes `contents` as `name` inside `dir`, returning the relative path the suite
/// passes on the command line (diagnostic ranges must echo the path as given).
fn fixture(dir: &Path, name: &str, contents: &str) -> String {
    std::fs::write(dir.join(name), contents)
        .unwrap_or_else(|err| panic!("suite broken: writing fixture {name}: {err}"));
    name.to_string()
}

/// Runs `tflike` in a fresh tempdir holding a config (and optionally a state file).
fn plan_in_tempdir(
    binary: &Path,
    config: &str,
    state: Option<&str>,
    extra_args: &[&str],
) -> Result<Output, String> {
    let dir = tempfile::tempdir().expect("suite broken: creating tempdir");
    let config_path = fixture(dir.path(), "main.tfl", config);
    if let Some(state) = state {
        fixture(dir.path(), "main.tfl.state", state);
    }
    let mut args = vec!["plan"];
    args.extend_from_slice(extra_args);
    args.extend_from_slice(&["--config", &config_path, "--output", "ndjson"]);
    run(binary, &args, dir.path())
}

/// Asserts the stream shape the spec fixes for every successful plan: a leading
/// `version` entry, one `planned_change` per difference (`expected_changes` as
/// `(resource, action)` pairs, order between resources unspecified), and a terminal
/// `change_summary` whose counts match.
fn assert_plan_stream(
    entries: &[serde_json::Value],
    expected_changes: &[(&str, &str)],
    add: i64,
    remove: i64,
) -> Result<(), String> {
    let first = entries.first().ok_or("the stream was empty")?;
    if first["type"] != "version" {
        return Err(format!(
            "the first stream entry must be a version entry, was {first}"
        ));
    }
    if !first["format_version"].is_i64() {
        return Err(format!(
            "the version entry must carry an integer format_version, was {first}"
        ));
    }
    let changes: Vec<_> = entries
        .iter()
        .filter(|e| e["type"] == "planned_change")
        .collect();
    if changes.len() != expected_changes.len() {
        return Err(format!(
            "expected {} planned_change entries, found {}",
            expected_changes.len(),
            changes.len()
        ));
    }
    for (resource, action) in expected_changes {
        if !changes
            .iter()
            .any(|e| e["resource"] == *resource && e["action"] == *action)
        {
            return Err(format!(
                "no planned_change entry with resource {resource:?} and action {action:?}"
            ));
        }
    }
    let last = entries.last().expect("non-empty checked via first()");
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
fn expected_fail_successful_plan_streams_version_changes_and_summary() {
    expect_gap(GATE, BIN, "no NDJSON stream mode exists", |binary| {
        let out = plan_in_tempdir(binary, CONFIG_TWO_CHANGES, None, &[])?;
        if out.code != Some(0) {
            return Err(format!(
                "a successful plan should exit 0, exited {:?}",
                out.code
            ));
        }
        let entries = parse_ndjson(&out.stdout)?;
        assert_plan_stream(&entries, &[("web", "create"), ("db", "create")], 2, 0)
    });
}

#[test]
fn expected_fail_plan_deletion_with_explicit_state_streams_a_delete() {
    expect_gap(GATE, BIN, "no NDJSON stream mode exists", |binary| {
        let dir = tempfile::tempdir().expect("suite broken: creating tempdir");
        let config_path = fixture(dir.path(), "main.tfl", "resource web absent\n");
        // A non-default name proves --state is honored rather than <config>.state.
        let state_path = fixture(dir.path(), "recorded.state", "web\n");
        let out = run(
            binary,
            &[
                "plan",
                "--config",
                &config_path,
                "--state",
                &state_path,
                "--output",
                "ndjson",
            ],
            dir.path(),
        )?;
        if out.code != Some(0) {
            return Err(format!(
                "a successful plan should exit 0, exited {:?}",
                out.code
            ));
        }
        let entries = parse_ndjson(&out.stdout)?;
        assert_plan_stream(&entries, &[("web", "delete")], 0, 1)
    });
}

#[test]
fn expected_fail_one_error_fixture_yields_exactly_one_error_diagnostic_with_range() {
    expect_gap(
        GATE,
        BIN,
        "failures are prose, not stream diagnostics",
        |binary| {
            let out = plan_in_tempdir(binary, CONFIG_LINE_2_BROKEN, None, &[])?;
            if out.code != Some(1) {
                return Err(format!(
                    "a failed plan should exit 1, exited {:?}",
                    out.code
                ));
            }
            let entries = parse_ndjson(&out.stdout)?;
            let errors: Vec<_> = entries
                .iter()
                .filter(|e| e["type"] == "diagnostic" && e["severity"] == "error")
                .collect();
            if errors.len() != 1 {
                return Err(format!(
                    "expected exactly one \"severity\":\"error\" entry, found {}",
                    errors.len()
                ));
            }
            let range = &errors[0]["range"];
            if range["filename"] != "main.tfl" {
                return Err(format!(
                    "range.filename should be the config path as given, was {}",
                    range["filename"]
                ));
            }
            if range["start"]["line"] != 2 {
                return Err(format!(
                    "range.start.line should point at the offending line 2, was {}",
                    range["start"]["line"]
                ));
            }
            Ok(())
        },
    );
}

#[test]
fn expected_fail_detailed_exitcode_returns_zero_with_no_changes() {
    expect_gap(
        GATE,
        BIN,
        "no empty/changed/failed exit-code vocabulary",
        |binary| {
            let out = plan_in_tempdir(
                binary,
                "resource web present\n",
                Some("web\n"),
                &["-detailed-exitcode"],
            )?;
            if out.code != Some(0) {
                return Err(format!("empty plan should exit 0, exited {:?}", out.code));
            }
            Ok(())
        },
    );
}

#[test]
fn expected_fail_detailed_exitcode_returns_two_with_changes() {
    expect_gap(
        GATE,
        BIN,
        "no empty/changed/failed exit-code vocabulary",
        |binary| {
            let out = plan_in_tempdir(binary, CONFIG_TWO_CHANGES, None, &["-detailed-exitcode"])?;
            if out.code != Some(2) {
                return Err(format!(
                    "a changed plan should exit 2, exited {:?}",
                    out.code
                ));
            }
            Ok(())
        },
    );
}

#[test]
fn expected_fail_detailed_exitcode_returns_one_on_error() {
    expect_gap(
        GATE,
        BIN,
        "no empty/changed/failed exit-code vocabulary",
        |binary| {
            let out = plan_in_tempdir(binary, CONFIG_LINE_2_BROKEN, None, &["-detailed-exitcode"])?;
            if out.code != Some(1) {
                return Err(format!(
                    "a failed plan should exit 1, exited {:?}",
                    out.code
                ));
            }
            Ok(())
        },
    );
}

#[test]
fn expected_fail_handler_error_yields_a_well_formed_diagnostic_not_prose() {
    expect_gap(
        GATE,
        BIN,
        "handler errors leak prose into machine output",
        |binary| {
            let dir = tempfile::tempdir().expect("suite broken: creating tempdir");
            let config_path = fixture(dir.path(), "main.tfl", "resource fail:web present\n");
            let out = run(
                binary,
                &["apply", "--config", &config_path, "--output", "ndjson"],
                dir.path(),
            )?;
            // The whole stream must still parse — prose leaking in breaks this first.
            let entries = parse_ndjson(&out.stdout)?;
            let diagnostic = entries
                .iter()
                .find(|e| e["type"] == "diagnostic" && e["severity"] == "error")
                .ok_or("no \"severity\":\"error\" diagnostic entry for the failed apply")?;
            // Well-formed per the spec's diagnostic shape, not merely typed: summary
            // and detail are non-empty strings.
            for field in ["summary", "detail"] {
                if diagnostic[field]
                    .as_str()
                    .filter(|text| !text.is_empty())
                    .is_none()
                {
                    return Err(format!(
                        "the diagnostic must carry a non-empty string {field}, was {}",
                        diagnostic[field]
                    ));
                }
            }
            // "Failures are stream entries, never prose": the old prose error path may
            // not survive on stderr alongside the stream diagnostic.
            if !out.stderr.is_empty() {
                return Err(format!(
                    "a structured-mode failure must not leak prose onto stderr, got {:?}",
                    out.stderr
                ));
            }
            if out.code != Some(1) {
                return Err(format!(
                    "a failed apply should exit 1, exited {:?}",
                    out.code
                ));
            }
            Ok(())
        },
    );
}
