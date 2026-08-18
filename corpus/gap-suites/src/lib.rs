//! Expected-fail harness for the corpus gap-spec acceptance suites.
//!
//! The gap archetypes (`corpus/archetypes/tflike`, `corpus/archetypes/jjlike`) describe
//! capability standout does not have; their suites are red on arrival by design. This
//! harness runs each black-box assertion with xfail semantics so `pixi run test`
//! reports **expected-fail rather than error** — see [`expect_gap`] for the exact
//! state machine, and `corpus/gap-suites/README.md` for which epic each test file
//! gates (PAR02, PAR03, and the unminted runtime-templates epic).
//!
//! Everything here is black-box: [`run`] spawns a produced binary and returns its
//! stdout/stderr/exit status; nothing links against standout.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long [`run`] lets a spawned binary live before declaring it hung.
///
/// Generous on purpose: this bounds *suite* runtime; per-assertion promptness
/// requirements (e.g. jjlike's render budget) are asserted separately.
pub const SPAWN_TIMEOUT: Duration = Duration::from_secs(30);

/// Captured result of one black-box invocation of an archetype binary.
pub struct Output {
    /// The process exit code; `None` when the process died without one (signal).
    pub code: Option<i32>,
    /// Everything the process wrote to stdout, decoded as UTF-8.
    pub stdout: String,
    /// Everything the process wrote to stderr, decoded as UTF-8.
    pub stderr: String,
}

/// Runs one gap assertion with expected-fail semantics.
///
/// `gate` names the milestone group and owning epic (printed with every outcome so a
/// red group is never ownerless); `binary_env` is the env var that locates the produced
/// archetype binary; `reason` states which missing capability keeps the assertion red;
/// `assertion` is the black-box check, given the binary path, returning `Err` with a
/// mismatch description when the gap's behavior is absent.
///
/// Outcomes:
/// - env var unset / path absent → **expected-fail** (no binary produced yet); passes.
/// - assertion returns `Err` → **expected-fail** (gap open); passes.
/// - assertion returns `Ok` → **panics** with "UNEXPECTED PASS" so a closed gap gets
///   promoted (drop the wrapper, keep the assertion).
/// - the assertion itself panics (spawn failure of an existing binary, fixture IO) →
///   an ordinary test error, distinct from expected-fail: the suite is broken, not the
///   gap open.
pub fn expect_gap(
    gate: &str,
    binary_env: &str,
    reason: &str,
    assertion: impl FnOnce(&Path) -> Result<(), String>,
) {
    let Some(binary) = produced_binary(binary_env) else {
        println!("expected-fail [{gate}] {reason}: no produced binary at ${binary_env}");
        return;
    };
    match assertion(&binary) {
        Err(mismatch) => {
            println!("expected-fail [{gate}] {reason}: {mismatch}");
        }
        Ok(()) => panic!(
            "UNEXPECTED PASS [{gate}]: this expected-fail assertion now holds against \
             {binary:?} — the gap looks closed; promote it by removing its expect_gap \
             wrapper (see corpus/gap-suites/README.md)"
        ),
    }
}

/// Resolves the archetype binary from `binary_env`, if one has been produced.
///
/// Returns `None` when the variable is unset, empty, or names a path that does not
/// exist — all read as "no implementation yet", the suites' steady state until the
/// owning epic closes the gap.
fn produced_binary(binary_env: &str) -> Option<PathBuf> {
    let value = std::env::var_os(binary_env)?;
    if value.is_empty() {
        return None;
    }
    let path = PathBuf::from(value);
    path.exists().then_some(path)
}

/// Spawns `binary` with `args` in `dir` and captures its output, killing it at
/// [`SPAWN_TIMEOUT`].
///
/// Returns `Err` for *behavioral* failures the assertions care about — the process hung
/// past the timeout, or wrote non-UTF-8 where the specs require a UTF-8 stream. Panics
/// when an existing binary cannot be spawned at all: that is a broken suite or
/// environment, which must surface as an error, not as expected-fail.
pub fn run(binary: &Path, args: &[&str], dir: &Path) -> Result<Output, String> {
    let mut child = Command::new(binary)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|err| panic!("suite broken: failed to spawn {binary:?}: {err}"));

    // Drain the pipes on threads so a chatty child can't deadlock against a full pipe
    // while we wait on it.
    let stdout_pipe = child.stdout.take().expect("stdout was piped");
    let stderr_pipe = child.stderr.take().expect("stderr was piped");
    let stdout_thread = std::thread::spawn(move || read_all(stdout_pipe));
    let stderr_thread = std::thread::spawn(move || read_all(stderr_pipe));

    let deadline = Instant::now() + SPAWN_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "process hung: still running after {SPAWN_TIMEOUT:?} (args: {args:?})"
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(err) => panic!("suite broken: waiting on {binary:?} failed: {err}"),
        }
    };

    let stdout = decode(
        stdout_thread.join().expect("stdout drain panicked"),
        "stdout",
    )?;
    let stderr = decode(
        stderr_thread.join().expect("stderr drain panicked"),
        "stderr",
    )?;
    Ok(Output {
        code: status.code(),
        stdout,
        stderr,
    })
}

/// Reads a child pipe to EOF, panicking (suite broken) on IO failure.
fn read_all(mut pipe: impl Read) -> Vec<u8> {
    let mut buf = Vec::new();
    pipe.read_to_end(&mut buf)
        .unwrap_or_else(|err| panic!("suite broken: reading child pipe failed: {err}"));
    buf
}

/// Decodes captured bytes as UTF-8, reporting a behavioral mismatch when they are not.
fn decode(bytes: Vec<u8>, stream: &str) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|_| format!("{stream} was not valid UTF-8"))
}

/// Parses every stdout line as an independent JSON object, per the tflike stream
/// contract, returning the parsed values or a mismatch naming the offending line.
pub fn parse_ndjson(stdout: &str) -> Result<Vec<serde_json::Value>, String> {
    let mut entries = Vec::new();
    for (index, line) in stdout.lines().enumerate() {
        let value: serde_json::Value = serde_json::from_str(line).map_err(|err| {
            format!(
                "stdout line {} is not a JSON object: {err} (line was: {line:?})",
                index + 1
            )
        })?;
        if !value.is_object() {
            return Err(format!(
                "stdout line {} parses as JSON but is not an object: {line:?}",
                index + 1
            ));
        }
        entries.push(value);
    }
    Ok(entries)
}

/// Reports a mismatch when `text` carries an ANSI escape byte — the black-box test for
/// "no color, no spinner redraws" on a stream that must stay machine-clean.
pub fn reject_ansi(text: &str, stream: &str) -> Result<(), String> {
    if text.contains('\u{1b}') {
        return Err(format!("{stream} contains ANSI escape sequences"));
    }
    Ok(())
}

/// Reports a mismatch when `stderr` shows Rust panic output — the archetypes' specs
/// require diagnostics, never panics, on hostile input.
pub fn reject_panic(stderr: &str) -> Result<(), String> {
    if stderr.contains("panicked at") || stderr.contains("RUST_BACKTRACE") {
        return Err(format!("process panicked instead of diagnosing: {stderr}"));
    }
    Ok(())
}
