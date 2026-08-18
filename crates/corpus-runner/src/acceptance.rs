//! Objective evaluation of the produced binary: build it, run the archetype's
//! pre-written acceptance checks, and sweep the ROB01 invariant matrix.
//!
//! Everything here is black-box — the binary is spawned as a real process,
//! exactly as an adopter's user would run it — and nothing here consults the
//! agent's self-assessment. The produced code is untrusted: the build and
//! every binary invocation run env-cleared to the recorded allowlist and
//! under a hard deadline (via `exec`). Check failures, build failures, and
//! timeouts are findings recorded in the report, never runner errors.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

use standout_test::invariants::{
    assert_no_unresolved_tag_markers_in_page, assert_styling_preserves_layout_in_pages,
};

use crate::archetype::{Check, Invariants};
use crate::exec;
use crate::report::{AcceptanceReport, CheckResult, InvariantCell};
use crate::workspace;

/// Builds the workspace app with cargo; returns the produced binary path.
///
/// The build runs env-cleared to the recorded allowlist (the produced code
/// is untrusted — its build script must not see the runner's secrets; the
/// allowlist keeps CARGO_HOME/HOME so caches stay shared), with an explicit
/// `--target-dir` so the binary lands where the runner looks even when the
/// host configures a shared target directory, and under `timeout`.
pub fn build_app(app_dir: &Path, binary: &str, timeout: Duration) -> Result<PathBuf, String> {
    let target_dir = app_dir.join("target");
    let mut command = Command::new("cargo");
    command
        .arg("build")
        .arg("--target-dir")
        .arg(&target_dir)
        .current_dir(app_dir);
    workspace::apply_env_policy(&mut command);
    let outcome =
        exec::run(&mut command, timeout, true).map_err(|err| format!("cargo build: {err}"))?;
    if outcome.timed_out {
        return Err(format!(
            "cargo build timed out after {}s",
            timeout.as_secs()
        ));
    }
    if outcome.exit_code != Some(0) {
        let tail: Vec<&str> = outcome.stderr.lines().rev().take(30).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        return Err(format!("cargo build failed:\n{}", tail.join("\n")));
    }
    let path = target_dir
        .join("debug")
        .join(format!("{binary}{}", std::env::consts::EXE_SUFFIX));
    if !path.exists() {
        return Err(format!(
            "build succeeded but expected binary {} does not exist",
            path.display()
        ));
    }
    Ok(path)
}

/// Runs every acceptance check against the binary, each under `timeout`.
pub fn run_checks(binary: &Path, checks: &[Check], timeout: Duration) -> AcceptanceReport {
    let results = checks
        .iter()
        .map(|check| {
            let (passed, detail) = evaluate_check(binary, check, timeout);
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

/// One check: spawn, compare exit code, stdout substrings, row-scoped
/// substrings, JSON shape, and JSON row groups.
fn evaluate_check(binary: &Path, check: &Check, timeout: Duration) -> (bool, Option<String>) {
    let (exit, stdout) = match run_binary(binary, &check.args, timeout) {
        Ok(pair) => pair,
        Err(detail) => return (false, Some(detail)),
    };
    let mut failures = Vec::new();

    if exit != Some(check.expect_exit) {
        failures.push(format!("expected exit {}, got {exit:?}", check.expect_exit));
    }
    for needle in &check.stdout_contains {
        if !stdout.contains(needle) {
            failures.push(format!("stdout does not contain {needle:?}"));
        }
    }
    for row in &check.stdout_row_contains {
        if !stdout
            .lines()
            .any(|line| row.iter().all(|cell| line.contains(cell.as_str())))
        {
            failures.push(format!("no single stdout line contains all of {row:?}"));
        }
    }
    if check.stdout_is_json || !check.stdout_json_rows.is_empty() {
        match serde_json::from_str::<serde_json::Value>(&stdout) {
            Ok(value) => {
                for row in &check.stdout_json_rows {
                    if !json_has_row(&value, row) {
                        failures.push(format!("no single JSON element carries all of {row:?}"));
                    }
                }
            }
            Err(err) => failures.push(format!("stdout is not valid JSON: {err}")),
        }
    }

    if failures.is_empty() {
        (true, None)
    } else {
        failures.push(format!("--- stdout ---\n{stdout}"));
        (false, Some(failures.join("\n")))
    }
}

/// True when some single array element anywhere in `value` (a "row") carries
/// every value in `row` among its scalars — the association check that keeps
/// e.g. a star bound to its own constellation and magnitude, which flat
/// substring matching cannot express.
fn json_has_row(value: &serde_json::Value, row: &[String]) -> bool {
    let mut candidates = Vec::new();
    collect_array_elements(value, &mut candidates);
    candidates.iter().any(|element| {
        let mut scalars = Vec::new();
        collect_scalars(element, &mut scalars);
        row.iter().all(|cell| scalars.iter().any(|s| s == cell))
    })
}

/// Collects every element of every array in `value`, at any depth.
fn collect_array_elements<'a>(value: &'a serde_json::Value, out: &mut Vec<&'a serde_json::Value>) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                out.push(item);
                collect_array_elements(item, out);
            }
        }
        serde_json::Value::Object(map) => {
            for item in map.values() {
                collect_array_elements(item, out);
            }
        }
        _ => {}
    }
}

