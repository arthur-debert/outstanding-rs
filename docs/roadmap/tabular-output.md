# Tabular Output Design

## Overview

Tabular and columnar output is a common need for CLI applications displaying lists of items with aligned fields. This feature provides a composable, template-friendly API for column-aligned output that integrates with outstanding's existing rendering pipeline.

## Goals

1. **Row-by-row formatting** - Enable interleaved output where content can be inserted between formatted rows
2. **ANSI-aware** - Correctly measure and align text containing terminal escape codes
3. **Template integration** - MiniJinja filters for declarative column formatting
4. **Truncation control** - Support start, middle, and end truncation (ecosystem gap)
5. **Minimal dependencies** - Build on existing deps: `console`, `unicode-width`, `minijinja`

## Non-Goals

- Full table rendering with box-drawing borders (defer to v2)
- Derive macro for struct-based tables (defer to v2)
- Responsive column hiding based on terminal width
- Ditto/repeat suppression for duplicate values

## Crate Strategy

### Use Existing Dependencies

| Crate | Purpose |
|-------|---------|
| `console` | ANSI-aware `measure_text_width()`, `pad_str()`, `truncate_str()` |
| `unicode-width` | Character width foundation |
| `minijinja` | Template rendering with custom filters |

### Do Not Add

- **comfy-table / tabled** - Render tables as atomic units, incompatible with interleaved output requirement
- **colonnade** - Low value-add given primitives already available via `console`

### Build Custom

- Truncation utilities for start/middle positions
- Column width calculation engine
- Row formatter with interleaved support
- MiniJinja filters for templates

## Data Model

### Alignment

Text alignment within a column: `Left` (default), `Right`, or `Center`.

### Truncation Position

Where to place the ellipsis when content exceeds max width: `End` (default), `Start`, or `Middle`.

### Column Width

How a column determines its width:

| Variant | Description |
|---------|-------------|
| `Fixed(n)` | Exactly n characters |
| `Bounded { min, max }` | Auto-calculate from content within bounds |
| `Fill` | Expand to consume remaining space (one per table) |

### Column Specification

Each column defines: width strategy, alignment, truncation position, ellipsis string (default "…"), null representation (default "-"), and optional style name.

### Table Specification

A table spec combines a vector of column specs with decoration settings: column separator, row prefix, and row suffix.

## API Design

### Imperative API

