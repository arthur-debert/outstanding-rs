# Seeker — Query Engine Specification

**Status:** Draft
**Created:** 2026-01-28
**Location:** `standout-seeker`

## Overview

Seeker is a generic querying engine for filtering Rust struct collections. The full system provides:

1. **Core logic + imperative API** — Types, operators, clause composition, ordering, execution
2. **Derive macros** — Syntactic sugar for field declaration
3. **CLI bridge** — Clap integration for automatic argument generation

This document specifies the complete design. **Current implementation scope is Phase 1 only: core logic and imperative API.**

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

## Phase 2: Derive Macros (Future)

**Out of scope for current work.**

Will provide:
- `#[derive(Seekable)]` for structs
- `#[seek(...)]` field attributes
- Auto-generated accessor functions
- Field name constants for type-safe queries

---

## Phase 3: CLI Bridge (Future)

**Out of scope for current work.**

Will provide:
- `FilterArgs<T>` for clap integration
- Auto-generated `--field-op=value` arguments
- `--AND`, `--OR`, `--NOT` group markers
- `--order-by`, `--limit`, `--offset` arguments
- Help text generation

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
