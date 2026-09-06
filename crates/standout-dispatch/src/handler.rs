use crate::artifact::{Artifact, ArtifactRun};
use crate::diagnostic::{Diagnostic, Severity};
use crate::escape::escape_control_characters;
use crate::hooks::HookPhase;
use crate::results::{NoEvents, Results};
use crate::verify::ExpectedArg;
use clap::ArgMatches;
use serde::Serialize;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;
#[derive(Default)]
pub struct Extensions {
    map: HashMap<TypeId, Box<dyn Any>>,
}
impl Extensions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert<T: 'static>(&mut self, val: T) -> Option<T> {
        self.map
            .insert(TypeId::of::<T>(), Box::new(val))
            .and_then(|boxed| boxed.downcast().ok().map(|b| *b))
    }
    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.map
            .get(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast_ref())
    }
    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.map
            .get_mut(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast_mut())
    }
    pub fn get_required<T: 'static>(&self) -> Result<&T, anyhow::Error> {
        self.get::<T>().ok_or_else(|| {
            anyhow::anyhow!(
                "Extension missing: type {} not found in context",
                std::any::type_name::<T>()
            )
        })
    }
    pub fn get_mut_required<T: 'static>(&mut self) -> Result<&mut T, anyhow::Error> {
        self.get_mut::<T>().ok_or_else(|| {
            anyhow::anyhow!(
                "Extension missing: type {} not found in context",
                std::any::type_name::<T>()
            )
        })
    }
    pub fn remove<T: 'static>(&mut self) -> Option<T> {
        self.map
            .remove(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast().ok().map(|b| *b))
    }
    pub fn contains<T: 'static>(&self) -> bool {
        self.map.contains_key(&TypeId::of::<T>())
    }
    pub fn len(&self) -> usize {
        self.map.len()
    }
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
    pub fn clear(&mut self) {
        self.map.clear();
    }
}
impl fmt::Debug for Extensions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Extensions")
            .field("len", &self.map.len())
            .finish_non_exhaustive()
    }
}
impl Clone for Extensions {
    // `Box<dyn Any>` isn't `Clone`: a clone starts empty.
    fn clone(&self) -> Self {
        Self::new()
    }
}
#[derive(Debug)]
pub struct CommandContext {
    pub command_path: Vec<String>,
    pub app_state: Rc<Extensions>,
    pub extensions: Extensions,
}
impl CommandContext {
    pub fn new(command_path: Vec<String>, app_state: Rc<Extensions>) -> Self {
        Self {
            command_path,
            app_state,
            extensions: Extensions::new(),
        }
    }
}
impl Default for CommandContext {
    fn default() -> Self {
        Self {
            command_path: Vec::new(),
            app_state: Rc::new(Extensions::new()),
            extensions: Extensions::new(),
        }
    }
}
#[derive(Debug)]
#[non_exhaustive]
pub enum Output<T: Serialize> {
    Render(T),
    Silent,
    Binary {
        data: Vec<u8>,
        filename: String,
    },
    Artifact(Artifact<T>),
    /// Emitted as `output` alone would be; the process exits with `status`.
    WithStatus {
        output: Box<Output<T>>,
        status: ExitStatus,
    },
}
impl<T: Serialize> Output<T> {
    /// A signal beside the result, never a failure; a later call replaces the earlier status.
    pub fn with_exit_status(self, status: ExitStatus) -> Self {
        let (output, _) = self.split_exit_status();
        Output::WithStatus {
            output: Box::new(output),
            status,
        }
    }
    pub fn split_exit_status(self) -> (Self, Option<ExitStatus>) {
        match self {
            Output::WithStatus { output, status } => (output.split_exit_status().0, Some(status)),
            other => (other, None),
        }
    }
    pub fn exit_status(&self) -> ExitStatus {
        match self {
            Output::WithStatus { status, .. } => *status,
            _ => ExitStatus::SUCCESS,
        }
    }
    pub fn map_render(self, f: impl FnOnce(T) -> T) -> Self {
        match self {
            Output::Render(data) => Output::Render(f(data)),
            Output::WithStatus { output, status } => Output::WithStatus {
                output: Box::new(output.map_render(f)),
                status,
            },
            other => other,
        }
    }
    fn declared(&self) -> &Self {
        match self {
            Output::WithStatus { output, .. } => output.declared(),
            other => other,
        }
    }
    pub fn is_render(&self) -> bool {
        matches!(self.declared(), Output::Render(_))
    }
    pub fn is_silent(&self) -> bool {
        matches!(self.declared(), Output::Silent)
    }
    pub fn is_binary(&self) -> bool {
        matches!(self.declared(), Output::Binary { .. })
    }
    pub fn is_artifact(&self) -> bool {
        matches!(self.declared(), Output::Artifact(_))
    }
}
pub type HandlerResult<T> = Result<Output<T>, anyhow::Error>;

