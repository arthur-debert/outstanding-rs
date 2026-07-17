//! CLI-free todo application logic and JSON persistence.
//!
//! This crate deliberately knows nothing about clap, Standout, environment
//! variables, templates, or terminal output. Both Rust callers and tests use
//! the same small interface: load a store from an explicit path, then add,
//! list, complete, or export todos.
//!
//! [`TodoStore::export_csv`] shows where that line falls for artifacts: the
//! core returns exact bytes, a *suggested* filename, and typed warnings, and
//! stops there. Choosing a destination, writing the file, and wording the
//! result belong to the shell.

mod export;
mod model;
mod store;

pub use export::{CsvExport, ExportWarning};
pub use model::{Todo, TodoFilter};
pub use store::TodoStore;
