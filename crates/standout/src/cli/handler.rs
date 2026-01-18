//! Command handler types for the declarative API.
//!
//! This module provides the core types for building command handlers:
//!
//! - [`CommandContext`]: Environment information passed to handlers
//! - [`Output`]: What a handler produces (render data, silent, or binary)
//! - [`HandlerResult`]: The result type for handlers (`Result<Output<T>, Error>`)
//! - [`RunResult`]: The result of running the CLI dispatcher
//! - [`Handler`]: Trait for thread-safe command handlers (`Send + Sync`, `&self`)
//! - [`LocalHandler`]: Trait for local command handlers (no `Send + Sync`, `&mut self`)
//!
//! # Handler Modes
//!
//! Standout supports two handler modes:
//!
//! ## Thread-safe handlers (default)
//!
//! Use [`Handler`] and [`FnHandler`] for the default `App`. These require
//! `Send + Sync` and immutable `&self`:
//!
//! ```rust,ignore
//! use standout::cli::{App, Handler, Output, HandlerResult, CommandContext};
//!
//! // Closure handler (most common)
//! App::builder()
//!     .command("list", |matches, ctx| {
//!         Ok(Output::Render(get_items()?))
//!     }, "{{ items }}")
//!
//! // Struct handler with interior mutability
//! struct CachedHandler {
//!     cache: Arc<Mutex<HashMap<String, Data>>>,
//! }
//!
//! impl Handler for CachedHandler {
//!     type Output = Data;
//!     fn handle(&self, m: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Data> {
//!         let mut cache = self.cache.lock().unwrap();
//!         // ...
//!     }
//! }
//! ```
//!
//! ## Local handlers (mutable state)
//!
//! Use [`LocalHandler`] and [`LocalFnHandler`] with `LocalApp`. These allow
//! `&mut self` without `Send + Sync`:
//!
//! ```rust,ignore
//! use standout::cli::{LocalApp, LocalHandler, Output, HandlerResult, CommandContext};
//!
//! struct MyDatabase {
//!     connection: Connection,
//! }
//!
//! impl MyDatabase {
//!     fn query_mut(&mut self) -> Vec<Row> { ... }
//! }
//!
//! // LocalHandler allows &mut self
//! impl LocalHandler for MyDatabase {
//!     type Output = Vec<Row>;
//!     fn handle(&mut self, m: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Vec<Row>> {
//!         Ok(Output::Render(self.query_mut()))
//!     }
//! }
//!
//! // Or use FnMut closures
//! let mut db = MyDatabase::connect()?;
//! LocalApp::builder()
//!     .command("query", move |m, ctx| {
//!         Ok(Output::Render(db.query_mut()))
//!     }, "{{ rows }}")
//! ```

use crate::OutputMode;
use clap::ArgMatches;
use serde::Serialize;

/// Context passed to command handlers.
///
/// Provides information about the execution environment, including
/// the output mode and the command path being executed.
///
/// # Example
///
/// ```rust
/// use standout::cli::CommandContext;
/// use standout::OutputMode;
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

/// What a handler produces.
///
/// This enum represents the different types of output a command handler can produce.
/// Use with `HandlerResult<T>` which wraps this in a `Result` for error handling.
///
/// # Example
///
/// ```rust
/// use standout::cli::{Output, HandlerResult};
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct ListOutput {
///     items: Vec<String>,
/// }
///
/// fn list_handler() -> HandlerResult<ListOutput> {
///     Ok(Output::Render(ListOutput {
///         items: vec!["one".into(), "two".into()],
///     }))
/// }
///
/// // For binary file exports:
/// fn export_handler() -> HandlerResult<()> {
///     let pdf_bytes = vec![0x25, 0x50, 0x44, 0x46]; // PDF magic bytes
///     Ok(Output::Binary {
///         data: pdf_bytes,
///         filename: "report.pdf".into(),
///     })
/// }
///
/// // For silent operations:
/// fn quiet_handler() -> HandlerResult<()> {
///     // Do work...
///     Ok(Output::Silent)
/// }
///
/// // Errors use standard ? operator:
/// fn fallible_handler() -> HandlerResult<String> {
///     let data = std::fs::read_to_string("config.json")?;
///     Ok(Output::Render(data))
/// }
/// ```
#[derive(Debug)]
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
}

/// The result type for command handlers.
///
/// This is the standard return type for handlers, allowing use of the `?` operator
/// for error propagation.
///
/// # Example
///
/// ```rust
/// use standout::cli::{Output, HandlerResult, CommandContext};
/// use clap::ArgMatches;
///
/// fn my_handler(_m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<String> {
///     // Fallible operations can use ?
///     let config = load_config()?;
///     Ok(Output::Render(config.name))
/// }
///
/// fn load_config() -> anyhow::Result<Config> {
///     // ...
/// #   Ok(Config { name: "test".into() })
/// }
/// # struct Config { name: String }
/// ```
pub type HandlerResult<T> = Result<Output<T>, anyhow::Error>;

