//! Command handler types.
//!
//! This module provides the core types for building CLI handler adapters in the
//! dispatch pipeline.
//!
//! # Design Rationale
//!
//! Handler adapters are responsible for the CLI-to-application seam. They:
//!
//! - Receive parsed CLI arguments (`&ArgMatches`) and execution context
//! - Call a CLI-free application library
//! - Map library results into serializable CLI view data for the render handler
//!
//! Handlers explicitly do not handle:
//! - Output formatting (that's the render handler's job)
//! - Template selection (that's configured at the framework level)
//! - Theme/style decisions (that's the render handler's job)
//!
//! This separation keeps handlers focused and testable - you can unit test
//! a handler by checking the data it returns, without worrying about rendering.
//!
//! # State Management: App State vs Extensions
//!
//! [`CommandContext`] provides two mechanisms for state injection:
//!
//! | Field | Mutability | Lifetime | Purpose |
//! |-------|------------|----------|---------|
//! | `app_state` | Immutable (`&`) | App lifetime (shared via Arc) | Database, Config, API clients |
//! | `extensions` | Mutable (`&mut`) | Request lifetime | Per-request state, user scope |
//!
//! **App State** is configured at app build time via `AppBuilder::app_state()` and shared
//! immutably across all command invocations. Use for long-lived resources:
//!
//! ```rust,ignore
//! // At app build time
//! App::builder()
//!     .app_state(Database::connect()?)
//!     .app_state(Config::load()?)
//!     .build()?
//!
//! // In handlers
//! fn list_handler(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Vec<User>> {
//!     let db = ctx.app_state.get_required::<Database>()?;
//!     Ok(Output::Render(db.list_users()?))
//! }
//! ```
//!
//! **Extensions** are injected per-request by pre-dispatch hooks. Use for request-scoped data:
//!
//! ```rust,ignore
//! Hooks::new().pre_dispatch(|matches, ctx| {
//!     let user_id = matches.get_one::<String>("user").unwrap();
//!     ctx.extensions.insert(UserScope { user_id: user_id.clone() });
//!     Ok(())
//! })
//! ```
//!
//! # Core Types
//!
//! - [`CommandContext`]: Environment information passed to handlers
//! - [`Extensions`]: Type-safe container for injecting custom state
//! - [`Output`]: What a handler produces (render data, silent, or binary)
//! - [`HandlerResult`]: The result type for handlers (`Result<Output<T>, Error>`)
//! - [`RunResult`]: The result of running the CLI dispatcher
//! - [`Handler`]: Trait for command handlers (`&mut self`)

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

/// Type-safe container for injecting custom state into handlers.
///
/// Extensions allow pre-dispatch hooks to inject state that handlers can retrieve.
/// This enables dependency injection without modifying handler signatures.
///
/// # Warning: Clone Behavior
///
/// `Extensions` is **not** cloned when the container is cloned. Cloning an `Extensions` instance
/// results in a new, empty map. This is because the underlying `Box<dyn Any>` values cannot
/// be cloned generically.
///
/// If you need to share state across threads/clones, use `Arc<T>` inside the extension.
///
/// # Example
///
/// ```rust
/// use standout_dispatch::{Extensions, CommandContext};
///
/// // Define your state types
/// struct ApiClient { base_url: String }
/// struct UserScope { user_id: u64 }
///
/// // In a pre-dispatch hook, inject state
/// let mut ctx = CommandContext::default();
/// ctx.extensions.insert(ApiClient { base_url: "https://api.example.com".into() });
/// ctx.extensions.insert(UserScope { user_id: 42 });
///
/// // In a handler, retrieve state
/// let api = ctx.extensions.get_required::<ApiClient>()?;
/// println!("API base: {}", api.base_url);
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Default)]
pub struct Extensions {
    map: HashMap<TypeId, Box<dyn Any>>,
}

