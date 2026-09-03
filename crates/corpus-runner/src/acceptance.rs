//! Objective evaluation of the produced binary: build it and sweep the
//! invariant matrix. Everything here is black-box and treats the produced
//! code as untrusted.

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
    ColorState, InvariantCommand, InvariantContract, InvariantMode, InvariantTheme, Invariants,
};
use crate::exec;
use crate::report::{InvariantCell, InvariantStatus};
use crate::workspace;

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

const MATRIX_CHECKS: [&str; 5] = [
    "exits 0",
    "no unresolved tag markers",
    "stdout parses as JSON",
    "styling preserves text layout",
    "opaque output preserves text bytes",
];

const NO_OUTPUT_FLAG_REASON: &str = "no output flag";

pub fn run_invariants(
    binary: &Path,
    invariants: &Invariants,
    timeout: Duration,
    isolation: &workspace::Isolation,
    matrix_root: &Path,
) -> Vec<InvariantCell> {
    if !accepts_output_flag(binary, timeout, isolation, matrix_root) {
        return sweep_plan(
            invariants,
            |_, _, _| ModeRuns::new(),
            NO_OUTPUT_FLAG_REASON,
            Some(NO_OUTPUT_FLAG_REASON),
        );
    }
    sweep_plan(
        invariants,
        |command, color, theme| {
            let home = matrix_root.join(format!(
                "{}-{}-{}",
                safe_label(&command.argv),
                color.as_str(),
                safe_label(std::slice::from_ref(&theme.name))
            ));
            invariants
                .modes
                .iter()
                .map(|mode| {
                    (
                        mode.as_str(),
                        run_mode(
                            binary,
                            command,
                            MatrixInvocation {
                                mode: *mode,
                                color,
                                theme_env: &theme.env,
                                home: &home,
                            },
                            timeout,
                            isolation,
                        ),
                    )
                })
                .collect()
        },
        "planned invocation was not executed",
        None,
    )
}

pub fn not_run_invariants(invariants: &Invariants, reason: &str) -> Vec<InvariantCell> {
    sweep_plan(invariants, |_, _, _| ModeRuns::new(), reason, None)
}

fn accepts_output_flag(
    binary: &Path,
    timeout: Duration,
    isolation: &workspace::Isolation,
    matrix_root: &Path,
) -> bool {
    let home = matrix_root.join("help-probe");
    match run_binary(
        binary,
        &["--help".to_string()],
        timeout,
        isolation,
        &home,
        &[],
    ) {
        Ok((Some(0), stdout, stderr)) => {
            mentions_output_flag(&stdout) || mentions_output_flag(&stderr)
        }
        Ok(_) => true,
        Err(_) => true,
    }
}

