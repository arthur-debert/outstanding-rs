//! In-process test harness for apps built on the `standout` CLI framework.
//!
//! [`TestHarness`] is a fluent builder over the injection seams a test
//! needs: env vars, cwd, stdin, clipboard, output mode, color/theme facts on
//! `TargetProperties`, and tempdir fixtures. `run` applies every override,
//! calls into the app in-process, and a `Drop` impl restores everything on
//! success or panic. [`TestResult`] then exposes what the run wrote to each
//! stream (raw and ANSI-stripped), plus structured facts like style-tag
//! resolutions that a text search can't get at; [`assert_page_snapshot!`]
//! and [`matrix`] pin a rendered page across output-mode/color/theme cells.
//!
//! Unset `TargetProperties` fields take fixed defaults rather than calling
//! `TargetProperties::detect`: `width: None`, `ColorMode::Dark`,
//! `IconMode::Classic`, `AmbiguousWidth::Narrow`, color capability off, both
//! streams non-terminal. `$COLUMNS`/`$NERD_FONT`/OS appearance cannot
//! change an in-process run — there is no in-process TTY simulation; use
//! [`TestHarness::run_process`] (spawns the real binary) or
//! [`TestHarness::run_pty`] (Unix, real pseudo-terminal) for TTY-dependent
//! behavior.
//!
//! `run` mutates process-global state (env vars, cwd) and a child process
//! inherits the ambient environment/cwd at spawn time, so any test binary
//! that mixes `run` with `run_process`/`run_pty` needs every such test
//! annotated `#[serial]` (re-exported from `serial_test`) to avoid one
//! run's overrides leaking into a concurrent spawn.

