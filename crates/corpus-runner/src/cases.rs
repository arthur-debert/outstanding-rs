//! Roster-case execution: the run semantics `corpus/README.md` documents,
//! made real against a produced binary.
//!
//! Each case runs black-box in its own sandbox directory with the scrubbed
//! baseline environment (`PATH`, `HOME` pointing into the sandbox,
//! `LANG`/`LC_ALL` = `C.UTF-8` — everything else unset unless the case sets
//! it), sandbox files materialized first, streams attached to a pty exactly
//! where the case's `tty` list says (the ROB01 harness's pty seam,
//! `standout_test::pty`), scripted stdin delivered as piped content or pty
//! keystrokes, and the case's own `timeout_seconds` as a hard deadline that
//! is itself an assertion. Assertion evaluation covers the documented
//! vocabulary; `expected = "fail"` cases report as *expected-fail* (and an
//! unexpected pass as news), never as suite errors.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::archetype::{Case, CaseExpect, Expected, TtyStream};
use crate::exec;
use crate::report::{AcceptanceReport, CaseOutcome, CaseResult};
use crate::workspace;

/// Runs every case against the binary, each in a fresh sandbox under
/// `cases_dir` (kept, not temp: a run's sandboxes are part of its
/// inspectable residue). Returns the roster-shaped acceptance report.
pub fn run_cases(
    binary: &Path,
    cases: &[Case],
    cases_dir: &Path,
    isolation: &workspace::Isolation,
) -> AcceptanceReport {
    let results = cases
        .iter()
        .map(|case| {
            let sandbox = cases_dir.join(&case.name);
            let (outcome, detail) = match execute(binary, case, &sandbox, isolation) {
                Ok(execution) => {
                    let (raw_pass, detail) = evaluate(case, &execution);
                    let outcome = match (case.expected, raw_pass) {
                        (Expected::Pass, true) => CaseOutcome::Pass,
                        (Expected::Pass, false) => CaseOutcome::Fail,
                        (Expected::Fail, false) => CaseOutcome::ExpectedFail,
                        (Expected::Fail, true) => CaseOutcome::UnexpectedPass,
                    };
                    (outcome, detail)
                }
                Err(err) => (
                    CaseOutcome::Fail,
                    Some(format!("case execution error: {err}")),
                ),
            };
            CaseResult {
                name: case.name.clone(),
                group: case.group.clone(),
                stresses: case.stresses.clone(),
                expected: match case.expected {
                    Expected::Pass => "pass".to_string(),
                    Expected::Fail => "fail".to_string(),
                },
                outcome,
                gap: case.gap.clone(),
                reason: case.reason.clone(),
                detail,
            }
        })
        .collect();
    AcceptanceReport {
        built: true,
        build_detail: None,
        cases: results,
    }
}

/// What one case's process run produced, LF-normalized.
struct Execution {
    exit_code: Option<i32>,
    timed_out: bool,
    stdout: String,
    stderr: String,
}

