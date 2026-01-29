//! Handler execution modes for CLI applications.
//!
//! This module provides the [`HandlerMode`] trait and its two implementations:
//!
//! - [`ThreadSafe`]: Default mode with `Send + Sync` handlers (works with `Arc`)
//! - [`Local`]: Single-threaded mode allowing `&mut self` handlers (works with `RefCell`)
//!
//! # Choosing a Mode
//!
//! Use `ThreadSafe` (the default) when:
//! - Your handlers are stateless or use interior mutability (`Arc<Mutex<_>>`)
//! - You want handlers that could theoretically be called from multiple threads
//! - You're building a library where consumers might need thread safety
//!
//! Use `Local` when:
//! - Your handlers need `&mut self` access to state
//! - You have a single-threaded CLI application
//! - You want to avoid the ceremony of interior mutability wrappers
//!
//! # Example: Thread-safe handlers (default)
//!
//! ```rust,ignore
//! use standout::cli::App;
//!
//! // Stateless handler - works naturally
//! App::builder()
//!     .command("list", |matches, ctx| {
//!         let items = load_items()?;
//!         Ok(Output::Render(items))
//!     }, "{{ items }}")
//!     .build()?
//!     .run(cmd, args);
//!
//! // Stateful handler with interior mutability
//! let cache = Arc::new(Mutex::new(HashMap::new()));
//! App::builder()
//!     .command_handler("cached", CachedHandler { cache })
//!     .build()?
//! ```
//!
//! # Example: Local handlers (mutable state)
//!
//! ```rust,ignore
//! use standout::cli::LocalApp;
//!
//! struct MyApi {
//!     index: HashMap<Uuid, Item>,
//! }
//!
//! impl MyApi {
//!     fn list(&self) -> Vec<Item> { ... }
//!     fn delete(&mut self, id: Uuid) { ... }  // Needs &mut self!
//! }
//!
//! let mut api = MyApi::new();
//!
//! // LocalApp allows FnMut handlers that capture &mut api
//! LocalApp::builder()
//!     .command("list", |m, ctx| {
//!         Ok(Output::Render(api.list()))
//!     }, "{{ items }}")
//!     .command("delete", |m, ctx| {
//!         let id = m.get_one::<Uuid>("id").unwrap();
//!         api.delete(*id);  // &mut self works!
//!         Ok(Output::Silent)
//!     }, "")
//!     .build()?
//!     .run(cmd, args);
//! ```
//!
//! # Design Rationale
//!
//! CLIs are fundamentally single-threaded: parse args → run one handler → output → exit.
//! The `ThreadSafe` mode's `Send + Sync` requirement is conventional (matching web
//! frameworks like Axum) but not strictly necessary for CLI tools.
//!
//! The `Local` mode removes this requirement, allowing natural Rust patterns like
//! `&mut self` methods without forcing interior mutability wrappers (`Arc<Mutex<_>>`).
//!
//! Both modes share the same rendering pipeline, templates, styles, and hooks.
//! The only difference is how handlers are stored and invoked.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use super::dispatch::{DispatchFn, Dispatchable, LocalDispatchFn};

/// Marker trait for handler execution modes.
///
/// This trait is sealed and cannot be implemented outside this crate.
/// Use [`ThreadSafe`] for shared handlers or [`Local`] for mutable handlers.
pub trait HandlerMode: sealed::Sealed + Default + Clone {
    /// The wrapper type for storing dispatch functions.
    ///
    /// - `ThreadSafe`: `Arc<dyn Fn(...) + Send + Sync>`
    /// - `Local`: `Rc<RefCell<dyn FnMut(...)>>`
    type DispatchWrapper<F>: Clone
    where
        F: ?Sized;

    /// Whether this mode requires `Send + Sync` bounds.
    const REQUIRES_SEND_SYNC: bool;

    /// The dispatch function type for this mode.
    type DispatchFn: Dispatchable + Clone;
}

/// Thread-safe handler mode (default).
///
/// Handlers must implement `Send + Sync` and use `Fn` (not `FnMut`).
/// This is the standard mode for most CLI applications and matches
/// the patterns used by web frameworks.
///
/// # When to Use
///
/// - Stateless handlers
/// - Handlers with interior mutability (`Arc<Mutex<_>>`, `Arc<RwLock<_>>`)
/// - When you want your handlers to be potentially reusable across threads
///
/// # Example
///
/// ```rust,ignore
/// use standout::cli::App;
///
/// // App uses ThreadSafe by default
/// App::builder()
///     .command("list", |m, ctx| {
///         Ok(Output::Render(get_items()?))
///     }, "{{ items }}")
///     .build()?
/// ```
#[derive(Debug, Default, Clone, Copy)]
pub struct ThreadSafe;

/// Local (single-threaded) handler mode.
///
/// Handlers can be `FnMut` and don't require `Send + Sync`.
/// This allows handlers to capture `&mut` references to state.
///
/// # When to Use
///
/// - Handlers that need `&mut self` access to state
/// - Single-threaded CLI applications
/// - When you want to avoid `Arc<Mutex<_>>` wrappers
///
/// # Trade-offs
///
/// - Cannot be used with async runtimes that move handlers between threads
/// - Not suitable for library APIs where consumers might need thread safety
/// - Slightly different internal storage (uses `Rc<RefCell<_>>` instead of `Arc`)
///
/// # Example
///
/// ```rust,ignore
/// use standout::cli::LocalApp;
///
/// struct Database {
///     connection: Connection,
/// }
///
/// impl Database {
///     fn query_mut(&mut self) -> Results { ... }
/// }
///
/// let mut db = Database::connect()?;
///
/// LocalApp::builder()
///     .command("query", |m, ctx| {
///         // Can use &mut db here!
///         let results = db.query_mut();
///         Ok(Output::Render(results))
///     }, "{{ results }}")
///     .build()?
/// ```
#[derive(Debug, Default, Clone, Copy)]
pub struct Local;

impl HandlerMode for ThreadSafe {
    type DispatchWrapper<F>
        = Arc<F>
    where
        F: ?Sized;
    const REQUIRES_SEND_SYNC: bool = true;
    type DispatchFn = DispatchFn;
}

impl HandlerMode for Local {
    type DispatchWrapper<F>
        = Rc<RefCell<F>>
    where
        F: ?Sized;
    const REQUIRES_SEND_SYNC: bool = false;
    type DispatchFn = LocalDispatchFn;
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::ThreadSafe {}
    impl Sealed for super::Local {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thread_safe_is_default() {
        let _: ThreadSafe = Default::default();
    }

    #[test]
    fn test_local_is_default() {
        let _: Local = Default::default();
    }

    #[test]
    fn test_mode_constants() {
        assert!(ThreadSafe::REQUIRES_SEND_SYNC);
        assert!(!Local::REQUIRES_SEND_SYNC);
    }
}
