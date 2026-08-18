//! `tflike` acceptance suite — **diagnostic milestone**, gating **PAR02** (the machine
//! contract, `docs/spec/parity-machine-contract.md`). PAR02 is done when this group
//! turns green; the progress milestone in `tflike_progress.rs` belongs to PAR03, not
//! here. Behavior under test: `corpus/archetypes/tflike/SPEC.md`. Every assertion is
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

#[test]
fn expected_fail_every_stdout_line_parses_as_json() {
    expect_gap(GATE, BIN, "no NDJSON stream mode exists", |binary| {
        let out = plan_in_tempdir(binary, CONFIG_TWO_CHANGES, None, &[])?;
        let entries = parse_ndjson(&out.stdout)?;
        if entries.is_empty() {
            return Err("the stream was empty".into());
        }
        Ok(())
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
            if !entries
                .iter()
                .any(|e| e["type"] == "diagnostic" && e["severity"] == "error")
            {
                return Err(
                    "no \"severity\":\"error\" diagnostic entry for the failed apply".into(),
                );
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
