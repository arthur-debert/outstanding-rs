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
//! # Observing a run
//!
//! [`TestResult`] exposes what the run put on each *text* stream, not just the
//! captured outcome: [`stdout`](TestResult::stdout) and
//! [`stderr`](TestResult::stderr) are separate channels, each reconstructing
//! what `App::run`'s writer seam would have sent there,
//! [`stdout_plain`](TestResult::stdout_plain) is stdout with the ANSI escapes
//! stripped, and [`assert_page_snapshot!`] pins a whole rendered page under a
//! name derived from its [`SnapshotCase`]. A whole-page snapshot is what makes
//! an *extra* wrong line fail a test — the thing a list of `contains`
//! assertions cannot do. To pin a page across every (output mode × color ×
//! theme) cell rather than in one hand-picked configuration, [`matrix`]
//! yields the cells and each [`MatrixCell`] configures its own harness and
//! names its own snapshot.
//!
//! Two things those accessors deliberately leave out, because they are not
//! text: binary output and artifact bytes, which stay on
//! [`binary`](TestResult::binary) and
//! [`artifact_bytes`](TestResult::artifact_bytes). Each accessor's docs say
//! precisely what it does and does not model.
//!
//! Not everything worth asserting is text at all.
//! [`tag_resolutions`](TestResult::tag_resolutions) carries what the run's
//! style-tag passes resolved — which tags the theme could not define, under
//! which transform and unknown-tag policy — so "this page is corrupt" is a
//! fact about structured data in every output mode, rather than a search for
//! the `[tag?]` marker in the one mode that emits it.
//!
//! # Invariants
//!
//! [`invariants`] holds the universal and negative assertions the existential
//! `contains` style cannot state: no unresolved tag, no possible-values row on
//! a valueless argument, a metavar on every value-taking one, styling that
//! changes no layout, aligned description columns. Each names the offending
//! element when it fails.
//!
//! # Crossing the process boundary
//!
//! [`TestHarness::run_process`] runs a real binary instead of calling into
//! the app in-process, and returns the [`ProcessResult`] the OS produced:
//! two pipes and an exit status. It is for the evidence an in-process run
//! cannot give — stream separation as the program performed it, a real exit
//! status, behavior that keys off stdout not being a terminal — and it is a
//! fork per call, so it is the exception, not the default.
//!
//! The harness does not simulate a TTY in-process. It once offered
//! `is_tty()`/`no_tty()`, which drove a detector nothing consulted; the seam
//! is deleted (see `docs/adr/0022-delete-the-in-process-tty-seam.md`) and
//! terminal-shaped questions belong to `run_process`. What the harness does
//! control is *color*: [`with_color`](TestHarness::with_color) opens both
//! gates between a styled template and ANSI bytes, so an ANSI-positive
//! assertion works in-process.
//!
//! # Concurrency and restoration
//!
//! [`TestHarness::run`] mutates process-global state (env vars, cwd,
//! environment detectors, `console`'s color switch, default input readers).
//! Tests that call it must be annotated `#[serial]` (from the re-exported
//! `serial_test` crate). [`TestHarness::run_process`] mutates none of it —
//! the child gets its own environment — so a process test needs no
//! annotation.
//!
//! A `Drop` impl restores every override on both normal exit and panic
//! unwind, with two nuances:
//!
//! - Env vars, cwd, and `console`'s color switch are restored to the values
//!   captured at `run()` time.
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
    set_terminal_width_detector, AmbiguousWidth, OutputMode,
};
use tempfile::TempDir;

pub mod clap_parity;
pub mod invariants;
mod matrix;
mod page;
mod process;
mod snapshot;

