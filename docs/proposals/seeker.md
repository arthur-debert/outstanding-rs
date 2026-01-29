# Seeker — Query Engine Specification

**Status:** Draft
**Created:** 2026-01-28
**Location:** `standout-seeker`

## Overview

Seeker is a generic querying engine for filtering Rust struct collections. The full system provides:

1. **Core logic + imperative API** — Types, operators, clause composition, ordering, execution
2. **Derive macros** — Syntactic sugar for field declaration
3. **String parsing** — Shell-agnostic query parsing from key-value pairs
4. **CLI bridge** — Clap integration for automatic argument generation

This document specifies the complete design.

---

## Prior Art Research

Before designing Seeker, we evaluated existing Rust crates:

| Crate | Operators | AND/OR/NOT | Ordering | Field Derive | Verdict |
|-------|-----------|------------|----------|--------------|---------|
| `predicates` | Excellent | Yes/Yes/Yes | No | No | Great composition, no field extraction |
| `modql` | Excellent | Yes/Yes/? | Yes | Yes | SQL-focused, needs custom executor |
| `vec_filter` | Good | Yes/Yes/No | No | Yes | Missing NOT and ordering |
| `fltrs` | Basic | Yes/Yes/Yes | No | PathResolver | Limited operators |

**Decision:** Build custom. No existing crate combines: derive macro + full operators + AND/OR/NOT + ordering. The gaps would require building most of the system anyway.

---

## Motivation

Many CLI tools operate on collections: file trees, task lists, log entries, git commits. Users need to filter these collections, but building good filtering UX is tedious:

- Defining CLI flags for each filterable field
- Parsing and validating filter values
- Implementing comparison logic per field type
- Combining multiple filters with AND/OR/NOT semantics
- Adding sorting and pagination

Seeker extracts this pattern into a reusable system where:

- **Field declaration** is a single annotation (Phase 2)
- **Query semantics** are consistent and predictable
- **CLI generation** is automatic (Phase 3)
- **Custom types** can be plugged in for domain-specific needs

### Example: Before and After

**Before** (manual implementation):

```rust
#[derive(Parser)]
struct ListArgs {
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    name_contains: Option<String>,
    #[arg(long)]
    status: Option<Status>,
    #[arg(long)]
    created_after: Option<DateTime>,
    // ... repeat for every field and operator
}

fn filter(items: &[Item], args: &ListArgs) -> Vec<&Item> {
    items.iter().filter(|item| {
        if let Some(ref name) = args.name {
            if &item.name != name { return false; }
        }
        // ... repeat for every filter
        true
    }).collect()
}
```

**After** (with Seeker — Phase 2+):

```rust
#[derive(Seekable)]
struct Item {
    #[seek(string)]
    name: String,
    #[seek(enum)]
    status: Status,
    #[seek(timestamp)]
    created_at: DateTime,
}

// CLI args generated automatically
// Filtering logic generated automatically
```

**After** (with Seeker — Phase 1, imperative API):

```rust
use seeker::{Query, Op, Dir};

let query = Query::new()
    .and("name", Op::Eq, "README.md")
    .and("status", Op::In, vec![Status::Active, Status::Pending])
    .order_by("created_at", Dir::Desc)
    .build();

let results = query.filter(&items, |item, field| {
    match field {
        "name" => Value::String(&item.name),
        "status" => Value::Enum(item.status),
        "created_at" => Value::Timestamp(item.created_at),
        _ => Value::None,
    }
});
```

---

## Non-Goals

This is **not**:

1. **A high-performance database** — No indexing, query planning, or ACID guarantees
2. **A solution for large datasets** — Target use cases are in-memory collections of hundreds to low thousands of items
3. **A complete SQL engine** — No joins, subqueries, or aggregations
4. **A nested/relational query system** (v1) — Queries operate on flat field access only

---

## Core Concepts

### Field Types

Each queryable field has one of these types:

| Type | Rust Types | Description |
|------|------------|-------------|
| `String` | `String`, `&str`, `Cow<str>` | Text data |
| `Number` | `i8`–`i128`, `u8`–`u128`, `f32`, `f64` | Numeric data |
| `Timestamp` | `SystemTime`, `chrono::DateTime<Tz>` | Temporal data |
| `Enum` | Unit enums (no associated data) | Discrete choices |
| `Bool` | `bool` | Boolean flags |

