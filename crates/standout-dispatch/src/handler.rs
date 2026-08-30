use crate::artifact::{Artifact, ArtifactRun};
use crate::hooks::HookPhase;
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
    Binary { data: Vec<u8>, filename: String },
    Artifact(Artifact<T>),
}
impl<T: Serialize> Output<T> {
    pub fn is_render(&self) -> bool {
        matches!(self, Output::Render(_))
    }
    pub fn is_silent(&self) -> bool {
        matches!(self, Output::Silent)
    }
    pub fn is_binary(&self) -> bool {
        matches!(self, Output::Binary { .. })
    }
    pub fn is_artifact(&self) -> bool {
        matches!(self, Output::Artifact(_))
    }
}
pub type HandlerResult<T> = Result<Output<T>, anyhow::Error>;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SuccessKind {
    Command,
    ClapHelp,
    ClapVersion,
    PagedHelp,
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
}
#[derive(Debug, Clone)]
pub struct RunOutput {
    text: String,
    kind: SuccessKind,
}
impl RunOutput {
    pub fn command(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: SuccessKind::Command,
        }
    }
    pub fn clap_help(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: SuccessKind::ClapHelp,
        }
    }
    pub fn paged_help(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: SuccessKind::PagedHelp,
        }
    }
    pub fn clap_version(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: SuccessKind::ClapVersion,
        }
    }
    pub fn as_str(&self) -> &str {
        &self.text
    }
    pub const fn kind(&self) -> SuccessKind {
        self.kind
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
    source: Option<Arc<dyn std::error::Error + Send + Sync + 'static>>,
}
impl RunError {
    pub fn new(message: impl Into<String>, kind: RunErrorKind) -> Self {
        assert!(
            kind != RunErrorKind::External,
            "external run errors must be constructed from ExternalFailure"
        );
        let status = match kind {
            RunErrorKind::ClapUsage => ExitStatus::USAGE_ERROR,
            _ => ExitStatus::FAILURE,
        };
        Self {
            message: message.into(),
            kind,
            status,
            source: None,
        }
    }
    pub fn with_source<E>(mut self, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        self.source = Some(Arc::new(source));
        self
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
            source: failure.source,
        }
    }
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
            DispatchResult::Handled(_)
            | DispatchResult::Binary(_, _)
            | DispatchResult::Artifact(_)
            | DispatchResult::Silent => Some(ExitStatus::SUCCESS),
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
    type Output: Serialize;
    fn handle(&mut self, matches: &ArgMatches, ctx: &CommandContext)
        -> HandlerResult<Self::Output>;
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
    type Output = T;
    fn handle(&mut self, matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<T> {
        (self.f)(matches, ctx).into_handler_result()
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
    type Output = T;
    fn handle(&mut self, matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<T> {
        (self.f)(matches).into_handler_result()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
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
        let result = handler.handle(&matches, &ctx);
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
        let _ = handler.handle(&matches, &ctx);
        let _ = handler.handle(&matches, &ctx);
        let result = handler.handle(&matches, &ctx);
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
    fn test_fn_handler_with_auto_wrap() {
        let mut handler = FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
            Ok::<_, anyhow::Error>("auto-wrapped".to_string())
        });
        let ctx = CommandContext::default();
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
        let result = handler.handle(&matches, &ctx);
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
        let result = handler.handle(&matches, &ctx);
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
        let result = handler.handle(&matches, &ctx);
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
        let result = handler.handle(&matches, &ctx);
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
        let result = handler.handle(&matches, &ctx);
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
        let result = handler.handle(&matches, &ctx);
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
        let result = handler.handle(&matches, &ctx);
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
        let _ = handler.handle(&matches, &ctx);
        let _ = handler.handle(&matches, &ctx);
        let result = handler.handle(&matches, &ctx);
        assert!(result.is_ok());
        match result.unwrap() {
            Output::Render(n) => assert_eq!(n, 3),
            _ => panic!("Expected Output::Render"),
        }
    }
}
