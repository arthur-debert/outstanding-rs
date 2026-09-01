//! CLI dispatch and integration for clap-based applications.
//!
//! Bridges Standout's rendering engine with clap's argument parsing: parse
//! args with a normal clap `Command` (augmented with `--output` etc.),
//! dispatch to a registered [`Handler`], run hooks (pre-dispatch,
//! post-dispatch, post-output), then render through the template engine —
//! structured modes (JSON, YAML) skip templating and serialize directly.
//!
//! CLI applications are single-threaded (parse → one handler → output →
//! exit), so handlers use `&mut self` / `FnMut` rather than
//! `Arc<Mutex<_>>`.
//!
//! Adoption is partial by default: register only the commands you want
//! Standout to handle. Unmatched commands come back as
//! [`DispatchResult::NoMatch`] with the `ArgMatches` for your own dispatch;
//! [`CompletedRun`] wraps that outcome plus framework warnings.
//!
//! ```rust,ignore
//! use standout::cli::{App, Output};
//!
//! App::builder()
//!     .command("list", |_m, ctx| Ok(Output::Render(load_items()?)),
//!         "{% for item in items %}{{ item }}\n{% endfor %}")?
//!     .build()?
//!     .run(cmd, std::env::args());
//! ```
//!
//! Key types: [`AppBuilder`] (configuration), [`App`] (built application),
//! [`Handler`] / [`FnHandler`] (command handlers), [`Output`] (what a
//! handler produces), [`HandlerResult`], [`CompletedRun`] /
//! [`DispatchResult`], [`Hooks`], [`CommandContext`].
//!
//! See also: [`crate::render`] for rendering without CLI integration,
//! [`handler`], [`hooks`], [`help`].

mod default_command;
mod dispatch;
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

pub use group::{CommandConfig, GroupBuilder};

pub use questionnaire::{
    Confirmation, ConfirmationAcceptance, ReviewStream, QUESTIONNAIRE_ANSWERS_ARG,
    QUESTIONNAIRE_YES_ARG,
};

pub use result::{CompletedRun, HelpResult};

pub use help::{
    default_help_theme, render_help, render_help_with_topics, validate_command_groups,
    CommandGroup, HelpConfig, HelpLength,
};

pub use handler::{
    AppFailure, Artifact, ArtifactDestination, ArtifactReceipt, ArtifactRun, CommandContext,
    CommandContextInput, DispatchResult, ExitStatus, ExternalFailure, FnHandler, Handler,
    HandlerResult, InvalidAppStatus, InvalidExternalStatus, Output, OutputKind, RunError,
    RunErrorKind, RunOutput, SuccessKind,
};

pub use hooks::{ArtifactOutput, HookError, HookPhase, Hooks, RenderedOutput};

pub use standout_macros::Dispatch;

pub use crate::setup::SetupError;

pub use default_command::{DefaultCommandContext, DefaultCommandResolver, UnknownDefaultCommand};
