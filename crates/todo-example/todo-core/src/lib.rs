//! CLI-free todo application logic and JSON persistence.
//!
//! This crate deliberately knows nothing about clap, Standout, environment
//! variables, templates, or terminal output. Both Rust callers and tests use
//! the same small interface: load a store from an explicit path, then add,
//! list, or complete todos.

mod model;
mod store;

pub use model::{Todo, TodoFilter};
pub use store::TodoStore;
