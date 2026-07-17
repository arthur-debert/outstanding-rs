//! In-process test harness for apps built on the `standout` CLI framework.
//!
//! `TestHarness` bundles the scattered injection seams — environment
//! detectors, env vars, working directory, stdin, clipboard, output mode,
//! and tempdir fixtures — into a single fluent builder, and restores every
//! override when the harness is dropped.
//!
//! # Example
//!
//! ```no_run
//! use standout_test::TestHarness;
//! # fn example(app: &standout::cli::App, cmd: clap::Command) {
//! let result = TestHarness::new()
//!     .env("HOME", "/tmp/fake")
//!     .clipboard("pasted content")
//!     .terminal_width(80)
//!     .piped_stdin("extra input\n")
//!     .no_color()
//!     .fixture("notes/todo.txt", "- buy milk\n")
//!     .run(app, cmd, ["myapp", "notes", "list"]);
//!
//! result.assert_success();
//! result.assert_stdout_contains("buy milk");
//! # }
//! ```
//!
//! # Concurrency and restoration
//!
//! The harness mutates process-global state (env vars, cwd, environment
//! detectors, default input readers). Tests that instantiate a
//! `TestHarness` must be annotated `#[serial]` (from the re-exported
//! `serial_test` crate).
//!
//! A `Drop` impl restores every override on both normal exit and panic
//! unwind, with two nuances:
//!
//! - Env vars and cwd are restored to the values captured at `run()` time.
//! - Terminal detectors and default input readers are reset to the
//!   library defaults, not to whatever was installed before `run()`. This
//!   matches the behavior of [`standout_render::DetectorGuard`]. Don't
//!   mix a `TestHarness` with a manually installed detector override on
//!   the same thread.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::Command;
use standout::cli::{
    App, ArtifactDestination, ArtifactRun, ExitStatus, RunErrorKind, RunResult, SuccessKind,
};
use standout_input::env::{MockClipboard, MockStdin};
use standout_input::{
    reset_default_clipboard_reader, reset_default_prompt_responder, reset_default_stdin_reader,
    set_default_clipboard_reader, set_default_prompt_responder, set_default_stdin_reader,
    PromptResponder,
};
use standout_render::{
    reset_environment_detectors, set_ambiguous_width_detector, set_color_capability_detector,
    set_terminal_width_detector, set_tty_detector, AmbiguousWidth, OutputMode,
};
use tempfile::TempDir;

pub use serial_test::serial;

/// How stdin should appear to handlers during the run.
#[derive(Debug, Clone)]
enum StdinMode {
    /// Leave the real-stdin default in place.
    Inherit,
    /// Simulate piped stdin with the given content.
    Piped(String),
    /// Simulate an interactive terminal (no piped input).
    Interactive,
}

/// Fluent builder for in-process CLI tests.
///
/// See the [crate-level docs](crate) for the usage pattern. The harness
/// installs every override in [`TestHarness::run`] and tears them down on
/// [`Drop`], so a failed assertion never leaks state into the next test.
#[must_use = "TestHarness is inert until you call run(...)"]
pub struct TestHarness {
    env_set: HashMap<String, String>,
    env_remove: Vec<String>,
    cwd: Option<PathBuf>,
    tempdir: Option<TempDir>,
    fixtures: Vec<(PathBuf, Vec<u8>)>,
    terminal_width: Option<Option<usize>>,
    ambiguous_width: Option<AmbiguousWidth>,
    is_tty: Option<bool>,
    color_capable: Option<bool>,
    output_mode: Option<OutputMode>,
    output_flag_name: String,
    stdin: StdinMode,
    clipboard: Option<String>,
    prompts: Option<Arc<dyn PromptResponder>>,
}

impl TestHarness {
    /// Creates an empty harness with no overrides applied.
    pub fn new() -> Self {
        Self {
            env_set: HashMap::new(),
            env_remove: Vec::new(),
            cwd: None,
            tempdir: None,
            fixtures: Vec::new(),
            terminal_width: None,
            ambiguous_width: None,
            is_tty: None,
            color_capable: None,
            output_mode: None,
            output_flag_name: "output".to_string(),
            stdin: StdinMode::Inherit,
            clipboard: None,
            prompts: None,
        }
    }