impl Extensions {
    /// Creates a new empty extensions container.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a value into the extensions.
    ///
    /// If a value of this type already exists, it is replaced and returned.
    pub fn insert<T: 'static>(&mut self, val: T) -> Option<T> {
        self.map
            .insert(TypeId::of::<T>(), Box::new(val))
            .and_then(|boxed| boxed.downcast().ok().map(|b| *b))
    }

    /// Gets a reference to a value of the specified type.
    ///
    /// Returns `None` if no value of this type exists.
    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.map
            .get(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast_ref())
    }

    /// Gets a mutable reference to a value of the specified type.
    ///
    /// Returns `None` if no value of this type exists.
    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.map
            .get_mut(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast_mut())
    }

    /// Gets a required reference to a value of the specified type.
    ///
    /// Returns an error if no value of this type exists.
    pub fn get_required<T: 'static>(&self) -> Result<&T, anyhow::Error> {
        self.get::<T>().ok_or_else(|| {
            anyhow::anyhow!(
                "Extension missing: type {} not found in context",
                std::any::type_name::<T>()
            )
        })
    }

    /// Gets a required mutable reference to a value of the specified type.
    ///
    /// Returns an error if no value of this type exists.
    pub fn get_mut_required<T: 'static>(&mut self) -> Result<&mut T, anyhow::Error> {
        self.get_mut::<T>().ok_or_else(|| {
            anyhow::anyhow!(
                "Extension missing: type {} not found in context",
                std::any::type_name::<T>()
            )
        })
    }

    /// Removes a value of the specified type, returning it if it existed.
    pub fn remove<T: 'static>(&mut self) -> Option<T> {
        self.map
            .remove(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast().ok().map(|b| *b))
    }

    /// Returns `true` if the extensions contain a value of the specified type.
    pub fn contains<T: 'static>(&self) -> bool {
        self.map.contains_key(&TypeId::of::<T>())
    }

    /// Returns the number of extensions stored.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns `true` if no extensions are stored.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Removes all extensions.
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
        // Extensions cannot be cloned because Box<dyn Any> isn't Clone.
        // Return empty extensions on clone - this is a limitation but
        // matches the behavior of http::Extensions.
        Self::new()
    }
}

/// Context passed to command handlers.
///
/// Provides information about the execution environment plus two mechanisms
/// for state injection:
///
/// - **`app_state`**: Immutable, app-lifetime state (Database, Config, API clients)
/// - **`extensions`**: Mutable, per-request state (UserScope, RequestId)
///
/// Note that output format is deliberately not included here - format decisions
/// are made by the render handler, not by logic handlers.
///
/// # App State (Immutable, Shared)
///
/// App state is configured at build time and shared across all dispatches:
///
/// ```rust,ignore
/// use standout::cli::App;
///
/// struct Database { /* ... */ }
/// struct Config { api_url: String }
///
/// App::builder()
///     .app_state(Database::connect()?)
///     .app_state(Config { api_url: "https://api.example.com".into() })
///     .command("list", list_handler, "{{ items }}")
///     .build()?
/// ```
///
/// Handlers retrieve app state immutably:
///
/// ```rust,ignore
/// fn list_handler(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Vec<Item>> {
///     let db = ctx.app_state.get_required::<Database>()?;
///     let config = ctx.app_state.get_required::<Config>()?;
///     Ok(Output::Render(db.list_items(&config.api_url)?))
/// }
/// ```
///
/// ## Shared Mutable State
///
/// Since `app_state` is shared via `Arc`, it is immutable by default. To share mutable state
/// (like counters or caches), use interior mutability primitives like `RwLock`, `Mutex`, or atomic types:
///
/// ```rust,ignore
/// use std::sync::atomic::AtomicUsize;
///
/// struct Metrics { request_count: AtomicUsize }
///
/// // Builder
/// App::builder().app_state(Metrics { request_count: AtomicUsize::new(0) });
///
/// // Handler
/// let metrics = ctx.app_state.get_required::<Metrics>()?;
/// metrics.request_count.fetch_add(1, Ordering::Relaxed);
/// ```
///
/// # Extensions (Mutable, Per-Request)
///
/// Pre-dispatch hooks inject per-request state into `extensions`:
///
/// ```rust
/// use standout_dispatch::{Hooks, HookError, CommandContext};
///
/// struct UserScope { user_id: String }
///
/// let hooks = Hooks::new()
///     .pre_dispatch(|matches, ctx| {
///         let user_id = matches.get_one::<String>("user").unwrap();
///         ctx.extensions.insert(UserScope { user_id: user_id.clone() });
///         Ok(())
///     });
///
/// // In handler:
/// fn my_handler(matches: &clap::ArgMatches, ctx: &CommandContext) -> anyhow::Result<()> {
///     let scope = ctx.extensions.get_required::<UserScope>()?;
///     // use scope.user_id...
///     Ok(())
/// }
/// ```
#[derive(Debug)]
pub struct CommandContext {
    /// The command path being executed (e.g., ["config", "get"])
    pub command_path: Vec<String>,

