# Tabular Implementation Plan

**Status:** Approved  
**Created:** 2025-01-16

## Overview

Implementation sequence for the tabular layout system redesign. Each phase is a cohesive unit with its own tests, buildable incrementally.

Derive macros are deferred - they will use these underlying APIs.

---

## Phase 1: Text Utilities

**Commit:** `Tabular Phase 1: Text utilities`

Add word-wrap functions:

- `wrap(s, width) -> Vec<String>` - simple word-wrap
- `wrap_indent(s, width, indent) -> Vec<String>` - wrap with continuation indent

Algorithm: split on whitespace, accumulate until line exceeds width, force-break words longer than width.

**Files:** `util.rs`  
**Tests:** Edge cases (empty, single word, exact fit, long words, ANSI preservation)

---

## Phase 2: Core Types

**Commit:** `Tabular Phase 2: Core types refactoring`

New types:

```rust
pub enum Overflow {
    Truncate { at: TruncateAt, marker: String },
    Wrap { indent: usize },
    Clip,
    Expand,
}

pub enum Anchor {
    Left,
    Right,
}

// Add to Width
Width::Fraction(usize)
```

Renames (as type aliases initially):

- `FlatDataSpec` → `TabularSpec`
- `TableFormatter` → `TabularFormatter`

Add `Col` shorthand:

```rust
Col::fixed(8)
Col::min(10)
Col::bounded(5, 20)
Col::fill()
Col::fraction(2)
```

Add fluent shortcuts to `Column`:

```rust
.right(), .center(), .anchor_right(), .wrap(), .clip(), .named()
```

Update `Column` struct with `overflow: Overflow` and `anchor: Anchor`.

**Files:** `types.rs`, `mod.rs`  
**Tests:** Serialization, builders, defaults

---

## Phase 3: Width Resolution

**Commit:** `Tabular Phase 3: Width resolution with Fraction`

Update algorithm:

```text
remaining = total - fixed - bounded
total_parts = sum(fraction_values) + count(fill)  // Fill = Fraction(1)
unit = remaining / total_parts
```

**Files:** `resolve.rs`  
**Tests:** Property tests, mixed width types

---

## Phase 4: Cell Formatting

**Commit:** `Tabular Phase 4: Cell formatting with Overflow`

Handle all overflow modes:

```rust
fn format_cell(value, column, width) -> CellResult {
    Truncate → Single(truncated_padded)
    Wrap → Multi(wrapped_lines)
    Clip → Single(clipped_padded)
    Expand → Single(padded_only)
}
```

**Files:** `formatter.rs`  
**Tests:** All overflow modes × alignment combinations

---

## Phase 5: Row Formatting with Anchors

**Commit:** `Tabular Phase 5: Row formatting and anchors`

```rust
impl TabularFormatter {
    fn row(&self, values) -> String           // Single-line
    fn row_lines(&self, values) -> Vec<String> // Multi-line
}
```

Anchor positioning: left columns from 0, right columns from edge.

**Files:** `formatter.rs`  
**Tests:** Anchors, multi-line row assembly

---

## Phase 6: Struct Extraction

**Commit:** `Tabular Phase 6: Struct-based row extraction`

```rust
impl TabularFormatter {
    fn row_from<T: Serialize>(&self, value: &T) -> String
}
```

Extract fields via `Column.key` (dot notation) or `Column.name`.

**Files:** `formatter.rs`  
**Tests:** Nested structs, missing fields

---

## Phase 7: Column Styles

**Commit:** `Tabular Phase 7: Column style integration`

- Wrap cell content in `[style]...[/style]` when `Column.style` is set
- Add `style_from_value()` mode (style = cell value)
- Add `style_as` template filter

**Files:** `formatter.rs`, `filters.rs`  
**Tests:** Style wrapping, dynamic styles

---

## Phase 8: Table Decorator

**Commit:** `Tabular Phase 8: Table decorator`

```rust
pub struct Table {
    spec: TabularSpec,
    header: Option<Vec<String>>,
    border: BorderStyle,
}

pub enum BorderStyle { None, Ascii, Light, Heavy, Double, Rounded }
```

Methods: `header()`, `separator()`, `footer()`, `row()`, `render()`

**Files:** New `decorator.rs`  
**Tests:** All border styles, full table rendering

---

## Phase 9: Template Integration

**Commit:** `Tabular Phase 9: Template functions`

Register MiniJinja global functions:

- `tabular(columns, separator=?, width=?)` → TabularFormatter object
- `table(columns, border=?, header_style=?)` → Table object

**Files:** `template/functions.rs`  
**Tests:** Template integration

---

## Summary

| Phase | Deliverable | Dependencies |
| ----- | ----------- | ------------ |
| 1 | `wrap()` | None |
| 2 | Types, `Col`, renames | None |
| 3 | Fraction resolution | Phase 2 |
| 4 | Cell overflow handling | Phases 1, 2 |
| 5 | Row + anchors | Phases 2, 3, 4 |
| 6 | `row_from<T>()` | Phase 5 |
| 7 | Column styles | Phase 5 |
| 8 | Table decorator | Phase 5 |
| 9 | Template functions | Phase 8 |

Phases 6, 7, 8 can be developed in parallel after Phase 5.
