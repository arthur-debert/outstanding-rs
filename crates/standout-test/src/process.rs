//! The process escape hatch: [`TestHarness::run_process`],
//! [`TestHarness::run_pty`], and their [`ProcessResult`].
//!
//! Private, like `snapshot`: the public items are re-exported from the
//! crate root, and their own docs carry the story — this module is only
//! where the code lives.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use tempfile::TempDir;

use crate::{output_mode_flag, StdinMode, TestHarness};

impl TestHarness {
    /// Runs `program` as a real child process and returns what the OS saw.
    ///
    /// [`run`](Self::run) calls into the app in the test's own process,
    /// which is what makes it fast and what makes its two streams a
    /// *reconstruction* of what `App::run`'s writer seam would have emitted
    /// rather than a recording. Some facts survive only the real boundary:
    ///
    /// - **Stream separation as the OS performed it.** Here stdout and
    ///   stderr are two real pipes filled by the binary's own writes, so a
    ///   claim about which stream a byte went to is a claim about the
    ///   program rather than about the harness's model of it.
    /// - **The process exit status.** `RunResult` carries the typed
    ///   [`ExitStatus`](standout::cli::ExitStatus) the framework *intends*;
    ///   only a real run proves `main` reached `std::process::exit` with the
    ///   matching code — or was killed by a signal, which has no in-process
    ///   equivalent at all.
    /// - **Not being a terminal.** The child's stdout is a pipe, exactly as
    ///   it is when a user redirects to a file, so behavior that keys off
    ///   "am I a TTY" is exercised as shipped rather than as simulated. The
    ///   TTY-positive counterpart is [`run_pty`](Self::run_pty), whose child
    ///   writes to a real pseudo-terminal.
    ///
    /// It is the expensive option — a compile and a fork per call — so it is
    /// for evidence in-process capture cannot produce, not for coverage.
    ///
    /// A test names the binary the way Cargo hands it over —
    /// `env!("CARGO_BIN_EXE_<name>")`, which resolves to the freshly built
    /// binary of the package the test belongs to:
    ///
    /// ```no_run
    /// use standout_test::TestHarness;
    ///
    /// # let binary = "/path/to/mycli"; // env!("CARGO_BIN_EXE_mycli")
    /// let result = TestHarness::new()
    ///     .env("TODO_FILE", "/tmp/todos.json")
    ///     .run_process(binary, ["--version"]);
    ///
    /// result.assert_success();
    /// result.assert_stderr_empty();
    /// ```
    ///
    /// The builder settings that describe a *process* are carried over:
    /// [`env`](Self::env) / [`env_remove`](Self::env_remove) become the
    /// child's environment, [`fixture`](Self::fixture) files are
    /// materialized and their tempdir becomes the child's working directory
    /// unless an explicit [`cwd`](Self::cwd) overrides it, and
    /// [`output_mode`](Self::output_mode) is appended to `args` as
    /// `--<flag>=<mode>`, the same argv edit `run` makes. Nothing
    /// process-global is touched in the *test's* process, so a
    /// `run_process` test needs no `#[serial]`.
    ///
    /// # Panics
    ///
    /// Panics if the harness carries a setting that cannot cross the process
    /// boundary — the terminal detectors
    /// ([`terminal_width`](Self::terminal_width),
    /// [`ambiguous_width`](Self::ambiguous_width),
    /// [`with_color`](Self::with_color) / [`no_color`](Self::no_color)),
    /// [`piped_stdin`](Self::piped_stdin) /
    /// [`interactive_stdin`](Self::interactive_stdin),
    /// [`clipboard`](Self::clipboard), or [`prompts`](Self::prompts). Those
    /// are in-process injection seams: a child process resolves each from
    /// its own environment and inherits none of them. Ignoring them silently
    /// is the trap this method exists to avoid — a test would read as if it
    /// had pinned the width or forced color, and the child would have
    /// answered from the CI machine's terminal instead.
    ///
    /// Also panics if the program cannot be spawned, reporting the program
    /// and the working directory it was spawned in.
    pub fn run_process<I, T>(mut self, program: impl AsRef<OsStr>, args: I) -> ProcessResult
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString>,
    {
        self.reject_in_process_only_settings();
        let (mut command, cwd) = self.prepare_command(program.as_ref(), args);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = command.output().unwrap_or_else(|err| {
            panic!(
                "TestHarness::run_process: failed to spawn {:?} (cwd {:?}): {err}",
                program.as_ref(),
                cwd.as_deref().unwrap_or_else(|| Path::new(".")),
            )
        });

        ProcessResult {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            stdout_bytes: output.stdout,
            stderr_bytes: output.stderr,
            status: output.status,
            _tempdir: self.tempdir.take(),
        }
    }

