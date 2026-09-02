//! Expected-fail harness for the corpus gap-spec acceptance suites; the
//! expected-fail semantics, the gate ledger and its tripwire are documented in
//! `corpus/gap-suites/README.md`.
//!
//! Everything here is black-box: [`run`] spawns a produced binary and returns its
//! stdout/stderr/exit status plus wall-clock duration. The suites hand binaries
//! hostile inputs on purpose, so every wait and every capture is bounded: the child
//! runs as its own process-group leader and [`SPAWN_TIMEOUT`] kills the whole tree;
//! each stream is drained on its own thread and retains at most [`OUTPUT_LIMIT`]
//! bytes, past which the child is killed; and captures come back over channels
//! with a `DRAIN_GRACE` timeout rather than a join, so a descendant that outlives
//! the child while holding a pipe writer is reported as a mismatch instead of
//! blocking the harness. Nothing here links against standout.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

/// Bounds suite runtime only; promptness assertions measure [`Output::duration`].
pub const SPAWN_TIMEOUT: Duration = Duration::from_secs(30);

/// Per-stream cap on retained child output; exceeding it on either stream is a mismatch.
pub const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;

const DRAIN_GRACE: Duration = Duration::from_secs(2);

pub struct Output {
    /// `None` when the process died of a signal.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    /// Wall-clock time from spawn to exit.
    pub duration: Duration,
}

/// Passes on a missing binary or an `Err` assertion; panics on `Ok`, the unexpected pass.
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

/// Panics when `binary_env` names nothing: a promoted assertion may never pass by not running.
pub fn required_binary(binary_env: &str) -> PathBuf {
    produced_binary(binary_env).unwrap_or_else(|| {
        panic!(
            "suite broken: ${binary_env} does not name a produced binary; the workspace's \
             .cargo/config.toml sets it to the in-repo fixture (target/debug), so export \
             it yourself under a custom CARGO_TARGET_DIR"
        )
    })
}

fn produced_binary(binary_env: &str) -> Option<PathBuf> {
    let value = std::env::var_os(binary_env)?;
    if value.is_empty() {
        return None;
    }
    let path = PathBuf::from(value);
    path.exists().then_some(path)
}

/// `Err` is a behavioral mismatch (hang, cap, leaked descendant, non-UTF-8); spawn failure panics.
pub fn run(binary: &Path, args: &[&str], dir: &Path) -> Result<Output, String> {
    let started = Instant::now();
    let mut command = Command::new(binary);
    command
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command
        .spawn()
        .unwrap_or_else(|err| panic!("suite broken: failed to spawn {binary:?}: {err}"));

    let stdout_pipe = child.stdout.take().expect("stdout was piped");
    let stderr_pipe = child.stderr.take().expect("stderr was piped");
    let exceeded = Arc::new(AtomicBool::new(false));
    let stdout_drain = spawn_drain(stdout_pipe, Arc::clone(&exceeded));
    let stderr_drain = spawn_drain(stderr_pipe, Arc::clone(&exceeded));

    let deadline = Instant::now() + SPAWN_TIMEOUT;
    let status = loop {
        if exceeded.load(Ordering::Relaxed) {
            kill_tree(&mut child);
            return Err(output_cap_mismatch(args));
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                kill_tree(&mut child);
                return Err(format!(
                    "process hung: still running after {SPAWN_TIMEOUT:?} (args: {args:?})"
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(err) => panic!("suite broken: waiting on {binary:?} failed: {err}"),
        }
    };
    let duration = started.elapsed();

    let stdout_captured = collect_drain(&stdout_drain, &mut child, "stdout", args)?;
    let stderr_captured = collect_drain(&stderr_drain, &mut child, "stderr", args)?;
    // The child can exceed the cap and exit before the wait loop sees the flag.
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

fn kill_tree(child: &mut Child) {
    #[cfg(unix)]
    // SAFETY: plain kill(2) on the process group `run` created for the child; ESRCH is harmless.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn spawn_drain(
    pipe: impl Read + Send + 'static,
    exceeded: Arc<AtomicBool>,
) -> mpsc::Receiver<Captured> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(drain_capped(pipe, &exceeded));
    });
    receiver
}

// A timeout here means a descendant still holds the pipe after the child exited.
fn collect_drain(
    drain: &mpsc::Receiver<Captured>,
    child: &mut Child,
    stream: &str,
    args: &[&str],
) -> Result<Captured, String> {
    match drain.recv_timeout(DRAIN_GRACE) {
        Ok(captured) => Ok(captured),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            kill_tree(child);
            Err(format!(
                "process leaked a descendant: {stream} still open {DRAIN_GRACE:?} \
                 after exit (args: {args:?})"
            ))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            panic!("suite broken: {stream} drain thread died without delivering")
        }
    }
}

fn output_cap_mismatch(args: &[&str]) -> String {
    format!(
        "process drowned a stream: more than {OUTPUT_LIMIT} bytes on stdout or stderr \
         (args: {args:?})"
    )
}

struct Captured {
    bytes: Vec<u8>,
    truncated: bool,
}

// Keeps draining past the cap so the child never blocks on a full pipe.
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

fn decode(bytes: Vec<u8>, stream: &str) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|_| format!("{stream} was not valid UTF-8"))
}

/// `Err` names the first line that is not a JSON object.
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

pub fn reject_ansi(text: &str, stream: &str) -> Result<(), String> {
    if text.contains('\u{1b}') {
        return Err(format!("{stream} contains ANSI escape sequences"));
    }
    Ok(())
}

pub fn reject_panic(stderr: &str) -> Result<(), String> {
    if stderr.contains("panicked at") || stderr.contains("RUST_BACKTRACE") {
        return Err(format!("process panicked instead of diagnosing: {stderr}"));
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn captures_output_and_exit_code() {
        let dir = tempfile::tempdir().unwrap();
        let out = run(
            Path::new("/bin/sh"),
            &["-c", "echo out; echo err >&2; exit 3"],
            dir.path(),
        )
        .unwrap();
        assert_eq!(out.code, Some(3));
        assert_eq!(out.stdout, "out\n");
        assert_eq!(out.stderr, "err\n");
    }

    // The sleep far outlives DRAIN_GRACE: this passes promptly only if the leak path fires.
    #[test]
    fn reports_descendant_holding_pipe_instead_of_hanging() {
        let dir = tempfile::tempdir().unwrap();
        let err = run(Path::new("/bin/sh"), &["-c", "sleep 30 &"], dir.path())
            .err()
            .expect("a leaked pipe-holding descendant must be a mismatch");
        assert!(err.contains("leaked a descendant"), "got: {err}");
    }
}