### Operators

Each type supports specific comparison operators:

**String Operators:**

| Operator | Description |
|----------|-------------|
| `Eq` | Exact match (default) |
| `Ne` | Not equal |
| `StartsWith` | Prefix match |
| `EndsWith` | Suffix match |
| `Contains` | Substring match |
| `Regex` | Regular expression match |

**Number Operators:**

| Operator | Description |
|----------|-------------|
| `Eq` | Equal (default) |
| `Ne` | Not equal |
| `Gt` | Greater than |
| `Gte` | Greater than or equal |
| `Lt` | Less than |
| `Lte` | Less than or equal |

**Timestamp Operators:**

| Operator | Description |
|----------|-------------|
| `Eq` | Exact match (default) |
| `Ne` | Not equal |
| `Before` | Earlier than |
| `After` | Later than |

**Enum Operators:**

| Operator | Description |
|----------|-------------|
| `Eq` | Exact match (default) |
| `Ne` | Not equal |
| `In` | Value is one of the given set |

**Bool Operators:**

| Operator | Description |
|----------|-------------|
| `Eq` | Equal (default) |
| `Is` | Alias for `Eq` |

### Clause

A **clause** is a single filter predicate consisting of:

- A field name
- An operator
- A comparison value

Examples:
- `name Eq "index.html"` — name equals "index.html"
- `size Lt 1024` — size less than 1024
- `status In [Active, Pending]` — status is Active or Pending

### Clause Groups

A **clause group** is a set of clauses sharing a logical role. There are three groups:

| Group | Role | Internal Logic |
|-------|------|----------------|
| **AND** | All must match | `clause₁ AND clause₂ AND ...` |
| **OR** | At least one must match | `clause₁ OR clause₂ OR ...` |
| **NOT** | None may match | `NOT clause₁ AND NOT clause₂ AND ...` |

### Query

A **query** combines clause groups using fixed semantics:

```
match = (all AND clauses match)
      ∧ (at least one OR clause matches, OR no OR clauses exist)
      ∧ (no NOT clause matches)
```

In Rust:

```rust
fn matches<T, F>(item: &T, query: &Query, accessor: F) -> bool
where
    F: Fn(&T, &str) -> Value,
{
    let and_pass = query.and_clauses.iter().all(|c| c.matches(item, &accessor));
    let or_pass = query.or_clauses.is_empty()
                  || query.or_clauses.iter().any(|c| c.matches(item, &accessor));
    let not_pass = query.not_clauses.iter().all(|c| !c.matches(item, &accessor));

    and_pass && or_pass && not_pass
}
```

**Key behaviors:**

- If no OR clauses exist, the OR group is trivially satisfied (allows pure-AND queries)
- If no AND clauses exist, the AND group is trivially satisfied
- NOT clauses are individually negated, then combined with AND (all must not match)

### Ordering

A query may include an **ordering specification**: a list of `(field, direction)` pairs applied in precedence order.

```rust
ordering: vec![
    ("name", Dir::Asc),
    ("modified_at", Dir::Desc),
]
```

The first pair is the primary sort key, the second breaks ties, and so on.

If no ordering is specified, the original collection order is preserved.

### Limits

A query may specify:

| Parameter | Description |
|-----------|-------------|
| `limit` | Maximum number of results to return |
| `offset` | Number of results to skip before returning |

---

## Phase 1: Core Logic + Imperative API

**This is the current implementation scope.**

### Crate Structure

```
crates/standout-seeker/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── value.rs       # Value enum for field values (+ unit tests)
│   ├── op.rs          # Operator enum and comparison logic (+ unit tests)
│   ├── clause.rs      # Clause struct (+ unit tests)
│   ├── query.rs       # Query builder and execution (+ unit tests)
│   ├── ordering.rs    # Ordering types and sort logic (+ unit tests)
│   └── error.rs       # Error types
└── tests/
    ├── coverage.rs    # Additional coverage tests for edge cases
    └── proptest.rs    # Property-based tests
```

**Note:** Unit tests are co-located with source files (idiomatic Rust `#[cfg(test)]` modules).
Integration-style tests are in `tests/coverage.rs` for additional coverage.

### Core Types