    /// Immutable app-level state shared across all dispatches.
    ///
    /// Configured via `AppBuilder::app_state()`. Contains long-lived resources
    /// like database connections, configuration, and API clients.
    ///
    /// Use `get::<T>()` or `get_required::<T>()` to retrieve values.
    pub app_state: Rc<Extensions>,

    /// Mutable per-request state container.
    ///
    /// Pre-dispatch hooks can insert values that handlers retrieve.
    /// Each dispatch gets a fresh Extensions instance.
    pub extensions: Extensions,
}

impl CommandContext {
    /// Creates a new CommandContext with the given path and shared app state.
    ///
    /// This is more efficient than `Default::default()` when you already have app_state.
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

/// What a handler produces.
///
/// This enum represents the different types of output a command handler can produce.
///
/// # Binary vs. Artifact
///
/// [`Output::Binary`] hands bytes and a filename *hint* back to the caller: the
/// framework writes them to stdout, or to an explicit `--output-file-path`
/// override, and the filename never authorizes a filesystem write on its own.
///
/// [`Output::Artifact`] is the opt-in compound shape: bytes plus an optional
/// suggested destination that *does* authorize a write, plus an
/// application-owned semantic report the framework renders only after the write
/// succeeds. See the [`artifact`](crate::artifact) module for the destination
/// policy and report channel.
///
/// Marked `#[non_exhaustive]` so future output shapes can be added without
/// breaking exhaustive matchers.
#[derive(Debug)]
#[non_exhaustive]
pub enum Output<T: Serialize> {
    /// Data to render with a template or serialize to JSON/YAML/etc.
    Render(T),
    /// Silent exit (no output produced)
    Silent,
    /// Binary output for file exports
    Binary {
        /// The binary data
        data: Vec<u8>,
        /// Suggested filename for the output
        filename: String,
    },
    /// Owned artifact bytes with an optional suggested destination and an
    /// application-owned report rendered after the framework-owned write.
    Artifact(Artifact<T>),
}

impl<T: Serialize> Output<T> {
    /// Returns true if this is a render result.
    pub fn is_render(&self) -> bool {
        matches!(self, Output::Render(_))
    }

    /// Returns true if this is a silent result.
    pub fn is_silent(&self) -> bool {
        matches!(self, Output::Silent)
    }

    /// Returns true if this is a binary result.
    pub fn is_binary(&self) -> bool {
        matches!(self, Output::Binary { .. })
    }