use clap::Command;
use standout::cli::DispatchResult;
use standout::cli::{
    App, ArtifactDestination, ArtifactRun, Diagnostic, ExitStatus, RunErrorKind, StreamSink,
    SuccessKind,
};
use standout::{ColorMode, IconMode, InputSources, TargetProperties};
use standout_input::env::{MockClipboard, MockStdin};
use standout_input::PromptResponder;
use standout_render::{AmbiguousWidth, OutputMode};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;
pub mod clap_parity;
#[cfg(test)]
mod decoder_roundtrip;
pub mod invariants;
mod matrix;
mod page;
mod process;
#[cfg(unix)]
pub mod pty;
mod snapshot;
pub use matrix::{matrix, MatrixCell};
pub use process::ProcessResult;
pub use serial_test::serial;
pub use snapshot::SnapshotCase;
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
    color_capable: Option<bool>,
    color_scheme: Option<ColorMode>,
    icon_mode: Option<IconMode>,
    output_mode: Option<OutputMode>,
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
            color_capable: None,
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
    pub fn with_color(mut self) -> Self {
        self.color_capable = Some(true);
        self
    }
    pub fn no_color(mut self) -> Self {
        self.color_capable = Some(false);
        self
    }
    pub fn color_scheme(mut self, scheme: ColorMode) -> Self {
        self.color_scheme = Some(scheme);
        self
    }
    pub fn icon_mode(mut self, mode: IconMode) -> Self {
        self.icon_mode = Some(mode);
        self
    }
    pub fn output_mode(mut self, mode: OutputMode) -> Self {
        self.output_mode = Some(mode);
        self
    }
    pub fn output_flag_name(mut self, name: impl Into<String>) -> Self {
        self.output_flag_name = name.into();
        self
    }
    pub fn text_output(self) -> Self {
        self.output_mode(OutputMode::Text)
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
        self.cwd = Some(path.into());
        self
    }
    pub fn fixture(mut self, path: impl AsRef<Path>, content: impl Into<String>) -> Self {
        let path = validate_fixture_path(path.as_ref());
        self.fixtures.push((path, content.into().into_bytes()));
        self.ensure_tempdir();
        self
    }
    pub fn fixture_bytes(mut self, path: impl AsRef<Path>, content: impl Into<Vec<u8>>) -> Self {
        let path = validate_fixture_path(path.as_ref());
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
        let cwd_target = self
            .cwd
            .clone()
            .or_else(|| self.tempdir.as_ref().map(|d| d.path().to_path_buf()));
        if let Some(target) = cwd_target {
            restore.original_cwd = std::env::current_dir().ok();
            std::env::set_current_dir(&target)
                .expect("TestHarness: failed to change working directory");
        }
        let _ = console::colors_enabled();
        let _ = console::colors_enabled_stderr();
        let _ = console::true_colors_enabled();
        let _ = console::true_colors_enabled_stderr();
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
        if let Some(mode) = self.output_mode {
            argv.push(format!("--{}={}", self.output_flag_name, output_mode_flag(mode)).into());
        }
        let target = self.target_properties();
        let streamed = CapturedStream::default();
        let run = app.run_with_sink(
            cmd,
            argv,
            target,
            sources,
            StreamSink::new(streamed.clone()),
        );
        let warnings = run.warnings().to_vec();
        let output_mode = run.output_mode();
        let outcome = run.into_outcome();
        let tag_resolutions = standout_render::diagnostics::take_captured();
        let theme = app.get_default_theme();
        // Stream entries reached stdout while the handler ran; the result and
        // the warning entries follow them, as in a process.
        let mut stdout = streamed.0.take();
        let mut stderr = Vec::new();
        standout::cli::emit_run_result(&outcome, output_mode, &mut stdout, &mut stderr)
            .expect("in-memory streams never fail a final write");
        standout::cli::emit_warning_entries(&warnings, output_mode, &mut stdout)
            .expect("in-memory streams never fail a final write");
        // `stdout()` is the rendered text; a process gets it plus the one
        // newline `emit_run_result` terminates rendered text with. A stream
        // is kept whole: every line, its newline included.
        if !output_mode.is_stream()
            && matches!(outcome, DispatchResult::Handled(_))
            && stdout.last() == Some(&b'\n')
        {
            stdout.pop();
        }
        let mut stderr = String::from_utf8_lossy(&stderr).into_owned();
        if !standout::cli::carries_warning_entries(output_mode) {
            stderr.push_str(&standout_render::warnings::render_block_for_target(
                theme,
                output_mode,
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
            tag_resolutions,
            _tempdir: self.tempdir.take(),
            _restore: restore,
        }
    }
    fn target_properties(&self) -> TargetProperties {
        TargetProperties {
            width: self.terminal_width.flatten(),
            stdout_is_terminal: false,
            stderr_is_terminal: false,
            stdout_color_capability: self.color_capable.unwrap_or(false),
            stderr_color_capability: self.color_capable.unwrap_or(false),
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
            .with_color()
            .color_scheme(ColorMode::Light)
            .icon_mode(IconMode::NerdFont)
            .ambiguous_width(AmbiguousWidth::Wide)
            .target_properties();
        assert_eq!(target.width, Some(42));
        assert!(!target.stdout_is_terminal);
        assert!(!target.stderr_is_terminal);
        assert!(target.stdout_color_capability);
        assert!(target.stderr_color_capability);
        assert_eq!(target.color_scheme, ColorMode::Light);
        assert_eq!(target.icon_mode, IconMode::NerdFont);
        assert_eq!(target.ambiguous_width, AmbiguousWidth::Wide);
    }
}
#[derive(Clone, Default)]
struct CapturedStream(std::rc::Rc<std::cell::Cell<Vec<u8>>>);
impl std::io::Write for CapturedStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut bytes = self.0.take();
        bytes.extend_from_slice(buf);
        self.0.set(bytes);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
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
        OutputMode::Ndjson => "ndjson",
    }
}
#[derive(Default)]
struct RestoreState {
    env_originals: HashMap<String, Option<String>>,
    original_cwd: Option<PathBuf>,
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
    }
}
pub struct TestResult {
    outcome: DispatchResult,
    stdout: String,
    stdout_bytes: Vec<u8>,
    stderr: String,
    warnings: Vec<String>,
    output_mode: OutputMode,
    tag_resolutions: Vec<TagResolution>,
    _tempdir: Option<TempDir>,
    _restore: RestoreState,
}
impl TestResult {
    pub fn outcome(&self) -> &DispatchResult {
        &self.outcome
    }
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
    pub fn tag_resolutions(&self) -> &[TagResolution] {
        &self.tag_resolutions
    }
    pub fn unresolved_tags(&self) -> Vec<&standout_render::UnknownTagError> {
        self.tag_resolutions
            .iter()
            .flat_map(TagResolution::unresolved)
            .collect()
    }
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
    pub fn exit_status(&self) -> Option<ExitStatus> {
        self.outcome.exit_status()
    }
    pub fn success_kind(&self) -> Option<SuccessKind> {
        self.outcome.success_kind()
    }
    pub fn error_kind(&self) -> Option<RunErrorKind> {
        self.outcome.error_kind()
    }
    pub fn output_mode(&self) -> OutputMode {
        self.output_mode
    }
    /// The diagnostic document the run wrote to stdout: `None` when the
    /// resolved mode carries no document or stdout is not one.
    pub fn diagnostic(&self) -> Option<Diagnostic> {
        standout::cli::parse_diagnostic(self.output_mode, &self.stdout).ok()
    }
    #[track_caller]
    pub fn expect_diagnostic(&self) -> Diagnostic {
        match standout::cli::parse_diagnostic(self.output_mode, &self.stdout) {
            Ok(diagnostic) => diagnostic,
            Err(error) => panic!(
                "expected a diagnostic document on stdout in {:?} mode ({error}), got:\n--- stdout ---\n{}\n--------------",
                self.output_mode, self.stdout
            ),
        }
    }
    pub fn stdout(&self) -> &str {
        &self.stdout
    }
    /// What the run wrote to stdout, byte for byte; `stdout()` is its lossy
    /// text, minus the newline that terminates rendered text.
    pub fn stdout_bytes(&self) -> &[u8] {
        &self.stdout_bytes
    }
    pub fn stderr(&self) -> &str {
        &self.stderr
    }
    pub fn stdout_plain(&self) -> String {
        console::strip_ansi_codes(self.stdout()).into_owned()
    }
    pub fn stderr_plain(&self) -> String {
        console::strip_ansi_codes(self.stderr()).into_owned()
    }
    pub fn is_handled(&self) -> bool {
        matches!(self.outcome, DispatchResult::Handled(_))
    }
    pub fn is_no_match(&self) -> bool {
        matches!(self.outcome, DispatchResult::NoMatch(_))
    }
    pub fn binary(&self) -> Option<(&[u8], &str)> {
        match &self.outcome {
            DispatchResult::Binary(bytes, filename) => Some((bytes.as_slice(), filename.as_str())),
            _ => None,
        }
    }
    pub fn artifact(&self) -> Option<&ArtifactRun> {
        self.outcome.artifact()
    }
    pub fn artifact_bytes(&self) -> Option<&[u8]> {
        self.artifact().map(ArtifactRun::bytes)
    }
    pub fn artifact_destination(&self) -> Option<&ArtifactDestination> {
        self.artifact().map(ArtifactRun::destination)
    }
    pub fn artifact_report(&self) -> Option<&str> {
        self.artifact().and_then(ArtifactRun::report)
    }
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
    #[track_caller]
    pub fn assert_artifact_suggested_destination(&self, expected: impl AsRef<Path>) {
        let actual = self.expect_artifact().suggested_destination();
        assert_eq!(
            actual,
            Some(expected.as_ref()),
            "unexpected suggested artifact destination"
        );
    }
    #[track_caller]
    pub fn assert_artifact_written_to(&self, expected: impl AsRef<Path>) {
        let receipt = self.expect_artifact().receipt();
        assert_eq!(
            receipt.path(),
            Some(expected.as_ref()),
            "unexpected artifact destination"
        );
    }
    #[track_caller]
    pub fn assert_artifact_to_stdout(&self) {
        let receipt = self.expect_artifact().receipt();
        assert!(
            receipt.is_stdout(),
            "expected the artifact to go to stdout, but it went to {}",
            receipt.destination().label()
        );
    }
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
    #[track_caller]
    pub fn assert_success(&self) {
        match &self.outcome {
            DispatchResult::Handled(_)
            | DispatchResult::Silent
            | DispatchResult::Binary(_, _)
            | DispatchResult::Artifact(_) => {}
            DispatchResult::NoMatch(_) => {
                panic!("expected successful dispatch but no handler matched; stdout was empty")
            }
            DispatchResult::Error(msg) => {
                panic!("expected successful dispatch, got error: {}", msg)
            }
            _ => panic!(
                "expected successful dispatch, got: {:?}",
                describe_outcome(&self.outcome)
            ),
        }
    }
    #[track_caller]
    pub fn assert_exit_status(&self, expected: ExitStatus) {
        assert_eq!(
            self.exit_status(),
            Some(expected),
            "unexpected exit status for {}",
            describe_outcome(&self.outcome)
        );
    }
    #[track_caller]
    pub fn assert_error_kind(&self, expected: RunErrorKind) {
        assert_eq!(
            self.error_kind(),
            Some(expected),
            "unexpected error kind for {}",
            describe_outcome(&self.outcome)
        );
    }
    pub fn is_error(&self) -> bool {
        matches!(self.outcome, DispatchResult::Error(_))
    }
    pub fn error(&self) -> Option<&str> {
        match &self.outcome {
            DispatchResult::Error(s) => Some(s.as_str()),
            _ => None,
        }
    }
    #[track_caller]
    pub fn assert_error(&self) {
        if !self.is_error() {
            panic!(
                "expected DispatchResult::Error, got: {:?}",
                describe_outcome(&self.outcome)
            );
        }
    }
    #[track_caller]
    pub fn assert_error_contains(&self, needle: &str) {
        match self.error() {
            Some(msg) if msg.contains(needle) => {}
            Some(msg) => panic!(
                "error did not contain {:?}\n--- error ---\n{}\n-------------",
                needle, msg
            ),
            None => panic!(
                "expected DispatchResult::Error, got: {:?}",
                describe_outcome(&self.outcome)
            ),
        }
    }
    #[track_caller]
    pub fn assert_no_match(&self) {
        if !self.is_no_match() {
            panic!(
                "expected no handler match, got: {:?}",
                describe_outcome(&self.outcome)
            );
        }
    }
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
    #[track_caller]
    pub fn assert_stderr_empty(&self) {
        if !self.stderr().is_empty() {
            panic!(
                "expected an empty stderr, got:\n--- stderr ---\n{}\n--------------",
                self.stderr()
            );
        }
    }
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
fn describe_outcome(o: &DispatchResult) -> String {
    match o {
        DispatchResult::Handled(s) => format!("Handled({:?})", s),
        DispatchResult::Silent => "Silent".into(),
        DispatchResult::Binary(b, f) => format!("Binary(len={}, {:?})", b.len(), f),
        DispatchResult::Artifact(run) => format!(
            "Artifact(len={}, destination={:?})",
            run.bytes().len(),
            run.destination().label()
        ),
        DispatchResult::Error(s) => format!("Error({:?})", s),
        DispatchResult::NoMatch(_) => "NoMatch".into(),
        _ => "Unknown".into(),
    }
}
