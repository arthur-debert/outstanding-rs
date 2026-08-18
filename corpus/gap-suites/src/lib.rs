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
//! stdout/stderr/exit status plus wall-clock duration; capture is bounded per stream
//! ([`OUTPUT_LIMIT`]) so a runaway child cannot exhaust the test runner's memory;
//! nothing links against standout.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long [`run`] lets a spawned binary live before declaring it hung.
///
/// Generous on purpose: this bounds *suite* runtime; per-assertion promptness
/// requirements (e.g. jjlike's render budget) are asserted separately against
/// [`Output::duration`].
pub const SPAWN_TIMEOUT: Duration = Duration::from_secs(30);

/// Per-stream cap on retained child output.
///
/// The archetype contracts emit at most a handful of kilobytes, so this is generous —
/// but the suites deliberately hand binaries hostile inputs (a billion-iteration
/// template), and a binary without budget enforcement could otherwise emit hundreds of
/// megabytes inside [`SPAWN_TIMEOUT`] and OOM the test runner. A child that exceeds the
/// cap on either stream is killed and reported as a behavioral mismatch by [`run`].
pub const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;

/// Captured result of one black-box invocation of an archetype binary.
pub struct Output {
    /// The process exit code; `None` when the process died without one (signal).
    pub code: Option<i32>,
    /// Everything the process wrote to stdout, decoded as UTF-8.
    pub stdout: String,
    /// Everything the process wrote to stderr, decoded as UTF-8.
    pub stderr: String,
    /// Wall-clock time from spawn to exit — what promptness assertions (e.g. jjlike's
    /// render budget) measure against.
    pub duration: Duration,
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
/// [`SPAWN_TIMEOUT`] or when a stream exceeds [`OUTPUT_LIMIT`].
///
/// Returns `Err` for *behavioral* failures the assertions care about — the process hung
/// past the timeout, drowned a stream past the output cap, or wrote non-UTF-8 where the
/// specs require a UTF-8 stream. Panics when an existing binary cannot be spawned at
/// all: that is a broken suite or environment, which must surface as an error, not as
/// expected-fail.
pub fn run(binary: &Path, args: &[&str], dir: &Path) -> Result<Output, String> {
    let started = Instant::now();
    let mut child = Command::new(binary)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|err| panic!("suite broken: failed to spawn {binary:?}: {err}"));

    // Drain the pipes on threads so a chatty child can't deadlock against a full pipe
    // while we wait on it. Each drain retains at most OUTPUT_LIMIT bytes and raises
    // `exceeded` past that, so the wait loop can kill a runaway child early.
    let stdout_pipe = child.stdout.take().expect("stdout was piped");
    let stderr_pipe = child.stderr.take().expect("stderr was piped");
    let exceeded = Arc::new(AtomicBool::new(false));
    let stdout_thread = {
        let exceeded = Arc::clone(&exceeded);
        std::thread::spawn(move || drain_capped(stdout_pipe, &exceeded))
    };
    let stderr_thread = {
        let exceeded = Arc::clone(&exceeded);
        std::thread::spawn(move || drain_capped(stderr_pipe, &exceeded))
    };

    let deadline = Instant::now() + SPAWN_TIMEOUT;
    let status = loop {
        if exceeded.load(Ordering::Relaxed) {
            // Killing the child closes its pipe ends, so the drains reach EOF and the
            // joins cannot block; joining (rather than detaching) keeps repeated
            // failures from leaking threads.
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_thread.join();
            let _ = stderr_thread.join();
            return Err(output_cap_mismatch(args));
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();
                return Err(format!(
                    "process hung: still running after {SPAWN_TIMEOUT:?} (args: {args:?})"
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(err) => panic!("suite broken: waiting on {binary:?} failed: {err}"),
        }
    };
    let duration = started.elapsed();

    let stdout_captured = stdout_thread.join().expect("stdout drain panicked");
    let stderr_captured = stderr_thread.join().expect("stderr drain panicked");
    // A child can blow the cap and exit before the wait loop notices the flag.
    if stdout_captured.truncated || stderr_captured.truncated {
        return Err(output_cap_mismatch(args));
    }
    let stdout = decode(stdout_captured.bytes, "stdout")?;
    let stderr = decode(stderr_captured.bytes, "stderr")?;
    Ok(Output {
        code: status.code(),
        stdout,
        stderr,
        duration,
    })
}

/// The behavioral mismatch [`run`] reports when a stream outgrew [`OUTPUT_LIMIT`].
fn output_cap_mismatch(args: &[&str]) -> String {
    format!(
        "process drowned a stream: more than {OUTPUT_LIMIT} bytes on stdout or stderr \
         (args: {args:?})"
    )
}

/// What one drain thread captured from a child pipe.
struct Captured {
    /// The first (at most) [`OUTPUT_LIMIT`] bytes the child wrote.
    bytes: Vec<u8>,
    /// Whether the child wrote past the cap; retained bytes are then incomplete.
    truncated: bool,
}

/// Reads a child pipe to EOF, retaining at most [`OUTPUT_LIMIT`] bytes and raising
/// `exceeded` when the child writes past the cap; keeps draining after truncation so
/// the child can never block on a full pipe. Panics (suite broken) on IO failure.
fn drain_capped(mut pipe: impl Read, exceeded: &AtomicBool) -> Captured {
    let mut retained = Vec::new();
    let mut truncated = false;
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let n = pipe
            .read(&mut chunk)
            .unwrap_or_else(|err| panic!("suite broken: reading child pipe failed: {err}"));
        if n == 0 {
            break;
        }
        if !truncated {
            let keep = n.min(OUTPUT_LIMIT - retained.len());
            retained.extend_from_slice(&chunk[..keep]);
            if keep < n {
                truncated = true;
                exceeded.store(true, Ordering::Relaxed);
            }
        }
    }
    Captured {
        bytes: retained,
        truncated,
    }
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