/// Provisions the sandbox and runs the binary once under the case's own
/// deadline. Errors here are sandbox/spawn failures — suite errors, not
/// case verdicts.
fn execute(
    binary: &Path,
    case: &Case,
    sandbox: &Path,
    isolation: &workspace::Isolation,
) -> Result<Execution, String> {
    std::fs::create_dir_all(sandbox).map_err(|err| format!("creating sandbox: {err}"))?;
    for (rel, content) in &case.run.files {
        let dest = sandbox_path(sandbox, rel)?;
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("creating sandbox dirs for {rel}: {err}"))?;
        }
        std::fs::write(&dest, content)
            .map_err(|err| format!("writing sandbox file {rel}: {err}"))?;
    }
    let cwd = match &case.run.cwd {
        Some(rel) => sandbox_path(sandbox, rel)?,
        None => sandbox.to_path_buf(),
    };
    std::fs::create_dir_all(&cwd).map_err(|err| format!("creating case cwd: {err}"))?;

    let tty_stdin = case.run.tty.contains(&TtyStream::Stdin);
    let tty_stdout = case.run.tty.contains(&TtyStream::Stdout);
    let tty_stderr = case.run.tty.contains(&TtyStream::Stderr);
    let use_pty = tty_stdin || tty_stdout || tty_stderr;

    let mut command = Command::new(binary);
    command.args(&case.run.argv).current_dir(&cwd);
    isolation.apply_check(&mut command, sandbox)?;
    for (key, value) in &case.run.env {
        command.env(key, value);
    }
    exec::place_in_own_group(&mut command);

    let mut master = None;
    if use_pty {
        #[cfg(not(unix))]
        return Err("pty-backed case streams are unsupported on this platform".to_string());
        #[cfg(unix)]
        {
            let (m, slave) = standout_test::pty::open_pair()
                .map_err(|err| format!("opening pty for case streams: {err}"))?;
            let dup = |name: &str| -> Result<Stdio, String> {
                Ok(Stdio::from(slave.try_clone().map_err(|err| {
                    format!("duplicating pty slave for {name}: {err}")
                })?))
            };
            command.stdin(if tty_stdin {
                dup("stdin")?
            } else {
                Stdio::piped()
            });
            command.stdout(if tty_stdout {
                dup("stdout")?
            } else {
                Stdio::piped()
            });
            command.stderr(if tty_stderr {
                dup("stderr")?
            } else {
                Stdio::piped()
            });
            master = Some(m);
            // `slave` drops here: the child holds the only remaining slave fds
            // (inside `command`'s Stdio handles, released by `drop(command)`
            // below), so the master read ends when the child exits.
        }
    } else {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
    }

    let mut child = command
        .spawn()
        .map_err(|err| format!("spawning {}: {err}", binary.display()))?;
    // A `Command` is re-spawnable, so it retains the slave `Stdio` handles it
    // was given even after `spawn`; a retained slave fd would keep the
    // master read from ever seeing EOF.
    drop(command);

    // Pipe captures.
    let stdout_capture = child.stdout.take().map(exec::capped_reader);
    let stderr_capture = child.stderr.take().map(exec::capped_reader);

    // The master capture serves every pty-attached output stream; with both
    // on the pty they interleave exactly as they would on a user's terminal,
    // and the capture is attributed to stdout.
    let master_capture = match (&master, tty_stdout || tty_stderr) {
        (Some(m), true) => Some(exec::capped_reader(std::fs::File::from(
            m.try_clone()
                .map_err(|err| format!("duplicating pty master for capture: {err}"))?,
        ))),
        _ => None,
    };

    // Scripted stdin. On a pipe: the content followed by EOF. On the pty:
    // keystrokes followed by the terminal's EOF character. Closing only the
    // writer handle is insufficient when a capture retains another master.
    let stdin_bytes = case.run.stdin.clone();
    if tty_stdin {
        if let Some(text) = stdin_bytes {
            let handle = master.take().expect("tty stdin implies an open master");
            std::thread::spawn(move || {
                let mut writer = std::fs::File::from(handle);
                let _ = writer.write_all(text.as_bytes());
                // In canonical mode one Ctrl-D flushes an unterminated final
                // line and the next delivers EOF; when the input already
                // ends in a newline, the first one delivers EOF and the
                // second write is harmless (and may see EIO).
                let _ = writer.write_all(&[0x04, 0x04]);
            });
        }
        // With no stdin string the master stays held below: an attended
        // terminal that never sends anything (and never hangs up early).
    } else {
        let mut stdin_pipe = child.stdin.take().expect("stdin was piped");
        if let Some(text) = stdin_bytes {
            std::thread::spawn(move || {
                let _ = stdin_pipe.write_all(text.as_bytes());
            });
        }
        // With no scripted content, dropping the pipe writer delivers the
        // documented adversarial default: a pipe already at EOF.
    }

    let captures: Vec<&exec::Capture> = [
        stdout_capture.as_ref(),
        stderr_capture.as_ref(),
        master_capture.as_ref(),
    ]
    .into_iter()
    .flatten()
    .collect();
    let supervised = exec::supervise(
        &mut child,
        Duration::from_secs(case.run.timeout_seconds),
        &captures,
    )?;
    drop(master); // release an attended-but-silent terminal, if any

    let master_text = master_capture.map(exec::Capture::text);
    let stdout = if tty_stdout {
        master_text.clone().unwrap_or_default()
    } else {
        stdout_capture.map(exec::Capture::text).unwrap_or_default()
    };
    let stderr = if tty_stderr && !tty_stdout {
        master_text.unwrap_or_default()
    } else if tty_stderr {
        String::new()
    } else {
        stderr_capture.map(exec::Capture::text).unwrap_or_default()
    };

    Ok(Execution {
        exit_code: supervised.exit_code,
        timed_out: supervised.timed_out,
        stdout: normalize_lf(&stdout),
        stderr: normalize_lf(&stderr),
    })
}

