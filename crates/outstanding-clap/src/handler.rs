//! Command handler types for the declarative API.
//!
//! This module provides the core types for building command handlers:
//!
//! - [`CommandContext`]: Environment information passed to handlers
//! - [`CommandResult`]: The result of executing a command handler
//! - [`RunResult`]: The result of running the CLI dispatcher
//! - [`Handler`]: Trait for command handlers (with closure support)

use clap::ArgMatches;
use outstanding::OutputMode;
use serde::Serialize;

/// Context passed to command handlers.
///
/// Provides information about the execution environment, including
/// the output mode and the command path being executed.
///
/// # Example
///
/// ```rust
/// use outstanding_clap::CommandContext;
/// use outstanding::OutputMode;
///
/// let ctx = CommandContext {
///     output_mode: OutputMode::Json,
///     command_path: vec!["config".into(), "get".into()],
/// };
///
/// assert!(ctx.output_mode.is_structured());
/// assert_eq!(ctx.command_path.join("."), "config.get");
/// ```
#[derive(Debug, Clone)]
pub struct CommandContext {
    /// The output mode for rendering (term, text, json, etc.)
    pub output_mode: OutputMode,
    /// The command path being executed (e.g., ["config", "get"])
    pub command_path: Vec<String>,
}

/// Result of a command handler.
///
/// Handlers return this enum to indicate success, failure, silent exit, or binary output.
///
/// # Example
///
/// ```rust
/// use outstanding_clap::CommandResult;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct ListOutput {
///     items: Vec<String>,
/// }
///
/// fn list_handler() -> CommandResult<ListOutput> {
///     CommandResult::Ok(ListOutput {
///         items: vec!["one".into(), "two".into()],
///     })
/// }
///
/// // For binary file exports:
/// fn export_handler() -> CommandResult<()> {
///     let pdf_bytes = vec![0x25, 0x50, 0x44, 0x46]; // PDF magic bytes
///     CommandResult::Archive(pdf_bytes, "report.pdf".into())
/// }
/// ```
#[derive(Debug)]
pub enum CommandResult<T: Serialize> {
    /// Success with data to render or serialize
    Ok(T),
    /// Error with context (will be displayed to user)
    Err(anyhow::Error),
    /// Silent exit (no output produced)
    Silent,
    /// Binary output for file exports (bytes, suggested filename)
    Archive(Vec<u8>, String),
}

impl<T: Serialize> CommandResult<T> {
    /// Returns true if this is a success result.
    pub fn is_ok(&self) -> bool {
        matches!(self, CommandResult::Ok(_))
    }

    /// Returns true if this is an error result.
    pub fn is_err(&self) -> bool {
        matches!(self, CommandResult::Err(_))
    }

    /// Returns true if this is a silent result.
    pub fn is_silent(&self) -> bool {
        matches!(self, CommandResult::Silent)
    }

    /// Returns true if this is an archive (binary) result.
    pub fn is_archive(&self) -> bool {
        matches!(self, CommandResult::Archive(_, _))
    }
}

/// Result of running the CLI dispatcher.
///
/// After processing arguments, the dispatcher either handles a command
/// (producing output) or falls through for manual handling.
///
/// # Example
///
/// ```rust,ignore
/// use outstanding_clap::{Outstanding, RunResult};
///
/// let result = Outstanding::builder()
///     .command("list", list_handler, "{{ items }}")
///     .dispatch(cmd, args);
///
/// match result {
///     RunResult::Handled(output) => println!("{}", output),
///     RunResult::Binary(bytes, filename) => {
///         std::fs::write(&filename, bytes).unwrap();
///     }
///     RunResult::Unhandled(matches) => {
///         // Handle manually
///     }
/// }
/// ```
#[derive(Debug)]
pub enum RunResult {
    /// A handler processed the command; contains the rendered output
    Handled(String),
    /// A handler produced binary output (bytes, suggested filename)
    Binary(Vec<u8>, String),
    /// No handler matched; contains the ArgMatches for manual handling
    Unhandled(ArgMatches),
}

impl RunResult {
    /// Returns true if a handler processed the command (text output).
    pub fn is_handled(&self) -> bool {
        matches!(self, RunResult::Handled(_))
    }

    /// Returns true if the result is binary output.
    pub fn is_binary(&self) -> bool {
        matches!(self, RunResult::Binary(_, _))
    }

