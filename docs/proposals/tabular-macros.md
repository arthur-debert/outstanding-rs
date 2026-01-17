# Tabular Derive Macros - Implementation Plan

**Status:** Approved
**Created:** 2025-01-17

## Overview

This document specifies derive macros for the tabular layout system. These macros were deferred during the initial tabular implementation (see `tabular-implementation-plan.md`) and build on the now-complete underlying APIs.

The macros provide compile-time specification generation from struct annotations, eliminating boilerplate and enabling type-safe column definitions.

## Current State

The tabular module is fully implemented:

| Component | Status | Location |
|-----------|--------|----------|
| Core types (`Column`, `Width`, `Align`, `Anchor`, `Overflow`) | Complete | `src/rendering/tabular/types.rs` |
| `TabularFormatter` with `row_from<T: Serialize>()` | Complete | `src/rendering/tabular/formatter.rs` |
| `Table` decorator (borders, headers) | Complete | `src/rendering/tabular/decorator.rs` |
| Width resolution algorithm | Complete | `src/rendering/tabular/resolve.rs` |
| Template integration (MiniJinja filters/functions) | Complete | `src/rendering/tabular/filters.rs` |

The `row_from<T: Serialize>()` method works by serializing to JSON and extracting fields at runtime. The macros provide compile-time alternatives.

---

## Macro 1: `#[derive(Tabular)]`

**Purpose:** Generate a `TabularSpec` from struct field annotations.

### Usage

```rust
use outstanding::tabular::{Tabular, TabularSpec};

#[derive(Serialize, Tabular)]
#[tabular(separator = " │ ")]
struct Task {
    #[col(width = 8, style = "muted")]
    id: String,

    #[col(width = "fill", overflow = "wrap")]
    title: String,

    #[col(width = 12, align = "right", header = "Status")]
    status: String,

    #[col(width = 10, anchor = "right", style = "muted")]
    due: String,
}

// Generated implementation:
impl Tabular for Task {
    fn tabular_spec() -> TabularSpec {
        TabularSpec::builder()
            .column(Col::fixed(8).named("id").style("muted"))
            .column(Col::fill().named("title").wrap())
            .column(Col::fixed(12).named("status").right().header("Status"))
            .column(Col::fixed(10).named("due").anchor_right().style("muted"))
            .separator(" │ ")
            .build()
    }
}
```

### Field Attributes (`#[col(...)]`)

| Attribute | Type | Maps to | Example |
|-----------|------|---------|---------|
| `width` | `usize` | `Width::Fixed(n)` | `width = 8` |
| `width` | `"fill"` | `Width::Fill` | `width = "fill"` |
| `width` | `"Nfr"` | `Width::Fraction(N)` | `width = "2fr"` |
| `min` | `usize` | `Width::Bounded { min, .. }` | `min = 10` |
| `max` | `usize` | `Width::Bounded { .., max }` | `max = 30` |
| `align` | `"left"`, `"right"`, `"center"` | `Align` | `align = "right"` |
| `anchor` | `"left"`, `"right"` | `Anchor` | `anchor = "right"` |
| `overflow` | `"truncate"`, `"wrap"`, `"clip"`, `"expand"` | `Overflow` | `overflow = "wrap"` |
| `truncate_at` | `"end"`, `"start"`, `"middle"` | `Overflow::Truncate { at }` | `truncate_at = "middle"` |
| `style` | string | `Column.style` | `style = "muted"` |
| `style_from_value` | bool | `Column.style_from_value` | `style_from_value` |
| `header` | string | `Column.header` | `header = "Due Date"` |
| `null_repr` | string | `Column.null_repr` | `null_repr = "N/A"` |
| `key` | string | `Column.key` (override path) | `key = "user.name"` |
| `skip` | — | Exclude from spec | `skip` |

### Container Attributes (`#[tabular(...)]`)

| Attribute | Type | Maps to | Example |
|-----------|------|---------|---------|
| `separator` | string | `Decorations.column_sep` | `separator = " │ "` |
| `prefix` | string | `Decorations.row_prefix` | `prefix = "│ "` |
| `suffix` | string | `Decorations.row_suffix` | `suffix = " │"` |

---

## Macro 2: `#[derive(TabularRow)]`

**Purpose:** Generate optimized row extraction without runtime JSON serialization.

### Usage

```rust
use outstanding::tabular::TabularRow;

#[derive(TabularRow)]
struct Task {
    id: String,
    title: String,

    #[col(skip)]
    internal_state: u32,  // Not included in row

    status: String,
}

// Generated implementation:
impl TabularRow for Task {
    fn to_row(&self) -> Vec<String> {
        vec![
            self.id.to_string(),
            self.title.to_string(),
            self.status.to_string(),
        ]
    }
}
```

### Trait Definition

```rust
pub trait TabularRow {
    fn to_row(&self) -> Vec<String>;
}
```

### Field Handling

- Fields implement `ToString` (via `Display`) for conversion
- `Option<T>` fields use the inner value or empty string
- `#[col(skip)]` excludes fields from the row
- Field order matches struct definition order

---

## Implementation Phases

