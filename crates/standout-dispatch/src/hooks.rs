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
//!   → render handler
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
use std::sync::Arc;
use thiserror::Error;

use crate::handler::CommandContext;
use clap::ArgMatches;

/// Output from a command, used in post-output hooks.
///
/// This represents the final output from a command handler after rendering.
#[derive(Debug, Clone)]
pub enum RenderedOutput {
    /// Text output (rendered template or error message)
    Text(String),
    /// Binary output with suggested filename
    Binary(Vec<u8>, String),
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

    /// Returns true if this is silent (no output).
    pub fn is_silent(&self) -> bool {
        matches!(self, RenderedOutput::Silent)
    }

    /// Returns the text content if this is text output.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            RenderedOutput::Text(s) => Some(s),
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
}

/// The phase at which a hook error occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
pub type PreDispatchFn =
    Arc<dyn Fn(&ArgMatches, &CommandContext) -> Result<(), HookError> + Send + Sync>;

/// Type alias for post-dispatch hook functions.
pub type PostDispatchFn = Arc<
    dyn Fn(&ArgMatches, &CommandContext, serde_json::Value) -> Result<serde_json::Value, HookError>
        + Send
        + Sync,
>;

/// Type alias for post-output hook functions.
pub type PostOutputFn = Arc<
    dyn Fn(&ArgMatches, &CommandContext, RenderedOutput) -> Result<RenderedOutput, HookError>
        + Send
        + Sync,
>;

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

    /// Adds a pre-dispatch hook.
    pub fn pre_dispatch<F>(mut self, f: F) -> Self
    where
        F: Fn(&ArgMatches, &CommandContext) -> Result<(), HookError> + Send + Sync + 'static,
    {
        self.pre_dispatch.push(Arc::new(f));
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
            + Send
            + Sync
            + 'static,
    {
        self.post_dispatch.push(Arc::new(f));
        self
    }

    /// Adds a post-output hook.
    pub fn post_output<F>(mut self, f: F) -> Self
    where
        F: Fn(&ArgMatches, &CommandContext, RenderedOutput) -> Result<RenderedOutput, HookError>
            + Send
            + Sync
            + 'static,
    {
        self.post_output.push(Arc::new(f));
        self
    }

    /// Runs all pre-dispatch hooks.
    pub fn run_pre_dispatch(
        &self,
        matches: &ArgMatches,
        ctx: &CommandContext,
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
        }
    }

    fn test_matches() -> ArgMatches {
        clap::Command::new("test").get_matches_from(vec!["test"])
    }

    #[test]
    fn test_rendered_output_variants() {
        let text = RenderedOutput::Text("hello".into());
        assert!(text.is_text());
        assert!(!text.is_binary());
        assert!(!text.is_silent());
        assert_eq!(text.as_text(), Some("hello"));

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
    fn test_pre_dispatch_success() {
        let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let called_clone = called.clone();

        let hooks = Hooks::new().pre_dispatch(move |_, _| {
            called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        });

        let ctx = test_context();
        let matches = test_matches();
        let result = hooks.run_pre_dispatch(&matches, &ctx);

        assert!(result.is_ok());
        assert!(called.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn test_pre_dispatch_error_aborts() {
        let hooks = Hooks::new()
            .pre_dispatch(|_, _| Err(HookError::pre_dispatch("first fails")))
            .pre_dispatch(|_, _| panic!("should not be called"));

        let ctx = test_context();
        let matches = test_matches();
        let result = hooks.run_pre_dispatch(&matches, &ctx);

        assert!(result.is_err());
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
            if let RenderedOutput::Text(text) = output {
                Ok(RenderedOutput::Text(text.to_uppercase()))
            } else {
                Ok(output)
            }
        });

        let ctx = test_context();
        let matches = test_matches();
        let result = hooks.run_post_output(&matches, &ctx, RenderedOutput::Text("hello".into()));

        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_text(), Some("HELLO"));
    }
}