    /// Runs `program` as a real child process with its stdout on a
    /// pseudo-terminal, and returns what the OS saw.
    ///
    /// [`run_process`](Self::run_process)'s pipe answers "am I a TTY?" with
    /// *no* — which is the right boundary for redirection behavior and the
    /// wrong one for everything that only happens on a terminal. This is the
    /// other answer: the child's stdout is the slave side of a real pty
    /// (opened via `openpty(3)`, 80×24), so `isatty(STDOUT)` is true and the
    /// child's own lazy color detection runs the TTY-positive path from a
    /// real environment. That makes it the one place the suppression
    /// conventions (`NO_COLOR`, `TERM=dumb`) are genuinely observable:
    /// against a color-capable pty baseline, deleting a convention turns a
    /// plain page back into ANSI.
    ///
    /// Everything else matches `run_process`: the same builder settings
    /// carry over, the same in-process-only settings panic, stdin is null,
    /// and stderr stays an ordinary pipe so the streams remain separable.
    /// One pty-specific note: the slave's `ONLCR` post-processing is
    /// disabled, so the recorded bytes are what the child wrote rather than
    /// the line discipline's `\r\n` rewrite of them.
    ///
    /// Unix only: a pty is a Unix object, and the conventions it exists to
    /// observe are Unix terminal conventions.
    #[cfg(unix)]
    pub fn run_pty<I, T>(mut self, program: impl AsRef<OsStr>, args: I) -> ProcessResult
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString>,
    {
        use std::io::Read;

        self.reject_in_process_only_settings();
        let (mut command, cwd) = self.prepare_command(program.as_ref(), args);

        let (master, slave) = pty::open_pair();
        command
            .stdin(Stdio::null())
            .stdout(Stdio::from(slave))
            .stderr(Stdio::piped());

        let mut child = command.spawn().unwrap_or_else(|err| {
            panic!(
                "TestHarness::run_pty: failed to spawn {:?} (cwd {:?}): {err}",
                program.as_ref(),
                cwd.as_deref().unwrap_or_else(|| Path::new(".")),
            )
        });
        // A `Command` is re-spawnable, so it keeps the slave `Stdio` it was
        // given even after `spawn`. Drop it: the child holds the only other
        // slave fd, and reading the master only ends once every slave fd is
        // closed — a retained one deadlocks the read below forever.
        drop(command);

        // Drain stderr on its own thread so a filling pipe can never block
        // the child while this thread is waiting on the master.
        let mut stderr_pipe = child.stderr.take().expect("stderr was piped");
        let stderr_reader = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stderr_pipe.read_to_end(&mut bytes);
            bytes
        });

        // Read the master until the child's slave side closes. Linux
        // reports that as EIO rather than EOF; both mean "no more output",
        // and `read_to_end` keeps what was read before the error.
        let mut stdout_bytes = Vec::new();
        let mut master_read = std::fs::File::from(master);
        if let Err(err) = master_read.read_to_end(&mut stdout_bytes) {
            if err.raw_os_error() != Some(libc::EIO) {
                panic!("TestHarness::run_pty: reading the pty master failed: {err}");
            }
        }

        let status = child
            .wait()
            .expect("TestHarness::run_pty: waiting for the child failed");
        let stderr_bytes = stderr_reader
            .join()
            .expect("TestHarness::run_pty: the stderr reader thread panicked");

        ProcessResult {
            stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
            stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
            stdout_bytes,
            stderr_bytes,
            status,
            _tempdir: self.tempdir.take(),
        }
    }

    /// The setup [`run_process`](Self::run_process) and
    /// [`run_pty`](Self::run_pty) share: fixtures materialized, the child's
    /// working directory resolved, [`output_mode`](Self::output_mode)
    /// appended to argv, and the environment applied. Stdio is the one thing
    /// left to the caller — it is the difference between the two runners.
    fn prepare_command<I, T>(&mut self, program: &OsStr, args: I) -> (Command, Option<PathBuf>)
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString>,
    {
        // Fixtures + working directory, without moving the test's own cwd:
        // the child gets `current_dir`, so parallel tests can't collide.
        if let Some(dir) = self.tempdir.as_ref() {
            for (rel, content) in &self.fixtures {
                let abs = dir.path().join(rel);
                if let Some(parent) = abs.parent() {
                    std::fs::create_dir_all(parent)
                        .expect("TestHarness: failed to create fixture parent dir");
                }
                std::fs::write(&abs, content).expect("TestHarness: failed to write fixture file");
            }
        }
        let cwd = self
            .cwd
            .clone()
            .or_else(|| self.tempdir.as_ref().map(|d| d.path().to_path_buf()));

        let mut argv: Vec<OsString> = args.into_iter().map(|a| a.into()).collect();
        if let Some(mode) = self.output_mode {
            argv.push(format!("--{}={}", self.output_flag_name, output_mode_flag(mode)).into());
        }

        let mut command = Command::new(program);
        command.args(&argv);
        for (key, value) in &self.env_set {
            command.env(key, value);
        }
        for key in &self.env_remove {
            command.env_remove(key);
        }
        if let Some(dir) = cwd.as_ref() {
            command.current_dir(dir);
        }

        (command, cwd)
    }

    /// Panics with a message naming every declared setting that only a
    /// same-process run can honor.
    ///
    /// Reported together rather than one at a time, so a harness that sets
    /// three of them takes one fix instead of three runs.
    #[track_caller]
    fn reject_in_process_only_settings(&self) {
        let mut rejected: Vec<&str> = Vec::new();
        if self.terminal_width.is_some() {
            rejected.push("terminal_width()/no_terminal_width()");
        }
        if self.ambiguous_width.is_some() {
            rejected.push("ambiguous_width()");
        }
        if self.color_capable.is_some() {
            rejected.push("with_color()/no_color()");
        }
        if !matches!(self.stdin, StdinMode::Inherit) {
            rejected.push("piped_stdin()/interactive_stdin()");
        }
        if self.clipboard.is_some() {
            rejected.push("clipboard()");
        }
        if self.prompts.is_some() {
            rejected.push("prompts()");
        }

        if !rejected.is_empty() {
            panic!(
                "TestHarness::run_process cannot honor {}: {} an in-process injection seam, and a \
                 child process resolves it from its own environment. Drop the setting, or express \
                 it in a way the child can see (an environment variable, a fixture file, argv).",
                rejected.join(", "),
                if rejected.len() == 1 {
                    "it is"
                } else {
                    "they are"
                },
            );
        }
    }
}