    // --- environment variables ------------------------------------------------

    /// Sets `key=value` as a real environment variable for the duration of
    /// the run. Handlers that use `EnvSource::new` / `std::env::var` will
    /// see it.
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_set.insert(key.into(), value.into());
        self
    }

    /// Removes `key` from the real environment for the duration of the run.
    pub fn env_remove(mut self, key: impl Into<String>) -> Self {
        self.env_remove.push(key.into());
        self
    }

    // --- terminal detectors ---------------------------------------------------

    /// Forces the reported terminal width to `cols`.
    pub fn terminal_width(mut self, cols: usize) -> Self {
        self.terminal_width = Some(Some(cols));
        self
    }

    /// Forces terminal-width detection to report "unknown" (as if stdout
    /// is not a TTY).
    pub fn no_terminal_width(mut self) -> Self {
        self.terminal_width = Some(None);
        self
    }

    /// Forces either narrow or wide treatment of East Asian Ambiguous text.
    pub fn ambiguous_width(mut self, policy: AmbiguousWidth) -> Self {
        self.ambiguous_width = Some(policy);
        self
    }

    /// Claims stdout is attached to a TTY.
    pub fn is_tty(mut self) -> Self {
        self.is_tty = Some(true);
        self
    }

    /// Claims stdout is not a TTY (piped, redirected, …).
    pub fn no_tty(mut self) -> Self {
        self.is_tty = Some(false);
        self
    }

    /// Declares that the output target supports ANSI color.
    pub fn with_color(mut self) -> Self {
        self.color_capable = Some(true);
        self
    }

    /// Declares that the output target does not support ANSI color. When
    /// `--output=auto` is used, this forces the `Text` render path.
    pub fn no_color(mut self) -> Self {
        self.color_capable = Some(false);
        self
    }

    // --- explicit output-mode override ---------------------------------------

    /// Forces a specific [`OutputMode`] regardless of the `--output` flag.
    ///
    /// Internally this injects `--<flag>=<mode>` as the last argument when
    /// [`TestHarness::run`] is called. `<flag>` defaults to `output`;
    /// override it with [`output_flag_name`](Self::output_flag_name) for
    /// apps that renamed the flag via `AppBuilder::output_flag(...)`.
    pub fn output_mode(mut self, mode: OutputMode) -> Self {
        self.output_mode = Some(mode);
        self
    }

    /// Configures the CLI flag name used to force [`output_mode`](Self::output_mode).
    ///
    /// Defaults to `"output"` (matching `AppBuilder`'s default). Change it
    /// if the app under test was built with a renamed flag (e.g.
    /// `AppBuilder::output_flag(Some("format"))`).
    ///
    /// No-op when [`output_mode`](Self::output_mode) isn't set.
    pub fn output_flag_name(mut self, name: impl Into<String>) -> Self {
        self.output_flag_name = name.into();
        self
    }

    /// Shortcut for [`output_mode(OutputMode::Text)`](Self::output_mode).
    pub fn text_output(self) -> Self {
        self.output_mode(OutputMode::Text)
    }

    // --- stdin ----------------------------------------------------------------

    /// Simulates piped stdin with `content`. Handlers using
    /// `StdinSource::new()` will see `is_terminal() == false` and read
    /// `content`.
    pub fn piped_stdin(mut self, content: impl Into<String>) -> Self {
        self.stdin = StdinMode::Piped(content.into());
        self
    }

    /// Simulates an interactive terminal for stdin (no piped content).
    pub fn interactive_stdin(mut self) -> Self {
        self.stdin = StdinMode::Interactive;
        self
    }

    // --- clipboard ------------------------------------------------------------

    /// Installs `content` as the mock clipboard. Handlers using
    /// `ClipboardSource::new()` will read it.
    pub fn clipboard(mut self, content: impl Into<String>) -> Self {
        self.clipboard = Some(content.into());
        self
    }

    // --- interactive prompts --------------------------------------------------

    /// Installs a [`PromptResponder`](standout_input::PromptResponder) that
    /// every `.prompt()` call on a [`standout_input`] interactive source
    /// will route through during the run.
    ///
    /// Use this to test wizard / setup / REPL flows that call
    /// `InquireText::new(...).prompt()`, `InquireSelect::new(...).prompt()`,
    /// etc., without launching real prompts. The
    /// [`ScriptedResponder`](standout_input::ScriptedResponder) bundled with
    /// `standout-input` covers the common case:
    ///
    /// ```ignore
    /// use standout_input::{PromptResponse, ScriptedResponder};
    /// use std::sync::Arc;
    ///
    /// let result = TestHarness::new()
    ///     .prompts(Arc::new(ScriptedResponder::new([
    ///         PromptResponse::text("buy milk"),       // first text prompt
    ///         PromptResponse::Bool(true),             // first confirm
    ///         PromptResponse::Choice(2),              // first select -> options[2]
    ///     ])))
    ///     .run(&app, cmd, ["mycli", "setup"]);
    /// ```
    ///
    /// The responder is installed via
    /// [`set_default_prompt_responder`](standout_input::set_default_prompt_responder)
    /// for the duration of the run and reset on drop, matching the
    /// stdin / clipboard pattern.
    pub fn prompts(mut self, responder: Arc<dyn PromptResponder>) -> Self {
        self.prompts = Some(responder);
        self
    }

    // --- filesystem -----------------------------------------------------------

    /// Sets the working directory for the run to `path`.
    ///
    /// If not set and any [`fixture`](Self::fixture) is declared, the
    /// harness uses the fixture tempdir as the cwd.
    pub fn cwd(mut self, path: impl Into<PathBuf>) -> Self {
        self.cwd = Some(path.into());
        self
    }

    /// Declares a file that should exist at `path` (relative to the
    /// fixture tempdir) with the given text `content`.
    ///
    /// The first call to `fixture` creates a fresh `tempfile::TempDir`
    /// which becomes the default cwd. Access it via [`tempdir`](Self::tempdir).
    ///
    /// # Panics
    ///
    /// Panics if `path` is absolute or contains a `..` component — both
    /// would let the fixture escape the harness-owned tempdir and
    /// potentially clobber files in the user's real filesystem.
    pub fn fixture(mut self, path: impl AsRef<Path>, content: impl Into<String>) -> Self {
        let path = validate_fixture_path(path.as_ref());
        self.fixtures.push((path, content.into().into_bytes()));
        self.ensure_tempdir();
        self
    }

    /// Declares a binary fixture file. Same as [`fixture`](Self::fixture)
    /// but takes raw bytes. Applies the same path validation.
    pub fn fixture_bytes(mut self, path: impl AsRef<Path>, content: impl Into<Vec<u8>>) -> Self {
        let path = validate_fixture_path(path.as_ref());
        self.fixtures.push((path, content.into()));
        self.ensure_tempdir();
        self
    }

    /// Returns the fixture tempdir path if one has been allocated.
    ///
    /// Useful for constructing absolute paths to pass as handler arguments.
    pub fn tempdir(&self) -> Option<&Path> {
        self.tempdir.as_ref().map(|t| t.path())
    }

    fn ensure_tempdir(&mut self) {
        if self.tempdir.is_none() {
            self.tempdir =
                Some(TempDir::new().expect("TestHarness: failed to create tempdir for fixtures"));
        }
    }

    // --- execution ------------------------------------------------------------

    /// Installs every override, runs `app` with the given `cmd` definition
    /// and argv, and returns a [`TestResult`].
    ///
    /// Overrides are torn down when the returned guard held inside the
    /// `TestResult` is dropped. The `TestResult` and the harness share the
    /// same lifetime, so a typical test binds the result and lets it fall
    /// out of scope at the end.
    pub fn run<I, T>(mut self, app: &App, cmd: Command, args: I) -> TestResult
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        // 1. Materialize fixtures + cwd.
        let mut restore = RestoreState::default();

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

        let cwd_target = self
            .cwd
            .clone()
            .or_else(|| self.tempdir.as_ref().map(|d| d.path().to_path_buf()));
        if let Some(target) = cwd_target {
            restore.original_cwd = std::env::current_dir().ok();
            std::env::set_current_dir(&target)
                .expect("TestHarness: failed to change working directory");
        }

        // 2. Env vars. Save originals so we can restore even on panic.
        // Record each original only once per key (before any mutation), so
        // if the same key appears in both env_set and env_remove, or is
        // listed multiple times, restore brings back the true original.
        // Precedence: set is applied first, then remove — so removal wins
        // when both are requested for the same key.
        for (k, v) in &self.env_set {
            restore
                .env_originals
                .entry(k.clone())
                .or_insert_with(|| std::env::var(k).ok());
            std::env::set_var(k, v);
        }
        for k in &self.env_remove {
            restore
                .env_originals
                .entry(k.clone())
                .or_insert_with(|| std::env::var(k).ok());
            std::env::remove_var(k);
        }

        // 3. Environment detectors.
        if let Some(w) = self.terminal_width {
            static WIDTH_SLOT: std::sync::OnceLock<std::sync::Mutex<Option<usize>>> =
                std::sync::OnceLock::new();
            let slot = WIDTH_SLOT.get_or_init(|| std::sync::Mutex::new(None));
            *slot.lock().unwrap() = w;
            set_terminal_width_detector(|| {
                *WIDTH_SLOT
                    .get()
                    .expect("width slot initialized above")
                    .lock()
                    .unwrap()
            });
            restore.reset_env_detectors = true;
        }
        if let Some(policy) = self.ambiguous_width {
            static POLICY_SLOT: std::sync::OnceLock<std::sync::Mutex<Option<AmbiguousWidth>>> =
                std::sync::OnceLock::new();
            let slot = POLICY_SLOT.get_or_init(|| std::sync::Mutex::new(None));
            *slot.lock().unwrap() = Some(policy);
            set_ambiguous_width_detector(|| {
                *POLICY_SLOT
                    .get()
                    .expect("ambiguous width slot initialized above")
                    .lock()
                    .unwrap()
            });
            restore.reset_env_detectors = true;
        }
        if let Some(flag) = self.is_tty {
            static TTY_SLOT: std::sync::OnceLock<std::sync::Mutex<bool>> =
                std::sync::OnceLock::new();
            let slot = TTY_SLOT.get_or_init(|| std::sync::Mutex::new(false));
            *slot.lock().unwrap() = flag;
            set_tty_detector(|| {
                *TTY_SLOT
                    .get()
                    .expect("tty slot initialized above")
                    .lock()
                    .unwrap()
            });
            restore.reset_env_detectors = true;
        }
        if let Some(flag) = self.color_capable {
            static COLOR_SLOT: std::sync::OnceLock<std::sync::Mutex<bool>> =
                std::sync::OnceLock::new();
            let slot = COLOR_SLOT.get_or_init(|| std::sync::Mutex::new(false));
            *slot.lock().unwrap() = flag;
            set_color_capability_detector(|| {
                *COLOR_SLOT
                    .get()
                    .expect("color slot initialized above")
                    .lock()
                    .unwrap()
            });
            restore.reset_env_detectors = true;
        }

        // 4. Stdin / clipboard overrides.
        match std::mem::replace(&mut self.stdin, StdinMode::Inherit) {
            StdinMode::Inherit => {}
            StdinMode::Piped(content) => {
                set_default_stdin_reader(Arc::new(MockStdin::piped(content)));
                restore.reset_stdin = true;
            }
            StdinMode::Interactive => {
                set_default_stdin_reader(Arc::new(MockStdin::terminal()));
                restore.reset_stdin = true;
            }
        }
        if let Some(content) = self.clipboard.take() {
            set_default_clipboard_reader(Arc::new(MockClipboard::with_content(content)));
            restore.reset_clipboard = true;
        }
        if let Some(responder) = self.prompts.take() {
            set_default_prompt_responder(responder);
            restore.reset_prompts = true;
        }

        // 5. Argv: append --<flag>=<mode> if an output mode was forced.
        let mut argv: Vec<OsString> = args.into_iter().map(|a| a.into()).collect();
        if let Some(mode) = self.output_mode {
            argv.push(format!("--{}={}", self.output_flag_name, output_mode_flag(mode)).into());
        }

        let outcome = app.run_to_string(cmd, argv);

        // `self` (and its tempdir) move into TestResult so the fixture dir
        // survives until the test is finished with the result.
        TestResult {
            outcome,
            _tempdir: self.tempdir.take(),
            _restore: restore,
        }
    }
}

