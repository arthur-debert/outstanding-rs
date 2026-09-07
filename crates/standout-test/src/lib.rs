//! In-process test harness for apps built on the `standout` CLI framework.
//!
//! [`TestHarness`] is a fluent builder over the injection seams a test needs:
//! env vars, cwd, stdin, clipboard, the representation, the color policy and
//! whether stdout is a terminal, theme facts on `TargetProperties`, and tempdir
//! fixtures. `run` applies every override, calls into the app in-process, and a
//! `Drop` impl restores everything on success or panic. [`TestResult`] exposes
//! the run's result values as data, the rendered bytes and the delivery
//! decision separately.
//!
//! There is no in-process TTY simulation: [`TestHarness::run_process`] spawns
//! the real binary and [`TestHarness::run_pty`] (Unix) gives it a
//! pseudo-terminal. The fixed `TargetProperties` defaults and the `#[serial]`
//! rule for tests that mutate env or cwd are in `docs/topics/testing.md`.

use clap::Command;
use standout::cli::DispatchResult;
use standout::cli::{App, Delivery, StreamCapture, StreamSink};
use standout::{ColorMode, ColorPolicy, IconMode, InputSources, TargetProperties};
use standout_input::env::{MockClipboard, MockStdin};
use standout_input::PromptResponder;
use standout_render::{AmbiguousWidth, Representation};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;
mod process;
#[cfg(unix)]
mod pty;
mod result;
mod schema;
pub use process::ProcessResult;
pub use serial_test::serial;
pub use standout_render::TagResolution;
#[derive(Debug, Clone)]
enum StdinMode {
    Inherit,
    Piped(String),
    Interactive,
}
#[must_use = "TestHarness is inert until you call run(...)"]
pub struct TestHarness {
    env_set: HashMap<String, String>,
    env_remove: Vec<String>,
    cwd: Option<PathBuf>,
    tempdir: Option<TempDir>,
    fixtures: Vec<(PathBuf, Vec<u8>)>,
    terminal_width: Option<Option<usize>>,
    ambiguous_width: Option<AmbiguousWidth>,
    color_policy: ColorPolicy,
    stdout_is_terminal: bool,
    stderr_is_terminal: bool,
    stdout_color_capability: bool,
    stderr_color_capability: bool,
    color_scheme: Option<ColorMode>,
    icon_mode: Option<IconMode>,
    output_mode: Option<Representation>,
    output_flag_name: String,
    stdin: StdinMode,
    clipboard: Option<String>,
    prompts: Option<Arc<dyn PromptResponder>>,
}
impl TestHarness {
    pub fn new() -> Self {
        Self {
            env_set: HashMap::new(),
            env_remove: Vec::new(),
            cwd: None,
            tempdir: None,
            fixtures: Vec::new(),
            terminal_width: None,
            ambiguous_width: None,
            color_policy: ColorPolicy::Auto,
            stdout_is_terminal: false,
            stderr_is_terminal: false,
            stdout_color_capability: false,
            stderr_color_capability: false,
            color_scheme: None,
            icon_mode: None,
            output_mode: None,
            output_flag_name: "output".to_string(),
            stdin: StdinMode::Inherit,
            clipboard: None,
            prompts: None,
        }
    }
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_set.insert(key.into(), value.into());
        self
    }
    pub fn env_remove(mut self, key: impl Into<String>) -> Self {
        self.env_remove.push(key.into());
        self
    }
    pub fn terminal_width(mut self, cols: usize) -> Self {
        self.terminal_width = Some(Some(cols));
        self
    }
    pub fn no_terminal_width(mut self) -> Self {
        self.terminal_width = Some(None);
        self
    }
    pub fn ambiguous_width(mut self, policy: AmbiguousWidth) -> Self {
        self.ambiguous_width = Some(policy);
        self
    }
    /// Whether the run may decorate human text; independent of the destination.
    pub fn color(mut self, policy: ColorPolicy) -> Self {
        self.color_policy = policy;
        self
    }
    pub fn stdout_is_terminal(mut self, is_terminal: bool) -> Self {
        self.stdout_is_terminal = is_terminal;
        self
    }
    pub fn stderr_is_terminal(mut self, is_terminal: bool) -> Self {
        self.stderr_is_terminal = is_terminal;
        self
    }
    /// The stdout destination fact an `auto` color policy reads.
    pub fn stdout_color_capability(mut self, capable: bool) -> Self {
        self.stdout_color_capability = capable;
        self
    }
    /// The stderr destination fact an `auto` color policy reads, which is what
    /// warning rendering consults.
    pub fn stderr_color_capability(mut self, capable: bool) -> Self {
        self.stderr_color_capability = capable;
        self
    }
    /// The ordinary color-capable TTY: both streams terminals, both reporting
    /// color capability.
    pub fn color_capable_terminal(self) -> Self {
        self.stdout_is_terminal(true)
            .stderr_is_terminal(true)
            .stdout_color_capability(true)
            .stderr_color_capability(true)
    }
    pub fn color_scheme(mut self, scheme: ColorMode) -> Self {
        self.color_scheme = Some(scheme);
        self
    }
    pub fn icon_mode(mut self, mode: IconMode) -> Self {
        self.icon_mode = Some(mode);
        self
    }
    pub fn output_mode(mut self, representation: Representation) -> Self {
        self.output_mode = Some(representation);
        self
    }
    pub fn rendering(self, representation: Representation, color: ColorPolicy) -> Self {
        self.output_mode(representation).color(color)
    }
    pub fn output_flag_name(mut self, name: impl Into<String>) -> Self {
        self.output_flag_name = name.into();
        self
    }
    pub fn text_output(self) -> Self {
        self.color(ColorPolicy::Never)
    }
    pub fn piped_stdin(mut self, content: impl Into<String>) -> Self {
        self.stdin = StdinMode::Piped(content.into());
        self
    }
    pub fn interactive_stdin(mut self) -> Self {
        self.stdin = StdinMode::Interactive;
        self
    }
    pub fn clipboard(mut self, content: impl Into<String>) -> Self {
        self.clipboard = Some(content.into());
        self
    }
    pub fn prompts(mut self, responder: Arc<dyn PromptResponder>) -> Self {
        self.prompts = Some(responder);
        self
    }
    pub fn cwd(mut self, path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        if !path.is_absolute() {
            validate_relative_path("cwd", &path);
            self.ensure_tempdir();
        }
        self.cwd = Some(path);
        self
    }
    pub fn fixture(mut self, path: impl AsRef<Path>, content: impl Into<String>) -> Self {
        let path = validate_relative_path("fixture", path.as_ref());
        self.fixtures.push((path, content.into().into_bytes()));
        self.ensure_tempdir();
        self
    }
    pub fn fixture_bytes(mut self, path: impl AsRef<Path>, content: impl Into<Vec<u8>>) -> Self {
        let path = validate_relative_path("fixture", path.as_ref());
        self.fixtures.push((path, content.into()));
        self.ensure_tempdir();
        self
    }
    pub fn tempdir(&self) -> Option<&Path> {
        self.tempdir.as_ref().map(|t| t.path())
    }
    fn ensure_tempdir(&mut self) {
        if self.tempdir.is_none() {
            self.tempdir =
                Some(TempDir::new().expect("TestHarness: failed to create tempdir for fixtures"));
        }
    }
    pub(crate) fn resolve_cwd(&mut self) -> Option<PathBuf> {
        let Some(cwd) = self.cwd.clone() else {
            return self.tempdir.as_ref().map(|d| d.path().to_path_buf());
        };
        if cwd.is_absolute() {
            return Some(cwd);
        }
        self.ensure_tempdir();
        let target = self.tempdir.as_ref().unwrap().path().join(cwd);
        std::fs::create_dir_all(&target).expect("TestHarness: failed to create cwd directory");
        Some(target)
    }
    pub fn run<I, T>(mut self, app: &App, cmd: Command, args: I) -> TestResult
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString>,
    {
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
        if let Some(target) = self.resolve_cwd() {
            restore.original_cwd = std::env::current_dir().ok();
            std::env::set_current_dir(&target)
                .expect("TestHarness: failed to change working directory");
        }
        let _ = console::colors_enabled();
        let _ = console::colors_enabled_stderr();
        let _ = console::true_colors_enabled();
        let _ = console::true_colors_enabled_stderr();
        for (k, v) in &self.env_set {
            restore.env.set_var(k.clone(), v);
        }
        for k in &self.env_remove {
            restore.env.remove_var(k.clone());
        }
        let mut sources = InputSources::from_process();
        match std::mem::replace(&mut self.stdin, StdinMode::Inherit) {
            StdinMode::Inherit => {}
            StdinMode::Piped(content) => {
                sources = sources.with_stdin(MockStdin::piped(content));
            }
            StdinMode::Interactive => {
                sources = sources.with_stdin(MockStdin::terminal());
            }
        }
        if let Some(content) = self.clipboard.take() {
            sources = sources.with_clipboard(MockClipboard::with_content(content));
        }
        if let Some(responder) = self.prompts.take() {
            sources = sources.with_responder(responder);
        }
        let mut argv: Vec<OsString> = args.into_iter().map(|a| a.into()).collect();
        if let Some(spelling) = self.output_mode.and_then(output_mode_flag) {
            argv.push(format!("--{}={}", self.output_flag_name, spelling).into());
        }
        let target = self.target_properties();
        let captured = StreamCapture::default();
        let sink = StreamSink::new(captured.clone());
        let run = app.run_with_sink(cmd, argv, target, self.color_policy, sources, sink.clone());
        let color_policy = run.color_policy();
        let warnings = run.warnings().to_vec();
        let output_mode = run.output_mode();
        let results = run.results().to_vec();
        let delivery = run.delivery().clone();
        let outcome = run.into_outcome();
        let tag_resolutions = standout_render::diagnostics::take_captured();
        let theme = app.get_default_theme();
        let mut stderr = Vec::new();
        sink.with_writer(|stdout| {
            standout::cli::emit_run_result(&outcome, output_mode, stdout, &mut stderr)
                .expect("in-memory streams never fail a final write");
            standout::cli::emit_warning_entries(&outcome, &warnings, output_mode, stdout)
                .expect("in-memory streams never fail a final write");
        });
        let mut stdout = captured.take();
        // A process gets one more newline than `stdout()`: the one that terminates rendered text.
        if !output_mode.is_stream()
            && matches!(outcome, DispatchResult::Handled(_))
            && stdout.last() == Some(&b'\n')
        {
            stdout.pop();
        }
        let mut stderr = String::from_utf8_lossy(&stderr).into_owned();
        if !standout::cli::warnings_delivered_on_stdout(&outcome, output_mode) {
            stderr.push_str(&standout_render::warnings::render_block_for_target(
                theme,
                color_policy,
                target,
                &warnings,
            ));
        }
        TestResult {
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stdout_bytes: stdout,
            stderr,
            outcome,
            warnings,
            output_mode,
            results,
            delivery,
            tag_resolutions,
            _tempdir: self.tempdir.take(),
            _restore: restore,
        }
    }
    fn target_properties(&self) -> TargetProperties {
        TargetProperties {
            width: self.terminal_width.flatten(),
            stdout_is_terminal: self.stdout_is_terminal,
            stderr_is_terminal: self.stderr_is_terminal,
            stdout_color_capability: self.stdout_color_capability,
            stderr_color_capability: self.stderr_color_capability,
            color_scheme: self.color_scheme.unwrap_or(ColorMode::Dark),
            icon_mode: self.icon_mode.unwrap_or(IconMode::Classic),
            ambiguous_width: self.ambiguous_width.unwrap_or(AmbiguousWidth::Narrow),
        }
    }
}
impl Default for TestHarness {
    fn default() -> Self {
        Self::new()
    }
}
#[cfg(test)]
mod relative_cwd {
    use super::*;
    #[test]
    fn a_relative_cwd_alone_creates_the_tempdir() {
        let harness = TestHarness::new().cwd("proj");
        assert!(harness.tempdir().is_some());
    }
    #[test]
    fn an_absolute_cwd_needs_no_tempdir() {
        let dir = TempDir::new().unwrap();
        let harness = TestHarness::new().cwd(dir.path().to_path_buf());
        assert!(harness.tempdir().is_none());
    }
}
#[cfg(test)]
mod target_properties_defaults {
    use super::*;
    #[test]
    fn unset_destination_facts_are_the_documented_fixed_defaults() {
        let target = TestHarness::new().target_properties();
        assert_eq!(target.width, None);
        assert!(!target.stdout_is_terminal);
        assert!(!target.stderr_is_terminal);
        assert!(!target.stdout_color_capability);
        assert!(!target.stderr_color_capability);
        assert_eq!(target.color_scheme, ColorMode::Dark);
        assert_eq!(target.icon_mode, IconMode::Classic);
        assert_eq!(target.ambiguous_width, AmbiguousWidth::Narrow);
    }
    #[test]
    fn explicit_overrides_replace_the_fixed_defaults() {
        let target = TestHarness::new()
            .terminal_width(42)
            .color_capable_terminal()
            .color_scheme(ColorMode::Light)
            .icon_mode(IconMode::NerdFont)
            .ambiguous_width(AmbiguousWidth::Wide)
            .target_properties();
        assert_eq!(target.width, Some(42));
        assert!(target.stdout_is_terminal);
        assert!(target.stderr_is_terminal);
        assert!(target.stdout_color_capability);
        assert!(target.stderr_color_capability);
        assert_eq!(target.color_scheme, ColorMode::Light);
        assert_eq!(target.icon_mode, IconMode::NerdFont);
        assert_eq!(target.ambiguous_width, AmbiguousWidth::Wide);
    }
    #[test]
    fn each_destination_fact_moves_alone() {
        let stdout_only = TestHarness::new()
            .stdout_is_terminal(true)
            .target_properties();
        assert!(stdout_only.stdout_is_terminal);
        assert!(!stdout_only.stderr_is_terminal);
        assert!(!stdout_only.stdout_color_capability);
        assert!(!stdout_only.stderr_color_capability);

        let colorless_terminal = TestHarness::new()
            .stdout_is_terminal(true)
            .stderr_is_terminal(true)
            .target_properties();
        assert!(colorless_terminal.stdout_is_terminal);
        assert!(!colorless_terminal.stdout_color_capability);

        let stderr_only = TestHarness::new()
            .stderr_color_capability(true)
            .target_properties();
        assert!(stderr_only.stderr_color_capability);
        assert!(!stderr_only.stdout_color_capability);
        assert!(!stderr_only.stderr_is_terminal);
    }
}
fn validate_relative_path(method: &str, path: &Path) -> PathBuf {
    use std::path::Component;
    if path.is_absolute() {
        panic!(
            "TestHarness::{method}: path {path:?} is absolute; only relative paths are allowed so \
             the {method} is confined to the harness tempdir"
        );
    }
    for component in path.components() {
        match component {
            Component::ParentDir => panic!(
                "TestHarness::{method}: path {path:?} contains a `..` component; only relative \
                 paths that stay inside the tempdir are allowed"
            ),
            Component::Prefix(_) | Component::RootDir => panic!(
                "TestHarness::{method}: path {path:?} has a root or prefix component; only \
                 relative paths inside the tempdir are allowed"
            ),
            _ => {}
        }
    }
    path.to_path_buf()
}
/// `None` for the human representation, which `--output` cannot name: a run
/// that wants it simply leaves the flag off.
fn output_mode_flag(representation: Representation) -> Option<&'static str> {
    match representation {
        Representation::Human => None,
        Representation::TermDebug => Some("term-debug"),
        Representation::Json => Some("json"),
        Representation::Yaml => Some("yaml"),
        Representation::Csv => Some("csv"),
        Representation::Ndjson => Some("ndjson"),
    }
}
#[derive(Default)]
struct RestoreState {
    env: ScopedEnv,
    original_cwd: Option<PathBuf>,
}
impl Drop for RestoreState {
    fn drop(&mut self) {
        if let Some(cwd) = self.original_cwd.take() {
            let _ = std::env::set_current_dir(cwd);
        }
    }
}