pub use matrix::{matrix, MatrixCell};
pub use process::ProcessResult;
pub use serial_test::serial;
pub use snapshot::SnapshotCase;
pub use standout_render::TagResolution;

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
    ///
    /// This does **not** reach variables a dependency read before the run
    /// started. `console` initializes its color globals lazily from `CLICOLOR`
    /// / `CLICOLOR_FORCE`, and **every** run latches them before applying this
    /// map (see [`with_color`](Self::with_color)); `NO_COLOR` and
    /// `TERM=dumb` land the same way. Setting them here changes what handler
    /// code reads, not what `console` decided — pin those conventions with
    /// [`run_process`](Self::run_process), where the child does its own
    /// initialization against the real environment.
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

    /// Declares that the output target supports ANSI color, at both of the
    /// gates that decide whether escapes reach the output.
    ///
    /// Two independent switches stand between a styled template and ANSI
    /// bytes, and opening one alone produces nothing:
    ///
    /// 1. Standout's own color decision — [`OutputMode::Auto`] consults
    ///    `detect_color_capability()` to choose between applying and
    ///    stripping style tags. This is the harness's
    ///    [`set_color_capability_detector`] override.
    /// 2. `console`'s process-global color switch, which
    ///    `Style::apply_to` reads before emitting any escape. It is off in
    ///    a non-TTY process, and a test binary is never a TTY — so a
    ///    styled `Term` render in a test emits plain text unless this is
    ///    turned on.
    ///
    /// Gate 2 is what made "ANSI-positive" untestable in-process before:
    /// forcing gate 1 open left gate 2 shut and the render silent. So
    /// `with_color()` sets both, and the original value of `console`'s
    /// switch is restored on drop with every other override.
    ///
    /// This is test-only. No production code path sets `console`'s switch;
    /// the harness sets it to model the terminal the test claims to be
    /// writing to.
    ///
    /// # The harness owns gate 2, so in-process env cannot reach it
    ///
    /// `console` initializes its process-globals lazily, on the first *read*,
    /// from `CLICOLOR` / `CLICOLOR_FORCE` (and `COLORTERM`, for the true-color
    /// pair). **Every** run reads them before applying [`env`](Self::env) /
    /// [`env_remove`](Self::env_remove) — including runs that set no color
    /// knob, since the first read in a binary can just as easily come from
    /// app code under test — so the one initialization is always spent on the
    /// real environment. A run that also *writes* the switch restores the
    /// pre-run value on drop.
    ///
    /// The consequence: by the time a run applies environment variables,
    /// `console` has already initialized, and a test's `.env("CLICOLOR_FORCE",
    /// …)` no longer influences it. Environment conventions that `console`
    /// reads — `NO_COLOR`, `TERM=dumb`, `CLICOLOR_FORCE` — are therefore only
    /// observable in a child process; assert them through
    /// [`run_process`](Self::run_process), not here. See
    /// `docs/adr/0022-delete-the-in-process-tty-seam.md`.
    pub fn with_color(mut self) -> Self {
        self.color_capable = Some(true);
        self
    }

    /// Declares that the output target does not support ANSI color, at both
    /// gates [`with_color`](Self::with_color) describes. When `--output=auto`
    /// is used, this forces the `Text` render path.
    ///
    /// A test process already has `console`'s switch off, so this changes
    /// nothing for the common case; it exists so the two methods stay exact
    /// opposites, and so a run cannot inherit an earlier `set_colors_enabled(true)`
    /// from the same test binary.
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
        T: Into<OsString>,
    {
        // 0. Read every pre-run value this run is about to overwrite, before
        // it can overwrite any of them.
        let mut restore = RestoreState::default();

        // `console`'s four color decisions are process-globals that
        // initialize *lazily*, on the first read in the process, from the
        // environment (`CLICOLOR` / `CLICOLOR_FORCE` for the color switches,
        // `COLORTERM` by way of `Term::features()` for the true-color ones).
        // Whichever run causes that first read decides them for the whole
        // test binary — so a read taken after step 2 installed this run's
        // `.env` overrides would latch a value this run itself caused
        // (`.env("CLICOLOR_FORCE", "1")` latching `true`) for every later
        // test. Reading them all here, before any override exists, spends the
        // one initialization on the real environment.
        //
        // Unconditional on purpose: the first read need not be the harness's.
        // A run with no color knob at all still installs `.env`, and any code
        // it runs that styles output reads these globals — so gating this on
        // `color_capable` would leave exactly that case initializing `console`
        // from the harness's temporary environment, with nothing recorded to
        // restore.
        let ambient_console_colors = console::colors_enabled();
        let _ = console::colors_enabled_stderr();
        let _ = console::true_colors_enabled();
        let _ = console::true_colors_enabled_stderr();

        // Only the stdout switch is ever written (step 3, by `with_color` /
        // `no_color`), so it is the only one with anything to restore. The
        // other three are initialized above and then left alone.
        if self.color_capable.is_some() {
            restore.console_colors = Some(ambient_console_colors);
        }

        // 1. Materialize fixtures + cwd.
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

            // Gate 2: `console`'s process-global switch, which
            // `Style::apply_to` reads before emitting an escape. Standout's
            // detector alone cannot produce ANSI in a non-TTY test process.
            // Its pre-run value was captured in step 0, before this run's
            // environment existed.
            console::set_colors_enabled(flag);
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
        let warnings = standout_render::warnings::take_captured_warnings();
        let tag_resolutions = standout_render::diagnostics::take_captured();

        // `self` (and its tempdir) move into TestResult so the fixture dir
        // survives until the test is finished with the result.
        TestResult {
            stdout: render_stdout(&outcome),
            stderr: render_stderr(&outcome, &warnings),
            outcome,
            warnings,
            tag_resolutions,
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

/// Reconstructs the text the framework's standard-output channel would have
/// carried.
///
/// `run_to_string` captures one outcome; the split into stdout and stderr is
/// made later, by `App::run`'s writer seam. This mirrors that seam's stdout
/// side so stream routing is assertable in-process:
///
/// - handled text is the rendered output itself;
/// - an artifact written to a file leaves stdout free, so its report goes
///   here, newline-terminated (an artifact that claimed stdout owns those
///   bytes and reports on stderr instead);
/// - every other outcome leaves stdout empty of *text*. Binary bytes are the
///   deliberate omission: they are not text and stay on
///   [`TestResult::binary`](TestResult::binary).
///
/// One departure from the seam: handled text is returned exactly as captured,
/// without the newline `App::run` writes after it. That is the shape
/// `stdout()` has always had and every existing assertion is written against;
/// modeling the terminator would be a suite-wide rewrite for no oracle this
/// slice needs.
fn render_stdout(outcome: &RunResult) -> String {
    match outcome {
        RunResult::Handled(text) => text.as_str().to_string(),
        RunResult::Artifact(run) if !run.destination().is_stdout() => report_line(run),
        _ => String::new(),
    }
}

/// Reconstructs the bytes the framework's error channel would have carried.
///
/// The companion to [`render_stdout`], mirroring the same writer seam:
///
/// - an error diagnostic goes to stderr, newline-terminated — except an
///   application-declared external failure, which the framework passes through
///   verbatim so the wrapped tool's own diagnostic is not reshaped;
/// - an artifact written to stdout owns stdout, so its report goes to stderr
///   (an artifact written to a file leaves stdout free and reports there);
/// - framework warnings close the channel, in the block `App::run` flushes
///   after the primary output — blank line, banner, one tab-indented line per
///   warning. The layout is `standout_render`'s own
///   [`render_block_plain`](standout_render::warnings::render_block_plain), so
///   the harness cannot drift from what the CLI layer writes. Only the styling
///   is not modeled: `run` themes that block according to the real stderr's
///   color capability, which an in-process run has no equivalent of, so the
///   block appears here as `stderr_plain` would show it. The warnings also
///   remain individually addressable on
///   [`TestResult::warnings`](TestResult::warnings).
fn render_stderr(outcome: &RunResult, warnings: &[String]) -> String {
    let primary = match outcome {
        RunResult::Error(error) if error.kind() == RunErrorKind::External => error.to_string(),
        RunResult::Error(error) => format!("{}\n", error),
        RunResult::Artifact(run) if run.destination().is_stdout() => report_line(run),
        _ => String::new(),
    };

    primary + &standout_render::warnings::render_block_plain(warnings)
}

/// Returns an artifact's report as the framework writes it — newline-terminated
/// — or `""` when there is no report to write.
fn report_line(run: &ArtifactRun) -> String {
    run.report()
        .filter(|report| !report.is_empty())
        .map(|report| format!("{}\n", report))
        .unwrap_or_default()
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
    /// `console`'s process-global color switch as it read *before* `run()`
    /// applied any of this run's overrides — read that early, on every run,
    /// because the switch initializes lazily from the environment the run is
    /// about to change. Recorded here only when the run also *writes* the
    /// switch (`with_color` / `no_color`); a run that merely forced the
    /// initialization has nothing to put back. Restoring writes the *value*
    /// back — the switch has no "return to auto-detection" setter — which in
    /// a test process is the same thing, since detection there is a constant.
    console_colors: Option<bool>,
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
        if let Some(enabled) = self.console_colors.take() {
            console::set_colors_enabled(enabled);
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
    stdout: String,
    stderr: String,
    warnings: Vec<String>,
    tag_resolutions: Vec<TagResolution>,
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

    /// Returns framework warnings captured during the run, one per entry.
    ///
    /// These are the warnings `App::run` renders to stderr after the primary
    /// output; [`stderr`](Self::stderr) carries that rendered block, while this
    /// accessor keeps each message addressable without parsing it back out.
    /// The harness drains and stores them per run so warning assertions are
    /// deterministic and cannot leak into a later test on the same thread.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Returns what each style-tag pass of the run resolved, in render order.
    ///
    /// A page's tags are resolved against the theme in a second pass after the
    /// template renders; this is that pass's record — the tags it could not
    /// resolve, and the transform and unknown-tag policy that were in effect.
    /// The framework computes this on every render and, until it was carried
    /// out here, discarded it.
    ///
    /// It is the *structural* form of "the theme is complete": it holds in
    /// every output mode, needs no ANSI and no TTY, and names the tag — where
    /// searching the page for the `[tag?]` marker only works in the one mode
    /// that emits it. [`invariants::assert_every_tag_resolved`] is the
    /// assertion written on it; [`unresolved_tags`](Self::unresolved_tags) is
    /// the flattened view.
    ///
    /// A run that rendered nothing (an error before dispatch, a structured
    /// output mode) records no passes. Only what this run rendered is here: the
    /// collector records only inside the window the run boundary opens, so a
    /// standalone render before or between runs is neither kept nor blamed on
    /// a run that follows it.
    ///
    /// A run the handler itself drove — an app invoking another app — counts as
    /// part of this run, and its passes appear here at
    /// [`TagResolution::nesting_depth`](standout_render::TagResolution::nesting_depth)
    /// `1` or deeper. That holds whatever the handler did with the nested run's
    /// output: the render path is handed a `String` and cannot see whether it
    /// was embedded in this page or thrown away, so it reports it either way
    /// rather than risk saying nothing about a page that carries corruption.
    /// Filter on depth `0` for the passes this run rendered itself.
    ///
    /// "Could not resolve" means the theme has no such tag. A tag the theme
    /// *does* define whose markup the template got wrong — an unbalanced open,
    /// an unexpected close — is
    /// [`TagResolution::malformed`](standout_render::TagResolution::malformed)
    /// instead, so an assertion about the theme's vocabulary never names a tag
    /// the theme has.
    pub fn tag_resolutions(&self) -> &[TagResolution] {
        &self.tag_resolutions
    }

    /// Returns every tag the run's renders could not resolve, across all passes.
    ///
    /// A tag used as a matched pair appears twice — once for the open, once for
    /// the close — since each carries its own position in the template output,
    /// and once more per pass: a run that renders a page for the terminal and
    /// again unstyled for a pipe applies the style-tag pass twice over the same
    /// template output. [`unresolved_tag_names`](Self::unresolved_tag_names) is
    /// the collapsed view.
    pub fn unresolved_tags(&self) -> Vec<&standout_render::UnknownTagError> {
        self.tag_resolutions
            .iter()
            .flat_map(TagResolution::unresolved)
            .collect()
    }

    /// Returns the distinct names of the tags the run could not resolve, in
    /// first-seen order.
    ///
    /// This is the form a failure message or an assertion wants: one entry per
    /// offending tag, with the open/close and per-pass repetitions of
    /// [`unresolved_tags`](Self::unresolved_tags) collapsed away.
    pub fn unresolved_tag_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = Vec::new();
        for error in self.unresolved_tags() {
            let name = error.tag.as_str();
            if !names.contains(&name) {
                names.push(name);
            }
        }
        names
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

    /// Returns the text the framework would have written to stdout, or `""`
    /// when the run put no text there.
    ///
    /// That is the rendered output of a handled run, or the report of an
    /// artifact written to a file — which leaves stdout free, so the framework
    /// reports there. An artifact that claimed stdout owns those bytes and its
    /// report goes to [`stderr`](Self::stderr) instead; the bytes themselves,
    /// like any binary output, are not text and stay on
    /// [`artifact_bytes`](Self::artifact_bytes) / [`binary`](Self::binary).
    ///
    /// Handled text is returned exactly as captured, without the newline
    /// `App::run` writes after it — the shape this accessor has always had.
    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    /// Returns the text the framework would have written to stderr, or `""`
    /// when the run left the error channel silent.
    ///
    /// This is the companion to [`stdout`](Self::stdout): together they say
    /// which bytes went to which stream, which a single captured outcome
    /// cannot. It mirrors the routing `App::run` performs — a diagnostic goes
    /// here newline-terminated, an application-declared external failure
    /// verbatim, the report of an artifact whose bytes claimed stdout lands
    /// here rather than corrupting them, and framework warnings close the
    /// channel in the banner block `run` flushes after the primary output.
    /// That block appears unstyled: `run` themes it from the real stderr's
    /// color capability, which an in-process run has no equivalent of. Each
    /// warning also stays individually addressable on
    /// [`warnings`](Self::warnings).
    pub fn stderr(&self) -> &str {
        &self.stderr
    }

    /// Returns [`stdout`](Self::stdout) with every ANSI escape removed.
    ///
    /// Styled output is compared against unstyled expectations often enough
    /// that every test file hand-rolling the same regex is a defect source of
    /// its own. The stripping is `console`'s — the same crate that emits the
    /// escapes — so what is stripped is exactly what was styled. The raw
    /// [`stdout`](Self::stdout) is untouched, so a test can assert on both.
    pub fn stdout_plain(&self) -> String {
        console::strip_ansi_codes(self.stdout()).into_owned()
    }

    /// Returns [`stderr`](Self::stderr) with every ANSI escape removed.
    pub fn stderr_plain(&self) -> String {
        console::strip_ansi_codes(self.stderr()).into_owned()
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

    /// Panics unless [`stderr`](Self::stderr) contains `needle`.
    #[track_caller]
    pub fn assert_stderr_contains(&self, needle: &str) {
        let err = self.stderr();
        if !err.contains(needle) {
            panic!(
                "stderr did not contain {:?}\n--- stderr ---\n{}\n--------------",
                needle, err
            );
        }
    }

    /// Panics unless [`stderr`](Self::stderr) equals `expected` exactly.
    #[track_caller]
    pub fn assert_stderr_eq(&self, expected: &str) {
        let err = self.stderr();
        if err != expected {
            panic!(
                "stderr mismatch\n--- expected ---\n{}\n--- actual -----\n{}\n----------------",
                expected, err
            );
        }
    }

    /// Panics unless the run left the error channel silent.
    #[track_caller]
    pub fn assert_stderr_empty(&self) {
        if !self.stderr().is_empty() {
            panic!(
                "expected an empty stderr, got:\n--- stderr ---\n{}\n--------------",
                self.stderr()
            );
        }
    }

    /// Panics unless the captured framework warnings contain `needle`.
    #[track_caller]
    pub fn assert_warning_contains(&self, needle: &str) {
        if self.warnings.iter().any(|warning| warning.contains(needle)) {
            return;
        }
        panic!(
            "warnings did not contain {:?}\n--- warnings ---\n{}\n----------------",
            needle,
            self.warnings.join("\n")
        );
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