impl Default for TestHarness {
    fn default() -> Self {
        Self::new()
    }
}

fn validate_fixture_path(path: &Path) -> PathBuf {
    use std::path::Component;
    if path.is_absolute() {
        panic!(
            "TestHarness::fixture: path {:?} is absolute; only relative paths are allowed so \
             the fixture is confined to the harness tempdir",
            path
        );
    }
    for component in path.components() {
        match component {
            Component::ParentDir => panic!(
                "TestHarness::fixture: path {:?} contains a `..` component; only relative \
                 paths that stay inside the tempdir are allowed",
                path
            ),
            Component::Prefix(_) | Component::RootDir => panic!(
                "TestHarness::fixture: path {:?} has a root or prefix component; only \
                 relative paths inside the tempdir are allowed",
                path
            ),
            _ => {}
        }
    }
    path.to_path_buf()
}

fn output_mode_flag(mode: OutputMode) -> &'static str {
    match mode {
        OutputMode::Auto => "auto",
        OutputMode::Term => "term",
        OutputMode::Text => "text",
        OutputMode::TermDebug => "term-debug",
        OutputMode::Json => "json",
        OutputMode::Yaml => "yaml",
        OutputMode::Xml => "xml",
        OutputMode::Csv => "csv",
    }
}

/// Restores process-global state when dropped.
///
/// The harness hands ownership of this to the [`TestResult`] so restoration
/// runs after the test has finished consuming the result (and on panic).
#[derive(Default)]
struct RestoreState {
    env_originals: HashMap<String, Option<String>>,
    original_cwd: Option<PathBuf>,
    reset_env_detectors: bool,
    reset_stdin: bool,
    reset_clipboard: bool,
    reset_prompts: bool,
}

