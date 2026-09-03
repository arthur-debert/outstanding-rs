//! The one subprocess supervisor: every external process the runner spawns
//! runs under `supervise`, which enforces a hard deadline over the child and
//! its output streams and kills the child's whole process group before
//! reaping it, whether it exits on its own or hits the deadline.

use std::io::Read;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const CAPTURE_CAP_BYTES: usize = 2 * 1024 * 1024;

const READER_GRACE: Duration = Duration::from_secs(2);
const GROUP_KILL_GRACE: Duration = Duration::from_millis(500);

#[derive(Debug)]
pub struct Outcome {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
}

pub fn run(command: &mut Command, deadline: Duration, capture: bool) -> Result<Outcome, String> {
    run_watched(command, deadline, capture, |_| {})
}

/// `on_spawn` runs before any output is read: the broker needs the pid before the agent connects.
pub fn run_watched(
    command: &mut Command,
    deadline: Duration,
    capture: bool,
    on_spawn: impl FnOnce(u32),
) -> Result<Outcome, String> {
    place_in_own_group(command);
    if capture {
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
    }

    let mut child = command.spawn().map_err(|err| format!("spawning: {err}"))?;
    on_spawn(child.id());
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

pub fn place_in_own_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
}

pub struct Supervised {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
}

pub fn supervise(
    child: &mut Child,
    deadline: Duration,
    captures: &[&Capture],
) -> Result<Supervised, String> {
    let started = Instant::now();
    let mut timed_out = false;
    let mut reaped_status: Option<ExitStatus> = None;
    loop {
        let exited = if cfg!(unix) {
            child_exited_without_reaping(child.id())?
        } else {
            match child.try_wait() {
                Ok(Some(status)) => {
                    reaped_status = Some(status);
                    true
                }
                Ok(None) => false,
                Err(err) => return Err(format!("waiting for child: {err}")),
            }
        };
        if exited && captures.iter().all(|c| c.is_finished()) {
            break;
        }
        if started.elapsed() >= deadline {
            timed_out = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    signal_process_group(child.id());
    let _ = child.kill();
    let status = match reaped_status {
        Some(status) => status,
        None => child
            .wait()
            .map_err(|err| format!("reaping child: {err}"))?,
    };
    wait_for_process_group_to_die(child.id());
    if timed_out {
        let grace_ends = Instant::now() + READER_GRACE;
        while !captures.iter().all(|c| c.is_finished()) && Instant::now() < grace_ends {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    Ok(Supervised {
        exit_code: status.code(),
        timed_out,
    })
}

#[cfg(unix)]
fn child_exited_without_reaping(pid: u32) -> Result<bool, String> {
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    let ret = unsafe {
        libc::waitid(
            libc::P_PID,
            pid as libc::id_t,
            &mut info,
            libc::WEXITED | libc::WNOWAIT | libc::WNOHANG,
        )
    };
    if ret != 0 {
        return Err(format!(
            "polling child exit status: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(info.si_signo != 0)
}

#[cfg(not(unix))]
fn child_exited_without_reaping(_pid: u32) -> Result<bool, String> {
    unreachable!("only called from the cfg!(unix) branch in supervise's loop")
}

fn signal_process_group(pid: u32) {
    #[cfg(unix)]
    {
        unsafe {
            libc::killpg(pid as libc::pid_t, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    let _ = pid;
}

fn wait_for_process_group_to_die(pid: u32) {
    #[cfg(unix)]
    {
        let deadline = Instant::now() + GROUP_KILL_GRACE;
        while Instant::now() < deadline {
            let anything_left = unsafe { libc::killpg(pid as libc::pid_t, 0) } == 0;
            if !anything_left {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[cfg(not(unix))]
    let _ = pid;
}

pub struct Capture {
    handle: JoinHandle<()>,
    state: Arc<Mutex<CaptureState>>,
}

struct CaptureState {
    kept: Vec<u8>,
    truncated: bool,
}

impl Capture {
    pub fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }

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

    #[test]
    fn exit_peek_does_not_reap_the_child() {
        let mut child = Command::new("sh").args(["-c", "exit 0"]).spawn().unwrap();
        let pid = child.id();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !child_exited_without_reaping(pid).unwrap() {
            assert!(Instant::now() < deadline, "child did not exit in time");
            std::thread::sleep(Duration::from_millis(5));
        }
        let status = child.wait().unwrap();
        assert_eq!(status.code(), Some(0));
    }

    #[test]
    fn a_trivial_child_is_supervised_promptly() {
        let pid = Arc::new(Mutex::new(0u32));
        let pid_for_capture = Arc::clone(&pid);
        let started = Instant::now();
        let outcome = run_watched(
            Command::new("sh").args(["-c", "exit 0"]),
            Duration::from_secs(30),
            true,
            move |child_pid| *pid_for_capture.lock().unwrap() = child_pid,
        )
        .unwrap();
        assert!(!outcome.timed_out);
        assert_eq!(outcome.exit_code, Some(0));

        let pid = *pid.lock().unwrap();
        let probe = unsafe { libc::killpg(pid as libc::pid_t, 0) };
        assert_eq!(probe, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH),
            "expected the process group to be gone once supervise returned"
        );
        assert!(
            started.elapsed() < GROUP_KILL_GRACE * 10,
            "supervise took {:?}, well past what a trivial child with nothing \
             left to signal should ever need",
            started.elapsed()
        );
    }
}
