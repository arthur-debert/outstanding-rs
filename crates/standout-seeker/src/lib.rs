//! Seeker - Generic query engine for filtering Rust struct collections.
//!
//! Seeker provides a fluent API for building and executing queries against
//! in-memory collections of structs. It supports:
//!
//! - Multiple field types: strings, numbers, timestamps, enums, booleans
//! - Rich operators: equality, comparison, string matching, regex
//! - Clause groups: AND, OR, NOT with fixed combination semantics
//! - Multi-field ordering with ascending/descending
//! - Pagination with limit and offset
//!
//! # Quick Start
//!
//! ```rust
//! use standout_seeker::{Query, Op, Dir, Value, Number};
//!
//! // Define your data
//! struct Task {
//!     name: String,
//!     priority: i32,
//!     archived: bool,
//! }
//!
//! // Create an accessor function
//! fn accessor<'a>(task: &'a Task, field: &str) -> Value<'a> {
//!     match field {
//!         "name" => Value::String(&task.name),
//!         "priority" => Value::Number(Number::I64(task.priority as i64)),
//!         "archived" => Value::Bool(task.archived),
//!         _ => Value::None,
//!     }
//! }
//!
//! // Build and execute a query
//! let tasks = vec![
//!     Task { name: "Write docs".into(), priority: 3, archived: false },
//!     Task { name: "Fix bug".into(), priority: 5, archived: false },
//!     Task { name: "Old task".into(), priority: 1, archived: true },
//! ];
//!
//! let query = Query::new()
//!     .and_gte("priority", 3i64)
//!     .not_eq("archived", true)
//!     .order_desc("priority")
//!     .build();
//!
//! let results = query.filter(&tasks, accessor);
//! assert_eq!(results.len(), 2);
//! assert_eq!(results[0].name, "Fix bug");
//! ```
//!
//! # Query Semantics
//!
//! Queries combine three clause groups with fixed logic:
//!
//! ```text
//! match = (all AND clauses match)
//!       ∧ (at least one OR clause matches, OR no OR clauses exist)
//!       ∧ (no NOT clause matches)
//! ```
//!
//! This provides predictable behavior without complex nesting:
//!
//! - **AND group**: All clauses must match (empty = trivially satisfied)
//! - **OR group**: At least one must match (empty = trivially satisfied)
//! - **NOT group**: None may match (empty = trivially satisfied)
//!
//! # Field Types and Operators
//!
//! | Type | Operators |
//! |------|-----------|
//! | String | `Eq`, `Ne`, `StartsWith`, `EndsWith`, `Contains`, `Regex` |
//! | Number | `Eq`, `Ne`, `Gt`, `Gte`, `Lt`, `Lte` |
//! | Timestamp | `Eq`, `Ne`, `Before`, `After`, `Gt`, `Gte`, `Lt`, `Lte` |
//! | Enum | `Eq`, `Ne`, `In` |
//! | Bool | `Eq`, `Ne`, `Is` |

mod clause;
mod error;
mod op;
mod ordering;
mod query;
mod value;

// Re-export public API
pub use clause::{Clause, ClauseValue};
pub use error::{Result, SeekerError};
pub use op::Op;
pub use ordering::{compare_values, Dir, OrderBy};
pub use query::Query;
pub use value::{Number, Timestamp, Value};