impl Drop for RestoreState {
    fn drop(&mut self) {
        for (k, original) in self.env_originals.drain() {
            match original {
                Some(v) => std::env::set_var(&k, v),
                None => std::env::remove_var(&k),
            }
        }
        if let Some(cwd) = self.original_cwd.take() {
            let _ = std::env::set_current_dir(cwd);
        }
        if self.reset_env_detectors {
            reset_environment_detectors();
        }
        if self.reset_stdin {
            reset_default_stdin_reader();
        }
        if self.reset_clipboard {
            reset_default_clipboard_reader();
        }
        if self.reset_prompts {
            reset_default_prompt_responder();
        }
    }
}

/// Outcome of a [`TestHarness::run`] invocation.
///
/// Holds the raw [`RunResult`] produced by the app, plus convenience
/// accessors and assertion helpers oriented at text output.
pub struct TestResult {
    outcome: RunResult,
    // Kept alive so fixture files remain readable while the test inspects
    // the result; dropped after restore state is torn down.
    _tempdir: Option<TempDir>,
    _restore: RestoreState,
}

impl TestResult {
    /// Returns the raw [`RunResult`] for cases where the structured
    /// accessors aren't enough.
    pub fn outcome(&self) -> &RunResult {
        &self.outcome
    }

    /// Returns the completed run's typed shell status.
    ///
    /// A no-match handoff has no framework status and returns `None`.
    pub fn exit_status(&self) -> Option<ExitStatus> {
        self.outcome.exit_status()
    }

