//! Hook system for pre/post command execution.
//!
//! Hooks allow you to run custom code before and after command handlers execute.
//! They are registered per-command and support chaining with transformation.
//!
//! # Hook Points
//!
//! - **Pre-dispatch**: Runs before the command handler. Can abort execution.
//! - **Post-dispatch**: Runs after the handler but before rendering. Receives the raw
//!   handler data as `serde_json::Value`. Can inspect, modify, or replace the data.
//! - **Post-output**: Runs after output is generated. Can transform output or abort.
//!
//! # Example
//!
//! ```rust,ignore
//! use outstanding::cli::{App, Hooks, RenderedOutput};
//! use serde_json::json;
//!
//! App::builder()
//!     .command("list", handler, template)
//!     .hooks("list", Hooks::new()
//!         .pre_dispatch(|_m, ctx| {
//!             println!("Running: {}", ctx.command_path.join(" "));
//!             Ok(())
//!         })
//!         .post_dispatch(|_m, _ctx, mut data| {
//!             // Add metadata before rendering
//!             if let Some(obj) = data.as_object_mut() {
//!                 obj.insert("timestamp".into(), json!(chrono::Utc::now().to_rfc3339()));
//!             }
//!             Ok(data)
//!         })
//!         .post_output(|_m, _ctx, output| {
//!             // Copy to clipboard (pseudo-code)
//!             if let RenderedOutput::Text(ref text) = output {
//!                 // clipboard::copy(text)?;
//!             }
//!             Ok(output)
//!         }))
//!     .build()?
//!     .run(cmd, args);
//! ```

use std::fmt;
use std::sync::Arc;
use thiserror::Error;

use crate::cli::handler::CommandContext;

/// Output from a command, used in post-output hooks.
///
/// This represents the final output from a command handler after rendering.
/// Hooks can inspect and transform this output before it's returned to the caller.
#[derive(Debug, Clone)]
pub enum RenderedOutput {
    /// Text output (rendered template or error message)
    Text(String),
    /// Binary output with suggested filename (e.g., PDF export)
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
    /// Error occurred during post-dispatch phase (after handler, before rendering)
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
///
/// Contains a message, the phase at which the error occurred, and an optional source error.
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

use clap::ArgMatches;

/// Type alias for pre-dispatch hook functions.
///
/// Pre-dispatch hooks receive the command context and arguments, and can abort execution
/// by returning an error.
pub type PreDispatchFn =
    Arc<dyn Fn(&ArgMatches, &CommandContext) -> Result<(), HookError> + Send + Sync>;

/// Type alias for post-dispatch hook functions.
///
/// Post-dispatch hooks receive the command context, arguments, and the raw handler
/// result as a `serde_json::Value`. They can inspect, modify, or replace the data
/// before it is rendered. Returning an error aborts execution.
pub type PostDispatchFn = Arc<
    dyn Fn(&ArgMatches, &CommandContext, serde_json::Value) -> Result<serde_json::Value, HookError>
        + Send
        + Sync,
>;

/// Type alias for post-output hook functions.
///
/// Post-output hooks receive the command context, arguments, and output, and can
/// transform the output or abort execution by returning an error.
pub type PostOutputFn = Arc<
    dyn Fn(&ArgMatches, &CommandContext, RenderedOutput) -> Result<RenderedOutput, HookError>
        + Send
        + Sync,
>;

/// Per-command hook configuration.
///
/// Hooks are registered per-command path and executed in order.
/// Multiple hooks at the same phase are chained:
/// - Pre-dispatch hooks run sequentially, aborting on first error
/// - Post-dispatch hooks chain data transformations, aborting on first error
/// - Post-output hooks chain output transformations, aborting on first error
///
/// # Example
///
/// ```rust
/// use outstanding::cli::{Hooks, HookError, RenderedOutput, CommandContext};
/// use serde_json::json;
///
/// let hooks = Hooks::new()
///     .pre_dispatch(|_m, ctx| {
///         println!("About to run: {:?}", ctx.command_path);
///         Ok(())
///     })
///     .post_dispatch(|_m, _ctx, mut data| {
///         // Modify raw data before rendering
///         if let Some(obj) = data.as_object_mut() {
///             obj.insert("hook_processed".into(), json!(true));
///         }
///         Ok(data)
///     })
///     .post_output(|_m, ctx, output| {
///         // Transform: add a prefix to text output
///         if let RenderedOutput::Text(text) = output {
///             Ok(RenderedOutput::Text(format!("[{}] {}", ctx.command_path.join("."), text)))
///         } else {
///             Ok(output)
///         }
///     });
/// ```
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
    ///
    /// Pre-dispatch hooks run before the command handler is invoked.
    /// They receive the `CommandContext` and can abort execution by returning
    /// an error.
    ///
    /// Multiple pre-dispatch hooks run in registration order. If any hook
    /// returns an error, subsequent hooks are not run.
    ///
    /// # Example
    ///
    /// ```rust
    /// use outstanding::cli::{Hooks, HookError};
    ///
    /// let hooks = Hooks::new()
    ///     .pre_dispatch(|_m, ctx| {
    ///         if ctx.command_path.contains(&"dangerous".to_string()) {
    ///             return Err(HookError::pre_dispatch("dangerous commands disabled"));
    ///         }
    ///         Ok(())
    ///     });
    /// ```
    pub fn pre_dispatch<F>(mut self, f: F) -> Self
    where
        F: Fn(&ArgMatches, &CommandContext) -> Result<(), HookError> + Send + Sync + 'static,
    {
        self.pre_dispatch.push(Arc::new(f));
        self
    }