    /// Returns true if this is a compound artifact result.
    pub fn is_artifact(&self) -> bool {
        matches!(self, Output::Artifact(_))
    }
}

/// The result type for command handlers.
///
/// Enables use of the `?` operator for error propagation.
pub type HandlerResult<T> = Result<Output<T>, anyhow::Error>;

/// Trait for types that can be converted into a [`HandlerResult`].
///
/// This enables handlers to return either `Result<T, E>` directly (auto-wrapped
/// in [`Output::Render`]) or the explicit [`HandlerResult<T>`] when fine-grained
/// control is needed (for [`Output::Silent`] or [`Output::Binary`]).
///
/// # Example
///
/// ```rust
/// use standout_dispatch::{HandlerResult, Output, IntoHandlerResult};
///
/// // Direct Result<T, E> is auto-wrapped in Output::Render
/// fn simple() -> Result<String, anyhow::Error> {
///     Ok("hello".to_string())
/// }
/// let result: HandlerResult<String> = simple().into_handler_result();
/// assert!(matches!(result, Ok(Output::Render(_))));
///
/// // HandlerResult<T> passes through unchanged
/// fn explicit() -> HandlerResult<String> {
///     Ok(Output::Silent)
/// }
/// let result: HandlerResult<String> = explicit().into_handler_result();
/// assert!(matches!(result, Ok(Output::Silent)));
/// ```
pub trait IntoHandlerResult<T: Serialize> {
    /// Convert this type into a [`HandlerResult<T>`].
    fn into_handler_result(self) -> HandlerResult<T>;
}

/// Implementation for `Result<T, E>` - auto-wraps successful values in [`Output::Render`].
///
/// This is the ergonomic path: handlers can return `Result<T, E>` directly
/// and the framework wraps it appropriately.
impl<T, E> IntoHandlerResult<T> for Result<T, E>
where
    T: Serialize,
    E: Into<anyhow::Error>,
{
    fn into_handler_result(self) -> HandlerResult<T> {
        self.map(Output::Render).map_err(Into::into)
    }
}

/// Implementation for `HandlerResult<T>` - passes through unchanged.
///
/// This is the explicit path: handlers that need [`Output::Silent`] or
/// [`Output::Binary`] can return `HandlerResult<T>` directly.
impl<T: Serialize> IntoHandlerResult<T> for HandlerResult<T> {
    fn into_handler_result(self) -> HandlerResult<T> {
        self
    }
}

/// Process exit status selected by Standout's execution policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExitStatus(u8);

impl ExitStatus {
    /// Successful command, help, or version display.
    pub const SUCCESS: Self = Self(0);
    /// Runtime or final-write failure.
    pub const FAILURE: Self = Self(1);
    /// Clap command-line usage error.
    pub const USAGE_ERROR: Self = Self(2);

    /// Returns the numeric status reported to the operating system.
    pub const fn code(self) -> u8 {
        self.0
    }
}

/// Error returned when an external failure attempts to declare success.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("an external failure status must be nonzero")]
pub struct InvalidExternalStatus;

/// An application-declared failure from an authoritative external operation.
///
/// Use this narrow escape hatch when a command delegates to another executable
/// and must preserve that executable's exact nonzero status and stderr payload.
/// Returning ordinary handler or hook errors continues to use Standout's normal
/// runtime status (`1`); this type does not configure general error mapping.
///
/// `ExternalFailure` can flow through a handler's existing [`HandlerResult`]
/// error seam via `?`/`Into<anyhow::Error>`. For pre-dispatch, wrap it with
/// [`HookError::pre_dispatch_external`](crate::HookError::pre_dispatch_external).
#[derive(Debug, Clone)]
pub struct ExternalFailure {
    status: ExitStatus,
    diagnostic: String,
    source: Option<Arc<dyn std::error::Error + Send + Sync + 'static>>,
}

impl ExternalFailure {
    /// Declares an external failure with an exact nonzero process status and
    /// verbatim stderr payload.
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

    /// Returns the exact status declared by the application.
    pub const fn exit_status(&self) -> ExitStatus {
        self.status
    }

    /// Returns the stderr payload without adding or removing any text.
    pub fn diagnostic(&self) -> &str {
        &self.diagnostic
    }

    /// Attaches the error that caused the external operation to fail.
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

/// Successful text outcome carried by [`RunResult::Handled`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SuccessKind {
    /// A registered command completed successfully.
    Command,
    /// Clap requested a help display.
    ClapHelp,
    /// Clap requested a version display.
    ClapVersion,
}

/// The final payload whose write failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OutputKind {
    /// Rendered text output.
    Text,
    /// Binary output.
    Binary,
    /// Compound artifact bytes. Also covers a destination that could not be
    /// selected: an unwritable artifact is a final-write failure, not a
    /// silently discarded one.
    Artifact,
}