```rust
// value.rs
/// Runtime value for comparison
#[derive(Debug, Clone, PartialEq)]
pub enum Value<'a> {
    String(&'a str),
    Number(Number),
    Timestamp(Timestamp),
    Enum(u32),  // Discriminant value
    Bool(bool),
    None,       // Field not present or null
}

/// Numeric value (supports all numeric types)
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum Number {
    I64(i64),
    U64(u64),
    F64(f64),
}

/// Timestamp value
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(pub i64);  // Unix timestamp in milliseconds
```

```rust
// op.rs
/// Comparison operator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    // Universal
    Eq,
    Ne,

    // String
    StartsWith,
    EndsWith,
    Contains,
    Regex,

    // Number/Timestamp
    Gt,
    Gte,
    Lt,
    Lte,

    // Timestamp aliases
    Before,  // Alias for Lt
    After,   // Alias for Gt

    // Enum
    In,

    // Bool alias
    Is,  // Alias for Eq
}
```

```rust
// clause.rs
/// A single filter predicate
#[derive(Debug, Clone)]
pub struct Clause {
    pub field: String,
    pub op: Op,
    pub value: ClauseValue,
}

/// Value in a clause (owned, for storage)
#[derive(Debug, Clone)]
pub enum ClauseValue {
    String(String),
    Number(Number),
    Timestamp(Timestamp),
    Enum(u32),
    EnumSet(Vec<u32>),
    Bool(bool),
    Regex(regex::Regex),
}
```

```rust
// ordering.rs
/// Sort direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Asc,
    Desc,
}

/// Single ordering clause
#[derive(Debug, Clone)]
pub struct OrderBy {
    pub field: String,
    pub dir: Dir,
}
```

```rust
// query.rs
/// Query builder and executor
#[derive(Debug, Clone, Default)]
pub struct Query {
    and_clauses: Vec<Clause>,
    or_clauses: Vec<Clause>,
    not_clauses: Vec<Clause>,
    ordering: Vec<OrderBy>,
    limit: Option<usize>,
    offset: Option<usize>,
}
```

### Query Builder API

```rust
impl Query {
    /// Create a new empty query
    pub fn new() -> Self;

    // --- Clause building ---

    /// Add an AND clause
    pub fn and(self, field: &str, op: Op, value: impl Into<ClauseValue>) -> Self;

    /// Add an OR clause
    pub fn or(self, field: &str, op: Op, value: impl Into<ClauseValue>) -> Self;

    /// Add a NOT clause
    pub fn not(self, field: &str, op: Op, value: impl Into<ClauseValue>) -> Self;

    // --- Shorthand methods ---

    pub fn and_eq(self, field: &str, value: impl Into<ClauseValue>) -> Self;
    pub fn and_ne(self, field: &str, value: impl Into<ClauseValue>) -> Self;
    pub fn and_gt(self, field: &str, value: impl Into<ClauseValue>) -> Self;
    pub fn and_gte(self, field: &str, value: impl Into<ClauseValue>) -> Self;
    pub fn and_lt(self, field: &str, value: impl Into<ClauseValue>) -> Self;
    pub fn and_lte(self, field: &str, value: impl Into<ClauseValue>) -> Self;
    pub fn and_contains(self, field: &str, value: &str) -> Self;
    pub fn and_startswith(self, field: &str, value: &str) -> Self;
    pub fn and_endswith(self, field: &str, value: &str) -> Self;
    pub fn and_regex(self, field: &str, pattern: &str) -> Result<Self, regex::Error>;
    pub fn and_in(self, field: &str, values: impl IntoIterator<Item = u32>) -> Self;
    pub fn and_before(self, field: &str, ts: Timestamp) -> Self;
    pub fn and_after(self, field: &str, ts: Timestamp) -> Self;

    pub fn or_eq(self, field: &str, value: impl Into<ClauseValue>) -> Self;
    // ... similar for or_* and not_*

    // --- Ordering ---

    pub fn order_by(self, field: &str, dir: Dir) -> Self;
    pub fn order_asc(self, field: &str) -> Self;
    pub fn order_desc(self, field: &str) -> Self;

    // --- Limits ---

    pub fn limit(self, n: usize) -> Self;
    pub fn offset(self, n: usize) -> Self;

    // --- Finalize ---

    pub fn build(self) -> Self;  // Validates and returns
}
```

### Query Execution API