/// What a command that declares events returns once the events are done: they
/// already carried the run's results, so a payload has nowhere left to go.
#[derive(Debug)]
#[non_exhaustive]
pub enum Summary<T: Serialize> {
    Render(T),
    Silent,
    /// Emitted as `summary` alone would be; the process exits with `status`.
    WithStatus {
        summary: Box<Summary<T>>,
        status: ExitStatus,
    },
}

impl<T: Serialize> Summary<T> {
    /// A signal beside the result, never a failure; a later call replaces the earlier status.
    pub fn with_exit_status(self, status: ExitStatus) -> Self {
        let (summary, _) = self.split_exit_status();
        Summary::WithStatus {
            summary: Box::new(summary),
            status,
        }
    }

    pub fn split_exit_status(self) -> (Self, Option<ExitStatus>) {
        match self {
            Summary::WithStatus { summary, status } => {
                (summary.split_exit_status().0, Some(status))
            }
            other => (other, None),
        }
    }

    pub fn exit_status(&self) -> ExitStatus {
        match self {
            Summary::WithStatus { status, .. } => *status,
            _ => ExitStatus::SUCCESS,
        }
    }

    pub fn map_render(self, f: impl FnOnce(T) -> T) -> Self {
        match self {
            Summary::Render(data) => Summary::Render(f(data)),
            Summary::WithStatus { summary, status } => Summary::WithStatus {
                summary: Box::new(summary.map_render(f)),
                status,
            },
            other => other,
        }
    }

    fn declared(&self) -> &Self {
        match self {
            Summary::WithStatus { summary, .. } => summary.declared(),
            other => other,
        }
    }

    pub fn is_render(&self) -> bool {
        matches!(self.declared(), Summary::Render(_))
    }

    pub fn is_silent(&self) -> bool {
        matches!(self.declared(), Summary::Silent)
    }
}

impl<T: Serialize> From<Summary<T>> for Output<T> {
    fn from(summary: Summary<T>) -> Self {
        match summary {
            Summary::Render(data) => Output::Render(data),
            Summary::Silent => Output::Silent,
            Summary::WithStatus { summary, status } => Output::WithStatus {
                output: Box::new(Output::from(*summary)),
                status,
            },
        }
    }
}

pub type SummaryResult<T> = Result<Summary<T>, anyhow::Error>;

#[diagnostic::on_unimplemented(
    message = "a command that declares events returns a `Summary`, not an `Output`",
    note = "the events carried the run's results already, so a summary is `Render` or `Silent`"
)]
pub trait IntoSummaryResult<T: Serialize> {
    fn into_summary_result(self) -> SummaryResult<T>;
}

#[diagnostic::do_not_recommend]
impl<T, E> IntoSummaryResult<T> for Result<T, E>
where
    T: Serialize,
    E: Into<anyhow::Error>,
{
    fn into_summary_result(self) -> SummaryResult<T> {
        self.map(Summary::Render).map_err(Into::into)
    }
}

impl<T, E> IntoSummaryResult<T> for Result<Summary<T>, E>
where
    T: Serialize,
    E: Into<anyhow::Error>,
{
    fn into_summary_result(self) -> SummaryResult<T> {
        self.map_err(Into::into)
    }
}

mod outcome {
    pub trait Sealed {}
}

/// What `Handler::handle` may return, tied to the command's event type:
/// `Output<T>` is an outcome only for `NoEvents`, so a command that declares
/// events has `Summary<T>` and no payload variant to return.
#[diagnostic::on_unimplemented(
    message = "a command that declares events returns `Summary<{T}>`, not `Output<{T}>`",
    note = "the events carried the run's results already, so a summary is `Render` or `Silent`"
)]
pub trait HandlerOutcome<T: Serialize, E: Serialize + 'static>: outcome::Sealed {
    fn into_output(self) -> Output<T>;
}

impl<T: Serialize> outcome::Sealed for Output<T> {}

impl<T: Serialize> outcome::Sealed for Summary<T> {}

impl<T: Serialize> HandlerOutcome<T, NoEvents> for Output<T> {
    fn into_output(self) -> Output<T> {
        self
    }
}

impl<T: Serialize, E: Serialize + 'static> HandlerOutcome<T, E> for Summary<T> {
    fn into_output(self) -> Output<T> {
        Output::from(self)
    }
}