/// Typed origin for an unsuccessful framework run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RunErrorKind {
    /// Clap rejected the command line.
    ClapUsage,
    /// The application handler returned an error.
    Handler,
    /// A hook failed in the identified phase. Pipe failures are post-output hooks.
    Hook(HookPhase),
    /// Handler data serialization or presentation rendering failed.
    Render,
    /// A framework-owned file/stdout write failed.
    FinalWrite(OutputKind),
    /// The application declared a failure from an authoritative external operation.
    External,
}

/// Metadata-bearing successful text compatible with string-oriented access.
#[derive(Debug, Clone)]
pub struct RunOutput {
    text: String,
    kind: SuccessKind,
}

impl RunOutput {
    /// Creates a successful command output.
    pub fn command(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: SuccessKind::Command,
        }
    }

    /// Creates a Clap help display.
    pub fn clap_help(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: SuccessKind::ClapHelp,
        }
    }

    /// Creates a Clap version display.
    pub fn clap_version(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: SuccessKind::ClapVersion,
        }
    }

    /// Returns the captured text.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Returns the typed success origin.
    pub const fn kind(&self) -> SuccessKind {
        self.kind
    }

    /// Consumes the wrapper and returns the captured text.
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

/// Metadata-bearing failure compatible with string-oriented access.
#[derive(Debug, Clone)]
pub struct RunError {
    message: String,
    kind: RunErrorKind,
    status: ExitStatus,
    source: Option<Arc<dyn std::error::Error + Send + Sync + 'static>>,
}

impl RunError {
    /// Creates a failure with its framework origin.
    ///
    /// # Panics
    ///
    /// Panics for [`RunErrorKind::External`]. External outcomes must be
    /// constructed through [`ExternalFailure`] so their nonzero declared
    /// status and verbatim diagnostic cannot become inconsistent.
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

    /// Attaches the error that caused this captured failure.
    pub fn with_source<E>(mut self, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        self.source = Some(Arc::new(source));
        self
    }

    /// Returns the existing human-readable diagnostic.
    pub fn as_str(&self) -> &str {
        &self.message
    }

    /// Returns the typed failure origin.
    pub const fn kind(&self) -> RunErrorKind {
        self.kind
    }

    /// Returns the shell status selected for this failure.
    pub const fn exit_status(&self) -> ExitStatus {
        self.status
    }

    /// Consumes the wrapper and returns the diagnostic text.
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

/// Result of running the CLI dispatcher.
///
/// After processing arguments, the dispatcher either handles a command,
/// surfaces an error, or falls through for manual handling.
///
/// Marked `#[non_exhaustive]` so future variants can be added without
/// breaking exhaustive matchers.
#[derive(Debug)]
#[non_exhaustive]
pub enum RunResult {
    /// A handler processed the command successfully; contains the rendered output
    Handled(RunOutput),
    /// A handler produced binary output (bytes, suggested filename)
    Binary(Vec<u8>, String),
    /// A handler produced a compound artifact the framework completed: the
    /// bytes, the receipt naming the destination the write completed to, and
    /// the report rendered after that write.
    ///
    /// For a file destination the bytes are already on disk. For the stdout
    /// destination the byte write is deferred to the framework's stdout writer.
    Artifact(ArtifactRun),
    /// Silent output (handler completed but produced no output)
    Silent,
    /// A handler, hook, or output step failed; contains the formatted error message.
    /// Consumers should write this to stderr and exit non-zero.
    Error(RunError),
    /// No handler matched; contains the ArgMatches for manual handling
    NoMatch(ArgMatches),
}

impl RunResult {
    /// Returns true if a handler processed the command successfully (text output).
    pub fn is_handled(&self) -> bool {
        matches!(self, RunResult::Handled(_))
    }

    /// Returns true if the result is binary output.
    pub fn is_binary(&self) -> bool {
        matches!(self, RunResult::Binary(_, _))
    }

    /// Returns true if the result is a completed artifact run.
    pub fn is_artifact(&self) -> bool {
        matches!(self, RunResult::Artifact(_))
    }

    /// Returns true if the result is silent.
    pub fn is_silent(&self) -> bool {
        matches!(self, RunResult::Silent)
    }

    /// Returns true if the result is an error.
    pub fn is_error(&self) -> bool {
        matches!(self, RunResult::Error(_))
    }

