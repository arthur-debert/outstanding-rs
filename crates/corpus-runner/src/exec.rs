//! The one subprocess executor: every external process the runner spawns —
//! agent session, cargo build, acceptance checks, invariant cells — goes
//! through [`run`], which enforces a hard deadline, terminates the child's
//! whole process group on expiry (so a shell's grandchildren cannot outlive
//! the run), and caps in-memory output capture. A phase that overruns its
//! deadline becomes a recorded finding, never a hung runner: the durable
//! `report.json` is always written.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Cap on each captured stream; excess is discarded with a truncation marker.
pub const CAPTURE_CAP_BYTES: usize = 2 * 1024 * 1024;

/// How one supervised subprocess ended.
#[derive(Debug)]
pub struct Outcome {
    /// The exit code; `None` when killed by a signal (including our own
    /// deadline kill).
    pub exit_code: Option<i32>,
    /// True when the deadline expired and the process group was killed.
    pub timed_out: bool,
    /// Captured stdout (empty when `capture` was false), lossy UTF-8,
    /// truncated at [`CAPTURE_CAP_BYTES`].
    pub stdout: String,
    /// Captured stderr, same rules as `stdout`.
    pub stderr: String,
}

/// Runs `command` to completion or `deadline`, whichever comes first.
///
/// The child is placed in its own process group; on expiry the whole group
/// is killed. With `capture` true, stdout/stderr are piped and drained into
/// memory (capped); with `capture` false the caller's stdio configuration
/// stands (e.g. the session phase writes straight to the transcript file).
///
/// Errors only when the process cannot be spawned at all — a deadline kill
/// or nonzero exit is an `Outcome`, not an error, because those are
/// reportable findings.
pub fn run(command: &mut Command, deadline: Duration, capture: bool) -> Result<Outcome, String> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    if capture {
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
    }

    let mut child = command.spawn().map_err(|err| format!("spawning: {err}"))?;
    let stdout_reader = child.stdout.take().map(capped_reader);
    let stderr_reader = child.stderr.take().map(capped_reader);

    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(err) => return Err(format!("waiting for child: {err}")),
        }
        if started.elapsed() >= deadline {
            timed_out = true;
            kill_group(&mut child);
            match child.wait() {
                Ok(status) => break status,
                Err(err) => return Err(format!("reaping timed-out child: {err}")),
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    };

    Ok(Outcome {
        exit_code: status.code(),
        timed_out,
        stdout: join_reader(stdout_reader),
        stderr: join_reader(stderr_reader),
    })
}

/// Kills the child's whole process group (the child is its own group leader
/// via `process_group(0)`), then the child itself as a fallback.
fn kill_group(child: &mut Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .status();
    }
    let _ = child.kill();
}

/// Drains a stream on its own thread, keeping at most [`CAPTURE_CAP_BYTES`]
/// and appending a truncation marker when output exceeded the cap.
fn capped_reader<R: Read + Send + 'static>(mut stream: R) -> JoinHandle<String> {
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let mut kept: Vec<u8> = Vec::new();
        let mut truncated = false;
        loop {
            match stream.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if kept.len() < CAPTURE_CAP_BYTES {
                        let take = n.min(CAPTURE_CAP_BYTES - kept.len());
                        kept.extend_from_slice(&buf[..take]);
                        truncated |= take < n;
                    } else {
                        truncated = true;
                    }
                }
            }
        }
        let mut text = String::from_utf8_lossy(&kept).into_owned();
        if truncated {
            text.push_str("\n[output truncated at capture cap]");
        }
        text
    })
}

fn join_reader(reader: Option<JoinHandle<String>>) -> String {
    reader
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadline_kills_the_child_and_reports_timeout() {
        let started = Instant::now();
        let outcome = run(
            Command::new("sh").args(["-c", "sleep 30"]),
            Duration::from_millis(200),
            true,
        )
        .unwrap();
        assert!(outcome.timed_out);
        assert_eq!(outcome.exit_code, None);
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn capture_is_capped_with_a_marker() {
        let outcome = run(
            Command::new("sh").args(["-c", "head -c 3000000 /dev/zero | tr '\\0' 'a'"]),
            Duration::from_secs(30),
            true,
        )
        .unwrap();
        assert!(!outcome.timed_out);
        assert!(outcome.stdout.len() <= CAPTURE_CAP_BYTES + 64);
        assert!(outcome
            .stdout
            .ends_with("[output truncated at capture cap]"));
    }

    #[test]
    fn completed_child_reports_exit_and_output() {
        let outcome = run(
            Command::new("sh").args(["-c", "echo out; echo err >&2; exit 4"]),
            Duration::from_secs(30),
            true,
        )
        .unwrap();
        assert!(!outcome.timed_out);
        assert_eq!(outcome.exit_code, Some(4));
        assert_eq!(outcome.stdout.trim(), "out");
        assert_eq!(outcome.stderr.trim(), "err");
    }
}
