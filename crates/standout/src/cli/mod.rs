//! CLI dispatch and integration for clap-based applications.
//!
//! Bridges Standout's rendering engine with clap's argument parsing: parse args
//! with a normal clap `Command` (augmented with `--output` etc.), dispatch to a
//! registered [`Handler`], run hooks, then render through the template engine —
//! structured modes skip templating and serialize directly.
//!
//! CLI applications are single-threaded (parse → one handler → output → exit),
//! so handlers use `&mut self` / `FnMut` rather than `Arc<Mutex<_>>`.
//!
//! Adoption is partial by default: register only the commands you want Standout
//! to handle. Unmatched commands come back as [`DispatchResult::NoMatch`] with
//! the `ArgMatches` for your own dispatch.

mod config;
mod default_command;
mod dispatch;
mod emit;
pub(crate) mod events;
pub(crate) mod pager;
mod questionnaire;
mod result;

pub(crate) mod app;

mod builder;

pub mod group;
pub mod handler;
pub mod help;
pub mod hooks;
#[macro_use]
pub mod macros;

pub use builder::{App, AppBuilder, STRICT_STYLE_TAGS_ENV};
pub use config::{MissingConfig, TermColor, TermOutput, TermSettings};

pub use group::{CommandConfig, GroupBuilder};

pub use questionnaire::{
    Confirmation, ConfirmationAcceptance, ReviewStream, QUESTIONNAIRE_ANSWERS_ARG,
    QUESTIONNAIRE_YES_ARG,
};

pub use result::{CompletedRun, HelpResult, ProcessOutcome};

pub use help::{
    default_help_theme, render_help, render_help_with_topics, validate_command_groups,
    CommandGroup, HelpConfig, HelpLength,
};

pub use handler::{
    emits_events, AppFailure, Artifact, ArtifactDestination, ArtifactReceipt, ArtifactRun,
    CommandContext, CommandContextInput, ContractSurface, Delivery, Diagnostic, DiagnosticKind,
    DiagnosticPosition, DiagnosticRange, DispatchResult, EmitError, Envelope, EventsFnHandler,
    ExitStatus, ExternalFailure, FnHandler, Handler, HandlerOutcome, HandlerResult,
    InvalidAppStatus, InvalidExternalStatus, NoEvents, Output, OutputKind, Results, RunError,
    RunErrorKind, RunOutput, RunRecorder, Severity, StreamCapture, StreamSink, SuccessKind,
    Summary, SummaryResult,
};

pub use help::{HelpArg, HelpDocument, HelpSubcommand};

#[cfg(feature = "test-support")]
pub use emit::warnings_delivered_on_stdout;
pub use emit::{
    carries_diagnostic_document, carries_warning_entries, emit_run_result, emit_warning_entries,
    parse_diagnostic, render_diagnostic, warning_records, DiagnosticDocumentError,
};

pub use hooks::{ArtifactOutput, HookError, HookPhase, Hooks, RenderedOutput};

pub use standout_macros::Dispatch;

pub use crate::setup::SetupError;

pub use default_command::{DefaultCommandContext, DefaultCommandResolver, UnknownDefaultCommand};
