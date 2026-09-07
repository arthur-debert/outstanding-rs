use crate::diagnostic::{Diagnostic, Severity};
use crate::escape::escape_control_characters;
use crate::hooks::HookPhase;
use std::fmt;
use std::sync::Arc;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExitStatus(u8);
impl ExitStatus {
    pub const SUCCESS: Self = Self(0);
    pub const FAILURE: Self = Self(1);
    pub const USAGE_ERROR: Self = Self(2);
    pub const fn code(self) -> u8 {
        self.0
    }
}
impl From<u8> for ExitStatus {
    fn from(code: u8) -> Self {
        Self(code)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("an external failure status must be nonzero")]
pub struct InvalidExternalStatus;
#[derive(Debug, Clone)]
pub struct ExternalFailure {
    status: ExitStatus,
    diagnostic: String,
    source: Option<Arc<dyn std::error::Error + Send + Sync + 'static>>,
}
impl ExternalFailure {
    pub fn new(status: u8, diagnostic: impl Into<String>) -> Result<Self, InvalidExternalStatus> {
        if status == 0 {
            return Err(InvalidExternalStatus);
        }
        Ok(Self {
            status: ExitStatus(status),
            diagnostic: diagnostic.into(),
            source: None,
        })
    }
    pub const fn exit_status(&self) -> ExitStatus {
        self.status
    }
    pub fn diagnostic(&self) -> &str {
        &self.diagnostic
    }
    pub fn with_source<E>(mut self, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        self.source = Some(Arc::new(source));
        self
    }
}
impl fmt::Display for ExternalFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.diagnostic())
    }
}
impl std::error::Error for ExternalFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn std::error::Error + 'static))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("an app failure status must be nonzero")]
pub struct InvalidAppStatus;
#[derive(Debug, Clone)]
pub struct AppFailure {
    status: ExitStatus,
    diagnostic: String,
    framed: bool,
    source: Option<Arc<dyn std::error::Error + Send + Sync + 'static>>,
}
impl AppFailure {
    pub fn new(status: u8, diagnostic: impl Into<String>) -> Result<Self, InvalidAppStatus> {
        if status == 0 {
            return Err(InvalidAppStatus);
        }
        Ok(Self {
            status: ExitStatus(status),
            diagnostic: diagnostic.into(),
            framed: false,
            source: None,
        })
    }
    pub fn framed(mut self) -> Self {
        self.framed = true;
        self
    }
    pub const fn exit_status(&self) -> ExitStatus {
        self.status
    }
    pub fn diagnostic(&self) -> &str {
        &self.diagnostic
    }
    pub fn with_source<E>(mut self, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        self.source = Some(Arc::new(source));
        self
    }
}
impl fmt::Display for AppFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.diagnostic())
    }
}
impl std::error::Error for AppFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn std::error::Error + 'static))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OutputKind {
    Text,
    Binary,
    Artifact,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RunErrorKind {
    ClapUsage,
    DefaultCommand,
    Handler,
    Hook(HookPhase),
    Render,
    FinalWrite(OutputKind),
    External,
    App,
    Config,
}
#[derive(Debug, Clone)]
pub struct RunError {
    message: String,
    kind: RunErrorKind,
    status: ExitStatus,
    verbatim: bool,
    source: Option<Arc<dyn std::error::Error + Send + Sync + 'static>>,
    diagnostic: Option<Box<Diagnostic>>,
}
impl RunError {
    pub fn new(message: impl Into<String>, kind: RunErrorKind) -> Self {
        assert!(
            kind != RunErrorKind::External,
            "external run errors must be constructed from ExternalFailure"
        );
        assert!(
            kind != RunErrorKind::App,
            "app run errors must be constructed from AppFailure"
        );
        assert!(
            kind != RunErrorKind::Config,
            "config run errors must be constructed from RunError::config"
        );
        Self::of_kind(message, kind)
    }
    /// A write that carried the run's output failed; `error` is what the destination reported.
    pub fn final_write(
        message: impl Into<String>,
        error: Arc<dyn std::error::Error + Send + Sync>,
        kind: OutputKind,
    ) -> Self {
        Self::of_kind(message, RunErrorKind::FinalWrite(kind)).with_source(error)
    }
    /// Turning the run's data into bytes failed; `error` is what the renderer or serializer reported.
    pub fn render(
        message: impl Into<String>,
        error: Arc<dyn std::error::Error + Send + Sync>,
    ) -> Self {
        Self::of_kind(message, RunErrorKind::Render).with_source(error)
    }
    /// Resolving the application's configuration failed; `error` is what the resolver reported.
    pub fn config(
        message: impl Into<String>,
        error: Arc<dyn std::error::Error + Send + Sync>,
    ) -> Self {
        Self::of_kind(message, RunErrorKind::Config).with_source(error)
    }
    fn of_kind(message: impl Into<String>, kind: RunErrorKind) -> Self {
        let status = match kind {
            RunErrorKind::ClapUsage => ExitStatus::USAGE_ERROR,
            _ => ExitStatus::FAILURE,
        };
        Self {
            message: escape_control_characters(message.into()),
            kind,
            status,
            verbatim: false,
            source: None,
            diagnostic: None,
        }
    }
    pub fn with_usage_exit_status(mut self, status: ExitStatus) -> Self {
        assert!(
            self.kind == RunErrorKind::ClapUsage,
            "a usage exit status applies to a clap rejection"
        );
        self.status = status;
        self
    }
    pub fn with_source(mut self, source: Arc<dyn std::error::Error + Send + Sync>) -> Self {
        self.source = Some(source);
        self
    }
    /// Replaces the summary `diagnostic()` would otherwise derive from the prose message.
    pub fn with_diagnostic(mut self, diagnostic: Diagnostic) -> Self {
        self.diagnostic = Some(Box::new(escape_diagnostic(diagnostic)));
        self
    }
    /// The carried diagnostic wins; otherwise the first prose line (one `Error: ` framing
    /// stripped) is `summary` and the rest `detail`.
    pub fn diagnostic(&self) -> Diagnostic {
        let mut diagnostic = match (&self.diagnostic, self.verbatim) {
            (Some(diagnostic), _) => (**diagnostic).clone(),
            (None, true) => {
                Diagnostic::error(first_line(&self.message)).detail(self.message.clone())
            }
            (None, false) => {
                let prose = ["Error: ", "error: "]
                    .iter()
                    .find_map(|framing| self.message.strip_prefix(framing))
                    .unwrap_or(&self.message);
                let (summary, detail) = prose.split_once('\n').unwrap_or((prose, ""));
                Diagnostic::error(summary.trim_end()).detail(detail.trim())
            }
        };
        diagnostic.kind = self.kind.into();
        diagnostic.severity = Severity::Error;
        diagnostic
    }
    pub fn as_str(&self) -> &str {
        &self.message
    }
    pub const fn kind(&self) -> RunErrorKind {
        self.kind
    }
    pub const fn exit_status(&self) -> ExitStatus {
        self.status
    }
    pub fn into_string(self) -> String {
        self.message
    }
    // A stderr payload its owner wrote: no `Error: ` framing, no trailing newline.
    pub const fn writes_diagnostic_verbatim(&self) -> bool {
        self.verbatim
    }
}
impl std::ops::Deref for RunError {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}
impl AsRef<str> for RunError {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl std::error::Error for RunError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn std::error::Error + 'static))
    }
}
impl From<ExternalFailure> for RunError {
    fn from(failure: ExternalFailure) -> Self {
        Self {
            message: failure.diagnostic,
            kind: RunErrorKind::External,
            status: failure.status,
            verbatim: true,
            source: failure.source,
            diagnostic: None,
        }
    }
}
impl From<AppFailure> for RunError {
    fn from(failure: AppFailure) -> Self {
        let message = if failure.framed {
            escape_control_characters(format!("Error: {}", failure.diagnostic))
        } else {
            failure.diagnostic
        };
        Self {
            message,
            kind: RunErrorKind::App,
            status: failure.status,
            verbatim: !failure.framed,
            source: failure.source,
            diagnostic: None,
        }
    }
}
fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("").trim_end()
}
fn escape_diagnostic(mut diagnostic: Diagnostic) -> Diagnostic {
    diagnostic.summary = escape_control_characters(diagnostic.summary);
    diagnostic.detail = escape_control_characters(diagnostic.detail);
    if let Some(range) = diagnostic.range.as_mut() {
        range.filename = escape_control_characters(std::mem::take(&mut range.filename));
    }
    diagnostic
}
impl From<String> for RunError {
    fn from(message: String) -> Self {
        Self::new(message, RunErrorKind::Handler)
    }
}
impl From<&str> for RunError {
    fn from(message: &str) -> Self {
        Self::new(message, RunErrorKind::Handler)
    }
}
impl From<RunError> for String {
    fn from(error: RunError) -> Self {
        error.into_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::DiagnosticKind;
    #[test]
    fn external_failure_rejects_success_and_preserves_metadata() {
        assert_eq!(
            ExternalFailure::new(0, "not a failure").unwrap_err(),
            InvalidExternalStatus
        );
        let failure = ExternalFailure::new(128, "fatal: repository missing\n")
            .unwrap()
            .with_source(std::io::Error::other("git failed"));
        assert_eq!(failure.exit_status().code(), 128);
        assert_eq!(failure.diagnostic(), "fatal: repository missing\n");
        assert_eq!(
            std::error::Error::source(&failure).unwrap().to_string(),
            "git failed"
        );
        let captured = RunError::from(failure);
        assert_eq!(captured.kind(), RunErrorKind::External);
        assert_eq!(captured.exit_status().code(), 128);
        assert_eq!(captured.as_str(), "fatal: repository missing\n");
        assert_eq!(
            std::error::Error::source(&captured).unwrap().to_string(),
            "git failed"
        );
    }
    #[test]
    #[should_panic(expected = "external run errors must be constructed from ExternalFailure")]
    fn run_error_new_rejects_external_kind() {
        let _ = RunError::new("inconsistent", RunErrorKind::External);
    }
    #[test]
    fn app_failure_rejects_success_and_preserves_metadata() {
        assert_eq!(
            AppFailure::new(0, "not a failure").unwrap_err(),
            InvalidAppStatus
        );
        let failure = AppFailure::new(1, "ghlike: repository not found: demo/gamma\n")
            .unwrap()
            .with_source(std::io::Error::other("lookup failed"));
        assert_eq!(failure.exit_status().code(), 1);
        assert_eq!(
            failure.diagnostic(),
            "ghlike: repository not found: demo/gamma\n"
        );
        assert_eq!(
            std::error::Error::source(&failure).unwrap().to_string(),
            "lookup failed"
        );
        let captured = RunError::from(failure);
        assert_eq!(captured.kind(), RunErrorKind::App);
        assert_eq!(captured.exit_status().code(), 1);
        assert_eq!(
            captured.as_str(),
            "ghlike: repository not found: demo/gamma\n"
        );
        assert!(captured.writes_diagnostic_verbatim());
        assert_eq!(
            std::error::Error::source(&captured).unwrap().to_string(),
            "lookup failed"
        );
    }
    #[test]
    fn an_app_failure_can_never_report_shell_success() {
        assert!(AppFailure::new(0, "").is_err());
        for status in 1..=u8::MAX {
            let failure = AppFailure::new(status, "domain error").expect("nonzero is accepted");
            assert_ne!(failure.exit_status(), ExitStatus::SUCCESS);
            assert_ne!(RunError::from(failure).exit_status(), ExitStatus::SUCCESS);
        }
    }
    #[test]
    #[should_panic(expected = "app run errors must be constructed from AppFailure")]
    fn run_error_new_rejects_app_kind() {
        let _ = RunError::new("inconsistent", RunErrorKind::App);
    }
    #[test]
    #[should_panic(expected = "config run errors must be constructed from RunError::config")]
    fn run_error_new_rejects_config_kind() {
        let _ = RunError::new("inconsistent", RunErrorKind::Config);
    }
    #[test]
    fn the_cause_carrying_constructors_keep_the_error_a_caller_can_downcast() {
        let write = RunError::final_write(
            "Error writing stdout",
            Arc::new(std::io::Error::from(std::io::ErrorKind::BrokenPipe)),
            OutputKind::Text,
        );
        assert_eq!(write.kind(), RunErrorKind::FinalWrite(OutputKind::Text));
        assert_eq!(
            std::error::Error::source(&write)
                .and_then(|source| source.downcast_ref::<std::io::Error>())
                .map(std::io::Error::kind),
            Some(std::io::ErrorKind::BrokenPipe)
        );

        let render = RunError::render("boom", Arc::new(std::io::Error::other("boom")));
        assert_eq!(render.kind(), RunErrorKind::Render);
        assert!(std::error::Error::source(&render).is_some());

        let config = RunError::config("boom", Arc::new(std::io::Error::other("boom")));
        assert_eq!(config.kind(), RunErrorKind::Config);
        assert_eq!(config.exit_status(), ExitStatus::FAILURE);
        assert!(std::error::Error::source(&config).is_some());
    }
    #[test]
    fn a_carried_diagnostic_wins_and_takes_the_framework_kind() {
        let carried = Diagnostic::error("line 2 does not parse")
            .detail("expected `resource <name> <state>`")
            .range("main.tfl", 2, 1);
        let error = RunError::new("Error: line 2 does not parse", RunErrorKind::Handler)
            .with_diagnostic(carried.clone());
        let diagnostic = error.diagnostic();
        assert_eq!(diagnostic.kind, DiagnosticKind::Handler);
        assert_eq!(diagnostic.severity, Severity::Error);
        assert_eq!(diagnostic.summary, carried.summary);
        assert_eq!(diagnostic.detail, carried.detail);
        assert_eq!(diagnostic.range, carried.range);
        let mut hook_carried = Diagnostic::warning("soft");
        hook_carried.kind = DiagnosticKind::ClapUsage;
        let hook = RunError::new("Error: soft", RunErrorKind::Hook(HookPhase::PostDispatch))
            .with_diagnostic(hook_carried);
        let hook = hook.diagnostic();
        assert_eq!(hook.kind, DiagnosticKind::HookPostDispatch);
        assert_eq!(hook.severity, Severity::Error);
    }
    #[test]
    fn a_prose_error_splits_into_summary_and_detail_without_its_framing() {
        let clap = RunError::new(
            "error: unexpected argument '--bogus' found\n\nUsage: app [OPTIONS]\n\nFor more information, try '--help'.\n",
            RunErrorKind::ClapUsage,
        )
        .diagnostic();
        assert_eq!(clap.kind, DiagnosticKind::ClapUsage);
        assert_eq!(clap.summary, "unexpected argument '--bogus' found");
        assert_eq!(
            clap.detail,
            "Usage: app [OPTIONS]\n\nFor more information, try '--help'."
        );
        assert_eq!(clap.range, None);
        let framed =
            RunError::new("Error: could not read config", RunErrorKind::Render).diagnostic();
        assert_eq!(framed.summary, "could not read config");
        assert_eq!(framed.detail, "");
        let bare = RunError::new("plain", RunErrorKind::FinalWrite(OutputKind::Text)).diagnostic();
        assert_eq!(bare.summary, "plain");
        assert_eq!(bare.kind, DiagnosticKind::FinalWrite);
    }
    #[test]
    fn framework_composed_prose_carries_no_terminal_escape_sequence() {
        let usage = RunError::new(
            "error: invalid value '\u{1b}]0;pwned\u{7}' for '--color <WHEN>'\n\nUsage: app [OPTIONS]\n",
            RunErrorKind::ClapUsage,
        );
        assert!(!usage.as_str().contains('\u{1b}'), "{:?}", usage.as_str());
        let diagnostic = usage.diagnostic();
        assert_eq!(
            diagnostic.summary,
            "invalid value '\\u{1b}]0;pwned\\u{7}' for '--color <WHEN>'"
        );
        assert_eq!(diagnostic.detail, "Usage: app [OPTIONS]");

        let carried = RunError::new("Error: bad archive", RunErrorKind::Handler).with_diagnostic(
            Diagnostic::error("bad entry \u{1b}]0;pwned\u{7}")
                .detail("in \u{1b}[2Jarchive")
                .range("\u{1b}]0;pwned\u{7}.tfl", 2, 1),
        );
        let carried = carried.diagnostic();
        assert_eq!(carried.summary, "bad entry \\u{1b}]0;pwned\\u{7}");
        assert_eq!(carried.detail, "in \\u{1b}[2Jarchive");
        assert_eq!(carried.range.unwrap().filename, "\\u{1b}]0;pwned\\u{7}.tfl");
    }
    #[test]
    fn owner_declared_failures_keep_their_bytes_verbatim() {
        let painted = "ghlike: \u{1b}]0;pwned\u{7}\n";
        let app = RunError::from(AppFailure::new(3, painted).unwrap());
        assert_eq!(app.as_str(), painted);
        assert_eq!(app.diagnostic().detail, painted);
        let external = RunError::from(ExternalFailure::new(128, painted).unwrap());
        assert_eq!(external.as_str(), painted);
        assert_eq!(external.diagnostic().detail, painted);
    }
    #[test]
    fn owner_declared_failures_keep_their_bytes_as_detail() {
        let app = RunError::from(
            AppFailure::new(3, "ghlike: not found: demo/gamma\nsee --help\n").unwrap(),
        )
        .diagnostic();
        assert_eq!(app.kind, DiagnosticKind::App);
        assert_eq!(app.summary, "ghlike: not found: demo/gamma");
        assert_eq!(app.detail, "ghlike: not found: demo/gamma\nsee --help\n");
        let external = RunError::from(ExternalFailure::new(128, "").unwrap()).diagnostic();
        assert_eq!(external.kind, DiagnosticKind::External);
        assert_eq!(external.summary, "");
        assert_eq!(external.detail, "");
    }
}