    /// Returns the typed success origin for handled/help/version output.
    pub fn success_kind(&self) -> Option<SuccessKind> {
        self.outcome.success_kind()
    }

    /// Returns the typed error origin, if execution failed.
    pub fn error_kind(&self) -> Option<RunErrorKind> {
        self.outcome.error_kind()
    }

    /// Returns the rendered text output, or `""` for `Silent` / `Binary` /
    /// `NoMatch`.
    pub fn stdout(&self) -> &str {
        match &self.outcome {
            RunResult::Handled(s) => s.as_str(),
            _ => "",
        }
    }

    /// Returns `true` if the run produced text output.
    pub fn is_handled(&self) -> bool {
        matches!(self.outcome, RunResult::Handled(_))
    }

    /// Returns `true` if no handler matched the argv.
    pub fn is_no_match(&self) -> bool {
        matches!(self.outcome, RunResult::NoMatch(_))
    }

    /// If the run produced binary output, returns the bytes and suggested
    /// filename.
    pub fn binary(&self) -> Option<(&[u8], &str)> {
        match &self.outcome {
            RunResult::Binary(bytes, filename) => Some((bytes.as_slice(), filename.as_str())),
            _ => None,
        }
    }

    /// If the run completed a compound artifact, returns it.
    ///
    /// The [`ArtifactRun`] carries everything worth asserting: the bytes, what
    /// the application suggested, the receipt naming where the framework
    /// actually wrote, and the report rendered after that write.
    pub fn artifact(&self) -> Option<&ArtifactRun> {
        self.outcome.artifact()
    }