/// Result of running the CLI dispatcher.
///
/// After processing arguments, the dispatcher either handles a command
/// (producing output) or falls through for manual handling.
///
/// # Example
///
/// ```rust,ignore
/// use standout::cli::{App, RunResult};
///
/// let result = App::builder()
///     .command("list", list_handler, "{{ items }}")
///     .dispatch(cmd, args);
///
/// match result {
///     RunResult::Handled(output) => println!("{}", output),
///     RunResult::Binary(bytes, filename) => {
///         std::fs::write(&filename, bytes).unwrap();
///     }
///     RunResult::NoMatch(matches) => {
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
    NoMatch(ArgMatches),
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
            RunResult::NoMatch(m) => Some(m),
            _ => None,
        }
    }
}

/// Trait for command handlers.
///
/// Handlers receive the clap `ArgMatches` and a `CommandContext`, and return
/// a `HandlerResult` with serializable data. The `Result` type enables standard
/// error handling with the `?` operator.
///
/// # Struct Handlers
///
/// For handlers that need state (like database connections), implement
/// the trait on a struct:
///
/// ```rust,ignore
/// use standout::cli::{Handler, HandlerResult, Output, CommandContext};
/// use clap::ArgMatches;
///
/// struct ListHandler {
///     db: DatabasePool,
/// }
///
/// impl Handler for ListHandler {
///     type Output = Vec<Item>;
///
///     fn handle(&self, matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Self::Output> {
///         let items = self.db.list_all()?; // Can use ? for error propagation
///         Ok(Output::Render(items))
///     }
/// }
/// ```
pub trait Handler: Send + Sync {
    /// The output type produced by this handler (must be serializable)
    type Output: Serialize;

