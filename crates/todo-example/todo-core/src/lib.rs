//! CLI-free todo application logic and JSON persistence.
//!
//! Knows nothing about clap, Standout, environment variables, templates, or
//! terminal output. Load a store from an explicit path, then add, list,
//! complete, or export todos. [`TodoStore::export_csv`] returns exact bytes,
//! a suggested filename, and typed warnings — choosing a destination,
//! writing the file, and wording the result belong to the caller.

mod export;
mod model;
mod store;

pub use export::{CsvExport, ExportWarning};
pub use model::{Todo, TodoFilter};
pub use store::TodoStore;
