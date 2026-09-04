//! Command dispatch and orchestration for clap-based CLIs.
//!
//! Routes parsed `ArgMatches` to a [`Handler`], running pre-dispatch, handler,
//! post-dispatch, and post-output hooks around it, and returns typed
//! [`HandlerResult`] data — presentation stays with the consuming framework.
//! A handler adapter takes `(&ArgMatches, &CommandContext)` and returns
//! serializable data, so application logic stays reusable outside a CLI. The
//! third parameter of [`Handler::handle`] is the typed results channel of
//! [`results`]; a batch command sets `Handler::Event` to [`NoEvents`] and
//! ignores it, and a command that produces its result while it runs takes the
//! channel as a third closure parameter through [`EventsFnHandler`].
//!
//! [`CommandContext`] carries two kinds of injected state: `app_state` is
//! immutable and app-lifetime (database, config, API clients), built once
//! and shared via `Rc`; `extensions` is mutable and per-request, set by
//! pre-dispatch hooks for things like a resolved user session.

pub mod artifact;
mod contract;
mod diagnostic;
mod dispatch;
mod handler;
mod hooks;
mod results;
mod stream;
pub mod verify;
pub use artifact::{Artifact, ArtifactDestination, ArtifactReceipt, ArtifactRun};
pub use contract::{ContractSurface, Envelope};
pub use diagnostic::{Diagnostic, DiagnosticKind, DiagnosticPosition, DiagnosticRange, Severity};
pub use dispatch::{
    extract_command_path, get_deepest_matches, has_subcommand, insert_default_command,
    path_to_string, string_to_path,
};
pub use handler::{
    AppFailure, CommandContext, DispatchResult, EventsFnHandler, ExitStatus, Extensions,
    ExternalFailure, FnHandler, Handler, HandlerOutcome, HandlerResult, IntoHandlerResult,
    IntoSummaryResult, InvalidAppStatus, InvalidExternalStatus, Output, OutputKind, RunError,
    RunErrorKind, RunOutput, SimpleFnHandler, SuccessKind, Summary, SummaryResult,
};
pub use hooks::{
    ArtifactOutput, HookError, HookPhase, Hooks, PostDispatchFn, PostOutputFn, PreDispatchFn,
    RenderedOutput, TextOutput,
};
pub use results::{emits_events, Delivery, EmitError, EventSink, NoEvents, Results, RunRecorder};
pub use stream::{StreamCapture, StreamSink};
