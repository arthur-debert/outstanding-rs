//! Command dispatch and orchestration for clap-based CLIs.
//!
//! Routes parsed `ArgMatches` to a [`Handler`], running pre-dispatch, handler,
//! post-dispatch, and post-output hooks around it, and returns typed
//! [`HandlerResult`] data — presentation stays with the consuming framework.
//! The third parameter of [`Handler::handle`] is the typed [`Results`] channel.
//!
//! [`CommandContext`] carries two kinds of injected state: `app_state` is
//! immutable and app-lifetime, built once and shared via `Rc`; `extensions` is
//! mutable and per-request, set by pre-dispatch hooks.

pub mod artifact;
mod contract;
mod diagnostic;
mod dispatch;
mod escape;
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
pub use standout_types::{ColorPolicy, Representation};
pub use stream::{StreamCapture, StreamSink};