    /// Returns the artifact bytes, or `None` if the run produced no artifact.
    pub fn artifact_bytes(&self) -> Option<&[u8]> {
        self.artifact().map(ArtifactRun::bytes)
    }

    /// Returns the destination the framework selected and wrote to.
    pub fn artifact_destination(&self) -> Option<&ArtifactDestination> {
        self.artifact().map(ArtifactRun::destination)
    }

    /// Returns the rendered artifact report.
    ///
    /// This is the report as the user would see it: for a file destination on
    /// stdout, for a stdout destination on stderr.
    pub fn artifact_report(&self) -> Option<&str> {
        self.artifact().and_then(ArtifactRun::report)
    }

    /// Panics unless the run completed an artifact, returning it.
    #[track_caller]
    pub fn expect_artifact(&self) -> &ArtifactRun {
        match self.artifact() {
            Some(run) => run,
            None => panic!(
                "expected a completed artifact, got: {:?}",
                describe_outcome(&self.outcome)
            ),
        }
    }

    /// Panics unless the artifact bytes are exactly `expected`.
    ///
    /// Artifact bytes must survive the framework byte-for-byte; this is the
    /// assertion that says so.
    #[track_caller]
    pub fn assert_artifact_bytes(&self, expected: &[u8]) {
        let actual = self.expect_artifact().bytes();
        if actual != expected {
            panic!(
                "artifact bytes mismatch\n--- expected ({} bytes) ---\n{:?}\n--- actual ({} bytes) ---\n{:?}",
                expected.len(),
                expected,
                actual.len(),
                actual
            );
        }
    }

    /// Panics unless the application suggested `expected` as the destination.
    #[track_caller]
    pub fn assert_artifact_suggested_destination(&self, expected: impl AsRef<Path>) {
        let actual = self.expect_artifact().suggested_destination();
        assert_eq!(
            actual,
            Some(expected.as_ref()),
            "unexpected suggested artifact destination"
        );
    }

    /// Panics unless the framework wrote the artifact to `expected`.
    #[track_caller]
    pub fn assert_artifact_written_to(&self, expected: impl AsRef<Path>) {
        let receipt = self.expect_artifact().receipt();
        assert_eq!(
            receipt.path(),
            Some(expected.as_ref()),
            "unexpected artifact destination"
        );
    }

    /// Panics unless the framework routed the artifact bytes to stdout.
    #[track_caller]
    pub fn assert_artifact_to_stdout(&self) {
        let receipt = self.expect_artifact().receipt();
        assert!(
            receipt.is_stdout(),
            "expected the artifact to go to stdout, but it went to {}",
            receipt.destination().label()
        );
    }

    /// Panics unless the rendered artifact report contains `needle`.
    #[track_caller]
    pub fn assert_artifact_report_contains(&self, needle: &str) {
        match self.expect_artifact().report() {
            Some(report) if report.contains(needle) => {}
            Some(report) => panic!(
                "artifact report did not contain {:?}\n--- report ---\n{}\n--------------",
                needle, report
            ),
            None => panic!("expected an artifact report, but the artifact carried none"),
        }
    }

    // --- assertions ----------------------------------------------------------

