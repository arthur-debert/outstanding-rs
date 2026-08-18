//! Objective evaluation of the produced binary: build it, run the archetype's
//! pre-written acceptance checks, and sweep the ROB01 invariant matrix.
//!
//! Everything here is black-box — the binary is spawned as a real process,
//! exactly as an adopter's user would run it — and nothing here consults the
//! agent's self-assessment. Check failures are findings recorded in the
//! report, never runner errors.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;
use standout_test::invariants::{
    assert_no_unresolved_tag_markers_in_page, assert_styling_preserves_layout_in_pages,
};

use crate::archetype::{Check, Invariants};
use crate::report::{AcceptanceReport, CheckResult, InvariantCell};

/// Builds the workspace app with cargo; returns the produced binary path.
///
/// The build runs with the runner's own environment (this is the runner's
/// side of the fence, not the agent's) so cargo caches are shared.
pub fn build_app(app_dir: &Path, binary: &str) -> Result<PathBuf, String> {
    let output = Command::new("cargo")
        .arg("build")
        .current_dir(app_dir)
        .output()
        .map_err(|err| format!("spawning cargo build: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = stderr.lines().rev().take(30).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        return Err(format!("cargo build failed:\n{}", tail.join("\n")));
    }
    let path = app_dir.join("target").join("debug").join(binary);
    if !path.exists() {
        return Err(format!(
            "build succeeded but expected binary {} does not exist",
            path.display()
        ));
    }
    Ok(path)
}

/// Runs every acceptance check against the binary.
pub fn run_checks(binary: &Path, checks: &[Check]) -> AcceptanceReport {
    let results = checks
        .iter()
        .map(|check| {
            let (passed, detail) = evaluate_check(binary, check);
            CheckResult {
                name: check.name.clone(),
                passed,
                detail,
            }
        })
        .collect();
    AcceptanceReport {
        built: true,
        build_detail: None,
        checks: results,
    }
}

/// One check: spawn, compare exit code, stdout substrings, JSON shape.
fn evaluate_check(binary: &Path, check: &Check) -> (bool, Option<String>) {
    let output = match Command::new(binary).args(&check.args).output() {
        Ok(output) => output,
        Err(err) => return (false, Some(format!("failed to run binary: {err}"))),
    };
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let mut failures = Vec::new();

    let exit = output.status.code();
    if exit != Some(check.expect_exit) {
        failures.push(format!("expected exit {}, got {exit:?}", check.expect_exit));
    }
    for needle in &check.stdout_contains {
        if !stdout.contains(needle) {
            failures.push(format!("stdout does not contain {needle:?}"));
        }
    }
    if check.stdout_is_json {
        if let Err(err) = serde_json::from_str::<serde_json::Value>(&stdout) {
            failures.push(format!("stdout is not valid JSON: {err}"));
        }
    }

    if failures.is_empty() {
        (true, None)
    } else {
        failures.push(format!("--- stdout ---\n{stdout}"));
        (false, Some(failures.join("\n")))
    }
}

/// Sweeps the ROB01 invariant matrix: each configured command runs across
/// output modes (`text`, `term`, `json`), asserting exit 0 everywhere, no
/// unresolved `[tag?]` marker in either rendered page, term stripped of
/// escapes byte-equal to text, and json well-formed.
pub fn run_invariants(binary: &Path, invariants: &Invariants) -> Vec<InvariantCell> {
    let mut cells = Vec::new();
    for command in &invariants.commands {
        let label = command.join(" ");
        let text = run_mode(binary, command, "text");
        let term = run_mode(binary, command, "term");
        let json = run_mode(binary, command, "json");

        for (mode, run) in [("text", &text), ("term", &term), ("json", &json)] {
            cells.push(cell(
                &label,
                format!("{mode}: exits 0"),
                match run {
                    Ok((status, _)) if *status == Some(0) => Ok(()),
                    Ok((status, _)) => Err(format!("exit {status:?}")),
                    Err(err) => Err(err.clone()),
                },
            ));
        }

        if let Ok((_, page)) = &text {
            cells.push(cell(
                &label,
                "text: no unresolved tag markers".to_string(),
                caught(|| assert_no_unresolved_tag_markers_in_page(page)),
            ));
        }
        if let Ok((_, page)) = &term {
            let stripped = console::strip_ansi_codes(page).into_owned();
            cells.push(cell(
                &label,
                "term: no unresolved tag markers".to_string(),
                caught(|| assert_no_unresolved_tag_markers_in_page(&stripped)),
            ));
            if let Ok((_, text_page)) = &text {
                cells.push(cell(
                    &label,
                    "term vs text: styling preserves layout".to_string(),
                    caught(|| assert_styling_preserves_layout_in_pages(&stripped, text_page)),
                ));
            }
        }
        if let Ok((_, page)) = &json {
            cells.push(cell(
                &label,
                "json: stdout parses as JSON".to_string(),
                serde_json::from_str::<serde_json::Value>(page)
                    .map(|_| ())
                    .map_err(|err| err.to_string()),
            ));
        }
    }
    cells
}

/// Runs the binary with `command` plus `--output <mode>` appended (the same
/// argv edit `standout-test`'s harness makes), returning exit code + stdout.
fn run_mode(
    binary: &Path,
    command: &[String],
    mode: &str,
) -> Result<(Option<i32>, String), String> {
    let output = Command::new(binary)
        .args(command)
        .args(["--output", mode])
        .output()
        .with_context(|| format!("running {} {mode}", binary.display()))
        .map_err(|err| err.to_string())?;
    Ok((
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    ))
}

/// Runs a panicking `standout-test` invariant assertion as a pass/fail
/// outcome, keeping the panic message as the failure detail and the runner's
/// stderr free of hook noise.
fn caught(assertion: impl FnOnce()) -> Result<(), String> {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = catch_unwind(AssertUnwindSafe(assertion));
    std::panic::set_hook(previous);
    outcome.map_err(|payload| {
        if let Some(message) = payload.downcast_ref::<String>() {
            message.clone()
        } else if let Some(message) = payload.downcast_ref::<&str>() {
            (*message).to_string()
        } else {
            "invariant assertion panicked".to_string()
        }
    })
}

fn cell(command: &str, check: String, outcome: Result<(), String>) -> InvariantCell {
    InvariantCell {
        command: command.to_string(),
        check,
        passed: outcome.is_ok(),
        detail: outcome.err(),
    }
}