/// True when some single array element anywhere in `value` (a "row") carries
/// every value in `row` among its scalars — the association check that keeps
/// e.g. a star bound to its own constellation and magnitude, which flat
/// substring matching cannot express.
pub(crate) fn json_has_row(value: &serde_json::Value, row: &[String]) -> bool {
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

/// Resolves a case-relative path inside the sandbox, refusing absolute
/// paths and parent traversal — a case must not reach outside its sandbox.
fn sandbox_path(sandbox: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute()
        || rel_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!("path {rel:?} escapes the case sandbox"));
    }
    Ok(sandbox.join(rel_path))
}

/// Pty captures are normalized to LF before comparison; pipes pass through
/// unchanged in practice (no CR to strip).
fn normalize_lf(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// Applies the case's assertion vocabulary to what the run produced,
/// returning the raw verdict (before expected-fail mapping) and failure
/// detail carrying the observed streams.
fn evaluate(case: &Case, execution: &Execution) -> (bool, Option<String>) {
    let mut failures = Vec::new();
    if execution.timed_out {
        failures.push(format!(
            "timed out: exceeded timeout_seconds = {}",
            case.run.timeout_seconds
        ));
    } else {
        apply_expectations(&case.expect, execution, &mut failures);
    }
    if failures.is_empty() {
        (true, None)
    } else {
        failures.push(format!(
            "--- stdout ---\n{}\n--- stderr ---\n{}",
            execution.stdout, execution.stderr
        ));
        (false, Some(failures.join("\n")))
    }
}

fn apply_expectations(expect: &CaseExpect, execution: &Execution, failures: &mut Vec<String>) {
    if let Some(code) = expect.exit_code {
        if execution.exit_code != Some(code) {
            failures.push(format!(
                "expected exit {code}, got {:?}",
                execution.exit_code
            ));
        }
    }
    if let Some(want) = &expect.stdout {
        if &execution.stdout != want {
            failures.push(format!("stdout differs from expected {want:?}"));
        }
    }
    if let Some(want) = &expect.stderr {
        if &execution.stderr != want {
            failures.push(format!("stderr differs from expected {want:?}"));
        }
    }
    if let Some(want) = &expect.stdout_json {
        match (
            serde_json::from_str::<serde_json::Value>(&execution.stdout),
            serde_json::from_str::<serde_json::Value>(want),
        ) {
            (_, Err(err)) => failures.push(format!(
                "suite defect: expected stdout_json is not valid JSON: {err}"
            )),
            (Err(err), _) => failures.push(format!("stdout is not valid JSON: {err}")),
            (Ok(got), Ok(want)) => {
                if got != want {
                    failures.push(format!(
                        "stdout JSON is not semantically equal to expected {want}"
                    ));
                }
            }
        }
    }
    for needle in &expect.stdout_contains {
        if !execution.stdout.contains(needle) {
            failures.push(format!("stdout does not contain {needle:?}"));
        }
    }
    for needle in &expect.stderr_contains {
        if !execution.stderr.contains(needle) {
            failures.push(format!("stderr does not contain {needle:?}"));
        }
    }
    for row in &expect.stdout_row_contains {
        if !execution
            .stdout
            .lines()
            .any(|line| row.iter().all(|cell| line.contains(cell.as_str())))
        {
            failures.push(format!("no single stdout line contains all of {row:?}"));
        }
    }
    if !expect.stdout_json_rows.is_empty() {
        match serde_json::from_str::<serde_json::Value>(&execution.stdout) {
            Ok(value) => {
                for row in &expect.stdout_json_rows {
                    if !json_has_row(&value, row) {
                        failures.push(format!("no single JSON element carries all of {row:?}"));
                    }
                }
            }
            Err(err) => failures.push(format!("stdout is not valid JSON: {err}")),
        }
    }
    for needle in &expect.stdout_not_contains {
        if execution.stdout.contains(needle) {
            failures.push(format!("stdout must not contain {needle:?}"));
        }
    }
    for needle in &expect.stderr_not_contains {
        if execution.stderr.contains(needle) {
            failures.push(format!("stderr must not contain {needle:?}"));
        }
    }
    for suffix in &expect.stdout_lines_end_with_once {
        let count = execution
            .stdout
            .lines()
            .filter(|line| !line.trim().is_empty() && line.trim_end().ends_with(suffix))
            .count();
        if count != 1 {
            failures.push(format!(
                "expected exactly one non-empty stdout line ending with {suffix:?}, got {count}"
            ));
        }
    }
}
