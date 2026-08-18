//! Hook system for pre/post command execution.
//!
//! Hooks allow you to run custom code at specific points in the dispatch pipeline.
//! They enable cross-cutting concerns (logging, validation, transformation) without
//! polluting handler logic.
//!
//! # Pipeline Position
//!
//! Hooks fit into the dispatch flow as follows:
//!
//! ```text
//! parsed CLI args
//!   → PRE-DISPATCH HOOK ← (validation, auth checks, setup)
//!   → logic handler
//!   → POST-DISPATCH HOOK ← (data transformation, enrichment)
//!   → framework rendering
//!   → POST-OUTPUT HOOK ← (output transformation, logging)
//! ```
//!
//! # Hook Points
//!
//! - Pre-dispatch: Runs before the command handler. Can abort execution.
//!   Use for: authentication, input validation, resource acquisition.
//!
//! - Post-dispatch: Runs after the handler but before rendering. Receives the raw
//!   handler data as `serde_json::Value`. Can inspect, modify, or replace the data.
//!   Use for: adding metadata, data transformation, caching.
//!
//! - Post-output: Runs after output is generated. Can transform output or abort.
//!   Use for: logging, clipboard copy, output filtering.

use std::fmt;
use std::path::PathBuf;
use std::rc::Rc;
use thiserror::Error;

use crate::handler::{CommandContext, ExternalFailure};
use clap::ArgMatches;

/// Text output with both formatted and raw versions.
///
/// This struct carries both the terminal-formatted output (with ANSI codes)
/// and the raw output (with style tags but no ANSI codes). This allows
/// post-output hooks like piping to choose the appropriate version.
#[derive(Debug, Clone)]
pub struct TextOutput {
    /// The formatted output with ANSI codes applied (for terminal display)
    pub formatted: String,
    /// The raw output with `[tag]...[/tag]` markers but no ANSI codes.
    /// This is the intermediate output after template rendering but before
    /// style tag processing. Piping uses this by default.
    pub raw: String,
}

impl TextOutput {
    /// Creates a new TextOutput with both formatted and raw versions.
    pub fn new(formatted: String, raw: String) -> Self {
        Self { formatted, raw }
    }

    /// Creates a TextOutput where formatted and raw are the same.
    /// Use this for output that doesn't go through style tag processing
    /// (e.g., JSON output, error messages).
    pub fn plain(text: String) -> Self {
        Self {
            formatted: text.clone(),
            raw: text,
        }
    }
}

/// Artifact bytes and report as post-output hooks see them.
///
/// Post-output hooks observe this *before* the framework selects a destination
/// and performs the final write, so a hook can still transform the bytes or
/// enrich the report. Hooks deliberately cannot perform the write themselves:
/// the framework owns it, which is what keeps the success report truthful and
/// the failure path single.
///
/// The fields are public so hooks can mutate them, mirroring [`TextOutput`].
#[derive(Debug, Clone)]
pub struct ArtifactOutput {
    /// The artifact bytes, byte-for-byte as the handler produced them.
    pub bytes: Vec<u8>,
    /// The destination the application suggested, if it opted in. `Some`
    /// authorizes the framework to write there absent an explicit override.
    pub suggested_destination: Option<PathBuf>,
    /// Whether the application authorized the stdout fallback.
    pub stdout_allowed: bool,
    /// The serialized application-owned report, after post-dispatch hooks.
    pub report: Option<serde_json::Value>,
}

/// Output from a command, used in post-output hooks.
///
/// This represents the final output from a command handler after rendering.
#[derive(Debug, Clone)]
pub enum RenderedOutput {
    /// Text output with both formatted (ANSI) and raw versions.
    /// The `formatted` field contains ANSI codes for terminal display.
    /// The `raw` field contains the intermediate output for piping.
    Text(TextOutput),
    /// Binary output with suggested filename
    Binary(Vec<u8>, String),
    /// A compound artifact awaiting the framework-owned write.
    Artifact(ArtifactOutput),
    /// No output (silent command)
    Silent,
}