pub trait IntoHandlerResult<T: Serialize> {
    fn into_handler_result(self) -> HandlerResult<T>;
}
impl<T, E> IntoHandlerResult<T> for Result<T, E>
where
    T: Serialize,
    E: Into<anyhow::Error>,
{
    fn into_handler_result(self) -> HandlerResult<T> {
        self.map(Output::Render).map_err(Into::into)
    }
}
impl<T: Serialize> IntoHandlerResult<T> for HandlerResult<T> {
    fn into_handler_result(self) -> HandlerResult<T> {
        self
    }
}
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
pub enum SuccessKind {
    Command,
    ClapHelp,
    ClapVersion,
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
pub struct RunOutput {
    text: String,
    kind: SuccessKind,
    status: ExitStatus,
    warnings_included: bool,
}
impl RunOutput {
    pub fn command(text: impl Into<String>) -> Self {
        Self::new(text, SuccessKind::Command)
    }
    pub fn clap_help(text: impl Into<String>) -> Self {
        Self::new(text, SuccessKind::ClapHelp)
    }
    pub fn clap_version(text: impl Into<String>) -> Self {
        Self::new(text, SuccessKind::ClapVersion)
    }
    fn new(text: impl Into<String>, kind: SuccessKind) -> Self {
        Self {
            text: text.into(),
            kind,
            status: ExitStatus::SUCCESS,
            warnings_included: false,
        }
    }
    pub fn with_exit_status(mut self, status: ExitStatus) -> Self {
        self.status = status;
        self
    }
    /// Marks output whose document already carries the run's warning records,
    /// so the framework neither appends them nor renders them to stderr.
    pub fn with_warnings_included(mut self, included: bool) -> Self {
        self.warnings_included = included;
        self
    }
    pub const fn warnings_included(&self) -> bool {
        self.warnings_included
    }
    pub fn as_str(&self) -> &str {
        &self.text
    }
    pub const fn kind(&self) -> SuccessKind {
        self.kind
    }
    pub const fn exit_status(&self) -> ExitStatus {
        self.status
    }
    pub fn into_string(self) -> String {
        self.text
    }
}
impl std::ops::Deref for RunOutput {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}
impl AsRef<str> for RunOutput {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl fmt::Display for RunOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl PartialEq<str> for RunOutput {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}
impl PartialEq<&str> for RunOutput {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}
impl PartialEq<String> for RunOutput {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other
    }
}
impl From<String> for RunOutput {
    fn from(text: String) -> Self {
        Self::command(text)
    }
}
impl From<&str> for RunOutput {
    fn from(text: &str) -> Self {
        Self::command(text)
    }
}
impl From<RunOutput> for String {
    fn from(output: RunOutput) -> Self {
        output.into_string()
    }
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
    pub fn final_write<E>(message: impl Into<String>, error: E, kind: OutputKind) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::of_kind(message, RunErrorKind::FinalWrite(kind)).with_source(error)
    }
    /// Turning the run's data into bytes failed; `error` is what the renderer or serializer reported.
    pub fn render<E>(message: impl Into<String>, error: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::of_kind(message, RunErrorKind::Render).with_source(error)
    }
    /// Resolving the application's configuration failed; `error` is what the resolver reported.
    pub fn config<E>(message: impl Into<String>, error: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
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
    pub fn with_source<E>(mut self, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        self.source = Some(Arc::new(source));
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
#[derive(Debug)]
#[non_exhaustive]
pub enum DispatchResult {
    Handled(RunOutput),
    Binary(Vec<u8>, String),
    Artifact(ArtifactRun),
    Silent,
    Error(RunError),
    NoMatch(ArgMatches),
}
impl DispatchResult {
    pub fn is_handled(&self) -> bool {
        matches!(self, DispatchResult::Handled(_))
    }
    pub fn is_binary(&self) -> bool {
        matches!(self, DispatchResult::Binary(_, _))
    }
    pub fn is_artifact(&self) -> bool {
        matches!(self, DispatchResult::Artifact(_))
    }
    pub fn is_silent(&self) -> bool {
        matches!(self, DispatchResult::Silent)
    }
    pub fn is_error(&self) -> bool {
        matches!(self, DispatchResult::Error(_))
    }
    pub fn output(&self) -> Option<&str> {
        match self {
            DispatchResult::Handled(s) => Some(s),
            _ => None,
        }
    }
    pub fn error(&self) -> Option<&str> {
        match self {
            DispatchResult::Error(s) => Some(s),
            _ => None,
        }
    }
    pub fn success_kind(&self) -> Option<SuccessKind> {
        match self {
            DispatchResult::Handled(output) => Some(output.kind()),
            DispatchResult::Binary(_, _) | DispatchResult::Artifact(_) | DispatchResult::Silent => {
                Some(SuccessKind::Command)
            }
            _ => None,
        }
    }
    pub fn error_kind(&self) -> Option<RunErrorKind> {
        match self {
            DispatchResult::Error(error) => Some(error.kind()),
            _ => None,
        }
    }
    pub fn exit_status(&self) -> Option<ExitStatus> {
        match self {
            DispatchResult::Handled(output) => Some(output.exit_status()),
            DispatchResult::Binary(_, _) | DispatchResult::Artifact(_) | DispatchResult::Silent => {
                Some(ExitStatus::SUCCESS)
            }
            DispatchResult::Error(error) => Some(error.exit_status()),
            DispatchResult::NoMatch(_) => None,
        }
    }
    pub fn binary(&self) -> Option<(&[u8], &str)> {
        match self {
            DispatchResult::Binary(bytes, filename) => Some((bytes, filename)),
            _ => None,
        }
    }
    pub fn artifact(&self) -> Option<&ArtifactRun> {
        match self {
            DispatchResult::Artifact(run) => Some(run),
            _ => None,
        }
    }
    pub fn matches(&self) -> Option<&ArgMatches> {
        match self {
            DispatchResult::NoMatch(m) => Some(m),
            _ => None,
        }
    }
}
pub trait Handler {
    /// The type of the values the command produces while it runs, or
    /// [`NoEvents`] for a command that produces none, which is what
    /// [`emits_events`](crate::emits_events) reads.
    type Event: Serialize + 'static;
    type Output: Serialize;
    /// [`Output`] for a batch command, [`Summary`] for one that declares
    /// events: [`HandlerOutcome`] admits the first only under [`NoEvents`].
    type Outcome: HandlerOutcome<Self::Output, Self::Event>;
    fn handle(
        &mut self,
        matches: &ArgMatches,
        ctx: &CommandContext,
        results: &mut Results<Self::Event>,
    ) -> Result<Self::Outcome, anyhow::Error>;
    fn expected_args(&self) -> Vec<ExpectedArg> {
        Vec::new()
    }
}
pub struct FnHandler<F, T, R = HandlerResult<T>>
where
    T: Serialize,
{
    f: F,
    _phantom: std::marker::PhantomData<fn() -> (T, R)>,
}
impl<F, T, R> FnHandler<F, T, R>
where
    F: FnMut(&ArgMatches, &CommandContext) -> R,
    R: IntoHandlerResult<T>,
    T: Serialize,
{
    pub fn new(f: F) -> Self {
        Self {
            f,
            _phantom: std::marker::PhantomData,
        }
    }
}
impl<F, T, R> Handler for FnHandler<F, T, R>
where
    F: FnMut(&ArgMatches, &CommandContext) -> R,
    R: IntoHandlerResult<T>,
    T: Serialize,
{
    type Event = NoEvents;
    type Output = T;
    type Outcome = Output<T>;
    fn handle(
        &mut self,
        matches: &ArgMatches,
        ctx: &CommandContext,
        _results: &mut Results<NoEvents>,
    ) -> HandlerResult<T> {
        (self.f)(matches, ctx).into_handler_result()
    }
}
/// The adapter behind a three-argument closure, whose third parameter is the
/// command's typed results channel, so `Event` is the closure's event type.
pub struct EventsFnHandler<F, E, T, R = SummaryResult<T>>
where
    E: Serialize + 'static,
    T: Serialize,
{
    f: F,
    _event: std::marker::PhantomData<fn(E)>,
    _phantom: std::marker::PhantomData<fn() -> (T, R)>,
}
impl<F, E, T, R> EventsFnHandler<F, E, T, R>
where
    F: FnMut(&ArgMatches, &CommandContext, &mut Results<E>) -> R,
    R: IntoSummaryResult<T>,
    E: Serialize + 'static,
    T: Serialize,
{
    pub fn new(f: F) -> Self {
        Self {
            f,
            _event: std::marker::PhantomData,
            _phantom: std::marker::PhantomData,
        }
    }
}
impl<F, E, T, R> Handler for EventsFnHandler<F, E, T, R>
where
    F: FnMut(&ArgMatches, &CommandContext, &mut Results<E>) -> R,
    R: IntoSummaryResult<T>,
    E: Serialize + 'static,
    T: Serialize,
{
    type Event = E;
    type Output = T;
    type Outcome = Summary<T>;
    fn handle(
        &mut self,
        matches: &ArgMatches,
        ctx: &CommandContext,
        results: &mut Results<E>,
    ) -> SummaryResult<T> {
        (self.f)(matches, ctx, results).into_summary_result()
    }
}
pub struct SimpleFnHandler<F, T, R = HandlerResult<T>>
where
    T: Serialize,
{
    f: F,
    _phantom: std::marker::PhantomData<fn() -> (T, R)>,
}
impl<F, T, R> SimpleFnHandler<F, T, R>
where
    F: FnMut(&ArgMatches) -> R,
    R: IntoHandlerResult<T>,
    T: Serialize,
{
    pub fn new(f: F) -> Self {
        Self {
            f,
            _phantom: std::marker::PhantomData,
        }
    }
}
impl<F, T, R> Handler for SimpleFnHandler<F, T, R>
where
    F: FnMut(&ArgMatches) -> R,
    R: IntoHandlerResult<T>,
    T: Serialize,
{
    type Event = NoEvents;
    type Output = T;
    type Outcome = Output<T>;
    fn handle(
        &mut self,
        matches: &ArgMatches,
        _ctx: &CommandContext,
        _results: &mut Results<NoEvents>,
    ) -> HandlerResult<T> {
        (self.f)(matches).into_handler_result()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::DiagnosticKind;
    use serde_json::json;
    #[test]
    fn test_command_context_creation() {
        let ctx = CommandContext {
            command_path: vec!["config".into(), "get".into()],
            app_state: Rc::new(Extensions::new()),
            extensions: Extensions::new(),
        };
        assert_eq!(ctx.command_path, vec!["config", "get"]);
    }
    #[derive(Debug, thiserror::Error)]
    #[error("the store refused")]
    struct StoreRefused;

    #[test]
    fn a_summary_carrying_an_error_of_its_own_converts_to_a_summary_result() {
        let rendered: Result<Summary<u8>, StoreRefused> = Ok(Summary::Render(7));
        assert!(matches!(
            rendered.into_summary_result(),
            Ok(Summary::Render(7))
        ));

        let refused: Result<Summary<u8>, StoreRefused> = Err(StoreRefused);
        assert_eq!(
            refused.into_summary_result().unwrap_err().to_string(),
            "the store refused"
        );
    }

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
            std::io::Error::from(std::io::ErrorKind::BrokenPipe),
            OutputKind::Text,
        );
        assert_eq!(write.kind(), RunErrorKind::FinalWrite(OutputKind::Text));
        assert_eq!(
            std::error::Error::source(&write)
                .and_then(|source| source.downcast_ref::<std::io::Error>())
                .map(std::io::Error::kind),
            Some(std::io::ErrorKind::BrokenPipe)
        );

        let render = RunError::render("boom", std::io::Error::other("boom"));
        assert_eq!(render.kind(), RunErrorKind::Render);
        assert!(std::error::Error::source(&render).is_some());

        let config = RunError::config("boom", std::io::Error::other("boom"));
        assert_eq!(config.kind(), RunErrorKind::Config);
        assert_eq!(config.exit_status(), ExitStatus::FAILURE);
        assert!(std::error::Error::source(&config).is_some());
    }
    #[test]
    fn test_command_context_default() {
        let ctx = CommandContext::default();
        assert!(ctx.command_path.is_empty());
        assert!(ctx.extensions.is_empty());
        assert!(ctx.app_state.is_empty());
    }
    #[test]
    fn test_command_context_with_app_state() {
        struct Database {
            url: String,
        }
        struct Config {
            debug: bool,
        }
        let mut app_state = Extensions::new();
        app_state.insert(Database {
            url: "postgres://localhost".into(),
        });
        app_state.insert(Config { debug: true });
        let app_state = Rc::new(app_state);
        let ctx = CommandContext {
            command_path: vec!["list".into()],
            app_state: app_state.clone(),
            extensions: Extensions::new(),
        };
        let db = ctx.app_state.get::<Database>().unwrap();
        assert_eq!(db.url, "postgres://localhost");
        let config = ctx.app_state.get::<Config>().unwrap();
        assert!(config.debug);
        assert_eq!(Rc::strong_count(&ctx.app_state), 2);
    }
    #[test]
    fn test_command_context_app_state_get_required() {
        struct Present;
        let mut app_state = Extensions::new();
        app_state.insert(Present);
        let ctx = CommandContext {
            command_path: vec![],
            app_state: Rc::new(app_state),
            extensions: Extensions::new(),
        };
        assert!(ctx.app_state.get_required::<Present>().is_ok());
        #[derive(Debug)]
        struct Missing;
        let err = ctx.app_state.get_required::<Missing>();
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("Extension missing"));
    }
    #[test]
    fn test_extensions_insert_and_get() {
        struct MyState {
            value: i32,
        }
        let mut ext = Extensions::new();
        assert!(ext.is_empty());
        ext.insert(MyState { value: 42 });
        assert!(!ext.is_empty());
        assert_eq!(ext.len(), 1);
        let state = ext.get::<MyState>().unwrap();
        assert_eq!(state.value, 42);
    }
    #[test]
    fn test_extensions_get_mut() {
        struct Counter {
            count: i32,
        }
        let mut ext = Extensions::new();
        ext.insert(Counter { count: 0 });
        if let Some(counter) = ext.get_mut::<Counter>() {
            counter.count += 1;
        }
        assert_eq!(ext.get::<Counter>().unwrap().count, 1);
    }
    #[test]
    fn test_extensions_multiple_types() {
        struct TypeA(i32);
        struct TypeB(String);
        let mut ext = Extensions::new();
        ext.insert(TypeA(1));
        ext.insert(TypeB("hello".into()));
        assert_eq!(ext.len(), 2);
        assert_eq!(ext.get::<TypeA>().unwrap().0, 1);
        assert_eq!(ext.get::<TypeB>().unwrap().0, "hello");
    }
    #[test]
    fn test_extensions_replace() {
        struct Value(i32);
        let mut ext = Extensions::new();
        ext.insert(Value(1));
        let old = ext.insert(Value(2));
        assert_eq!(old.unwrap().0, 1);
        assert_eq!(ext.get::<Value>().unwrap().0, 2);
    }
    #[test]
    fn test_extensions_remove() {
        struct Value(i32);
        let mut ext = Extensions::new();
        ext.insert(Value(42));
        let removed = ext.remove::<Value>();
        assert_eq!(removed.unwrap().0, 42);
        assert!(ext.is_empty());
        assert!(ext.get::<Value>().is_none());
    }
    #[test]
    fn test_extensions_contains() {
        struct Present;
        struct Absent;
        let mut ext = Extensions::new();
        ext.insert(Present);
        assert!(ext.contains::<Present>());
        assert!(!ext.contains::<Absent>());
    }
    #[test]
    fn test_extensions_clear() {
        struct A;
        struct B;
        let mut ext = Extensions::new();
        ext.insert(A);
        ext.insert(B);
        assert_eq!(ext.len(), 2);
        ext.clear();
        assert!(ext.is_empty());
    }
    #[test]
    fn test_extensions_missing_type_returns_none() {
        struct NotInserted;
        let ext = Extensions::new();
        assert!(ext.get::<NotInserted>().is_none());
    }
    #[test]
    fn test_extensions_get_required() {
        #[derive(Debug)]
        struct Config {
            value: i32,
        }
        let mut ext = Extensions::new();
        ext.insert(Config { value: 100 });
        let val = ext.get_required::<Config>();
        assert!(val.is_ok());
        assert_eq!(val.unwrap().value, 100);
        #[derive(Debug)]
        struct Missing;
        let err = ext.get_required::<Missing>();
        assert!(err.is_err());
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("Extension missing: type"));
    }
    #[test]
    fn test_extensions_get_mut_required() {
        #[derive(Debug)]
        struct State {
            count: i32,
        }
        let mut ext = Extensions::new();
        ext.insert(State { count: 0 });
        {
            let val = ext.get_mut_required::<State>();
            assert!(val.is_ok());
            val.unwrap().count += 1;
        }
        assert_eq!(ext.get_required::<State>().unwrap().count, 1);
        #[derive(Debug)]
        struct Missing;
        let err = ext.get_mut_required::<Missing>();
        assert!(err.is_err());
    }
    #[test]
    fn test_extensions_clone_behavior() {
        struct Data(#[allow(dead_code)] i32);
        let mut original = Extensions::new();
        original.insert(Data(42));
        let cloned = original.clone();
        assert!(original.get::<Data>().is_some());
        assert!(cloned.is_empty());
        assert!(cloned.get::<Data>().is_none());
    }
    #[test]
    fn test_output_render() {
        let output: Output<String> = Output::Render("success".into());
        assert!(output.is_render());
        assert!(!output.is_silent());
        assert!(!output.is_binary());
    }
    #[test]
    fn test_output_silent() {
        let output: Output<String> = Output::Silent;
        assert!(!output.is_render());
        assert!(output.is_silent());
        assert!(!output.is_binary());
    }
    #[test]
    fn a_declared_status_rides_beside_the_output_and_the_last_one_wins() {
        let plain: Output<String> = Output::Render("found nothing".into());
        assert_eq!(plain.exit_status(), ExitStatus::SUCCESS);
        assert_eq!(plain.split_exit_status().1, None);

        let signalled = Output::Render(String::from("changes"))
            .with_exit_status(ExitStatus::from(3))
            .with_exit_status(ExitStatus::from(2));
        assert_eq!(signalled.exit_status(), ExitStatus::from(2));
        assert!(signalled.is_render());
        assert!(!signalled.is_silent());

        let stamped = signalled.map_render(|text| format!("{text}!"));
        let (output, status) = stamped.split_exit_status();
        assert_eq!(status, Some(ExitStatus::from(2)));
        assert!(matches!(output, Output::Render(ref text) if text == "changes!"));

        let silent: Output<()> = Output::Silent.with_exit_status(ExitStatus::from(4));
        assert!(silent.is_silent());
        assert_eq!(silent.split_exit_status().1, Some(ExitStatus::from(4)));
    }
    #[test]
    fn a_handled_run_reports_the_status_its_output_declared() {
        let handled = DispatchResult::Handled(
            RunOutput::command("plan").with_exit_status(ExitStatus::from(2)),
        );
        assert_eq!(handled.exit_status(), Some(ExitStatus::from(2)));
        assert_eq!(handled.success_kind(), Some(SuccessKind::Command));
        assert!(!handled.is_error());
        assert_eq!(
            DispatchResult::Handled(RunOutput::command("plan")).exit_status(),
            Some(ExitStatus::SUCCESS)
        );
    }
    #[test]
    fn test_output_binary() {
        let output: Output<String> = Output::Binary {
            data: vec![0x25, 0x50, 0x44, 0x46],
            filename: "report.pdf".into(),
        };
        assert!(!output.is_render());
        assert!(!output.is_silent());
        assert!(output.is_binary());
    }
    #[test]
    fn test_run_result_handled() {
        let result = DispatchResult::Handled("output".into());
        assert!(result.is_handled());
        assert!(!result.is_binary());
        assert!(!result.is_silent());
        assert_eq!(result.output(), Some("output"));
        assert!(result.matches().is_none());
    }
    #[test]
    fn test_run_result_silent() {
        let result = DispatchResult::Silent;
        assert!(!result.is_handled());
        assert!(!result.is_binary());
        assert!(result.is_silent());
    }
    #[test]
    fn test_run_result_binary() {
        let bytes = vec![0x25, 0x50, 0x44, 0x46];
        let result = DispatchResult::Binary(bytes.clone(), "report.pdf".into());
        assert!(!result.is_handled());
        assert!(result.is_binary());
        assert!(!result.is_silent());
        let (data, filename) = result.binary().unwrap();
        assert_eq!(data, &bytes);
        assert_eq!(filename, "report.pdf");
    }
    #[test]
    fn test_run_result_no_match() {
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
        let result = DispatchResult::NoMatch(matches);
        assert!(!result.is_handled());
        assert!(!result.is_binary());
        assert!(result.matches().is_some());
    }
    #[test]
    fn test_fn_handler() {
        let mut handler = FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
            Ok(Output::Render(json!({"status": "ok"})))
        });
        let ctx = CommandContext::default();
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
        let result = handler.handle(&matches, &ctx, &mut Results::discarding());
        assert!(result.is_ok());
    }
    #[test]
    fn test_fn_handler_mutation() {
        let mut counter = 0u32;
        let mut handler = FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
            counter += 1;
            Ok(Output::Render(counter))
        });
        let ctx = CommandContext::default();
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
        let _ = handler.handle(&matches, &ctx, &mut Results::discarding());
        let _ = handler.handle(&matches, &ctx, &mut Results::discarding());
        let result = handler.handle(&matches, &ctx, &mut Results::discarding());
        assert!(result.is_ok());
        if let Ok(Output::Render(count)) = result {
            assert_eq!(count, 3);
        }
    }
    #[test]
    fn test_into_handler_result_from_result_ok() {
        use super::IntoHandlerResult;
        let result: Result<String, anyhow::Error> = Ok("hello".to_string());
        let handler_result = result.into_handler_result();
        assert!(handler_result.is_ok());
        match handler_result.unwrap() {
            Output::Render(s) => assert_eq!(s, "hello"),
            _ => panic!("Expected Output::Render"),
        }
    }
    #[test]
    fn test_into_handler_result_from_result_err() {
        use super::IntoHandlerResult;
        let result: Result<String, anyhow::Error> = Err(anyhow::anyhow!("test error"));
        let handler_result = result.into_handler_result();
        assert!(handler_result.is_err());
        assert!(handler_result
            .unwrap_err()
            .to_string()
            .contains("test error"));
    }
    #[test]
    fn test_into_handler_result_passthrough_render() {
        use super::IntoHandlerResult;
        let handler_result: HandlerResult<String> = Ok(Output::Render("hello".to_string()));
        let result = handler_result.into_handler_result();
        assert!(result.is_ok());
        match result.unwrap() {
            Output::Render(s) => assert_eq!(s, "hello"),
            _ => panic!("Expected Output::Render"),
        }
    }
    #[test]
    fn test_into_handler_result_passthrough_silent() {
        use super::IntoHandlerResult;
        let handler_result: HandlerResult<String> = Ok(Output::Silent);
        let result = handler_result.into_handler_result();
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Output::Silent));
    }
    #[test]
    fn test_into_handler_result_passthrough_binary() {
        use super::IntoHandlerResult;
        let handler_result: HandlerResult<String> = Ok(Output::Binary {
            data: vec![1, 2, 3],
            filename: "test.bin".to_string(),
        });
        let result = handler_result.into_handler_result();
        assert!(result.is_ok());
        match result.unwrap() {
            Output::Binary { data, filename } => {
                assert_eq!(data, vec![1, 2, 3]);
                assert_eq!(filename, "test.bin");
            }
            _ => panic!("Expected Output::Binary"),
        }
    }
    #[test]
    fn a_summary_becomes_the_output_the_presentation_pipeline_consumes() {
        assert!(Output::from(Summary::Render("done".to_string())).is_render());
        assert!(Output::from(Summary::<String>::Silent).is_silent());

        let carried =
            Output::from(Summary::Render("done".to_string()).with_exit_status(ExitStatus::from(2)));
        assert_eq!(carried.exit_status(), ExitStatus::from(2));
        assert!(carried.is_render());
    }

    #[test]
    fn a_later_exit_status_on_a_summary_replaces_the_earlier_one() {
        let summary = Summary::<String>::Silent
            .with_exit_status(ExitStatus::from(2))
            .with_exit_status(ExitStatus::from(3));
        let (declared, status) = summary.split_exit_status();
        assert_eq!(status, Some(ExitStatus::from(3)));
        assert!(declared.is_silent());
    }

    #[test]
    fn a_plain_value_from_an_emitting_closure_becomes_a_rendered_summary() {
        use super::IntoSummaryResult;
        let summary = Ok::<_, anyhow::Error>("hello".to_string())
            .into_summary_result()
            .unwrap();
        assert!(matches!(summary, Summary::Render(ref s) if s == "hello"));
    }

    #[test]
    fn test_fn_handler_with_auto_wrap() {
        let mut handler = FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
            Ok::<_, anyhow::Error>("auto-wrapped".to_string())
        });
        let ctx = CommandContext::default();
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
        let result = handler.handle(&matches, &ctx, &mut Results::discarding());
        assert!(result.is_ok());
        match result.unwrap() {
            Output::Render(s) => assert_eq!(s, "auto-wrapped"),
            _ => panic!("Expected Output::Render"),
        }
    }
    #[test]
    fn test_fn_handler_with_explicit_output() {
        let mut handler =
            FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| Ok(Output::<()>::Silent));
        let ctx = CommandContext::default();
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
        let result = handler.handle(&matches, &ctx, &mut Results::discarding());
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Output::Silent));
    }
    #[test]
    fn test_fn_handler_with_custom_error_type() {
        #[derive(Debug)]
        struct CustomError(String);
        impl std::fmt::Display for CustomError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "CustomError: {}", self.0)
            }
        }
        impl std::error::Error for CustomError {}
        let mut handler = FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
            Err::<String, CustomError>(CustomError("oops".to_string()))
        });
        let ctx = CommandContext::default();
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
        let result = handler.handle(&matches, &ctx, &mut Results::discarding());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("CustomError: oops"));
    }
    #[test]
    fn test_simple_fn_handler_basic() {
        use super::SimpleFnHandler;
        let mut handler = SimpleFnHandler::new(|_m: &ArgMatches| {
            Ok::<_, anyhow::Error>("no context needed".to_string())
        });
        let ctx = CommandContext::default();
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
        let result = handler.handle(&matches, &ctx, &mut Results::discarding());
        assert!(result.is_ok());
        match result.unwrap() {
            Output::Render(s) => assert_eq!(s, "no context needed"),
            _ => panic!("Expected Output::Render"),
        }
    }
    #[test]
    fn test_simple_fn_handler_with_args() {
        use super::SimpleFnHandler;
        let mut handler = SimpleFnHandler::new(|m: &ArgMatches| {
            let verbose = m.get_flag("verbose");
            Ok::<_, anyhow::Error>(verbose)
        });
        let ctx = CommandContext::default();
        let matches = clap::Command::new("test")
            .arg(
                clap::Arg::new("verbose")
                    .short('v')
                    .action(clap::ArgAction::SetTrue),
            )
            .get_matches_from(vec!["test", "-v"]);
        let result = handler.handle(&matches, &ctx, &mut Results::discarding());
        assert!(result.is_ok());
        match result.unwrap() {
            Output::Render(v) => assert!(v),
            _ => panic!("Expected Output::Render"),
        }
    }
    #[test]
    fn test_simple_fn_handler_explicit_output() {
        use super::SimpleFnHandler;
        let mut handler = SimpleFnHandler::new(|_m: &ArgMatches| Ok(Output::<()>::Silent));
        let ctx = CommandContext::default();
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
        let result = handler.handle(&matches, &ctx, &mut Results::discarding());
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), Output::Silent));
    }
    #[test]
    fn test_simple_fn_handler_error() {
        use super::SimpleFnHandler;
        let mut handler = SimpleFnHandler::new(|_m: &ArgMatches| {
            Err::<String, _>(anyhow::anyhow!("simple error"))
        });
        let ctx = CommandContext::default();
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
        let result = handler.handle(&matches, &ctx, &mut Results::discarding());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("simple error"));
    }
    #[test]
    fn test_simple_fn_handler_mutation() {
        use super::SimpleFnHandler;
        let mut counter = 0u32;
        let mut handler = SimpleFnHandler::new(|_m: &ArgMatches| {
            counter += 1;
            Ok::<_, anyhow::Error>(counter)
        });
        let ctx = CommandContext::default();
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
        let _ = handler.handle(&matches, &ctx, &mut Results::discarding());
        let _ = handler.handle(&matches, &ctx, &mut Results::discarding());
        let result = handler.handle(&matches, &ctx, &mut Results::discarding());
        assert!(result.is_ok());
        match result.unwrap() {
            Output::Render(n) => assert_eq!(n, 3),
            _ => panic!("Expected Output::Render"),
        }
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
