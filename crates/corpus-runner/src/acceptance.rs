//! Objective evaluation of the produced binary: build it, run the archetype's
//! pre-written acceptance checks, and sweep the ROB01 invariant matrix.
//!
//! Everything here is black-box — the binary is spawned as a real process,
//! exactly as an adopter's user would run it — and nothing here consults the
//! agent's self-assessment. The produced code is untrusted: the build and
//! every binary invocation run env-cleared to the recorded allowlist and
//! under a hard deadline (via `exec`). Check failures, build failures, and
//! timeouts are findings recorded in the report, never runner errors.

use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

use standout_test::invariants::{
    assert_no_unresolved_tag_markers_in_page, assert_styling_preserves_layout_in_pages,
};

use crate::archetype::{
    Check, ColorState, InvariantCommand, InvariantContract, InvariantMode, Invariants,
};
use crate::cases::json_has_row;
use crate::exec;
use crate::report::{AcceptanceReport, CheckResult, InvariantCell, InvariantStatus};
use crate::workspace;

/// Builds the workspace app with cargo; returns the produced binary path.
///
/// The build runs env-cleared with disposable HOME/CARGO_HOME directories and
/// kernel filesystem isolation because the produced build script is
/// untrusted. An explicit `--target-dir` makes the binary location independent
/// of host cargo configuration, and `timeout` bounds the whole build.
pub fn build_app(
    app_dir: &Path,
    binary: &str,
    timeout: Duration,
    isolation: &workspace::Isolation,
) -> Result<PathBuf, String> {
    let target_dir = app_dir.join("target");
    let mut command = Command::new("cargo");
    command
        .arg("build")
        .arg("--target-dir")
        .arg(&target_dir)
        .current_dir(app_dir);
    isolation.apply_build(&mut command)?;
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
pub fn run_checks(
    binary: &Path,
    checks: &[Check],
    timeout: Duration,
    isolation: &workspace::Isolation,
) -> AcceptanceReport {
    let results = checks
        .iter()
        .map(|check| {
            let (passed, detail) = evaluate_check(binary, check, timeout, isolation);
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
        cases: Vec::new(),
    }
}

/// One check: spawn, compare exit code, stdout substrings, row-scoped
/// substrings, JSON shape, and JSON row groups.
fn evaluate_check(
    binary: &Path,
    check: &Check,
    timeout: Duration,
    isolation: &workspace::Isolation,
) -> (bool, Option<String>) {
    let home = isolation.check_home.join("smoke-checks");
    let (exit, stdout) = match run_binary(binary, &check.args, timeout, isolation, &home, &[]) {
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

const MATRIX_CHECKS: [&str; 5] = [
    "exits 0",
    "no unresolved tag markers",
    "stdout parses as JSON",
    "styling preserves text layout",
    "opaque output preserves text bytes",
];

/// Sweeps the declarative ROB01 matrix. Every global
/// command×mode×color×theme×check identity is emitted exactly once. Command
/// declarations decide applicability; an invocation failure leaves dependent
/// checks `not-run` rather than silently shrinking the denominator.
pub fn run_invariants(
    binary: &Path,
    invariants: &Invariants,
    timeout: Duration,
    isolation: &workspace::Isolation,
    matrix_root: &Path,
) -> Vec<InvariantCell> {
    let mut cells = Vec::new();
    for command in &invariants.commands {
        for color in &invariants.colors {
            for theme in &invariants.themes {
                let home = matrix_root.join(format!(
                    "{}-{}-{}",
                    safe_label(&command.argv),
                    color.as_str(),
                    safe_label(std::slice::from_ref(&theme.name))
                ));
                let mut runs = BTreeMap::new();
                for mode in &invariants.modes {
                    if command_applies(command, *mode, *color, &theme.name) {
                        runs.insert(
                            mode.as_str(),
                            run_mode(
                                binary,
                                command,
                                MatrixInvocation {
                                    mode: *mode,
                                    color: *color,
                                    theme_env: &theme.env,
                                    home: &home,
                                },
                                timeout,
                                isolation,
                            ),
                        );
                    }
                }
                for mode in &invariants.modes {
                    emit_axis_cells(&mut cells, command, *mode, *color, &theme.name, &runs);
                }
            }
        }
    }
    cells
}

/// Stable matrix identities for a build that never produced a binary.
pub fn not_run_invariants(invariants: &Invariants, reason: &str) -> Vec<InvariantCell> {
    let mut cells = Vec::new();
    for command in &invariants.commands {
        for color in &invariants.colors {
            for theme in &invariants.themes {
                for mode in &invariants.modes {
                    for check in MATRIX_CHECKS {
                        let applicable = command_applies(command, *mode, *color, &theme.name)
                            && check_applies(command.contract, *mode, check);
                        cells.push(matrix_cell(
                            command,
                            *mode,
                            *color,
                            &theme.name,
                            check,
                            if applicable {
                                InvariantStatus::NotRun
                            } else {
                                InvariantStatus::NotApplicable
                            },
                            Some(if applicable {
                                reason.to_string()
                            } else {
                                applicability_reason(command, *mode, *color, &theme.name, check)
                            }),
                        ));
                    }
                }
            }
        }
    }
    cells
}

type ModeRuns = BTreeMap<&'static str, Result<(Option<i32>, String), String>>;

fn emit_axis_cells(
    out: &mut Vec<InvariantCell>,
    command: &InvariantCommand,
    mode: InvariantMode,
    color: ColorState,
    theme: &str,
    runs: &ModeRuns,
) {
    let axes_apply = command_applies(command, mode, color, theme);
    for check in MATRIX_CHECKS {
        if !axes_apply || !check_applies(command.contract, mode, check) {
            out.push(matrix_cell(
                command,
                mode,
                color,
                theme,
                check,
                InvariantStatus::NotApplicable,
                Some(applicability_reason(command, mode, color, theme, check)),
            ));
            continue;
        }
        let Some(run) = runs.get(mode.as_str()) else {
            out.push(matrix_cell(
                command,
                mode,
                color,
                theme,
                check,
                InvariantStatus::NotRun,
                Some("planned invocation was not executed".to_string()),
            ));
            continue;
        };
        let outcome = match (check, run) {
            ("exits 0", Ok((Some(0), _))) => Ok(()),
            ("exits 0", Ok((status, _))) => Err(format!("exit {status:?}")),
            ("exits 0", Err(err)) => Err(err.clone()),
            (_, Err(err)) => {
                out.push(matrix_cell(
                    command,
                    mode,
                    color,
                    theme,
                    check,
                    InvariantStatus::NotRun,
                    Some(err.clone()),
                ));
                continue;
            }
            ("no unresolved tag markers", Ok((_, page))) => {
                let plain = console::strip_ansi_codes(page).into_owned();
                caught(|| assert_no_unresolved_tag_markers_in_page(&plain))
            }
            ("stdout parses as JSON", Ok((_, page))) => {
                serde_json::from_str::<serde_json::Value>(page)
                    .map(|_| ())
                    .map_err(|err| err.to_string())
            }
            ("styling preserves text layout", Ok((_, page))) => {
                match runs.get(InvariantMode::Text.as_str()) {
                    Some(Ok((_, text))) => {
                        let plain = console::strip_ansi_codes(page).into_owned();
                        caught(|| assert_styling_preserves_layout_in_pages(&plain, text))
                    }
                    Some(Err(err)) => Err(format!("text baseline unavailable: {err}")),
                    None => Err("text baseline was not planned".to_string()),
                }
            }
            ("opaque output preserves text bytes", Ok((_, page))) => {
                match runs.get(InvariantMode::Text.as_str()) {
                    Some(Ok((_, text))) if page == text => Ok(()),
                    Some(Ok(_)) => Err("bytes differ from text-mode baseline".to_string()),
                    Some(Err(err)) => Err(format!("text baseline unavailable: {err}")),
                    None => Err("text baseline was not planned".to_string()),
                }
            }
            _ => unreachable!("applicability and check table agree"),
        };
        out.push(matrix_cell(
            command,
            mode,
            color,
            theme,
            check,
            if outcome.is_ok() {
                InvariantStatus::Pass
            } else {
                InvariantStatus::Fail
            },
            outcome.err(),
        ));
    }
}

fn command_applies(
    command: &InvariantCommand,
    mode: InvariantMode,
    color: ColorState,
    theme: &str,
) -> bool {
    (command.modes.is_empty() || command.modes.contains(&mode))
        && (command.colors.is_empty() || command.colors.contains(&color))
        && (command.themes.is_empty() || command.themes.iter().any(|t| t == theme))
}

fn check_applies(contract: InvariantContract, mode: InvariantMode, check: &str) -> bool {
    match check {
        "exits 0" => true,
        "no unresolved tag markers" => {
            contract == InvariantContract::Rendered && mode != InvariantMode::Json
        }
        "stdout parses as JSON" => {
            contract == InvariantContract::Rendered && mode == InvariantMode::Json
        }
        "styling preserves text layout" => {
            contract == InvariantContract::Rendered && mode == InvariantMode::Term
        }
        "opaque output preserves text bytes" => {
            contract == InvariantContract::OpaqueBytes && mode != InvariantMode::Text
        }
        _ => false,
    }
}

fn applicability_reason(
    command: &InvariantCommand,
    mode: InvariantMode,
    color: ColorState,
    theme: &str,
    check: &str,
) -> String {
    if !command_applies(command, mode, color, theme) {
        "command declaration excludes this axis combination".to_string()
    } else {
        match (command.contract, check) {
            (InvariantContract::OpaqueBytes, "stdout parses as JSON") => {
                "opaque-byte command is not structured JSON".to_string()
            }
            (InvariantContract::OpaqueBytes, _) => {
                "opaque-byte command uses byte-identity checks".to_string()
            }
            (_, "opaque output preserves text bytes") => {
                "rendered command uses render invariants".to_string()
            }
            (_, _) => format!("check does not apply to {} mode", mode.as_str()),
        }
    }
}

/// Runs the binary with `command` plus `--output <mode>` appended (the same
/// argv edit `standout-test`'s harness makes), returning exit code + stdout.
struct MatrixInvocation<'a> {
    mode: InvariantMode,
    color: ColorState,
    theme_env: &'a BTreeMap<String, String>,
    home: &'a Path,
}

fn run_mode(
    binary: &Path,
    command: &InvariantCommand,
    invocation: MatrixInvocation<'_>,
    timeout: Duration,
    isolation: &workspace::Isolation,
) -> Result<(Option<i32>, String), String> {
    let mut args: Vec<String> = command.argv.clone();
    args.push("--output".to_string());
    args.push(invocation.mode.as_str().to_string());
    let mut env: Vec<(String, String)> = invocation
        .theme_env
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    match invocation.color {
        ColorState::Off => {
            env.extend([
                ("NO_COLOR".to_string(), "1".to_string()),
                ("CLICOLOR_FORCE".to_string(), "0".to_string()),
                ("FORCE_COLOR".to_string(), "0".to_string()),
            ]);
        }
        ColorState::On => {
            env.extend([
                ("TERM".to_string(), "xterm-256color".to_string()),
                ("CLICOLOR_FORCE".to_string(), "1".to_string()),
                ("FORCE_COLOR".to_string(), "1".to_string()),
            ]);
        }
    }
    run_binary(binary, &args, timeout, isolation, invocation.home, &env)
}

/// One deadlined, env-scrubbed invocation of the produced (untrusted)
/// binary, returning exit code + captured stdout.
fn run_binary(
    binary: &Path,
    args: &[String],
    timeout: Duration,
    isolation: &workspace::Isolation,
    home: &Path,
    env: &[(String, String)],
) -> Result<(Option<i32>, String), String> {
    let mut command = Command::new(binary);
    command.args(args).current_dir(home);
    isolation.apply_check(&mut command, home)?;
    for (key, value) in env {
        command.env(key, value);
    }
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

fn matrix_cell(
    command: &InvariantCommand,
    mode: InvariantMode,
    color: ColorState,
    theme: &str,
    check: &str,
    status: InvariantStatus,
    detail: Option<String>,
) -> InvariantCell {
    InvariantCell {
        command: command.argv.join(" "),
        mode: mode.as_str().to_string(),
        color: color.as_str().to_string(),
        theme: theme.to_string(),
        check: check.to_string(),
        status,
        detail,
    }
}

fn safe_label(words: &[String]) -> String {
    let label = if words.is_empty() {
        "root".to_string()
    } else {
        words.join("-")
    };
    label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}