impl RenderedOutput {
    /// Returns true if this is text output.
    pub fn is_text(&self) -> bool {
        matches!(self, RenderedOutput::Text(_))
    }

    /// Returns true if this is binary output.
    pub fn is_binary(&self) -> bool {
        matches!(self, RenderedOutput::Binary(_, _))
    }

    /// Returns true if this is a compound artifact awaiting the final write.
    pub fn is_artifact(&self) -> bool {
        matches!(self, RenderedOutput::Artifact(_))
    }

    /// Returns true if this is silent (no output).
    pub fn is_silent(&self) -> bool {
        matches!(self, RenderedOutput::Silent)
    }

    /// Returns the formatted text content (with ANSI codes) if this is text output.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            RenderedOutput::Text(t) => Some(&t.formatted),
            _ => None,
        }
    }

    /// Returns the raw text content (without ANSI codes) if this is text output.
    /// This is the intermediate output suitable for piping.
    pub fn as_raw_text(&self) -> Option<&str> {
        match self {
            RenderedOutput::Text(t) => Some(&t.raw),
            _ => None,
        }
    }

    /// Returns the full TextOutput if this is text output.
    pub fn as_text_output(&self) -> Option<&TextOutput> {
        match self {
            RenderedOutput::Text(t) => Some(t),
            _ => None,
        }
    }

    /// Returns the binary content and filename if this is binary output.
    pub fn as_binary(&self) -> Option<(&[u8], &str)> {
        match self {
            RenderedOutput::Binary(bytes, filename) => Some((bytes, filename)),
            _ => None,
        }
    }

    /// Returns the pending artifact if this is artifact output.
    pub fn as_artifact(&self) -> Option<&ArtifactOutput> {
        match self {
            RenderedOutput::Artifact(artifact) => Some(artifact),
            _ => None,
        }
    }

    /// Returns the pending artifact mutably, for hooks that transform bytes or
    /// enrich the report before the framework-owned write.
    pub fn as_artifact_mut(&mut self) -> Option<&mut ArtifactOutput> {
        match self {
            RenderedOutput::Artifact(artifact) => Some(artifact),
            _ => None,
        }
    }
}

/// The phase at which a hook error occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookPhase {
    /// Error occurred during pre-dispatch phase
    PreDispatch,
    /// Error occurred during post-dispatch phase
    PostDispatch,
    /// Error occurred during post-output phase
    PostOutput,
}

impl fmt::Display for HookPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookPhase::PreDispatch => write!(f, "pre-dispatch"),
            HookPhase::PostDispatch => write!(f, "post-dispatch"),
            HookPhase::PostOutput => write!(f, "post-output"),
        }
    }
}

/// Error returned by a hook.
#[derive(Debug, Error)]
#[error("hook error ({phase}): {message}")]
pub struct HookError {
    /// Human-readable error message
    pub message: String,
    /// The hook phase where the error occurred
    pub phase: HookPhase,
    /// The underlying error source, if any
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl HookError {
    /// Creates a new hook error for the pre-dispatch phase.
    pub fn pre_dispatch(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            phase: HookPhase::PreDispatch,
            source: None,
        }
    }

    /// Declares an authoritative external failure before handler dispatch.
    ///
    /// Standout preserves the failure's exact nonzero status and diagnostic.
    /// This constructor is intentionally limited to the pre-dispatch seam;
    /// ordinary hook failures retain status `1`.
    pub fn pre_dispatch_external(failure: ExternalFailure) -> Self {
        Self {
            message: failure.diagnostic().to_owned(),
            phase: HookPhase::PreDispatch,
            source: Some(Box::new(failure)),
        }
    }

    /// Creates a new hook error for the post-dispatch phase.
    pub fn post_dispatch(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            phase: HookPhase::PostDispatch,
            source: None,
        }
    }

    /// Creates a new hook error for the post-output phase.
    pub fn post_output(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            phase: HookPhase::PostOutput,
            source: None,
        }
    }

    /// Sets the source error.
    pub fn with_source<E>(mut self, source: E) -> Self
    where
        E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    {
        self.source = Some(source.into());
        self
    }
}