    /// Returns the output if handled, or None otherwise.
    pub fn output(&self) -> Option<&str> {
        match self {
            RunResult::Handled(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the error message if this is an error, or None otherwise.
    pub fn error(&self) -> Option<&str> {
        match self {
            RunResult::Error(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the typed success origin for captured text.
    pub fn success_kind(&self) -> Option<SuccessKind> {
        match self {
            RunResult::Handled(output) => Some(output.kind()),
            RunResult::Binary(_, _) | RunResult::Artifact(_) | RunResult::Silent => {
                Some(SuccessKind::Command)
            }
            _ => None,
        }
    }

    /// Returns the typed error origin, if this run failed.
    pub fn error_kind(&self) -> Option<RunErrorKind> {
        match self {
            RunResult::Error(error) => Some(error.kind()),
            _ => None,
        }
    }

    /// Returns the completed run's shell status.
    ///
    /// `NoMatch` returns `None`: it is a fallback handoff, not a completed
    /// framework execution and is deliberately not treated as a usage error.
    pub fn exit_status(&self) -> Option<ExitStatus> {
        match self {
            RunResult::Handled(_)
            | RunResult::Binary(_, _)
            | RunResult::Artifact(_)
            | RunResult::Silent => Some(ExitStatus::SUCCESS),
            RunResult::Error(error) => Some(error.exit_status()),
            RunResult::NoMatch(_) => None,
        }
    }

    /// Returns the binary data and filename if binary, or None otherwise.
    pub fn binary(&self) -> Option<(&[u8], &str)> {
        match self {
            RunResult::Binary(bytes, filename) => Some((bytes, filename)),
            _ => None,
        }
    }

    /// Returns the completed artifact run, or None otherwise.
    pub fn artifact(&self) -> Option<&ArtifactRun> {
        match self {
            RunResult::Artifact(run) => Some(run),
            _ => None,
        }
    }

    /// Returns the matches if unhandled, or None if handled.
    pub fn matches(&self) -> Option<&ArgMatches> {
        match self {
            RunResult::NoMatch(m) => Some(m),
            _ => None,
        }
    }
}

/// Trait for command handlers.
///
/// Handlers take `&mut self` allowing direct mutation of internal state.
/// This is the common case for CLI applications which are single-threaded.
///
/// # Example
///
/// ```rust
/// use standout_dispatch::{Handler, HandlerResult, Output, CommandContext};
/// use clap::ArgMatches;
/// use serde::Serialize;
///
/// struct Counter { count: u32 }
///
/// impl Handler for Counter {
///     type Output = u32;
///
///     fn handle(&mut self, _m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<u32> {
///         self.count += 1;
///         Ok(Output::Render(self.count))
///     }
/// }
/// ```
pub trait Handler {
    /// The output type produced by this handler (must be serializable)
    type Output: Serialize;

    /// Execute the handler with the given matches and context.
    fn handle(&mut self, matches: &ArgMatches, ctx: &CommandContext)
        -> HandlerResult<Self::Output>;

    /// Returns the arguments expected by this handler for verification.
    ///
    /// This is used to verify that the CLI command definition matches the handler's expectations.
    /// Handlers generated by the `#[handler]` macro implement this automatically.
    fn expected_args(&self) -> Vec<ExpectedArg> {
        Vec::new()
    }
}

/// A wrapper that implements Handler for FnMut closures.
///
/// The closure can return either:
/// - `Result<T, E>` - automatically wrapped in [`Output::Render`]
/// - `HandlerResult<T>` - passed through unchanged (for [`Output::Silent`] or [`Output::Binary`])
///
/// # Example
///
/// ```rust
/// use standout_dispatch::{FnHandler, Handler, CommandContext, Output};
/// use clap::ArgMatches;
///
/// // Returning Result<T, E> directly (auto-wrapped)
/// let mut handler = FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
///     Ok::<_, anyhow::Error>("hello".to_string())
/// });
///
/// // Returning HandlerResult<T> explicitly (for Silent/Binary)
/// let mut silent_handler = FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
///     Ok(Output::<()>::Silent)
/// });
/// ```
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
    /// Creates a new FnHandler wrapping the given FnMut closure.
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

/// A handler wrapper for functions that don't need [`CommandContext`].
///
/// This is the simpler variant of [`FnHandler`] for handlers that only need
/// [`ArgMatches`]. The context parameter is accepted but ignored internally.
///
/// The closure can return either:
/// - `Result<T, E>` - automatically wrapped in [`Output::Render`]
/// - `HandlerResult<T>` - passed through unchanged (for [`Output::Silent`] or [`Output::Binary`])
///
/// # Example
///
/// ```rust
/// use standout_dispatch::{SimpleFnHandler, Handler, CommandContext, Output};
/// use clap::ArgMatches;
///
/// // Handler that doesn't need context - just uses ArgMatches
/// let mut handler = SimpleFnHandler::new(|_m: &ArgMatches| {
///     Ok::<_, anyhow::Error>("Hello, world!".to_string())
/// });
///
/// // Can still be used via Handler trait (context is ignored)
/// let ctx = CommandContext::default();
/// let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
/// let result = handler.handle(&matches, &ctx);
/// assert!(matches!(result, Ok(Output::Render(_))));
/// ```
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
    /// Creates a new SimpleFnHandler wrapping the given FnMut closure.
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

        // Build app state
        let mut app_state = Extensions::new();
        app_state.insert(Database {
            url: "postgres://localhost".into(),
        });
        app_state.insert(Config { debug: true });
        let app_state = Rc::new(app_state);

        // Create context with app state
        let ctx = CommandContext {
            command_path: vec!["list".into()],
            app_state: app_state.clone(),
            extensions: Extensions::new(),
        };

        // Retrieve app state
        let db = ctx.app_state.get::<Database>().unwrap();
        assert_eq!(db.url, "postgres://localhost");

        let config = ctx.app_state.get::<Config>().unwrap();
        assert!(config.debug);

        // App state is shared via Rc
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

        // Success case
        assert!(ctx.app_state.get_required::<Present>().is_ok());

        // Failure case
        #[derive(Debug)]
        struct Missing;
        let err = ctx.app_state.get_required::<Missing>();
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("Extension missing"));
    }

    // Extensions tests
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

        // Success case
        let val = ext.get_required::<Config>();
        assert!(val.is_ok());
        assert_eq!(val.unwrap().value, 100);

        // Failure case
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

        // Success case
        {
            let val = ext.get_mut_required::<State>();
            assert!(val.is_ok());
            val.unwrap().count += 1;
        }
        assert_eq!(ext.get_required::<State>().unwrap().count, 1);

        // Failure case
        #[derive(Debug)]
        struct Missing;
        let err = ext.get_mut_required::<Missing>();
        assert!(err.is_err());
    }

    #[test]
    fn test_extensions_clone_behavior() {
        // Verify the documented behavior that Clone drops extensions
        struct Data(#[allow(dead_code)] i32);

        let mut original = Extensions::new();
        original.insert(Data(42));

        let cloned = original.clone();

        // Original has data
        assert!(original.get::<Data>().is_some());

        // Cloned is empty
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
        let result = RunResult::Handled("output".into());
        assert!(result.is_handled());
        assert!(!result.is_binary());
        assert!(!result.is_silent());
        assert_eq!(result.output(), Some("output"));
        assert!(result.matches().is_none());
    }

    #[test]
    fn test_run_result_silent() {
        let result = RunResult::Silent;
        assert!(!result.is_handled());
        assert!(!result.is_binary());
        assert!(result.is_silent());
    }

    #[test]
    fn test_run_result_binary() {
        let bytes = vec![0x25, 0x50, 0x44, 0x46];
        let result = RunResult::Binary(bytes.clone(), "report.pdf".into());
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
        let result = RunResult::NoMatch(matches);
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

    // IntoHandlerResult tests
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
        // Handler that returns Result<T, E> directly (not HandlerResult)
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
        // Handler that returns HandlerResult directly (for Silent/Binary)
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
        // Custom error type that implements Into<anyhow::Error>
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

    // SimpleFnHandler tests (no CommandContext)
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