    /// Panics unless the run ended in a successful dispatch
    /// (`RunResult::Handled`, `RunResult::Silent`, `RunResult::Binary`, or
    /// `RunResult::Artifact`). `RunResult::NoMatch` and `RunResult::Error`
    /// trigger a panic.
    #[track_caller]
    pub fn assert_success(&self) {
        match &self.outcome {
            RunResult::Handled(_)
            | RunResult::Silent
            | RunResult::Binary(_, _)
            | RunResult::Artifact(_) => {}
            RunResult::NoMatch(_) => {
                panic!("expected successful dispatch but no handler matched; stdout was empty")
            }
            RunResult::Error(msg) => {
                panic!("expected successful dispatch, got error: {}", msg)
            }
            _ => panic!(
                "expected successful dispatch, got: {:?}",
                describe_outcome(&self.outcome)
            ),
        }
    }

    /// Panics unless the run exposes `expected` as its shell status.
    #[track_caller]
    pub fn assert_exit_status(&self, expected: ExitStatus) {
        assert_eq!(
            self.exit_status(),
            Some(expected),
            "unexpected exit status for {}",
            describe_outcome(&self.outcome)
        );
    }

    /// Panics unless the run exposes the expected typed error origin.
    #[track_caller]
    pub fn assert_error_kind(&self, expected: RunErrorKind) {
        assert_eq!(
            self.error_kind(),
            Some(expected),
            "unexpected error kind for {}",
            describe_outcome(&self.outcome)
        );
    }

    /// Returns `true` if the run produced an error.
    pub fn is_error(&self) -> bool {
        matches!(self.outcome, RunResult::Error(_))
    }

    /// Returns the error message if the run produced one, or `None`.
    pub fn error(&self) -> Option<&str> {
        match &self.outcome {
            RunResult::Error(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Panics unless the run ended in `RunResult::Error`.
    #[track_caller]
    pub fn assert_error(&self) {
        if !self.is_error() {
            panic!(
                "expected RunResult::Error, got: {:?}",
                describe_outcome(&self.outcome)
            );
        }
    }

    /// Panics unless the run ended in `RunResult::Error` and the message
    /// contains `needle`.
    #[track_caller]
    pub fn assert_error_contains(&self, needle: &str) {
        match self.error() {
            Some(msg) if msg.contains(needle) => {}
            Some(msg) => panic!(
                "error did not contain {:?}\n--- error ---\n{}\n-------------",
                needle, msg
            ),
            None => panic!(
                "expected RunResult::Error, got: {:?}",
                describe_outcome(&self.outcome)
            ),
        }
    }

    /// Panics unless the run ended in `RunResult::NoMatch`.
    #[track_caller]
    pub fn assert_no_match(&self) {
        if !self.is_no_match() {
            panic!(
                "expected no handler match, got: {:?}",
                describe_outcome(&self.outcome)
            );
        }
    }

    /// Panics unless [`stdout`](Self::stdout) contains `needle`.
    #[track_caller]
    pub fn assert_stdout_contains(&self, needle: &str) {
        let out = self.stdout();
        if !out.contains(needle) {
            panic!(
                "stdout did not contain {:?}\n--- stdout ---\n{}\n--------------",
                needle, out
            );
        }
    }

    /// Panics unless [`stdout`](Self::stdout) equals `expected` exactly.
    #[track_caller]
    pub fn assert_stdout_eq(&self, expected: &str) {
        let out = self.stdout();
        if out != expected {
            panic!(
                "stdout mismatch\n--- expected ---\n{}\n--- actual -----\n{}\n----------------",
                expected, out
            );
        }
    }
}

fn describe_outcome(o: &RunResult) -> String {
    match o {
        RunResult::Handled(s) => format!("Handled({:?})", s),
        RunResult::Silent => "Silent".into(),
        RunResult::Binary(b, f) => format!("Binary(len={}, {:?})", b.len(), f),
        RunResult::Artifact(run) => format!(
            "Artifact(len={}, destination={:?})",
            run.bytes().len(),
            run.destination().label()
        ),
        RunResult::Error(s) => format!("Error({:?})", s),
        RunResult::NoMatch(_) => "NoMatch".into(),
        _ => "Unknown".into(),
    }
}
