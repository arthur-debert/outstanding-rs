use super::{schema, TestResult};
use standout::cli::{
    ArtifactDestination, ArtifactRun, Delivery, Diagnostic, DispatchResult, ExitStatus,
    RunErrorKind, SuccessKind,
};
use standout_render::{Representation, TagResolution};
use std::path::Path;

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
    pub fn output_mode(&self) -> Representation {
        self.output_mode
    }
    /// The run's last result value as data, whatever representation it
    /// selected. `None` when the run produced no result value.
    pub fn result(&self) -> Option<&serde_json::Value> {
        self.results.last()
    }
    /// Every recorded result value, in the order the run produced it.
    pub fn results(&self) -> &[serde_json::Value] {
        &self.results
    }
    /// Where the rendered bytes went: stdout, the file the user named, or
    /// the pager the environment named. An in-process run reports the pager
    /// decision without starting one.
    pub fn delivery(&self) -> &Delivery {
        &self.delivery
    }
    /// `None` when the resolved mode carries no document or stdout is not one.
    pub fn diagnostic(&self) -> Option<Diagnostic> {
        standout::cli::parse_diagnostic(self.output_mode, &self.stdout).ok()
    }
    /// Keys and value types vs `tests/schemas/<name>`; `STANDOUT_UPDATE_SNAPSHOTS=1` updates it.
    #[track_caller]
    pub fn assert_schema_snapshot(&self, name: &str) {
        schema::assert_schema_snapshot(self.output_mode, &self.stdout, name);
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
    /// Byte for byte; `stdout()` is the lossy text minus the newline that terminates rendered text.
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