/// Type alias for pre-dispatch hook functions.
///
/// Pre-dispatch hooks receive mutable access to [`CommandContext`], allowing them
/// to inject state into `ctx.extensions` that handlers can retrieve.
pub type PreDispatchFn = Rc<dyn Fn(&ArgMatches, &mut CommandContext) -> Result<(), HookError>>;

/// Type alias for post-dispatch hook functions.
pub type PostDispatchFn = Rc<
    dyn Fn(&ArgMatches, &CommandContext, serde_json::Value) -> Result<serde_json::Value, HookError>,
>;

/// Type alias for post-output hook functions.
pub type PostOutputFn =
    Rc<dyn Fn(&ArgMatches, &CommandContext, RenderedOutput) -> Result<RenderedOutput, HookError>>;

/// Per-command hook configuration.
///
/// Hooks are registered per-command path and executed in order.
#[derive(Clone, Default)]
pub struct Hooks {
    pre_dispatch: Vec<PreDispatchFn>,
    post_dispatch: Vec<PostDispatchFn>,
    post_output: Vec<PostOutputFn>,
}

impl Hooks {
    /// Creates a new empty hooks configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if no hooks are registered.
    pub fn is_empty(&self) -> bool {
        self.pre_dispatch.is_empty() && self.post_dispatch.is_empty() && self.post_output.is_empty()
    }

    /// Returns true when at least one hook is registered for `phase`.
    pub fn has_phase(&self, phase: HookPhase) -> bool {
        match phase {
            HookPhase::PreDispatch => !self.pre_dispatch.is_empty(),
            HookPhase::PostDispatch => !self.post_dispatch.is_empty(),
            HookPhase::PostOutput => !self.post_output.is_empty(),
        }
    }

    /// Returns the phases that contain at least one hook.
    pub fn phases(&self) -> impl Iterator<Item = HookPhase> + '_ {
        [
            HookPhase::PreDispatch,
            HookPhase::PostDispatch,
            HookPhase::PostOutput,
        ]
        .into_iter()
        .filter(|phase| self.has_phase(*phase))
    }

    /// Appends another hook configuration after this one.
    ///
    /// Hooks in the same phase keep registration order: the hooks already in
    /// `self` run before hooks from `other`.
    pub fn append(mut self, mut other: Hooks) -> Self {
        self.pre_dispatch.append(&mut other.pre_dispatch);
        self.post_dispatch.append(&mut other.post_dispatch);
        self.post_output.append(&mut other.post_output);
        self
    }

    /// Adds a pre-dispatch hook.
    ///
    /// Pre-dispatch hooks receive mutable access to [`CommandContext`], allowing
    /// state injection via `ctx.extensions`. Handlers can then retrieve this state.
    ///
    /// # Example
    ///
    /// ```rust
    /// use standout_dispatch::{Hooks, HookError};
    ///
    /// struct ApiClient { base_url: String }
    ///
    /// let hooks = Hooks::new()
    ///     .pre_dispatch(|_matches, ctx| {
    ///         ctx.extensions.insert(ApiClient {
    ///             base_url: "https://api.example.com".into()
    ///         });
    ///         Ok(())
    ///     });
    /// ```
    pub fn pre_dispatch<F>(mut self, f: F) -> Self
    where
        F: Fn(&ArgMatches, &mut CommandContext) -> Result<(), HookError> + 'static,
    {
        self.pre_dispatch.push(Rc::new(f));
        self
    }

    /// Adds a post-dispatch hook.
    pub fn post_dispatch<F>(mut self, f: F) -> Self
    where
        F: Fn(
                &ArgMatches,
                &CommandContext,
                serde_json::Value,
            ) -> Result<serde_json::Value, HookError>
            + 'static,
    {
        self.post_dispatch.push(Rc::new(f));
        self
    }

    /// Adds a post-output hook.
    pub fn post_output<F>(mut self, f: F) -> Self
    where
        F: Fn(&ArgMatches, &CommandContext, RenderedOutput) -> Result<RenderedOutput, HookError>
            + 'static,
    {
        self.post_output.push(Rc::new(f));
        self
    }

    /// Runs all pre-dispatch hooks.
    ///
    /// Hooks receive mutable access to the context, allowing state injection.
    pub fn run_pre_dispatch(
        &self,
        matches: &ArgMatches,
        ctx: &mut CommandContext,
    ) -> Result<(), HookError> {
        for hook in &self.pre_dispatch {
            hook(matches, ctx)?;
        }
        Ok(())
    }

    /// Runs all post-dispatch hooks, chaining transformations.
    pub fn run_post_dispatch(
        &self,
        matches: &ArgMatches,
        ctx: &CommandContext,
        data: serde_json::Value,
    ) -> Result<serde_json::Value, HookError> {
        let mut current = data;
        for hook in &self.post_dispatch {
            current = hook(matches, ctx, current)?;
        }
        Ok(current)
    }

    /// Runs all post-output hooks, chaining transformations.
    pub fn run_post_output(
        &self,
        matches: &ArgMatches,
        ctx: &CommandContext,
        output: RenderedOutput,
    ) -> Result<RenderedOutput, HookError> {
        let mut current = output;
        for hook in &self.post_output {
            current = hook(matches, ctx, current)?;
        }
        Ok(current)
    }
}