    /// Execute the handler with the given matches and context.
    fn handle(&self, matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Self::Output>;
}

/// A wrapper that implements Handler for closures.
///
/// This is used internally by `AppBuilder::command()` to wrap closures.
pub struct FnHandler<F, T>
where
    F: Fn(&ArgMatches, &CommandContext) -> HandlerResult<T> + Send + Sync,
    T: Serialize + Send + Sync,
{
    f: F,
    _phantom: std::marker::PhantomData<fn() -> T>,
}

impl<F, T> FnHandler<F, T>
where
    F: Fn(&ArgMatches, &CommandContext) -> HandlerResult<T> + Send + Sync,
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
    F: Fn(&ArgMatches, &CommandContext) -> HandlerResult<T> + Send + Sync,
    T: Serialize + Send + Sync,
{
    type Output = T;

    fn handle(&self, matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<T> {
        (self.f)(matches, ctx)
    }
}

// ============================================================================
// Local Handler Types (for LocalApp - single-threaded, mutable handlers)
// ============================================================================

/// Trait for local (single-threaded) command handlers.
///
/// Unlike [`Handler`], this trait:
/// - Does NOT require `Send + Sync`
/// - Takes `&mut self` instead of `&self`
/// - Allows handlers to mutate their internal state directly
///
/// Use this with [`LocalApp`](super::LocalApp) when you need mutable access
/// to state without interior mutability wrappers.
///
/// # When to Use LocalHandler
///
/// - Your handler struct has `&mut self` methods
/// - You want to avoid `Arc<Mutex<_>>` or `RefCell` wrappers
/// - Your CLI is single-threaded (the common case)
///
/// # Example
///
/// ```rust,ignore
/// use standout::cli::{LocalHandler, HandlerResult, Output, CommandContext};
/// use clap::ArgMatches;
///
/// struct Counter {
///     count: u32,
/// }
///
/// impl LocalHandler for Counter {
///     type Output = u32;
///
///     fn handle(&mut self, _m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<u32> {
///         self.count += 1;  // Can mutate directly!
///         Ok(Output::Render(self.count))
///     }
/// }
/// ```
///
/// # Comparison with Handler
///
/// | Aspect | `Handler` | `LocalHandler` |
/// |--------|-----------|----------------|
/// | Self reference | `&self` | `&mut self` |
/// | Thread bounds | `Send + Sync` | None |
/// | State mutation | Via interior mutability | Direct |
/// | Use with | `App` | `LocalApp` |
pub trait LocalHandler {
    /// The output type produced by this handler (must be serializable)
    type Output: Serialize;

    /// Execute the handler with the given matches and context.
    ///
    /// Unlike [`Handler::handle`], this takes `&mut self`, allowing
    /// direct mutation of handler state.
    fn handle(&mut self, matches: &ArgMatches, ctx: &CommandContext)
        -> HandlerResult<Self::Output>;
}

/// A wrapper that implements LocalHandler for FnMut closures.
///
/// This is used internally by `LocalAppBuilder::command()` to wrap closures.
/// Unlike [`FnHandler`], this accepts `FnMut` closures that can capture
/// mutable state.
///
/// # Example
///
/// ```rust,ignore
/// use standout::cli::{LocalFnHandler, LocalHandler, Output, CommandContext};
///
/// let mut counter = 0u32;
///
/// let handler = LocalFnHandler::new(move |_m, _ctx| {
///     counter += 1;  // FnMut allows mutation
///     Ok(Output::Render(counter))
/// });
/// ```
pub struct LocalFnHandler<F, T>
where
    F: FnMut(&ArgMatches, &CommandContext) -> HandlerResult<T>,
    T: Serialize,
{
    f: F,
    _phantom: std::marker::PhantomData<fn() -> T>,
}

impl<F, T> LocalFnHandler<F, T>
where
    F: FnMut(&ArgMatches, &CommandContext) -> HandlerResult<T>,
    T: Serialize,
{
    /// Creates a new LocalFnHandler wrapping the given FnMut closure.
    pub fn new(f: F) -> Self {
        Self {
            f,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<F, T> LocalHandler for LocalFnHandler<F, T>
where
    F: FnMut(&ArgMatches, &CommandContext) -> HandlerResult<T>,
    T: Serialize,
{
    type Output = T;

    fn handle(&mut self, matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<T> {
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
    fn test_handler_result_ok() {
        let result: HandlerResult<String> = Ok(Output::Render("success".into()));
        assert!(result.is_ok());
    }

    #[test]
    fn test_handler_result_err() {
        let result: HandlerResult<String> = Err(anyhow::anyhow!("failed"));
        assert!(result.is_err());
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
        let result = RunResult::NoMatch(matches);
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
            Ok(Output::Render(json!({"status": "ok"})))
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

            fn handle(&self, _m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<String> {
                Ok(Output::Render(format!("{}: done", self.prefix)))
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

    // ============================================================================
    // LocalHandler Tests
    // ============================================================================

    #[test]
    fn test_local_fn_handler() {
        let mut counter = 0u32;

        let mut handler = LocalFnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
            counter += 1;
            Ok(Output::Render(counter))
        });

        let ctx = CommandContext {
            output_mode: OutputMode::Auto,
            command_path: vec![],
        };
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);

        // First call
        let result = handler.handle(&matches, &ctx);
        assert!(result.is_ok());
        if let Ok(Output::Render(count)) = result {
            assert_eq!(count, 1);
        }

        // Second call - counter should increment
        let result = handler.handle(&matches, &ctx);
        assert!(result.is_ok());
        if let Ok(Output::Render(count)) = result {
            assert_eq!(count, 2);
        }
    }

    #[test]
    fn test_local_struct_handler() {
        struct MutableCounter {
            count: u32,
        }

        impl LocalHandler for MutableCounter {
            type Output = u32;

            fn handle(&mut self, _m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<u32> {
                self.count += 1;
                Ok(Output::Render(self.count))
            }
        }

        let mut handler = MutableCounter { count: 0 };
        let ctx = CommandContext {
            output_mode: OutputMode::Auto,
            command_path: vec![],
        };
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);

        // Call multiple times - state should accumulate
        let _ = handler.handle(&matches, &ctx);
        let _ = handler.handle(&matches, &ctx);
        let result = handler.handle(&matches, &ctx);

        assert!(result.is_ok());
        if let Ok(Output::Render(count)) = result {
            assert_eq!(count, 3);
        }
    }

    #[test]
    fn test_local_handler_with_captured_mut_ref() {
        // This is the key use case: a mutable reference captured in a closure
        struct Database {
            records: Vec<String>,
        }

        impl Database {
            fn add_record(&mut self, record: &str) {
                self.records.push(record.to_string());
            }

            fn count(&self) -> usize {
                self.records.len()
            }
        }

        let mut db = Database { records: vec![] };

        // Simulate the pattern: closure captures &mut db
        let mut handler = LocalFnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
            db.add_record("new_record");
            Ok(Output::Render(db.count()))
        });

        let ctx = CommandContext {
            output_mode: OutputMode::Auto,
            command_path: vec![],
        };
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);

        // Each call should add a record
        let _ = handler.handle(&matches, &ctx);
        let _ = handler.handle(&matches, &ctx);
        let result = handler.handle(&matches, &ctx);

        assert!(result.is_ok());
        if let Ok(Output::Render(count)) = result {
            assert_eq!(count, 3);
        }
    }
}