The builder pattern constructs table specifications. See [Example A1](#a1-imperative-table-construction) for full usage.

Key types:
- `TableSpec::builder()` - Fluent construction
- `TableFormatter::new(&spec, width)` - Creates formatter with resolved widths
- `formatter.format_row(&[...])` - Formats a single row (enables interleaving)

For data-driven width calculation where column widths depend on content, a two-pass approach resolves widths from data before formatting. See [Example A2](#a2-data-driven-width-calculation).

### Template API

New MiniJinja filters enable declarative column formatting in templates.

#### Core Filter: `col`

The `col` filter handles width, alignment, and truncation in one call. Basic signature: `{{ value | col(width) }}` or with options `{{ value | col(width, align="right", truncate="middle") }}`.

The special width value `"fill"` expands to remaining space (requires `__term_width__` in context).

#### Convenience Filters

| Filter | Purpose |
|--------|---------|
| `pad_left(n)` | Right-align in n characters |
| `pad_right(n)` | Left-align in n characters |
| `truncate_at(n, pos)` | Truncate at position with ellipsis |

See [Example A3](#a3-template-usage) for template patterns.

### Utility Functions

Public functions for direct use outside templates:

| Function | Purpose |
|----------|---------|
| `truncate_end(s, width, ellipsis)` | Truncate at end |
| `truncate_start(s, width, ellipsis)` | Truncate at start |
| `truncate_middle(s, width, ellipsis)` | Truncate in middle |
| `pad_left(s, width)` | Right-align with padding |
| `pad_right(s, width)` | Left-align with padding |
| `pad_center(s, width)` | Center with padding |
| `display_width(s)` | ANSI-aware width measurement |

All functions are ANSI-aware: escape codes don't count toward display width.

## Width Resolution Algorithm

When a table contains `Bounded` or `Fill` columns, widths must be resolved before formatting:

1. Calculate fixed column widths and separator overhead
2. For `Bounded` columns, scan data to find actual max width, clamp to bounds
3. Remaining space after fixed and bounded columns goes to `Fill` column
4. If no `Fill` column and space remains, distribute to rightmost `Bounded` column

## Implementation Plan

### Phase 1: Utility Functions

Create `src/table/mod.rs` with core truncation and padding utilities.

**Deliverables:**
- `truncate_end`, `truncate_start`, `truncate_middle`
- `pad_left`, `pad_right`, `pad_center`
- `display_width` wrapper
- Comprehensive tests including ANSI sequences and CJK characters

**Tests:** Unit tests for each function covering edge cases (empty strings, exact fit, off-by-one, zero width, ANSI codes, wide characters).

### Phase 2: Core Types

Define the data model types with builder patterns.

**Deliverables:**
- `Align`, `TruncateAt`, `Width` enums
- `Column` struct with builder
- `Decorations` struct
- `TableSpec` struct with builder
- Serde derive for configuration loading

**Tests:** Builder ergonomics, default values, serde round-trip.

### Phase 3: Width Resolution

Implement the width calculation algorithm.

**Deliverables:**
- `TableSpec::resolve_widths(&self, data, total_width) -> Vec<usize>`
- Handle all `Width` variants
- Edge cases: no Fill column, multiple Bounded, overflow

**Tests:** Property-based tests with proptest for width invariants (sum ≤ total, min ≤ resolved ≤ max).

### Phase 4: Row Formatter

Build the `TableFormatter` that produces formatted rows.

**Deliverables:**
- `TableFormatter::new(&spec, width)`
- `TableFormatter::from_resolved(&spec, widths)`
- `format_row(&self, values: &[&str]) -> String`
- `format_rows(&self, rows) -> Vec<String>`

**Tests:** Integration tests with various specs, ANSI passthrough, interleaved output patterns.

### Phase 5: MiniJinja Filters

Register filters with the outstanding renderer.

**Deliverables:**
- `col` filter with width/align/truncate options
- `pad_left`, `pad_right` filters
- `truncate_at` filter
- Registration in renderer setup

**Tests:** Template rendering tests, filter argument parsing, error handling.

### Phase 6: Integration

Wire everything into outstanding's public API.

**Deliverables:**
- Re-export from `outstanding::table`
- Documentation and examples
- Integration with `OutputMode` (tables serialize as arrays in JSON mode)

## File Structure

```
crates/outstanding/src/
├── lib.rs              # Add `pub mod table;` and re-exports
└── table/
    ├── mod.rs          # Public API, re-exports
    ├── types.rs        # Align, TruncateAt, Width, Column, TableSpec
    ├── util.rs         # Truncation and padding functions
    ├── resolve.rs      # Width resolution algorithm
    ├── formatter.rs    # TableFormatter
    └── filters.rs      # MiniJinja filter implementations
```

---

## Appendix: Code Examples

### A1: Imperative Table Construction

```rust
use outstanding::table::{TableSpec, Column, Width, Align, TruncateAt, TableFormatter};

let spec = TableSpec::builder()
    .column(Column::new(Width::Fixed(8)))
    .column(Column::new(Width::Fill)
        .truncate(TruncateAt::Middle)
        .ellipsis("…"))
    .column(Column::new(Width::Bounded { min: Some(6), max: Some(12) })
        .align(Align::Right)
        .style("status"))
    .separator("  ")
    .build();

let formatter = TableFormatter::new(&spec, 80);

// Row-by-row formatting enables interleaved output
println!("{}", formatter.format_row(&["abc123", "path/to/file.rs", "pending"]));
println!("  └─ Note: this file needs review");
println!("{}", formatter.format_row(&["def456", "src/lib.rs", "done"]));
```

### A2: Data-Driven Width Calculation

```rust
use outstanding::table::{TableSpec, TableFormatter};

let spec = TableSpec::builder()
    .column(Column::new(Width::Bounded { min: Some(5), max: Some(20) }))
    .column(Column::new(Width::Fill))
    .column(Column::new(Width::Fixed(8)))
    .separator("  ")
    .build();

// First pass: analyze data
let data: Vec<Vec<&str>> = vec![
    vec!["short", "A description", "pending"],
    vec!["much longer id", "Another one", "done"],
];
let resolved = spec.resolve_widths(&data, 80);

// Second pass: format with resolved widths
let formatter = TableFormatter::from_resolved(&spec, &resolved);
for row in &data {
    println!("{}", formatter.format_row(row));
}
```

### A3: Template Usage

```jinja
{# Basic fixed-width columns #}
{% for entry in entries %}
{{ entry.hash | col(7) }}  {{ entry.author | col(12) }}  {{ entry.message | col(50) }}
{% endfor %}

{# With alignment and truncation #}
{% for item in items %}
{{ item.count | col(6, align="right") }}  {{ item.path | col(40, truncate="middle") }}
{% endfor %}

{# Fill column (requires __term_width__ in context) #}
{% for task in tasks %}
{{ task.id | col(4, align="right") }}  {{ task.title | col("fill") }}  {{ task.status | col(10) }}
{% endfor %}

{# Convenience filters #}
{{ value | pad_left(10) }}
{{ path | truncate_at(30, "start") }}
```

### A4: Complete Integration Example

```rust
use outstanding::{render, Theme};
use outstanding::table::{TableSpec, Column, Width, TableFormatter};
use serde::Serialize;

#[derive(Serialize)]
struct Task {
    id: String,
    title: String,
    status: String,
}

fn main() {
    let tasks = vec![
        Task { id: "1".into(), title: "Call the family".into(), status: "pending".into() },
        Task { id: "1.1".into(), title: "Call Mom".into(), status: "pending".into() },
        Task { id: "1.2".into(), title: "Call Dad".into(), status: "done".into() },
    ];

    let spec = TableSpec::builder()
        .column(Column::new(Width::Fixed(4)).align(Align::Right))
        .column(Column::new(Width::Fill))
        .column(Column::new(Width::Fixed(10)).style("status"))
        .separator("  ")
        .build();

    let formatter = TableFormatter::new(&spec, 80);

    for task in &tasks {
        println!("{}", formatter.format_row(&[&task.id, &task.title, &task.status]));
    }
}
```

Output:
```
   1  Call the family                                              pending
 1.1  Call Mom                                                     pending
 1.2  Call Dad                                                     done
```

### A5: Property Test Example

```rust
use proptest::prelude::*;
use outstanding::table::{TableSpec, Column, Width};

proptest! {
    #[test]
    fn resolved_widths_sum_to_at_most_total(
        total_width in 20usize..200,
        num_cols in 1usize..5,
    ) {
        let spec = TableSpec::builder()
            .columns((0..num_cols).map(|_|
                Column::new(Width::Bounded { min: Some(5), max: Some(30) })
            ))
            .separator("  ")
            .build();

        let data: Vec<Vec<&str>> = vec![vec!["test"; num_cols]];
        let widths = spec.resolve_widths(&data, total_width);

        let sep_overhead = (num_cols - 1) * 2;
        let total: usize = widths.iter().sum();
        prop_assert!(total + sep_overhead <= total_width);
    }
}
```
