//! The process escape hatch: [`TestHarness::run_process`] and its
//! [`ProcessResult`].
//!
//! Private, like `snapshot`: the two public items are re-exported from the
//! crate root, and their own docs carry the story — this module is only
//! where the code lives.

use std::ffi::{OsStr, OsString};
use std::path::Path;
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
    ///   "am I a TTY" is exercised as shipped rather than as simulated.
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
    /// (as does an explicit [`cwd`](Self::cwd)), and
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
        T: Into<OsString> + Clone,
    {
        self.reject_in_process_only_settings();

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

        let mut command = Command::new(program.as_ref());
        command
            .args(&argv)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in &self.env_set {
            command.env(key, value);
        }
        for key in &self.env_remove {
            command.env_remove(key);
        }
        if let Some(dir) = cwd.as_ref() {
            command.current_dir(dir);
        }

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
    /// This is where the child ran, so a file it wrote is under this path.
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