/// The pty plumbing behind [`TestHarness::run_pty`]: one function that opens
/// a configured master/slave pair, and the only unsafe code in the crate —
/// three C calls (`openpty`, `tcgetattr`, `tcsetattr`) wrapped
/// straight into owned fds.
#[cfg(unix)]
mod pty {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    /// Opens a pty pair sized 80×24 with `ONLCR` off, returning
    /// `(master, slave)`.
    ///
    /// `ONLCR` is the line discipline's `\n` → `\r\n` output rewrite; with
    /// it on, the master would record a translation of the child's bytes
    /// instead of the bytes, and [`ProcessResult`](super::ProcessResult)
    /// promises a recording. The fixed window size keeps width-sensitive
    /// rendering deterministic instead of inheriting a 0×0 window.
    pub(super) fn open_pair() -> (OwnedFd, OwnedFd) {
        let mut master: libc::c_int = -1;
        let mut slave: libc::c_int = -1;
        let mut winsize = libc::winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: openpty only writes the two fd out-parameters and reads
        // the winsize; the name and termios parameters are documented to
        // accept null. The winsize argument is a raw borrow because libc
        // declares `winp` as `*mut` on Apple and `*const` on Linux — `&raw
        // mut` satisfies both, where `&mut` trips Linux clippy's
        // `unnecessary_mut_passed` and `&` fails the Apple build.
        let rc = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &raw mut winsize,
            )
        };
        assert_eq!(
            rc,
            0,
            "TestHarness::run_pty: openpty failed: {}",
            std::io::Error::last_os_error()
        );
        // SAFETY: on success both fds are freshly opened and unowned; these
        // OwnedFds are their sole owners from here on.
        let (master, slave) =
            unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };

        // SAFETY: tcgetattr/tcsetattr read and write the termios
        // out-parameter for an fd this function owns.
        unsafe {
            let mut termios: libc::termios = std::mem::zeroed();
            let rc = libc::tcgetattr(slave.as_raw_fd(), &mut termios);
            assert_eq!(
                rc,
                0,
                "TestHarness::run_pty: tcgetattr failed: {}",
                std::io::Error::last_os_error()
            );
            termios.c_oflag &= !(libc::ONLCR as libc::tcflag_t);
            let rc = libc::tcsetattr(slave.as_raw_fd(), libc::TCSANOW, &termios);
            assert_eq!(
                rc,
                0,
                "TestHarness::run_pty: tcsetattr failed: {}",
                std::io::Error::last_os_error()
            );
        }

        (master, slave)
    }
}