fn mentions_output_flag(page: &str) -> bool {
    const FLAG: &str = "--output";
    let is_word_char = |c: char| c.is_alphanumeric() || c == '-' || c == '_';
    for (pos, _) in page.match_indices(FLAG) {
        let before_ok = page[..pos]
            .chars()
            .next_back()
            .map(|c| !is_word_char(c))
            .unwrap_or(true);
        let after_ok = page[pos + FLAG.len()..]
            .chars()
            .next()
            .map(|c| !is_word_char(c))
            .unwrap_or(true);
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

type ModeRuns = BTreeMap<&'static str, Result<(Option<i32>, String), String>>;

fn sweep_plan(
    invariants: &Invariants,
    mut mode_runs: impl FnMut(&InvariantCommand, ColorState, &InvariantTheme) -> ModeRuns,
    not_run_reason: &str,
    force_not_applicable: Option<&str>,
) -> Vec<InvariantCell> {
    let mut cells = Vec::new();
    for command in &invariants.commands {
        let mut resolved_either: Option<InvariantContract> = None;
        for color in &invariants.colors {
            for theme in &invariants.themes {
                let runs = mode_runs(command, *color, theme);
                if command.contract == InvariantContract::Either && resolved_either.is_none() {
                    resolved_either = resolve_either_contract(&runs);
                }
                let contract = match command.contract {
                    InvariantContract::Either => {
                        resolved_either.unwrap_or(InvariantContract::Rendered)
                    }
                    concrete => concrete,
                };
                for mode in &invariants.modes {
                    emit_axis_cells(
                        &mut cells,
                        command,
                        contract,
                        *mode,
                        *color,
                        &theme.name,
                        &runs,
                        not_run_reason,
                        force_not_applicable,
                    );
                }
            }
        }
    }
    cells
}

fn resolve_either_contract(runs: &ModeRuns) -> Option<InvariantContract> {
    if let Some(Ok((Some(0), page))) = runs.get(InvariantMode::Json.as_str()) {
        if serde_json::from_str::<serde_json::Value>(page).is_ok() {
            return Some(InvariantContract::Rendered);
        }
    }
    if let Some(Ok((Some(0), text))) = runs.get(InvariantMode::Text.as_str()) {
        for mode in [InvariantMode::Term, InvariantMode::Json] {
            if let Some(Ok((Some(0), page))) = runs.get(mode.as_str()) {
                if page == text {
                    return Some(InvariantContract::OpaqueBytes);
                }
            }
        }
    }
    if runs.values().all(|run| matches!(run, Ok((Some(0), _)))) {
        Some(InvariantContract::Rendered)
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_axis_cells(
    out: &mut Vec<InvariantCell>,
    command: &InvariantCommand,
    contract: InvariantContract,
    mode: InvariantMode,
    color: ColorState,
    theme: &str,
    runs: &ModeRuns,
    not_run_reason: &str,
    force_not_applicable: Option<&str>,
) {
    for check in MATRIX_CHECKS {
        if let Some(reason) = force_not_applicable {
            out.push(matrix_cell(
                command,
                mode,
                color,
                theme,
                check,
                InvariantStatus::NotApplicable,
                Some(reason.to_string()),
            ));
            continue;
        }
        if !check_applies(contract, mode, check, command.equal_across_modes) {
            out.push(matrix_cell(
                command,
                mode,
                color,
                theme,
                check,
                InvariantStatus::NotApplicable,
                Some(applicability_reason(
                    contract,
                    mode,
                    check,
                    command.equal_across_modes,
                )),
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
                Some(not_run_reason.to_string()),
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

fn check_applies(
    contract: InvariantContract,
    mode: InvariantMode,
    check: &str,
    equal_across_modes: bool,
) -> bool {
    match check {
        "exits 0" => true,
        "no unresolved tag markers" => {
            contract == InvariantContract::Rendered && mode != InvariantMode::Json
        }
        "stdout parses as JSON" => {
            contract == InvariantContract::Rendered && mode == InvariantMode::Json
        }
        "styling preserves text layout" => {
            contract == InvariantContract::Rendered
                && mode == InvariantMode::Term
                && equal_across_modes
        }
        "opaque output preserves text bytes" => {
            contract == InvariantContract::OpaqueBytes
                && mode != InvariantMode::Text
                && equal_across_modes
        }
        _ => false,
    }
}

fn applicability_reason(
    contract: InvariantContract,
    mode: InvariantMode,
    check: &str,
    equal_across_modes: bool,
) -> String {
    if !equal_across_modes && check_applies(contract, mode, check, true) {
        return "command's content varies by output mode".to_string();
    }
    match (contract, check) {
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
        .map(|(exit_code, stdout, _stderr)| (exit_code, stdout))
}

fn run_binary(
    binary: &Path,
    args: &[String],
    timeout: Duration,
    isolation: &workspace::Isolation,
    home: &Path,
    env: &[(String, String)],
) -> Result<(Option<i32>, String, String), String> {
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
    Ok((outcome.exit_code, outcome.stdout, outcome.stderr))
}

// The panic hook is process-wide; concurrent swaps would restore a stale one.
static PANIC_HOOK_LOCK: Mutex<()> = Mutex::new(());

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