    /// Adds a post-dispatch hook.
    ///
    /// Post-dispatch hooks run after the command handler has executed but before
    /// the output is rendered. They receive the raw handler result as a
    /// `serde_json::Value`, allowing inspection and transformation of the data.
    ///
    /// Multiple post-dispatch hooks chain transformations: each hook receives
    /// the data from the previous hook. If any hook returns an error,
    /// subsequent hooks are not run.
    ///
    /// # Example
    ///
    /// ```rust
    /// use outstanding::cli::{Hooks, HookError};
    /// use serde_json::json;
    ///
    /// let hooks = Hooks::new()
    ///     .post_dispatch(|_m, _ctx, mut data| {
    ///         // Add metadata to the result before rendering
    ///         if let Some(obj) = data.as_object_mut() {
    ///             obj.insert("processed".into(), json!(true));
    ///         }
    ///         Ok(data)
    ///     })
    ///     .post_dispatch(|_m, _ctx, data| {
    ///         // Validate data before rendering
    ///         if data.get("items").map(|v| v.as_array().map(|a| a.is_empty())).flatten() == Some(true) {
    ///             return Err(HookError::post_dispatch("no items to display"));
    ///         }
    ///         Ok(data)
    ///     });
    /// ```
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
    ///
    /// Post-output hooks run after the command handler has executed and
    /// output has been rendered. They receive the `CommandContext` and
    /// `RenderedOutput`, and can transform the output or abort execution.
    ///
    /// Multiple post-output hooks chain transformations: each hook receives
    /// the output from the previous hook. If any hook returns an error,
    /// subsequent hooks are not run.
    ///
    /// # Example
    ///
    /// ```rust
    /// use outstanding::cli::{Hooks, HookError, RenderedOutput};
    ///
    /// let hooks = Hooks::new()
    ///     .post_output(|_m, _ctx, output| {
    ///         // Copy text to clipboard (pseudo-code)
    ///         if let RenderedOutput::Text(ref text) = output {
    ///             // clipboard::copy(text)?;
    ///         }
    ///         Ok(output) // Pass through unchanged
    ///     })
    ///     .post_output(|_m, _ctx, output| {
    ///         // Add newline to text output
    ///         if let RenderedOutput::Text(text) = output {
    ///             Ok(RenderedOutput::Text(format!("{}\n", text)))
    ///         } else {
    ///             Ok(output)
    ///         }
    ///     });
    /// ```
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
    ///
    /// Hooks are executed in registration order. If any hook returns an error,
    /// execution stops and the error is returned.
    pub(crate) fn run_pre_dispatch(
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
    ///
    /// Each hook receives the data from the previous hook (or the original
    /// handler result for the first hook). If any hook returns an error,
    /// execution stops and the error is returned.
    pub(crate) fn run_post_dispatch(
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
    ///
    /// Each hook receives the output from the previous hook (or the original
    /// output for the first hook). If any hook returns an error, execution
    /// stops and the error is returned.
    pub(crate) fn run_post_output(
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
    use crate::cli::handler::CommandContext;
    use crate::OutputMode;

    use clap::ArgMatches;

    fn test_context() -> CommandContext {
        CommandContext {
            output_mode: OutputMode::Text,
            command_path: vec!["test".into()],
        }
    }

    fn test_matches() -> ArgMatches {
        clap::Command::new("test").get_matches_from(vec!["test"])
    }

    #[test]
    fn test_output_variants() {
        let text = RenderedOutput::Text("hello".into());
        assert!(text.is_text());
        assert!(!text.is_binary());
        assert!(!text.is_silent());
        assert_eq!(text.as_text(), Some("hello"));
        assert!(text.as_binary().is_none());

        let binary = RenderedOutput::Binary(vec![1, 2, 3], "file.bin".into());
        assert!(!binary.is_text());
        assert!(binary.is_binary());
        assert!(!binary.is_silent());
        assert!(binary.as_text().is_none());
        assert_eq!(binary.as_binary(), Some((&[1u8, 2, 3][..], "file.bin")));

        let silent = RenderedOutput::Silent;
        assert!(!silent.is_text());
        assert!(!silent.is_binary());
        assert!(silent.is_silent());
    }

    #[test]
    fn test_hook_error_creation() {
        let pre_err = HookError::pre_dispatch("pre error");
        assert_eq!(pre_err.phase, HookPhase::PreDispatch);
        assert_eq!(pre_err.message, "pre error");

        let post_dispatch_err = HookError::post_dispatch("post dispatch error");
        assert_eq!(post_dispatch_err.phase, HookPhase::PostDispatch);
        assert_eq!(post_dispatch_err.message, "post dispatch error");

        let post_err = HookError::post_output("post error");
        assert_eq!(post_err.phase, HookPhase::PostOutput);
        assert_eq!(post_err.message, "post error");
    }

    #[test]
    fn test_hook_error_display() {
        let err = HookError::pre_dispatch("something failed");
        assert_eq!(
            err.to_string(),
            "hook error (pre-dispatch): something failed"
        );
    }

    #[test]
    fn test_hooks_empty() {
        let hooks = Hooks::new();
        assert!(hooks.is_empty());
    }

    #[test]
    fn test_hooks_not_empty_after_adding() {
        let hooks = Hooks::new().pre_dispatch(|_, _| Ok(()));
        assert!(!hooks.is_empty());

        let hooks = Hooks::new().post_dispatch(|_, _, d| Ok(d));
        assert!(!hooks.is_empty());

        let hooks = Hooks::new().post_output(|_, _, o| Ok(o));
        assert!(!hooks.is_empty());
    }

    #[test]
    fn test_pre_dispatch_success() {
        let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let called_clone = called.clone();

        let hooks = Hooks::new().pre_dispatch(move |_, _ctx| {
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
        assert_eq!(result.unwrap_err().message, "first fails");
    }

    #[test]
    fn test_pre_dispatch_multiple_success() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c1 = counter.clone();
        let c2 = counter.clone();

        let hooks = Hooks::new()
            .pre_dispatch(move |_, _| {
                c1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            })
            .pre_dispatch(move |_, _| {
                c2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            });

        let ctx = test_context();
        let matches = test_matches();
        let result = hooks.run_pre_dispatch(&matches, &ctx);

        assert!(result.is_ok());
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn test_post_output_passthrough() {
        let hooks = Hooks::new().post_output(|_, _, output| Ok(output));

        let ctx = test_context();
        let matches = test_matches();
        let result = hooks.run_post_output(&matches, &ctx, RenderedOutput::Text("hello".into()));

        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_text(), Some("hello"));
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

    #[test]
    fn test_post_output_chained_transformations() {
        let hooks = Hooks::new()
            .post_output(|_, _, output| {
                if let RenderedOutput::Text(text) = output {
                    Ok(RenderedOutput::Text(format!("[{}]", text)))
                } else {
                    Ok(output)
                }
            })
            .post_output(|_, _, output| {
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
        assert_eq!(result.unwrap().as_text(), Some("[HELLO]"));
    }

    #[test]
    fn test_post_output_error_aborts() {
        let hooks = Hooks::new()
            .post_output(|_, _, _| Err(HookError::post_output("transform failed")))
            .post_output(|_, _, _| panic!("should not be called"));

        let ctx = test_context();
        let matches = test_matches();
        let result = hooks.run_post_output(&matches, &ctx, RenderedOutput::Text("hello".into()));

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().message, "transform failed");
    }

    #[test]
    fn test_post_output_binary() {
        let hooks = Hooks::new().post_output(|_, _, output| {
            if let RenderedOutput::Binary(mut bytes, filename) = output {
                bytes.push(0xFF);
                Ok(RenderedOutput::Binary(bytes, filename))
            } else {
                Ok(output)
            }
        });

        let ctx = test_context();
        let matches = test_matches();
        let result = hooks.run_post_output(
            &matches,
            &ctx,
            RenderedOutput::Binary(vec![1, 2], "test.bin".into()),
        );

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.is_binary());
        let (bytes, filename) = output.as_binary().unwrap();
        assert_eq!(bytes, &[1, 2, 0xFF]);
        assert_eq!(filename, "test.bin");
    }

    #[test]
    fn test_post_output_silent() {
        let hooks = Hooks::new().post_output(|_, _, output| {
            assert!(output.is_silent());
            Ok(output)
        });

        let ctx = test_context();
        let matches = test_matches();
        let result = hooks.run_post_output(&matches, &ctx, RenderedOutput::Silent);

        assert!(result.is_ok());
        assert!(result.unwrap().is_silent());
    }

    #[test]
    fn test_hooks_receive_context() {
        let hooks = Hooks::new()
            .pre_dispatch(|_, ctx| {
                assert_eq!(ctx.command_path, vec!["config", "get"]);
                Ok(())
            })
            .post_output(|_, ctx, output| {
                assert_eq!(ctx.command_path, vec!["config", "get"]);
                Ok(output)
            });

        let ctx = CommandContext {
            output_mode: OutputMode::Json,
            command_path: vec!["config".into(), "get".into()],
        };
        let matches = test_matches();

        assert!(hooks.run_pre_dispatch(&matches, &ctx).is_ok());
        assert!(hooks
            .run_post_output(&matches, &ctx, RenderedOutput::Silent)
            .is_ok());
    }

    #[test]
    fn test_hooks_debug() {
        let hooks = Hooks::new()
            .pre_dispatch(|_, _| Ok(()))
            .pre_dispatch(|_, _| Ok(()))
            .post_dispatch(|_, _, d| Ok(d))
            .post_output(|_, _, o| Ok(o));

        let debug = format!("{:?}", hooks);
        assert!(debug.contains("pre_dispatch_count: 2"));
        assert!(debug.contains("post_dispatch_count: 1"));
        assert!(debug.contains("post_output_count: 1"));
    }

    // ============================================================================
    // Post-dispatch hook tests
    // ============================================================================

    #[test]
    fn test_post_dispatch_passthrough() {
        use serde_json::json;

        let hooks = Hooks::new().post_dispatch(|_, _, data| Ok(data));

        let ctx = test_context();
        let matches = test_matches();
        let data = json!({"value": 42});
        let result = hooks.run_post_dispatch(&matches, &ctx, data);

        assert!(result.is_ok());
        assert_eq!(result.unwrap()["value"], 42);
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
    fn test_post_dispatch_chained_transformations() {
        use serde_json::json;

        let hooks = Hooks::new()
            .post_dispatch(|_, _, mut data| {
                if let Some(obj) = data.as_object_mut() {
                    obj.insert("step1".into(), json!(true));
                }
                Ok(data)
            })
            .post_dispatch(|_, _, mut data| {
                if let Some(obj) = data.as_object_mut() {
                    obj.insert("step2".into(), json!(true));
                }
                Ok(data)
            });

        let ctx = test_context();
        let matches = test_matches();
        let data = json!({"original": "data"});
        let result = hooks.run_post_dispatch(&matches, &ctx, data);

        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output["original"], "data");
        assert_eq!(output["step1"], true);
        assert_eq!(output["step2"], true);
    }

    #[test]
    fn test_post_dispatch_error_aborts() {
        use serde_json::json;

        let hooks = Hooks::new()
            .post_dispatch(|_, _, _| Err(HookError::post_dispatch("validation failed")))
            .post_dispatch(|_, _, _| panic!("should not be called"));

        let ctx = test_context();
        let matches = test_matches();
        let data = json!({"value": 42});
        let result = hooks.run_post_dispatch(&matches, &ctx, data);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.message, "validation failed");
        assert_eq!(err.phase, HookPhase::PostDispatch);
    }

    #[test]
    fn test_post_dispatch_error_display() {
        let err = HookError::post_dispatch("data validation failed");
        assert_eq!(
            err.to_string(),
            "hook error (post-dispatch): data validation failed"
        );
    }

    #[test]
    fn test_post_dispatch_receives_context() {
        use serde_json::json;

        let hooks = Hooks::new().post_dispatch(|_, ctx, data| {
            assert_eq!(ctx.command_path, vec!["config", "get"]);
            Ok(data)
        });

        let ctx = CommandContext {
            output_mode: OutputMode::Json,
            command_path: vec!["config".into(), "get".into()],
        };
        let matches = test_matches();
        let data = json!({"key": "value"});

        assert!(hooks.run_post_dispatch(&matches, &ctx, data).is_ok());
    }

    #[test]
    fn test_post_dispatch_can_replace_data() {
        use serde_json::json;

        let hooks = Hooks::new().post_dispatch(|_, _, _| {
            // Completely replace the data
            Ok(json!({"replaced": true, "new_data": [1, 2, 3]}))
        });

        let ctx = test_context();
        let matches = test_matches();
        let data = json!({"original": "data"});
        let result = hooks.run_post_dispatch(&matches, &ctx, data);

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.get("original").is_none());
        assert_eq!(output["replaced"], true);
        assert_eq!(output["new_data"], json!([1, 2, 3]));
    }
}