```rust
impl Query {
    /// Filter a slice, returning references to matching items
    pub fn filter<'a, T, F>(&self, items: &'a [T], accessor: F) -> Vec<&'a T>
    where
        F: Fn(&T, &str) -> Value<'_>;

    /// Filter and clone matching items
    pub fn filter_cloned<T, F>(&self, items: &[T], accessor: F) -> Vec<T>
    where
        T: Clone,
        F: Fn(&T, &str) -> Value<'_>;

    /// Filter in place
    pub fn filter_mut<T, F>(&self, items: &mut Vec<T>, accessor: F)
    where
        F: Fn(&T, &str) -> Value<'_>;

    /// Count matching items without collecting
    pub fn count<T, F>(&self, items: &[T], accessor: F) -> usize
    where
        F: Fn(&T, &str) -> Value<'_>;

    /// Check if any item matches
    pub fn any<T, F>(&self, items: &[T], accessor: F) -> bool
    where
        F: Fn(&T, &str) -> Value<'_>;

    /// Check if all items match
    pub fn all<T, F>(&self, items: &[T], accessor: F) -> bool
    where
        F: Fn(&T, &str) -> Value<'_>;

    /// Find first matching item
    pub fn find<'a, T, F>(&self, items: &'a [T], accessor: F) -> Option<&'a T>
    where
        F: Fn(&T, &str) -> Value<'_>;
}
```

### Usage Example (Phase 1)

```rust
use seeker::{Query, Op, Dir, Value, Number, Timestamp};

#[derive(Debug, Clone)]
struct Task {
    name: String,
    priority: u8,
    status: Status,
    created_at: i64,
    archived: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Status { Pending, Active, Completed }

impl Status {
    fn discriminant(self) -> u32 {
        match self {
            Status::Pending => 0,
            Status::Active => 1,
            Status::Completed => 2,
        }
    }
}

fn main() {
    let tasks: Vec<Task> = load_tasks();

    // Build query
    let query = Query::new()
        .and_eq("status", Status::Active.discriminant())
        .and_gte("priority", 5u8)
        .or_contains("name", "urgent")
        .or_contains("name", "critical")
        .not_eq("archived", true)
        .order_by("priority", Dir::Desc)
        .order_by("created_at", Dir::Asc)
        .limit(20)
        .build();

    // Execute with accessor function
    let results = query.filter(&tasks, |task, field| {
        match field {
            "name" => Value::String(&task.name),
            "priority" => Value::Number(Number::U64(task.priority as u64)),
            "status" => Value::Enum(task.status.discriminant()),
            "created_at" => Value::Timestamp(Timestamp(task.created_at)),
            "archived" => Value::Bool(task.archived),
            _ => Value::None,
        }
    });

    for task in results {
        println!("{}: {:?}", task.name, task.status);
    }
}
```

---

## Testing Requirements

### Coverage Target

Aim for **90%+ line coverage** on core logic. Use `cargo tarpaulin` to verify:

```bash
cargo tarpaulin --out Html --output-dir coverage/
```

### Test Categories

1. **Unit tests** (`tests/operators.rs`)
   - Every operator on every applicable type
   - Edge cases: empty strings, zero, negative numbers, boundary timestamps
   - Invalid comparisons (e.g., `Contains` on `Number`) should return false or error appropriately

2. **Clause tests** (`tests/clauses.rs`)
   - Single clause matching
   - Clause with `Value::None` (missing field)
   - Regex compilation and matching

3. **Query composition tests** (`tests/query.rs`)
   - Empty query matches everything
   - AND-only queries
   - OR-only queries
   - NOT-only queries
   - Combined AND + OR + NOT
   - Empty OR group (trivially satisfied)

4. **Ordering tests** (`tests/ordering.rs`)
   - Single field ascending/descending
   - Multi-field ordering (tie-breaking)
   - Stable sort (preserve order for equal keys)
   - Ordering with limit/offset

5. **Integration tests** (`tests/integration.rs`)
   - Real-world scenarios with realistic data
   - Large collections (1000+ items)
   - Complex queries combining all features

6. **Property-based tests** (`tests/proptest.rs`)
   - Use `proptest` for fuzzing
   - Properties to test:
     - `filter(items).len() <= items.len()`
     - `count(items) == filter(items).len()`
     - Empty query: `filter(items).len() == items.len()`
     - NOT negation: `filter(q) ∩ filter(not_q) == ∅` for single-clause queries
     - Ordering is stable
     - Limit respects bound: `filter(items).len() <= limit`

