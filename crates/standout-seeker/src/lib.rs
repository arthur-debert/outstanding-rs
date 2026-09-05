//! Generic query engine for filtering in-memory collections of structs.
//!
//! A [`Query`] holds three clause groups combined with fixed semantics:
//! `match = (all AND clauses match) AND (any OR clause matches, or none exist)
//! AND (no NOT clause matches)` — each group is trivially satisfied when empty.
//! [`Query::filter`] runs a query against a slice via a caller-supplied
//! accessor function (`Fn(&T, field) -> Value`), so the engine never needs
//! reflection or derive macros.

mod clause;
mod error;
mod op;
mod ordering;
mod parse;
mod query;
mod schema;
mod traits;
mod value;
pub use clause::{Clause, ClauseValue};
pub use error::{Result, SeekerError};
pub use op::Op;
pub use ordering::{compare_values, Dir, OrderBy};
pub use parse::{
    parse_key, parse_operator, parse_ordering, parse_query, parse_value, ClauseGroup, ParseError,
    ParseResult,
};
pub use query::Query;
pub use schema::{SeekType, SeekerSchema};
pub use traits::{Seekable, SeekerEnum, SeekerTimestamp};
pub use value::{Number, Timestamp, Value};
