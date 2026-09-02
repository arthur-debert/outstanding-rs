// Roster-case execution: the run semantics `corpus/README.md` documents,
// made real against a produced binary.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::archetype::{Case, CaseExpect, Expected, TtyStream};
use crate::exec;
use crate::manifest::GapEntry;
use crate::report::{AcceptanceReport, CaseOutcome, CaseResult};
use crate::workspace;

pub fn run_cases(
    binary: &Path,
    cases: &[Case],
    cases_dir: &Path,
    isolation: &workspace::Isolation,
    gaps: &BTreeMap<String, GapEntry>,
    app_cargo_toml: Result<&str, &str>,
) -> AcceptanceReport {
    let results = cases
        .iter()
        .map(|case| {
            let sandbox = cases_dir.join(&case.name);
            let (outcome, detail) = match execute(binary, case, &sandbox, isolation) {
                Ok(execution) => {
                    let (raw_pass, detail) = evaluate(case, &execution);
                    match (case.expected, raw_pass) {
                        (Expected::Pass, true) => (CaseOutcome::Pass, detail),
                        (Expected::Pass, false) => (CaseOutcome::Fail, detail),
                        (Expected::Fail, false) => (CaseOutcome::ExpectedFail, detail),
                        (Expected::Fail, true) => {
                            let gap = case.gap.as_deref();
                            let evidence = gap.and_then(|gap| gaps.get(gap)).and_then(GapEntry::evidence);
                            match evidence {
                                None => (CaseOutcome::UnexpectedPass, detail),
                                Some(evidence) => match app_cargo_toml {
                                    Ok(cargo_toml) if evidence.satisfied_by(cargo_toml) => {
                                        (CaseOutcome::UnexpectedPass, detail)
                                    }
                                    Ok(_) => (CaseOutcome::HandRolledPass, detail),
                                    Err(read_err) => (
                                        CaseOutcome::Fail,
                                        Some(format!(
                                            "evidence for gap {:?} could not be checked: the produced app's Cargo.toml could not be read: {read_err}",
                                            gap.unwrap_or("?")
                                        )),
                                    ),
                                },
                            }
                        }
                    }
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

struct Execution {
    exit_code: Option<i32>,
    timed_out: bool,
    stdout: String,
    stderr: String,
    sandbox_inventory: SandboxInventory,
}

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
            // `slave` drops here, so the master read ends when the child exits.
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
    // `Command` retains the slave `Stdio` handles after `spawn`; kept, the master never sees EOF.
    drop(command);

    let stdout_capture = child.stdout.take().map(exec::capped_reader);
    let stderr_capture = child.stderr.take().map(exec::capped_reader);

    // Both streams on the pty interleave as on a terminal; the capture is attributed to stdout.
    let master_capture = match (&master, tty_stdout || tty_stderr) {
        (Some(m), true) => Some(exec::capped_reader(std::fs::File::from(
            m.try_clone()
                .map_err(|err| format!("duplicating pty master for capture: {err}"))?,
        ))),
        _ => None,
    };

    let stdin_bytes = case.run.stdin.clone();
    if tty_stdin {
        if let Some(text) = stdin_bytes {
            let handle = master.take().expect("tty stdin implies an open master");
            std::thread::spawn(move || {
                let mut writer = std::fs::File::from(handle);
                let _ = writer.write_all(text.as_bytes());
                let _ = writer.write_all(&[0x04, 0x04]);
            });
        }
    } else {
        let mut stdin_pipe = child.stdin.take().expect("stdin was piped");
        if let Some(text) = stdin_bytes {
            std::thread::spawn(move || {
                let _ = stdin_pipe.write_all(text.as_bytes());
            });
        }
    }

    let captures: Vec<&exec::Capture> = [
        stdout_capture.as_ref(),
        stderr_capture.as_ref(),
        master_capture.as_ref(),
    ]
    .into_iter()
    .flatten()
    .collect();
    // `supervise` kills the case's whole process group before returning,
    // whether the case exits on its own or hits its deadline, so nothing in
    // it can still be writing into the sandbox by the time the one
    // inventory read below runs.
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

    let sandbox_inventory = if supervised.timed_out
        || (case.expect.files.is_empty() && case.expect.files_absent.is_empty())
    {
        SandboxInventory::default()
    } else {
        build_sandbox_inventory(sandbox)?
    };

    Ok(Execution {
        exit_code: supervised.exit_code,
        timed_out: supervised.timed_out,
        stdout: normalize_lf(&stdout),
        stderr: normalize_lf(&stderr),
        sandbox_inventory,
    })
}

// Only objects may carry keys the expectation omits; arrays and scalars must match.
fn json_is_subset(got: &serde_json::Value, want: &serde_json::Value) -> bool {
    match (got, want) {
        (serde_json::Value::Object(got), serde_json::Value::Object(want)) => want
            .iter()
            .all(|(key, want)| got.get(key).is_some_and(|got| json_is_subset(got, want))),
        (serde_json::Value::Array(got), serde_json::Value::Array(want)) => {
            got.len() == want.len()
                && got
                    .iter()
                    .zip(want)
                    .all(|(got, want)| json_is_subset(got, want))
        }
        _ => got == want,
    }
}

fn json_has_row(value: &serde_json::Value, row: &[String]) -> bool {
    let mut candidates = Vec::new();
    collect_array_elements(value, &mut candidates);
    candidates.iter().any(|element| {
        let mut scalars = Vec::new();
        collect_scalars(element, &mut scalars);
        row.iter().all(|cell| scalars.iter().any(|s| s == cell))
    })
}

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

fn normalize_lf(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// Bounds how many bytes a single call to `read_regular_file_no_follow` will
/// read, and how many cumulative bytes `build_sandbox_inventory` will read
/// across a whole sandbox. Exceeding it is a case error naming the size —
/// the sandbox belongs to an untrusted produced app, so a `files`/
/// `files_absent` assertion must never launch an unbounded read.
pub(crate) const MAX_INVENTORIED_BYTES: u64 = 1024 * 1024;

/// What a path in a `SandboxInventory` resolved to, without ever following
/// a symlink.
#[derive(Debug)]
enum SandboxEntry {
    File(Vec<u8>),
    Directory,
    /// A symlink, FIFO, socket, device, or anything else that isn't a
    /// regular file or a directory.
    Other,
}

/// A snapshot of a case sandbox's directory tree, taken once by
/// `build_sandbox_inventory` after `execute` confirms the case's whole
/// process group is dead — nothing can still be writing into the sandbox by
/// the time this exists. `files` and `files_absent` are lookups against it;
/// neither touches the filesystem again. Keyed by the native relative
/// `PathBuf`, not a lossily-converted string: a produced app is untrusted
/// and could otherwise name a non-UTF-8 file whose lossy spelling collides
/// with a legitimate expectation.
#[derive(Debug, Default)]
struct SandboxInventory {
    entries: BTreeMap<PathBuf, SandboxEntry>,
}

impl SandboxInventory {
    fn get(&self, rel: &str) -> Result<Option<&SandboxEntry>, String> {
        let rel_path = Path::new(rel);
        if rel_path.is_absolute()
            || rel_path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(format!("path {rel:?} escapes the case sandbox"));
        }
        Ok(self.entries.get(rel_path))
    }
}

/// Walks `root` recursively, never following a symlink, and records every
/// entry: a directory, a regular file (read in full and counted against
/// `MAX_INVENTORIED_BYTES`), or `SandboxEntry::Other` for anything else
/// (symlink, FIFO, socket, device). A symlinked directory is recorded as
/// `Other` and not descended into, so a produced app cannot redirect the
/// walk outside the sandbox by replacing a directory with a symlink.
fn build_sandbox_inventory(root: &Path) -> Result<SandboxInventory, String> {
    let mut entries = BTreeMap::new();
    let mut total_bytes: u64 = 0;
    let mut pending = vec![PathBuf::new()];
    while let Some(rel_dir) = pending.pop() {
        let dir = root.join(&rel_dir);
        let read_dir = std::fs::read_dir(&dir)
            .map_err(|err| format!("reading sandbox directory {}: {err}", rel_dir.display()))?;
        for entry in read_dir {
            let entry = entry
                .map_err(|err| format!("reading sandbox directory {}: {err}", rel_dir.display()))?;
            let rel = rel_dir.join(entry.file_name());
            // `DirEntry::file_type` reads the directory-entry type directly
            // (no extra syscall on most platforms) and, like
            // `symlink_metadata`, never follows a final symlink — unlike
            // `DirEntry::metadata`/`std::fs::metadata`, which do.
            let file_type = entry
                .file_type()
                .map_err(|err| format!("inspecting sandbox entry {}: {err}", rel.display()))?;
            if file_type.is_dir() {
                entries.insert(rel.clone(), SandboxEntry::Directory);
                pending.push(rel);
            } else if file_type.is_file() {
                let metadata = entry
                    .metadata()
                    .map_err(|err| format!("inspecting sandbox entry {}: {err}", rel.display()))?;
                let remaining = MAX_INVENTORIED_BYTES.saturating_sub(total_bytes);
                // A cheap early exit for a grossly oversized file, before
                // attempting to read it at all; the read below is the real
                // enforcement (belt: nothing is still writing, by the time
                // this runs, so `metadata.len()` is accurate; braces: the
                // read itself is still bounded).
                if metadata.len() > remaining {
                    return Err(format!(
                        "sandbox contents exceed the {MAX_INVENTORIED_BYTES}-byte inventory \
                         budget (over budget at {}, would be {} bytes)",
                        rel.display(),
                        total_bytes.saturating_add(metadata.len())
                    ));
                }
                let bytes = read_regular_file_no_follow(&root.join(&rel), remaining)
                    .map_err(|err| format!("reading sandbox entry {}: {err}", rel.display()))?;
                // The bytes actually read, not `metadata.len()`: the two
                // can differ, and the cumulative budget must reflect what
                // was truly consumed.
                total_bytes = total_bytes.saturating_add(bytes.len() as u64);
                entries.insert(rel, SandboxEntry::File(bytes));
            } else {
                entries.insert(rel, SandboxEntry::Other);
            }
        }
    }
    Ok(SandboxInventory { entries })
}

pub(crate) fn read_regular_file_no_follow(path: &Path, max_bytes: u64) -> Result<Vec<u8>, String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|err| err.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err("refusing to follow a symlink".to_string());
    }
    if !metadata.is_file() {
        return Err("not a regular file".to_string());
    }
    let file = open_no_follow(path).map_err(|err| err.to_string())?;
    let mut buf = Vec::new();
    // Capped at the stream level, not just by the stat above: a file that
    // grows between the stat and this read still can't make the read
    // itself unbounded. The extra byte lets an oversized file be detected
    // as a mismatch without reading past the cap.
    Read::take(file, max_bytes + 1)
        .read_to_end(&mut buf)
        .map_err(|err| err.to_string())?;
    if buf.len() as u64 > max_bytes {
        return Err(format!("file is over the {max_bytes}-byte read budget"));
    }
    Ok(buf)
}

#[cfg(unix)]
fn open_no_follow(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(not(unix))]
fn open_no_follow(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::File::open(path)
}

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
            (Err(err), _) => failures.push(format!("stdout_json: stdout is not valid JSON: {err}")),
            (Ok(got), Ok(want)) => {
                if got != want {
                    failures.push(format!(
                        "stdout_json: stdout is not semantically equal to expected {want}"
                    ));
                }
            }
        }
    }
    if let Some(want) = &expect.stdout_json_subset {
        match (
            serde_json::from_str::<serde_json::Value>(&execution.stdout),
            serde_json::from_str::<serde_json::Value>(want),
        ) {
            (_, Err(err)) => failures.push(format!(
                "suite defect: expected stdout_json_subset is not valid JSON: {err}"
            )),
            (Err(err), _) => failures.push(format!(
                "stdout_json_subset: stdout is not valid JSON: {err}"
            )),
            (Ok(got), Ok(want)) => {
                if !json_is_subset(&got, &want) {
                    failures.push(format!(
                        "stdout_json_subset: stdout does not carry expected {want}"
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
                        failures.push(format!(
                            "stdout_json_rows: no single JSON element carries all of {row:?}"
                        ));
                    }
                }
            }
            Err(err) => failures.push(format!("stdout_json_rows: stdout is not valid JSON: {err}")),
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
    for (rel, want) in &expect.files {
        match execution.sandbox_inventory.get(rel) {
            Ok(Some(SandboxEntry::File(bytes))) => match std::str::from_utf8(bytes) {
                Ok(text) if &normalize_lf(text) == want => {}
                _ => failures.push(format!("file {rel:?} content differs from expected")),
            },
            Ok(Some(SandboxEntry::Directory)) => {
                failures.push(format!("file {rel:?} is a directory, not a regular file"));
            }
            Ok(Some(SandboxEntry::Other)) => {
                failures.push(format!("file {rel:?} is not a regular file"));
            }
            Ok(None) => failures.push(format!("file {rel:?} does not exist")),
            Err(err) => failures.push(err),
        }
    }
    for rel in &expect.files_absent {
        match execution.sandbox_inventory.get(rel) {
            Ok(Some(_)) => failures.push(format!("file {rel:?} must not exist")),
            Ok(None) => {}
            Err(err) => failures.push(err),
        }
    }
}