/// What a real child process produced: two streams and a status.
///
/// The companion to [`TestResult`](crate::TestResult) for runs that crossed
/// the process boundary. Where `TestResult` reconstructs the text channels
/// from one captured `RunResult`, every field here is a recording: the bytes
/// the OS carried on each pipe, and the status the kernel reported.
pub struct ProcessResult {
    stdout: String,
    stderr: String,
    stdout_bytes: Vec<u8>,
    stderr_bytes: Vec<u8>,
    status: ExitStatus,
    // Kept alive so fixture files (and anything the child wrote next to
    // them) stay readable while the test inspects the result.
    _tempdir: Option<TempDir>,
}

impl ProcessResult {
    /// Returns the status the kernel reported for the finished child.
    ///
    /// This is [`std::process::ExitStatus`], not the framework's typed
    /// [`ExitStatus`](standout::cli::ExitStatus): it also carries
    /// termination by signal, which is precisely the outcome an in-process
    /// run cannot represent.
    pub fn status(&self) -> ExitStatus {
        self.status
    }

    /// Returns the child's exit code, or `None` when a signal ended it.
    pub fn code(&self) -> Option<i32> {
        self.status.code()
    }

    /// Returns `true` when the child exited with a success status.
    pub fn success(&self) -> bool {
        self.status.success()
    }

    /// Returns what the child wrote to stdout, decoded lossily as UTF-8.
    ///
    /// Bytes that are not valid UTF-8 become the replacement character; when
    /// the output is not text (an artifact written to stdout, say), read
    /// [`stdout_bytes`](Self::stdout_bytes) instead.
    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    /// Returns what the child wrote to stderr, decoded lossily as UTF-8.
    pub fn stderr(&self) -> &str {
        &self.stderr
    }

    /// Returns the exact bytes the child wrote to stdout.
    pub fn stdout_bytes(&self) -> &[u8] {
        &self.stdout_bytes
    }

    /// Returns the exact bytes the child wrote to stderr.
    pub fn stderr_bytes(&self) -> &[u8] {
        &self.stderr_bytes
    }

    /// Returns [`stdout`](Self::stdout) with every ANSI escape removed,
    /// stripped by `console` — the same crate that emits them.
    pub fn stdout_plain(&self) -> String {
        console::strip_ansi_codes(self.stdout()).into_owned()
    }

    /// Returns [`stderr`](Self::stderr) with every ANSI escape removed.
    pub fn stderr_plain(&self) -> String {
        console::strip_ansi_codes(self.stderr()).into_owned()
    }

    /// Returns the fixture tempdir, if the harness allocated one.
    ///
    /// This is where [`fixture`](TestHarness::fixture) files were
    /// materialized. It is also the directory the child ran in — so a file
    /// the child wrote is under this path — *unless* the harness also
    /// declared an explicit [`cwd`](TestHarness::cwd), which wins as the
    /// child's working directory and is where its relative writes land.
    pub fn tempdir(&self) -> Option<&Path> {
        self._tempdir.as_ref().map(TempDir::path)
    }

    // --- assertions ----------------------------------------------------------

    /// Panics unless the child exited successfully, showing both streams.
    #[track_caller]
    pub fn assert_success(&self) {
        if !self.status.success() {
            panic!("{}", self.describe("expected a successful exit"));
        }
    }

    /// Panics unless the child exited with exactly `expected`.
    #[track_caller]
    pub fn assert_exit_code(&self, expected: i32) {
        if self.code() != Some(expected) {
            panic!(
                "{}",
                self.describe(&format!("expected exit code {expected}"))
            );
        }
    }

    /// Panics unless the child's stdout contains `needle`.
    #[track_caller]
    pub fn assert_stdout_contains(&self, needle: &str) {
        if !self.stdout.contains(needle) {
            panic!(
                "{}",
                self.describe(&format!("expected stdout to contain {needle:?}"))
            );
        }
    }

    /// Panics unless the child's stderr contains `needle`.
    #[track_caller]
    pub fn assert_stderr_contains(&self, needle: &str) {
        if !self.stderr.contains(needle) {
            panic!(
                "{}",
                self.describe(&format!("expected stderr to contain {needle:?}"))
            );
        }
    }

    /// Panics unless the child wrote nothing to stdout.
    #[track_caller]
    pub fn assert_stdout_empty(&self) {
        if !self.stdout_bytes.is_empty() {
            panic!("{}", self.describe("expected an empty stdout"));
        }
    }

    /// Panics unless the child wrote nothing to stderr.
    #[track_caller]
    pub fn assert_stderr_empty(&self) {
        if !self.stderr_bytes.is_empty() {
            panic!("{}", self.describe("expected an empty stderr"));
        }
    }

    /// Renders a failure message carrying the whole run, since a failing
    /// process assertion is usually explained by the *other* stream.
    fn describe(&self, expectation: &str) -> String {
        format!(
            "{expectation}, but the process exited with {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}\n--------------",
            self.status, self.stdout, self.stderr,
        )
    }
}
