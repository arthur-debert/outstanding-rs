//! The one subprocess supervisor: every external process the runner spawns —
//! agent session, cargo build, acceptance checks and cases, invariant cells —
//! runs under [`supervise`], which enforces a hard deadline over the child
//! AND its output streams, terminates the child's whole process group on
//! expiry (so a shell's grandchildren cannot outlive the run or hold the
//! pipes open), and caps in-memory output capture. [`run`] is the
//! pipes-only convenience over it; the roster case executor (`cases`) wires
//! its own stdio — ptys included — and calls [`supervise`] directly. A
//! phase that overruns its deadline becomes a recorded finding, never a
//! hung runner: the durable `report.json` is always written.

use std::io::Read;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Cap on each captured stream; excess is discarded with a truncation marker.
pub const CAPTURE_CAP_BYTES: usize = 2 * 1024 * 1024;

/// After a deadline group-kill, how long to wait for the pipe readers to see
/// EOF before abandoning them. Killing the group closes every writer inside
/// it; only a descendant that escaped the group (its own setsid) can still
/// hold a pipe, and that one must not block the run.
const READER_GRACE: Duration = Duration::from_secs(2);

/// How one supervised subprocess ended.
#[derive(Debug)]
pub struct Outcome {
    /// The exit code; `None` when killed by a signal (including our own
    /// deadline kill).
    pub exit_code: Option<i32>,
    /// True when the deadline expired before the run fully completed — the
    /// child still running, or its output streams still open (a descendant
    /// inheriting stdout/stderr keeps a stream open past the child's own
    /// exit). Either way the process group was killed at the deadline.
    pub timed_out: bool,
    /// Captured stdout (empty when `capture` was false), lossy UTF-8,
    /// truncated at [`CAPTURE_CAP_BYTES`].
    pub stdout: String,
    /// Captured stderr, same rules as `stdout`.
    pub stderr: String,
}

/// Runs `command` to completion or `deadline`, whichever comes first.
///
/// The child is placed in its own process group. Supervision covers both the
/// child process and (with `capture` true) its piped stdout/stderr until EOF,
/// under one absolute deadline: a descendant that inherits a pipe and
/// outlives the child cannot stall the run. On expiry the whole group is
/// killed; a reader still open after the group kill (an escaped descendant)
/// is abandoned after a short grace, returning whatever was captured so far.
/// With `capture` false the caller's stdio configuration stands (e.g. the
/// session phase writes straight to the transcript file).
///
/// Errors only when the process cannot be spawned at all — a deadline kill
/// or nonzero exit is an `Outcome`, not an error, because those are
/// reportable findings.
pub fn run(command: &mut Command, deadline: Duration, capture: bool) -> Result<Outcome, String> {
    place_in_own_group(command);
    if capture {
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
    }

    let mut child = command.spawn().map_err(|err| format!("spawning: {err}"))?;
    let stdout_capture = child.stdout.take().map(capped_reader);
    let stderr_capture = child.stderr.take().map(capped_reader);

    let captures: Vec<&Capture> = [stdout_capture.as_ref(), stderr_capture.as_ref()]
        .into_iter()
        .flatten()
        .collect();
    let supervised = supervise(&mut child, deadline, &captures)?;

    Ok(Outcome {
        exit_code: supervised.exit_code,
        timed_out: supervised.timed_out,
        stdout: stdout_capture.map(Capture::text).unwrap_or_default(),
        stderr: stderr_capture.map(Capture::text).unwrap_or_default(),
    })
}

/// Configures `command` to become its own process-group leader, so the
/// deadline kill can take out its whole descendant tree. Every supervised
/// spawn must pass through this before `spawn()`.
pub fn place_in_own_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
}

/// How a supervised child ended: its exit code (None when signal-killed)
/// and whether the deadline expired first.
pub struct Supervised {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
}