### Proptest Example

```rust
use proptest::prelude::*;
use seeker::{Query, Op, Value, Number};

proptest! {
    #[test]
    fn filter_never_grows_collection(
        items in prop::collection::vec(any::<i64>(), 0..100),
        threshold in any::<i64>(),
    ) {
        let query = Query::new()
            .and("value", Op::Gt, threshold)
            .build();

        let results = query.filter(&items, |&n, _| Value::Number(Number::I64(n)));
        prop_assert!(results.len() <= items.len());
    }

    #[test]
    fn count_equals_filter_len(
        items in prop::collection::vec(any::<i64>(), 0..100),
        threshold in any::<i64>(),
    ) {
        let query = Query::new()
            .and("value", Op::Gte, threshold)
            .build();

        let accessor = |&n: &i64, _: &str| Value::Number(Number::I64(n));
        let filtered = query.filter(&items, accessor);
        let counted = query.count(&items, accessor);

        prop_assert_eq!(filtered.len(), counted);
    }

    #[test]
    fn empty_query_matches_all(
        items in prop::collection::vec(any::<String>(), 0..50),
    ) {
        let query = Query::new().build();
        let results = query.filter(&items, |s, _| Value::String(s));
        prop_assert_eq!(results.len(), items.len());
    }
}
```

---

## Phase 2: Derive Macros

**Status: Implemented**

The `#[derive(Seekable)]` macro generates accessor functions and field constants.

### Field Attributes

| Attribute | Description |
|-----------|-------------|
| `#[seek(String)]` | String field (Eq, Ne, Contains, StartsWith, EndsWith, Regex) |
| `#[seek(Number)]` | Numeric field (Eq, Ne, Gt, Gte, Lt, Lte) |
| `#[seek(Timestamp)]` | Timestamp field (Eq, Ne, Before, After) - requires `SeekerTimestamp` impl |
| `#[seek(Enum)]` | Enum field (Eq, Ne, In) - requires `SeekerEnum` impl |
| `#[seek(Bool)]` | Boolean field (Eq, Ne, Is) |
| `#[seek(skip)]` | Exclude field from queries |
| `#[seek(ty = "enum")]` | Alternative syntax for reserved keywords |
| `rename = "..."` | Custom query field name |

### Generated Code

For each annotated field, the macro generates:

1. **Field constant**: `Task::NAME`, `Task::PRIORITY`, etc. (SCREAMING_SNAKE_CASE)
2. **Seekable trait impl**: `seeker_field_value()` method
3. **Accessor function**: `Task::accessor()` for use with `Query::filter()`

### Example

```rust
use standout_macros::Seekable;
use standout_seeker::{Query, Seekable, SeekerEnum};

#[derive(Clone, Copy)]
enum Status { Pending, Active, Done }

impl SeekerEnum for Status {
    fn seeker_discriminant(&self) -> u32 {
        match self {
            Status::Pending => 0,
            Status::Active => 1,
            Status::Done => 2,
        }
    }
}

#[derive(Seekable)]
struct Task {
    #[seek(String)]
    name: String,

    #[seek(Number)]
    priority: u8,

    #[seek(Enum)]
    status: Status,

    #[seek(Bool)]
    done: bool,

    #[seek(skip)]
    internal_id: u64,
}

// Usage with Query
let tasks = vec![/* ... */];
let query = Query::new()
    .and_gte(Task::PRIORITY, 3u8)
    .not_eq(Task::DONE, true)
    .build();

let results = query.filter(&tasks, Task::accessor);
```

### Helper Traits

For enum and timestamp fields, implement the corresponding traits:

- **`SeekerEnum`**: `fn seeker_discriminant(&self) -> u32`
- **`SeekerTimestamp`**: `fn seeker_timestamp(&self) -> Timestamp`

Built-in `SeekerTimestamp` implementations exist for `i64` and `u64`.

---

## Phase 3: String Parsing

**Status: In Progress**

A shell-agnostic layer that parses key-value string pairs into typed `Query` objects. This layer is reusable across different interfaces (CLI, web APIs, configuration files).

### Motivation

The string format `field-operator=value` appears in multiple contexts:
- CLI arguments: `--name-contains=foo`
- URL query params: `?name-contains=foo`
- Configuration: `name-contains = foo`