/// Collects every scalar under `value` as its canonical string form
/// (numbers via their shortest decimal representation, so `0.86` matches
/// the literal "0.86" whether the producer emitted a number or a string).
fn collect_scalars(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(s) => out.push(s.clone()),
        serde_json::Value::Number(n) => out.push(n.to_string()),
        serde_json::Value::Bool(b) => out.push(b.to_string()),
        serde_json::Value::Null => {}
        serde_json::Value::Array(items) => {
            for item in items {
                collect_scalars(item, out);
            }
        }
        serde_json::Value::Object(map) => {
            for item in map.values() {
                collect_scalars(item, out);
            }
        }
    }
}

/// Sweeps the ROB01 invariant matrix: each configured command runs across
/// output modes (`text`, `term`, `json`), asserting exit 0 everywhere, no
/// unresolved `[tag?]` marker in either rendered page, term stripped of
/// escapes byte-equal to text, and json well-formed. Each invocation runs
/// under `timeout`.
pub fn run_invariants(
    binary: &Path,
    invariants: &Invariants,
    timeout: Duration,
) -> Vec<InvariantCell> {
    let mut cells = Vec::new();
    for command in &invariants.commands {
        let label = command.join(" ");
        let text = run_mode(binary, command, "text", timeout);
        let term = run_mode(binary, command, "term", timeout);
        let json = run_mode(binary, command, "json", timeout);

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
    timeout: Duration,
) -> Result<(Option<i32>, String), String> {
    let mut args: Vec<String> = command.to_vec();
    args.push("--output".to_string());
    args.push(mode.to_string());
    run_binary(binary, &args, timeout)
}

/// One deadlined, env-scrubbed invocation of the produced (untrusted)
/// binary, returning exit code + captured stdout.
fn run_binary(
    binary: &Path,
    args: &[String],
    timeout: Duration,
) -> Result<(Option<i32>, String), String> {
    let mut command = Command::new(binary);
    command.args(args);
    workspace::apply_env_policy(&mut command);
    let outcome = exec::run(&mut command, timeout, true)
        .map_err(|err| format!("running {}: {err}", binary.display()))?;
    if outcome.timed_out {
        return Err(format!("timed out after {}s", timeout.as_secs()));
    }
    Ok((outcome.exit_code, outcome.stdout))
}

/// Serializes swaps of the global panic hook in `caught`: the hook is
/// process-wide, so concurrent swaps (parallel test threads) could restore
/// a stale hook or mute an unrelated thread's panic report.
static PANIC_HOOK_LOCK: Mutex<()> = Mutex::new(());

/// Runs a panicking `standout-test` invariant assertion as a pass/fail
/// outcome, keeping the panic message as the failure detail and the runner's
/// stderr free of hook noise.
fn caught(assertion: impl FnOnce()) -> Result<(), String> {
    let _guard = PANIC_HOOK_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
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