/// Environment variables set or removed for as long as the guard lives, with
/// whatever stood there restored when it drops, on a panic as much as on a
/// pass. The environment is process-wide, so the `#[serial]` rule applies the
/// same way it does to the harness.
///
/// ```no_run
/// # use standout_test::ScopedEnv;
/// let _env = ScopedEnv::new().set("MYAPP_PAGER", "sed -n 1p").remove("PAGER");
/// ```
#[derive(Default)]
pub struct ScopedEnv {
    originals: HashMap<String, Option<String>>,
}

impl ScopedEnv {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(mut self, key: impl Into<String>, value: impl AsRef<str>) -> Self {
        self.set_var(key, value.as_ref());
        self
    }

    pub fn remove(mut self, key: impl Into<String>) -> Self {
        self.remove_var(key);
        self
    }

    fn set_var(&mut self, key: impl Into<String>, value: impl AsRef<str>) {
        let key = self.remember(key);
        std::env::set_var(key, value.as_ref());
    }

    fn remove_var(&mut self, key: impl Into<String>) {
        let key = self.remember(key);
        std::env::remove_var(key);
    }

    /// Kept from the first touch, so a later one never records what the guard
    /// itself set.
    fn remember(&mut self, key: impl Into<String>) -> String {
        let key = key.into();
        self.originals
            .entry(key.clone())
            .or_insert_with(|| std::env::var(&key).ok());
        key
    }
}

impl Drop for ScopedEnv {
    fn drop(&mut self) {
        for (key, original) in self.originals.drain() {
            match original {
                Some(value) => std::env::set_var(&key, value),
                None => std::env::remove_var(&key),
            }
        }
    }
}
pub struct TestResult {
    outcome: DispatchResult,
    stdout: String,
    stdout_bytes: Vec<u8>,
    stderr: String,
    warnings: Vec<String>,
    output_mode: Representation,
    results: Vec<serde_json::Value>,
    delivery: Delivery,
    tag_resolutions: Vec<TagResolution>,
    _tempdir: Option<TempDir>,
    _restore: RestoreState,
}
