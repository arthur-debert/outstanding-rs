//! Command handler types.
//!
//! This module provides the core types for building logic handlers - the
//! business logic layer in the dispatch pipeline.
//!
//! # Design Rationale
//!
//! Logic handlers are responsible for business logic only. They:
//!
//! - Receive parsed CLI arguments (`&ArgMatches`) and execution context
//! - Perform application logic (database queries, file operations, etc.)
//! - Return serializable data that will be passed to the render handler
//!
//! Handlers explicitly do not handle:
//! - Output formatting (that's the render handler's job)
//! - Template selection (that's configured at the framework level)
//! - Theme/style decisions (that's the render handler's job)
//!
//! This separation keeps handlers focused and testable - you can unit test
//! a handler by checking the data it returns, without worrying about rendering.
//!
//! # Core Types
//!
//! - [`CommandContext`]: Environment information passed to handlers
//! - [`Extensions`]: Type-safe container for injecting custom state
//! - [`Output`]: What a handler produces (render data, silent, or binary)
//! - [`HandlerResult`]: The result type for handlers (`Result<Output<T>, Error>`)
//! - [`RunResult`]: The result of running the CLI dispatcher
//! - [`Handler`]: Trait for thread-safe command handlers (`Send + Sync`, `&self`)
//! - [`LocalHandler`]: Trait for local command handlers (no `Send + Sync`, `&mut self`)

use clap::ArgMatches;
use serde::Serialize;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;

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
    map: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Extensions {
    /// Creates a new empty extensions container.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a value into the extensions.
    ///
    /// If a value of this type already exists, it is replaced and returned.
    pub fn insert<T: Send + Sync + 'static>(&mut self, val: T) -> Option<T> {
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
/// Provides information about the execution environment plus an extensions
/// container for injecting custom state via pre-dispatch hooks.
///
/// Note that output format is deliberately not included here - format decisions
/// are made by the render handler, not by logic handlers.
///
/// # Extensions
///
/// Pre-dispatch hooks can inject state into `extensions` that handlers retrieve:
///
/// ```rust
/// use standout_dispatch::{Hooks, HookError, CommandContext};
///
/// struct Database { /* ... */ }
///
/// let hooks = Hooks::new()
///     .pre_dispatch(|_matches, ctx| {
///         ctx.extensions.insert(Database { /* ... */ });
///         Ok(())
///     });
///
/// // In handler:
/// fn my_handler(matches: &clap::ArgMatches, ctx: &CommandContext) -> anyhow::Result<()> {
///     let db = ctx.extensions.get_required::<Database>()?;
///     // use db...
///     Ok(())
/// }
/// ```
#[derive(Debug, Default)]
pub struct CommandContext {
    /// The command path being executed (e.g., ["config", "get"])
    pub command_path: Vec<String>,

    /// Type-safe container for custom state injection.
    ///
    /// Pre-dispatch hooks can insert values that handlers retrieve.
    pub extensions: Extensions,
}

/// What a handler produces.
///
/// This enum represents the different types of output a command handler can produce.
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
/// Enables use of the `?` operator for error propagation.
pub type HandlerResult<T> = Result<Output<T>, anyhow::Error>;

/// Result of running the CLI dispatcher.
///
/// After processing arguments, the dispatcher either handles a command
/// or falls through for manual handling.
#[derive(Debug)]
pub enum RunResult {
    /// A handler processed the command; contains the rendered output
    Handled(String),
    /// A handler produced binary output (bytes, suggested filename)
    Binary(Vec<u8>, String),
    /// Silent output (handler completed but produced no output)
    Silent,
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

    /// Returns true if the result is silent.
    pub fn is_silent(&self) -> bool {
        matches!(self, RunResult::Silent)
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

/// Trait for thread-safe command handlers.
///
/// Handlers must be `Send + Sync` and use immutable `&self`.
pub trait Handler: Send + Sync {
    /// The output type produced by this handler (must be serializable)
    type Output: Serialize;

    /// Execute the handler with the given matches and context.
    fn handle(&self, matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Self::Output>;
}

/// A wrapper that implements Handler for closures.
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

/// Trait for local (single-threaded) command handlers.
///
/// Unlike [`Handler`], this trait:
/// - Does NOT require `Send + Sync`
/// - Takes `&mut self` instead of `&self`
/// - Allows handlers to mutate their internal state directly
pub trait LocalHandler {
    /// The output type produced by this handler (must be serializable)
    type Output: Serialize;

    /// Execute the handler with the given matches and context.
    fn handle(&mut self, matches: &ArgMatches, ctx: &CommandContext)
        -> HandlerResult<Self::Output>;
}

/// A wrapper that implements LocalHandler for FnMut closures.
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
            command_path: vec!["config".into(), "get".into()],
            extensions: Extensions::new(),
        };
        assert_eq!(ctx.command_path, vec!["config", "get"]);
    }

    #[test]
    fn test_command_context_default() {
        let ctx = CommandContext::default();
        assert!(ctx.command_path.is_empty());
        assert!(ctx.extensions.is_empty());
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
        struct Data(i32);

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
        let handler = FnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
            Ok(Output::Render(json!({"status": "ok"})))
        });

        let ctx = CommandContext::default();
        let matches = clap::Command::new("test").get_matches_from(vec!["test"]);

        let result = handler.handle(&matches, &ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_local_fn_handler_mutation() {
        let mut counter = 0u32;

        let mut handler = LocalFnHandler::new(|_m: &ArgMatches, _ctx: &CommandContext| {
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
}