/// The deadline loop shared by every supervised spawn: waits for the child
/// AND all `captures` to finish, killing the child's whole process group
/// when `deadline` expires. The caller owns stdio wiring and the capture
/// threads — this is what lets the case executor put streams on a pty
/// while keeping the one deadline discipline.
pub fn supervise(
    child: &mut Child,
    deadline: Duration,
    captures: &[&Capture],
) -> Result<Supervised, String> {
    let started = Instant::now();
    let mut timed_out = false;
    let mut status: Option<ExitStatus> = None;
    loop {
        if status.is_none() {
            match child.try_wait() {
                Ok(Some(exited)) => status = Some(exited),
                Ok(None) => {}
                Err(err) => return Err(format!("waiting for child: {err}")),
            }
        }
        if status.is_some() && captures.iter().all(|c| c.is_finished()) {
            break;
        }
        if started.elapsed() >= deadline {
            timed_out = true;
            kill_group(child);
            if status.is_none() {
                match child.wait() {
                    Ok(exited) => status = Some(exited),
                    Err(err) => return Err(format!("reaping timed-out child: {err}")),
                }
            }
            // The group kill closed every in-group writer; give the readers a
            // bounded window to reach EOF, then abandon any still blocked on
            // an escaped descendant's write end.
            let grace_ends = Instant::now() + READER_GRACE;
            while !captures.iter().all(|c| c.is_finished()) && Instant::now() < grace_ends {
                std::thread::sleep(Duration::from_millis(10));
            }
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let status = status.expect("supervision loop always breaks with a status");
    Ok(Supervised {
        exit_code: status.code(),
        timed_out,
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

/// One stream's capture: the draining thread plus the shared buffer it fills,
/// so the captured text stays reachable even when the thread must be
/// abandoned (blocked on a pipe an escaped descendant holds open).
pub struct Capture {
    handle: JoinHandle<()>,
    state: Arc<Mutex<CaptureState>>,
}

struct CaptureState {
    kept: Vec<u8>,
    truncated: bool,
}

impl Capture {
    /// True when the reader thread drained its stream to EOF (or gave up
    /// on an error).
    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }

    /// The captured text so far — complete when the stream reached EOF,
    /// partial when the reader was abandoned. Never blocks: joins the thread
    /// only when it has already finished.
    pub fn text(self) -> String {
        if self.handle.is_finished() {
            let _ = self.handle.join();
        }
        let state = self.state.lock().expect("capture state poisoned");
        let mut text = String::from_utf8_lossy(&state.kept).into_owned();
        if state.truncated {
            text.push_str("\n[output truncated at capture cap]");
        }
        text
    }
}

/// Drains a stream on its own thread into a shared buffer, keeping at most
/// [`CAPTURE_CAP_BYTES`] and flagging truncation when output exceeded the cap.
pub fn capped_reader<R: Read + Send + 'static>(mut stream: R) -> Capture {
    let state = Arc::new(Mutex::new(CaptureState {
        kept: Vec::new(),
        truncated: false,
    }));
    let shared = Arc::clone(&state);
    let handle = std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match stream.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let mut state = shared.lock().expect("capture state poisoned");
                    if state.kept.len() < CAPTURE_CAP_BYTES {
                        let take = n.min(CAPTURE_CAP_BYTES - state.kept.len());
                        let chunk = &buf[..take];
                        state.kept.extend_from_slice(chunk);
                        state.truncated |= take < n;
                    } else {
                        state.truncated = true;
                    }
                }
            }
        }
    });
    Capture { handle, state }
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
    fn descendant_holding_the_pipe_cannot_outlive_the_deadline() {
        // The direct child exits immediately, but its backgrounded descendant
        // inherits stdout and would hold the pipe open for 30s: the run must
        // still return at the deadline, reporting the overrun.
        let started = Instant::now();
        let outcome = run(
            Command::new("sh").args(["-c", "echo before-exit; sleep 30 & exit 0"]),
            Duration::from_millis(300),
            true,
        )
        .unwrap();
        assert!(outcome.timed_out);
        assert_eq!(outcome.exit_code, Some(0));
        assert_eq!(outcome.stdout.trim(), "before-exit");
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
