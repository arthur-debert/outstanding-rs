use crate::{output_mode_flag, StdinMode, TestHarness};
use standout::cli::Diagnostic;
use standout_render::OutputMode;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use tempfile::TempDir;
impl TestHarness {
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
    #[cfg(unix)]
    pub fn run_pty<I, T>(mut self, program: impl AsRef<OsStr>, args: I) -> ProcessResult
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString>,
    {
        use std::io::Read;
        self.reject_in_process_only_settings();
        let (mut command, cwd) = self.prepare_command(program.as_ref(), args);
        let (master, slave) = crate::pty::open_pair()
            .unwrap_or_else(|err| panic!("TestHarness::run_pty: opening pty failed: {err}"));
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
        drop(command);
        let mut stderr_pipe = child.stderr.take().expect("stderr was piped");
        let stderr_reader = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stderr_pipe.read_to_end(&mut bytes);
            bytes
        });
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
    fn prepare_command<I, T>(&mut self, program: &OsStr, args: I) -> (Command, Option<PathBuf>)
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString>,
    {
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
        let cwd = self.resolve_cwd();
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
pub struct ProcessResult {
    stdout: String,
    stderr: String,
    stdout_bytes: Vec<u8>,
    stderr_bytes: Vec<u8>,
    status: ExitStatus,
    _tempdir: Option<TempDir>,
}
impl ProcessResult {
    pub fn status(&self) -> ExitStatus {
        self.status
    }
    pub fn code(&self) -> Option<i32> {
        self.status.code()
    }
    pub fn success(&self) -> bool {
        self.status.success()
    }
    pub fn stdout(&self) -> &str {
        &self.stdout
    }
    pub fn stderr(&self) -> &str {
        &self.stderr
    }
    pub fn stdout_bytes(&self) -> &[u8] {
        &self.stdout_bytes
    }
    pub fn stderr_bytes(&self) -> &[u8] {
        &self.stderr_bytes
    }
    pub fn stdout_plain(&self) -> String {
        console::strip_ansi_codes(self.stdout()).into_owned()
    }
    pub fn stderr_plain(&self) -> String {
        console::strip_ansi_codes(self.stderr()).into_owned()
    }
    pub fn tempdir(&self) -> Option<&Path> {
        self._tempdir.as_ref().map(TempDir::path)
    }
    /// A process result carries no resolved mode, so the caller names the one it asked for.
    pub fn diagnostic(&self, output_mode: OutputMode) -> Option<Diagnostic> {
        standout::cli::parse_diagnostic(output_mode, &self.stdout).ok()
    }
    #[track_caller]
    pub fn assert_success(&self) {
        if !self.status.success() {
            panic!("{}", self.describe("expected a successful exit"));
        }
    }
    #[track_caller]
    pub fn assert_exit_code(&self, expected: i32) {
        if self.code() != Some(expected) {
            panic!(
                "{}",
                self.describe(&format!("expected exit code {expected}"))
            );
        }
    }
    #[track_caller]
    pub fn assert_stdout_contains(&self, needle: &str) {
        if !self.stdout.contains(needle) {
            panic!(
                "{}",
                self.describe(&format!("expected stdout to contain {needle:?}"))
            );
        }
    }
    #[track_caller]
    pub fn assert_stderr_contains(&self, needle: &str) {
        if !self.stderr.contains(needle) {
            panic!(
                "{}",
                self.describe(&format!("expected stderr to contain {needle:?}"))
            );
        }
    }
    #[track_caller]
    pub fn assert_stdout_empty(&self) {
        if !self.stdout_bytes.is_empty() {
            panic!("{}", self.describe("expected an empty stdout"));
        }
    }
    #[track_caller]
    pub fn assert_stderr_empty(&self) {
        if !self.stderr_bytes.is_empty() {
            panic!("{}", self.describe("expected an empty stderr"));
        }
    }
    fn describe(&self, expectation: &str) -> String {
        format!(
            "{expectation}, but the process exited with {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}\n--------------",
            self.status, self.stdout, self.stderr,
        )
    }
}