### Phase 1: Macro Infrastructure

**Commit:** `Tabular macros: Phase 1 - Infrastructure`

Create the macro module structure and attribute parsing utilities.

**Deliverables:**

1. Create `crates/outstanding-macros/src/tabular/` module
2. Implement attribute parsing:
   - `ColAttr` struct for field attributes
   - `TabularAttr` struct for container attributes
   - `parse_width()` → `Width` token generation
   - `parse_align()` → `Align` token generation
   - `parse_overflow()` → `Overflow` token generation
3. Error reporting with `syn::Error`

**Files:**
- `crates/outstanding-macros/src/tabular/mod.rs`
- `crates/outstanding-macros/src/tabular/attrs.rs`

**Tests:** Unit tests for attribute parsing

---

### Phase 2: `#[derive(Tabular)]`

**Commit:** `Tabular macros: Phase 2 - Tabular derive`

Implement the spec generation macro.

**Deliverables:**

1. Implement `#[proc_macro_derive(Tabular, attributes(col, tabular))]`
2. Parse struct fields and collect `#[col(...)]` attributes
3. Generate `impl Tabular for T { fn tabular_spec() -> TabularSpec }`
4. Handle container `#[tabular(...)]` attributes

**Files:**
- `crates/outstanding-macros/src/tabular/derive_tabular.rs`
- `crates/outstanding-macros/src/lib.rs` (register macro)
- `crates/outstanding/src/rendering/tabular/traits.rs` (Tabular trait)

**Tests:**
- Simple struct with fixed widths
- All width variants (fixed, fill, fraction, bounded)
- All attribute combinations
- Container attributes
- Skip fields
- Error cases (invalid values, unsupported types)

---

### Phase 3: `#[derive(TabularRow)]`

**Commit:** `Tabular macros: Phase 3 - TabularRow derive`

Implement the row extraction macro.

**Deliverables:**

1. Define `TabularRow` trait in `outstanding` crate
2. Implement `#[proc_macro_derive(TabularRow, attributes(col))]`
3. Generate `to_row()` with direct field access
4. Handle `#[col(skip)]` attribute

**Files:**
- `crates/outstanding/src/rendering/tabular/traits.rs` (TabularRow trait)
- `crates/outstanding-macros/src/tabular/derive_row.rs`

**Tests:**
- String fields
- Numeric fields (i32, u64, f64)
- Option fields
- Skip fields
- Mixed field types

---

### Phase 4: Formatter Integration

**Commit:** `Tabular macros: Phase 4 - Formatter integration`

Integrate macros with existing formatter.

**Deliverables:**

1. Add `TabularFormatter::from_type<T: Tabular>(width: usize)` constructor
2. Add `TabularFormatter::row_from_trait<T: TabularRow>(&self, value: &T)` method
3. Add same methods to `Table` decorator
4. Benchmark trait-based vs serde-based extraction

**Files:**
- `crates/outstanding/src/rendering/tabular/formatter.rs`
- `crates/outstanding/src/rendering/tabular/decorator.rs`

**Tests:**
- End-to-end: derive → format → output
- Equivalence: trait output matches serde output
- Performance comparison

---

### Phase 5: Template Integration

**Commit:** `Tabular macros: Phase 5 - Template helpers`

Enable macro-derived specs in templates.

**Deliverables:**

1. Context injection helper for derived specs
2. Documentation updates
3. Example templates

**Files:**
- `crates/outstanding/src/rendering/tabular/filters.rs`
- Examples in `examples/`

---

### Phase 6: Documentation

**Commit:** `Tabular macros: Phase 6 - Documentation`

Complete documentation and examples.

**Deliverables:**

1. Update `docs/guides/intro-to-tabular.md` with macro examples
2. Update `docs/guides/tabular.md` API reference
3. Add macro-specific documentation to rustdoc

---

## Phase Dependencies

```
Phase 1 (Infrastructure)
    │
    ├──→ Phase 2 (Tabular derive)
    │         │
    │         └──→ Phase 4 (Formatter integration)
    │                   │
    └──→ Phase 3 (TabularRow derive)    │
              │                         │
              └─────────────────────────┘
                          │
                          ▼
                    Phase 5 (Template)
                          │
                          ▼
                    Phase 6 (Docs)
```

Phases 2 and 3 can be developed in parallel after Phase 1.

---

## Design Decisions

### Why Two Macros?

1. **`Tabular`** generates the spec (column definitions, widths, styles)
2. **`TabularRow`** generates row extraction (field → string conversion)

They serve different purposes and can be used independently:
- Use only `Tabular` with `row_from<T: Serialize>()` for flexibility
- Use only `TabularRow` with manually-built specs for control
- Use both for maximum type safety and performance

### Why Not Combine Them?

Separation allows:
- Using `Tabular` without `TabularRow` (keep serde flexibility)
- Using `TabularRow` with different specs (same data, different views)
- Clearer error messages and simpler macro logic

### Field Ordering

Both macros preserve struct field order. This ensures:
- `tabular_spec()` columns match `to_row()` values
- Predictable output without explicit ordering attributes
- Simple mental model for users