impl fmt::Debug for Hooks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Hooks")
            .field("pre_dispatch_count", &self.pre_dispatch.len())
            .field("post_dispatch_count", &self.post_dispatch.len())
            .field("post_output_count", &self.post_output.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_context() -> CommandContext {
        CommandContext {
            command_path: vec!["test".into()],
            ..Default::default()
        }
    }

    fn test_matches() -> ArgMatches {
        clap::Command::new("test").get_matches_from(vec!["test"])
    }

    #[test]
    fn test_rendered_output_variants() {
        let text = RenderedOutput::Text(TextOutput::new("formatted".into(), "raw".into()));
        assert!(text.is_text());
        assert!(!text.is_binary());
        assert!(!text.is_silent());
        assert_eq!(text.as_text(), Some("formatted"));
        assert_eq!(text.as_raw_text(), Some("raw"));

        // Test plain constructor (formatted == raw)
        let plain = RenderedOutput::Text(TextOutput::plain("hello".into()));
        assert_eq!(plain.as_text(), Some("hello"));
        assert_eq!(plain.as_raw_text(), Some("hello"));

        let binary = RenderedOutput::Binary(vec![1, 2, 3], "file.bin".into());
        assert!(!binary.is_text());
        assert!(binary.is_binary());
        assert_eq!(binary.as_binary(), Some((&[1u8, 2, 3][..], "file.bin")));

        let silent = RenderedOutput::Silent;
        assert!(silent.is_silent());
    }

    #[test]
    fn test_hook_error_creation() {
        let err = HookError::pre_dispatch("test error");
        assert_eq!(err.phase, HookPhase::PreDispatch);
        assert_eq!(err.message, "test error");
    }

    #[test]
    fn test_hooks_empty() {
        let hooks = Hooks::new();
        assert!(hooks.is_empty());
    }

    #[test]
    fn test_hooks_report_registered_phases() {
        let hooks = Hooks::new()
            .pre_dispatch(|_, _| Ok(()))
            .post_output(|_, _, output| Ok(output));

        let phases: Vec<_> = hooks.phases().collect();

        assert_eq!(phases, vec![HookPhase::PreDispatch, HookPhase::PostOutput]);
        assert!(hooks.has_phase(HookPhase::PreDispatch));
        assert!(!hooks.has_phase(HookPhase::PostDispatch));
    }

    #[test]
    fn test_hooks_append_preserves_phase_order() {
        use std::cell::RefCell;

        let calls = Rc::new(RefCell::new(Vec::new()));
        let first_calls = calls.clone();
        let second_calls = calls.clone();
        let hooks = Hooks::new()
            .pre_dispatch(move |_, _| {
                first_calls.borrow_mut().push("first");
                Ok(())
            })
            .append(Hooks::new().pre_dispatch(move |_, _| {
                second_calls.borrow_mut().push("second");
                Ok(())
            }));
        let mut ctx = test_context();
        let matches = test_matches();

        hooks.run_pre_dispatch(&matches, &mut ctx).unwrap();

        assert_eq!(&*calls.borrow(), &["first", "second"]);
    }

    #[test]
    fn test_pre_dispatch_success() {
        use std::cell::Cell;
        use std::rc::Rc;

        let called = Rc::new(Cell::new(false));
        let called_clone = called.clone();

        let hooks = Hooks::new().pre_dispatch(move |_, _| {
            called_clone.set(true);
            Ok(())
        });

        let mut ctx = test_context();
        let matches = test_matches();
        let result = hooks.run_pre_dispatch(&matches, &mut ctx);

        assert!(result.is_ok());
        assert!(called.get());
    }

    #[test]
    fn test_pre_dispatch_error_aborts() {
        let hooks = Hooks::new()
            .pre_dispatch(|_, _| Err(HookError::pre_dispatch("first fails")))
            .pre_dispatch(|_, _| panic!("should not be called"));

        let mut ctx = test_context();
        let matches = test_matches();
        let result = hooks.run_pre_dispatch(&matches, &mut ctx);

        assert!(result.is_err());
    }

    #[test]
    fn test_pre_dispatch_injects_extensions() {
        struct TestState {
            value: i32,
        }

        let hooks = Hooks::new().pre_dispatch(|_, ctx| {
            ctx.extensions.insert(TestState { value: 42 });
            Ok(())
        });

        let mut ctx = test_context();
        let matches = test_matches();

        // Before hook runs, extension is not present
        assert!(!ctx.extensions.contains::<TestState>());

        hooks.run_pre_dispatch(&matches, &mut ctx).unwrap();

        // After hook runs, extension is available
        let state = ctx.extensions.get::<TestState>().unwrap();
        assert_eq!(state.value, 42);
    }

    #[test]
    fn test_pre_dispatch_multiple_hooks_share_context() {
        struct Counter {
            count: i32,
        }

        let hooks = Hooks::new()
            .pre_dispatch(|_, ctx| {
                ctx.extensions.insert(Counter { count: 1 });
                Ok(())
            })
            .pre_dispatch(|_, ctx| {
                // Second hook can read and modify what first hook inserted
                if let Some(counter) = ctx.extensions.get_mut::<Counter>() {
                    counter.count += 10;
                }
                Ok(())
            });

        let mut ctx = test_context();
        let matches = test_matches();
        hooks.run_pre_dispatch(&matches, &mut ctx).unwrap();

        let counter = ctx.extensions.get::<Counter>().unwrap();
        assert_eq!(counter.count, 11);
    }

    #[test]
    fn test_post_dispatch_transformation() {
        use serde_json::json;

        let hooks = Hooks::new().post_dispatch(|_, _, mut data| {
            if let Some(obj) = data.as_object_mut() {
                obj.insert("modified".into(), json!(true));
            }
            Ok(data)
        });

        let ctx = test_context();
        let matches = test_matches();
        let data = json!({"value": 42});
        let result = hooks.run_post_dispatch(&matches, &ctx, data);

        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output["value"], 42);
        assert_eq!(output["modified"], true);
    }

    #[test]
    fn test_post_output_transformation() {
        let hooks = Hooks::new().post_output(|_, _, output| {
            if let RenderedOutput::Text(text_output) = output {
                Ok(RenderedOutput::Text(TextOutput::new(
                    text_output.formatted.to_uppercase(),
                    text_output.raw.to_uppercase(),
                )))
            } else {
                Ok(output)
            }
        });

        let ctx = test_context();
        let matches = test_matches();
        let input = RenderedOutput::Text(TextOutput::plain("hello".into()));
        let result = hooks.run_post_output(&matches, &ctx, input);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_text(), Some("HELLO"));
    }
}