By separating string parsing from CLI specifics, we get a reusable layer.

### SeekerSchema Trait

The parser needs field metadata to:
1. Validate field names exist
2. Determine field types for value parsing
3. Validate operators are appropriate for the field type

```rust
/// Field metadata for query parsing.
///
/// This trait is optional — only needed for string-based query parsing.
/// Types using only the imperative API don't need to implement it.
pub trait SeekerSchema {
    /// Returns the type of a field, or None if field doesn't exist.
    fn field_type(field: &str) -> Option<SeekType>;

    /// Returns all queryable field names.
    fn field_names() -> &'static [&'static str];
}

/// The type of a seekable field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeekType {
    String,
    Number,
    Timestamp,
    Enum,
    Bool,
}
```

The derive macro generates `SeekerSchema` alongside `Seekable` by default. Users can override with a manual implementation to expose only specific fields.

### String Format

**Key format:** `field-name-operator`

Rules:
1. Split on `-` (dash)
2. Operators cannot contain dashes
3. Last segment is the operator (if it's a valid operator name)
4. Preceding segments form the field name (rejoined with `-`)
5. If no operator segment, use the type's default operator

**Examples:**
- `name-contains` → field: `name`, operator: `contains`
- `created-at-before` → field: `created-at`, operator: `before`
- `modified-by-eq` → field: `modified-by`, operator: `eq`
- `name` → field: `name`, operator: `eq` (default)
- `archived` → field: `archived`, operator: `is` (bool default)

**Valid operator names:**

| Operator | String Name | Aliases |
|----------|-------------|---------|
| `Eq` | `eq` | |
| `Ne` | `ne` | `neq` |
| `Gt` | `gt` | |
| `Gte` | `gte` | |
| `Lt` | `lt` | |
| `Lte` | `lte` | |
| `StartsWith` | `startswith` | `prefix` |
| `EndsWith` | `endswith` | `suffix` |
| `Contains` | `contains` | |
| `Regex` | `regex` | `re`, `match` |
| `Before` | `before` | |
| `After` | `after` | |
| `In` | `in` | |
| `Is` | `is` | |

### Default Operators by Type

When no operator is specified, the default depends on field type:

| Field Type | Default Operator |
|------------|------------------|
| `String` | `Eq` |
| `Number` | `Eq` |
| `Timestamp` | `Eq` |
| `Enum` | `Eq` |
| `Bool` | `Is` (with implicit `true` value) |

For boolean fields, bare `--archived` means `archived-is=true`.

### Value Parsing

Values are parsed based on field type:

| Field Type | Value Format | Examples |
|------------|--------------|----------|
| `String` | Raw string | `foo`, `hello world` |
| `Number` | Integer or float | `42`, `-17`, `3.14` |
| `Timestamp` | ISO 8601 or Unix ms | `2024-01-15`, `2024-01-15T10:30:00Z`, `1705312200000` |
| `Enum` | Discriminant (u32) or variant name | `0`, `1`, `active`, `pending` |
| `Bool` | Boolean literal | `true`, `false`, `1`, `0` |

For `In` operator, values are comma-separated: `--status-in=active,pending,done`

### Clause Groups

Group markers change which group subsequent clauses are added to:

| Marker | Effect |
|--------|--------|
| `AND` | Following clauses go to AND group (default) |
| `OR` | Following clauses go to OR group |
| `NOT` | Following clauses go to NOT group |

Groups apply to all subsequent clauses until the next marker.

### Ordering and Limits

Special keys for ordering and pagination:

| Key | Format | Example |
|-----|--------|---------|
| `order` | `field` or `field-asc` or `field-desc` | `order=priority-desc` |
| `limit` | Integer | `limit=20` |
| `offset` | Integer | `offset=10` |

Multiple `order` entries create multi-field ordering (first is primary).

### Parser API

```rust
/// Error from parsing a query string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Field name not found in schema.
    UnknownField { field: String },
    /// Operator not valid for field type.
    InvalidOperator { field: String, operator: String, field_type: SeekType },
    /// Value could not be parsed for field type.
    InvalidValue { field: String, value: String, expected: SeekType },
    /// Invalid regex pattern.
    InvalidRegex { field: String, pattern: String, error: String },
    /// Invalid ordering specification.
    InvalidOrdering { value: String },
    /// Invalid limit/offset value.
    InvalidLimit { value: String },
}

/// Parse key-value pairs into a Query.
///
/// # Arguments
/// * `pairs` - Iterator of (key, value) string pairs
///
/// # Type Parameters
/// * `S` - Type implementing `SeekerSchema` for field metadata
///
/// # Example
/// ```rust
/// let pairs = vec![
///     ("name-contains".to_string(), "readme".to_string()),
///     ("OR".to_string(), "".to_string()),
///     ("priority-gte".to_string(), "5".to_string()),
///     ("order".to_string(), "priority-desc".to_string()),
/// ];
/// let query = parse_query::<Task>(pairs)?;
/// ```
pub fn parse_query<S: SeekerSchema>(
    pairs: impl IntoIterator<Item = (String, String)>,
) -> Result<Query, ParseError>;

/// Parse a single key into field name and operator.
///
/// Returns (field_name, operator) or just field_name with default operator.
pub fn parse_key<S: SeekerSchema>(key: &str) -> Result<(String, Op), ParseError>;

/// Parse a value string into a ClauseValue based on field type.
pub fn parse_value(
    value: &str,
    field_type: SeekType,
    op: Op,
) -> Result<ClauseValue, ParseError>;
```

### Enum Value Resolution

For enum fields, values can be either:
1. **Numeric discriminant:** `0`, `1`, `2`
2. **Variant name:** `pending`, `active`, `completed`

To support variant names, add an optional method to `SeekerSchema`:

```rust
pub trait SeekerSchema {
    // ... existing methods ...

    /// Resolve an enum variant name to its discriminant.
    /// Returns None if not an enum field or variant not found.
    fn resolve_enum_variant(field: &str, variant: &str) -> Option<u32> {
        None // Default: only numeric discriminants supported
    }
}
```

### Usage Example

```rust
use standout_seeker::{parse_query, Query, SeekerSchema, SeekType};

#[derive(Seekable)]  // Generates both Seekable and SeekerSchema
struct Task {
    #[seek(String)]
    name: String,
    #[seek(Number)]
    priority: u8,
    #[seek(Bool)]
    done: bool,
}

// Parse from any source of key-value pairs
let pairs = vec![
    ("name-contains", "urgent"),
    ("priority-gte", "3"),
    ("NOT", ""),
    ("done", "true"),
    ("order", "priority-desc"),
    ("limit", "10"),
];

let query = parse_query::<Task>(
    pairs.into_iter().map(|(k, v)| (k.to_string(), v.to_string()))
)?;

// Use the query
let results = query.filter(&tasks, Task::accessor);
```

### Crate Structure Addition

```
crates/standout-seeker/
├── src/
│   ├── ...existing files...
│   ├── schema.rs     # SeekerSchema trait and SeekType enum
│   └── parse.rs      # String parsing logic (+ unit tests)
└── tests/
    ├── ...existing files...
    └── parse_proptest.rs  # Property-based tests for parsing
```

---

## Phase 4: CLI Bridge (Future)

**Status: Not Started**
**Prerequisite:** ListView implementation (see `docs/proposals/list-view.md`) — Phase 4 integrates with ListView's `filterable` dispatch attribute.

Clap integration for automatic CLI argument generation.

### Overview

Phase 4 builds on Phase 3's string parsing to provide seamless clap integration:
- Auto-generated `--field-op=value` arguments from `SeekerSchema`
- `--AND`, `--OR`, `--NOT` flags for group markers
- `--order-by`, `--limit`, `--offset` standard arguments
- Help text generation showing available fields and operators

### FilterArgs Derive

```rust
/// Automatically generates clap arguments from a Seekable type.
#[derive(FilterArgs)]
#[filter(schema = Task)]
struct ListArgs {
    // Auto-generated filter arguments from Task's SeekerSchema

    // Can add custom arguments alongside
    #[arg(long)]
    verbose: bool,
}
```

### Generated Arguments

For a `Task` with fields `name: String`, `priority: Number`, `done: Bool`:

```
OPTIONS:
    --name <VALUE>              Filter by name (equals)
    --name-eq <VALUE>           Filter by name equals
    --name-ne <VALUE>           Filter by name not equals
    --name-contains <VALUE>     Filter by name contains
    --name-startswith <VALUE>   Filter by name starts with
    --name-endswith <VALUE>     Filter by name ends with
    --name-regex <PATTERN>      Filter by name matches regex

    --priority <VALUE>          Filter by priority (equals)
    --priority-eq <VALUE>       Filter by priority equals
    --priority-ne <VALUE>       Filter by priority not equals
    --priority-gt <VALUE>       Filter by priority greater than
    --priority-gte <VALUE>      Filter by priority greater or equal
    --priority-lt <VALUE>       Filter by priority less than
    --priority-lte <VALUE>      Filter by priority less or equal

    --done                      Filter by done is true
    --done-eq <VALUE>           Filter by done equals
    --done-ne <VALUE>           Filter by done not equals

    --AND                       Following filters use AND logic (default)
    --OR                        Following filters use OR logic
    --NOT                       Following filters use NOT logic

    --order-by <FIELD>          Order by field (append -asc or -desc)
    --limit <N>                 Maximum results to return
    --offset <N>                Results to skip
```

### Integration API

```rust
impl ListArgs {
    /// Build a Query from the parsed CLI arguments.
    pub fn to_query(&self) -> Result<Query, ParseError>;
}

// Usage in main
fn main() -> Result<()> {
    let args = ListArgs::parse();
    let query = args.to_query()?;

    let tasks = load_tasks();
    let results = query.filter(&tasks, Task::accessor);

    for task in results {
        println!("{}", task.name);
    }
    Ok(())
}
```

### Custom Field Exposure

To expose only specific fields in CLI (while keeping all fields queryable programmatically):

```rust
#[derive(FilterArgs)]
#[filter(schema = Task, fields = ["name", "priority"])]  // Only these in CLI
struct ListArgs { }
```

Or implement `SeekerSchema` manually to control exposure.

### Help Topics Integration

Integrates with standout's help topics system to provide detailed filter documentation:

```
$ myapp help filters

QUERY FILTERS

  Filters select which items to include in results.

  FORMAT: --field-operator=value

  FIELDS:
    name      (string)     The task name
    priority  (number)     Priority level 1-10
    done      (boolean)    Completion status

  OPERATORS BY TYPE:
    String:    eq, ne, contains, startswith, endswith, regex
    Number:    eq, ne, gt, gte, lt, lte
    Boolean:   eq, is
    ...

  EXAMPLES:
    --name-contains=urgent --priority-gte=5
    --OR --name=README --name=CHANGELOG
    --NOT --done
```

---

## Design Decisions

### Why Fixed Group Semantics?

The `AND ∧ OR ∧ NOT` combination model was chosen over arbitrary nesting because:

1. **Predictable** — Users can understand query behavior without mental gymnastics
2. **Covers 95% of use cases** — Most real filters fit this pattern
3. **Simple CLI mapping** — Group markers map directly to semantics (Phase 3)
4. **No ambiguity** — Operator precedence is fixed, not context-dependent

### Why NOT as "None May Match"?

The NOT group means "none of these clauses may match" (each is negated, combined with AND).

The chosen interpretation is clear: `.not_eq("name", "foo").not_eq("name", "bar")` means "name is not foo AND name is not bar."

### Why Empty OR is Trivially Satisfied?

If no OR clauses exist, requiring "at least one OR matches" would make the query unmatchable. Treating empty OR as satisfied allows pure-AND queries to work naturally.

### Why Accessor Functions (Phase 1)?

Without derive macros, we need a way to extract field values from arbitrary structs. The accessor function pattern:

1. Works with any struct without requiring traits
2. Allows selective field exposure
3. Can handle computed fields
4. Is explicit about what's queryable

Phase 2 macros will generate these accessors automatically.

### Why Build Custom?

Evaluated existing crates (`predicates`, `modql`, `vec_filter`, `fltrs`). None combine all required features: full operators + AND/OR/NOT + ordering + future macro/CLI support. Building custom avoids awkward integration seams.

---

## Future Considerations

Out of scope for all phases but may be added later:

| Feature | Description |
|---------|-------------|
| **Nested field access** | `"file.content.size"` for struct fields |
| **Enum variant fields** | Query data inside enum variants |
| **Aggregations** | count, sum, avg over results |
| **Grouping** | Group by field with aggregates |
| **Saved queries** | Named, reusable query definitions |
| **Query algebra** | Combine queries: `query1.and(query2)` |
| **Streaming** | Lazy evaluation for large collections |
| **Indexing hints** | Annotations for query optimization |