    /// Returns the output if handled, or None otherwise.
    pub fn output(&self) -> Option<&str> {
        match self {
            RunResult::Handled(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the binary data and filename if binary, or None otherwise.
    pub fn binary(&self) -> Option<(&[u8], &str)> {
        match self {
            RunResult::Binary(bytes, filename) => Some((bytes, filename)),
            _ => None,
        }
    }

    /// Returns the matches if unhandled, or None if handled.
    pub fn matches(&self) -> Option<&ArgMatches> {
        match self {
            RunResult::Unhandled(m) => Some(m),
            _ => None,
        }
    }
}

/// Trait for command handlers.
///
/// Handlers receive the clap `ArgMatches` and a `CommandContext`, and return
/// a `CommandResult` with serializable data.
///
/// # Struct Handlers
///
/// For handlers that need state (like database connections), implement
/// the trait on a struct:
///
/// ```rust,ignore
/// use outstanding_clap::{Handler, CommandResult, CommandContext};
/// use clap::ArgMatches;
///
/// struct ListHandler {
///     db: DatabasePool,
/// }
///
/// impl Handler for ListHandler {
///     type Output = Vec<Item>;
///
///     fn handle(&self, matches: &ArgMatches, ctx: &CommandContext) -> CommandResult<Self::Output> {
///         let items = self.db.list_all();
///         CommandResult::Ok(items)
///     }
/// }
/// ```
pub trait Handler: Send + Sync {
    /// The output type produced by this handler (must be serializable)
    type Output: Serialize;

    /// Execute the handler with the given matches and context.
    fn handle(&self, matches: &ArgMatches, ctx: &CommandContext) -> CommandResult<Self::Output>;
}

/// A wrapper that implements Handler for closures.
///
/// This is used internally by `OutstandingBuilder::command()` to wrap closures.
pub struct FnHandler<F, T>
where
    F: Fn(&ArgMatches, &CommandContext) -> CommandResult<T> + Send + Sync,
    T: Serialize + Send + Sync,
{
    f: F,
    _phantom: std::marker::PhantomData<fn() -> T>,
}

impl<F, T> FnHandler<F, T>
where
    F: Fn(&ArgMatches, &CommandContext) -> CommandResult<T> + Send + Sync,
    T: Serialize + Send + Sync,
{
    /// Creates a new FnHandler wrapping the given closure.
    pub fn new(f: F) -> Self {
        Self {
            f,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<F, T> Handler for FnHandler<F, T>
where
    F: Fn(&ArgMatches, &CommandContext) -> CommandResult<T> + Send + Sync,
    T: Serialize + Send + Sync,
{
    type Output = T;

    fn handle(&self, matches: &ArgMatches, ctx: &CommandContext) -> CommandResult<T> {
        (self.f)(matches, ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_command_context_creation() {
        let ctx = CommandContext {
            output_mode: OutputMode::Json,
            command_path: vec!["test".into()],
        };
        assert!(ctx.output_mode.is_structured());
        assert_eq!(ctx.command_path.len(), 1);
    }

    #[test]
    fn test_command_result_ok() {
        let result: CommandResult<String> = CommandResult::Ok("success".into());
        assert!(result.is_ok());
        assert!(!result.is_err());
        assert!(!result.is_silent());
    }

    #[test]
    fn test_command_result_err() {
        let result: CommandResult<String> = CommandResult::Err(anyhow::anyhow!("failed"));
        assert!(!result.is_ok());
        assert!(result.is_err());
        assert!(!result.is_silent());
    }

    #[test]
    fn test_command_result_silent() {
        let result: CommandResult<String> = CommandResult::Silent;
        assert!(!result.is_ok());
        assert!(!result.is_err());
        assert!(result.is_silent());
        assert!(!result.is_archive());
    }

    #[test]
    fn test_command_result_archive() {
        let result: CommandResult<String> =
            CommandResult::Archive(vec![0x25, 0x50, 0x44, 0x46], "report.pdf".into());
        assert!(!result.is_ok());
        assert!(!result.is_err());
        assert!(!result.is_silent());
        assert!(result.is_archive());
    }

    #[test]
    fn test_run_result_handled() {
        let result = RunResult::Handled("output".into());
        assert!(result.is_handled());
        assert_eq!(result.output(), Some("output"));
        assert!(result.matches().is_none());
    }

    #[test]
    fn test_run_result_unhandled() {
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);
        let result = RunResult::Unhandled(matches);
        assert!(!result.is_handled());
        assert!(!result.is_binary());
        assert!(result.output().is_none());
        assert!(result.binary().is_none());
        assert!(result.matches().is_some());
    }

    #[test]
    fn test_run_result_binary() {
        let bytes = vec![0x25, 0x50, 0x44, 0x46]; // PDF magic
        let result = RunResult::Binary(bytes.clone(), "report.pdf".into());
        assert!(!result.is_handled());
        assert!(result.is_binary());
        assert!(result.output().is_none());
        assert!(result.matches().is_none());

        let (data, filename) = result.binary().unwrap();
        assert_eq!(data, &bytes);
        assert_eq!(filename, "report.pdf");
    }

    #[test]
    fn test_fn_handler() {
        let handler = FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
            CommandResult::Ok(json!({"status": "ok"}))
        });

        let ctx = CommandContext {
            output_mode: OutputMode::Auto,
            command_path: vec![],
        };
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);

        let result = handler.handle(&matches, &ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_struct_handler() {
        struct TestHandler {
            prefix: String,
        }

        impl Handler for TestHandler {
            type Output = String;

            fn handle(&self, _m: &ArgMatches, _ctx: &CommandContext) -> CommandResult<String> {
                CommandResult::Ok(format!("{}: done", self.prefix))
            }
        }

        let handler = TestHandler {
            prefix: "Test".into(),
        };
        let ctx = CommandContext {
            output_mode: OutputMode::Auto,
            command_path: vec![],
        };
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);

        let result = handler.handle(&matches, &ctx);
        assert!(result.is_ok());
    }
}
