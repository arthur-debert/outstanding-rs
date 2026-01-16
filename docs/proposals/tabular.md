# Tabular Layout System - Technical Specification

**Status:** Draft
**Author:** Outstanding Team
**Created:** 2025-01-16

## 1. Motivation

Outstanding excels at formatting (colors, styles, weight) but layout support is limited. For CLI applications, "layout" primarily means **vertical alignment** - ensuring fields like dates, names, and statuses line up across multiple output lines.

This isn't about TUIs with scrolling, selection, or interaction. It's about making log entries, file listings, and status displays visually coherent.

Two manifestations:
1. **Tabular output**: Aligned columns without visual borders (the common case)
2. **Tables**: Explicit headers, separators, and borders (the decorated case)

Both share a core engine. This spec defines that engine and the decoration layer.

## 2. Design Principles

1. **Template-centric**: Outstanding is template-first. The tabular system must feel native in templates.
2. **Progressive complexity**: Simple cases should be trivial; complex cases should be possible.
3. **Declarative over imperative**: Define what you want, not how to achieve it.
4. **No magic extraction**: Field access is explicit, not inferred.

## 3. Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      User Interface                         │
│  ┌──────────────────┐  ┌──────────────────────────────────┐│
│  │ Template Filters │  │ Rust API (TabularSpec builder)   ││
│  │ col(), row()     │  │                                  ││
│  └────────┬─────────┘  └───────────────┬──────────────────┘│
│           │                            │                    │
│           ▼                            ▼                    │
│  ┌──────────────────────────────────────────────────────┐  │
│  │              TabularFormatter                         │  │
│  │  - Holds resolved widths                              │  │
│  │  - Formats individual rows                            │  │
│  │  - Handles overflow (truncate, wrap)                  │  │
│  └────────────────────────┬─────────────────────────────┘  │
│                           │                                 │
│                           ▼                                 │
│  ┌──────────────────────────────────────────────────────┐  │
│  │              Table (Decorator)                        │  │
│  │  - Headers                                            │  │
│  │  - Column/row separators                              │  │
│  │  - Borders                                            │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│                    Text Utilities                           │
│  display_width(), truncate_*(), pad_*(), wrap()            │
│  (ANSI-aware, Unicode-aware)                               │
└─────────────────────────────────────────────────────────────┘
```

## 4. Core Types

### 4.1 Width

How a column determines its size.

```rust
pub enum Width {
    /// Exactly n display columns
    Fixed(usize),

    /// At least min, grows to fit content up to max
    /// If max is None, grows unbounded (until total width constraint)
    Bounded { min: usize, max: Option<usize> },

    /// Takes remaining space after Fixed and Bounded are allocated
    /// Multiple Fill columns share remaining space equally
    Fill,

    /// Proportional: takes n parts of the fill space
    /// `Fraction(2)` gets twice the space of `Fraction(1)`
    Fraction(usize),
}
```

**Resolution algorithm:**
1. Calculate overhead (separators, borders)
2. Allocate `Fixed` columns their exact width
3. Allocate `Bounded` columns: content width clamped to min/max
4. Calculate remaining space
5. Distribute to `Fill` and `Fraction` columns proportionally
6. If no Fill/Fraction columns, expand rightmost Bounded (ignoring max)

### 4.2 Align

Content alignment within a column.

```rust
pub enum Align {
    Left,    // Default
    Right,
    Center,
}
```

### 4.3 Anchor

Column position on the line.

```rust
pub enum Anchor {
    /// Column flows left-to-right (default)
    Left,

    /// Column is positioned at right edge
    /// Remaining columns fill space between left and right anchored
    Right,
}
```

Example with anchor:
```
| left1 | left2 |          gap          | right1 |
```

### 4.4 Overflow

What happens when content exceeds column width.

```rust
pub enum Overflow {
    /// Truncate with ellipsis (default)
    Truncate {
        at: TruncateAt,      // End, Start, Middle
        marker: String,       // Default: "…"
    },

    /// Wrap to multiple lines
    Wrap {
        indent: usize,        // Continuation indent (default: 0)
    },

    /// Hard cut, no marker
    Clip,

    /// Ignore width limit, let content overflow
    Expand,
}

pub enum TruncateAt {
    End,     // "Hello W…" (default)
    Start,   // "…o World"
    Middle,  // "Hel…rld"
}
```

### 4.5 Column

A single column definition.

```rust
pub struct Column {
    /// Column identifier (used for headers, field extraction)
    pub name: Option<String>,

    /// Width strategy
    pub width: Width,

    /// Content alignment within column
    pub align: Align,

    /// Column position on line
    pub anchor: Anchor,

    /// Overflow behavior
    pub overflow: Overflow,

    /// Style to apply to cell content
    /// Can be static "status" or dynamic via template
    pub style: Option<String>,

    /// Display for null/missing values
    pub null_repr: String,  // Default: "-"
}
```

Builder API:
```rust
Column::new("name")
    .width(Width::Fixed(20))
    .align(Align::Left)
    .overflow(Overflow::Truncate { at: TruncateAt::Middle, marker: "…".into() })
    .style("author")
    .build()

// Shortcuts
Col::fixed(20)                    // Width::Fixed(20)
Col::bounded(5, 20)               // Width::Bounded { min: 5, max: Some(20) }
Col::min(10)                      // Width::Bounded { min: 10, max: None }
Col::fill()                       // Width::Fill
Col::fraction(2)                  // Width::Fraction(2)

// Chained shortcuts
Col::fixed(20).right()            // align: Right
Col::fixed(20).center()           // align: Center
Col::fixed(20).anchor_right()     // anchor: Right
Col::fixed(20).wrap()             // overflow: Wrap
Col::fixed(20).clip()             // overflow: Clip
```

### 4.6 TabularSpec

Complete specification for tabular layout.

```rust
pub struct TabularSpec {
    pub columns: Vec<Column>,
    pub column_separator: String,   // Default: "  "
    pub total_width: TotalWidth,
}

pub enum TotalWidth {
    /// Auto-detect terminal width
    Auto,

    /// Fixed width
    Fixed(usize),

    /// Range with auto-detection
    Bounded { min: usize, max: usize },
}
```

Builder API:
```rust
TabularSpec::builder()
    .column(Col::fixed(8).named("id"))
    .column(Col::min(10).named("name").style("author"))
    .column(Col::fill().named("description"))
    .column(Col::fixed(10).named("status").anchor_right())
    .separator(" │ ")
    .width(TotalWidth::Auto)
    .build()
```

### 4.7 TabularFormatter

Runtime formatter with resolved widths.

```rust
pub struct TabularFormatter {
    spec: TabularSpec,
    widths: Vec<usize>,  // Resolved actual widths
}

impl TabularFormatter {
    /// Create formatter, resolving widths
    pub fn new(spec: &TabularSpec, available_width: usize) -> Self;

    /// Create with auto-detected terminal width
    pub fn auto(spec: &TabularSpec) -> Self;

    /// Format a single row
    pub fn row(&self, values: &[impl AsRef<str>]) -> String;

    /// Format a row from a serializable struct
    pub fn row_from<T: Serialize>(&self, value: &T) -> String;

    /// Get formatted row as Vec<String> (one per line, for wrapped cells)
    pub fn row_lines(&self, values: &[impl AsRef<str>]) -> Vec<String>;
}
```

### 4.8 Table (Decorator)

Wraps TabularSpec with table decorations.

```rust
pub struct Table {
    spec: TabularSpec,
    header: Option<HeaderConfig>,
    separators: SeparatorConfig,
    border: Option<BorderStyle>,
}

pub struct HeaderConfig {
    pub titles: Vec<String>,      // Or extracted from Column.name
    pub style: Option<String>,
    pub separator: Option<String>, // Line below header
}

pub struct SeparatorConfig {
    pub row: Option<String>,       // Between data rows
}

pub enum BorderStyle {
    None,
    Ascii,      // +--+--+
    Light,      // ┌──┬──┐
    Heavy,      // ┏━━┳━━┓
    Double,     // ╔══╦══╗
    Rounded,    // ╭──┬──╮
}
```

Usage:
```rust
let table = Table::from(spec)
    .header_from_columns()
    .header_style("bold")
    .border(BorderStyle::Rounded)
    .build();

// Returns full table string including header, borders, all rows
table.render(&data)
```

## 5. Template Integration

MiniJinja doesn't support custom tags, so we use global functions and macros.

### 5.1 Simple Filter: `col`

For simple, stateless column formatting:

```jinja
{% for entry in entries %}
{{ entry.id | col(8) }}  {{ entry.name | col(20) }}  {{ entry.status | col(10, align="right") }}
{% endfor %}
```

This is unchanged from current behavior - it's the quick path.

### 5.2 Spec-Based: `tabular()` Function

For complex layouts, inject a formatter into the template context:

**In Rust:**
```rust
let spec = TabularSpec::builder()
    .column(Col::fixed(8).named("id"))
    .column(Col::min(10).named("name"))
    .column(Col::fill().named("desc"))
    .column(Col::fixed(10).named("status").anchor_right())
    .separator("  ")
    .build();

let formatter = TabularFormatter::auto(&spec);
context.insert("table", formatter);
```

**In template:**
```jinja
{% for entry in entries %}
{{ table.row([entry.id, entry.name, entry.desc, entry.status]) }}
{% endfor %}
```

### 5.3 Inline Spec: `tabular()` Global Function

Define spec inline in templates using a global function:

```jinja
{% set table = tabular([
    {"name": "id", "width": 8},
    {"name": "name", "width": {"min": 10}},
    {"name": "desc", "width": "fill"},
    {"name": "status", "width": 10, "anchor": "right", "style": "status"}
], separator="  ") %}

{% for entry in entries %}
{{ table.row([entry.id, entry.name, entry.desc, entry.status]) }}
{% endfor %}
```

The `tabular()` function:
- Takes a list of column definitions (as dicts/objects)
- Takes optional `separator`, `width` parameters
- Returns a TabularFormatter object
- Auto-detects terminal width

### 5.4 Struct-Based Rows

When columns have names matching struct fields:

```jinja
{% set table = tabular([
    {"name": "id", "width": 8},
    {"name": "name", "width": 20},
    {"name": "status", "width": 10}
]) %}

{% for entry in entries %}
{{ table.row_from(entry) }}  {# Extracts entry.id, entry.name, entry.status #}
{% endfor %}
```

### 5.5 Table Macro

For full tables with headers and borders:

```jinja
{% set t = table([
    {"name": "ID", "key": "id", "width": 8},
    {"name": "Name", "key": "name", "width": 20},
    {"name": "Status", "key": "status", "width": 10}
], border="rounded", header_style="bold") %}

{{ t.header() }}
{{ t.separator() }}
{% for entry in entries %}
{{ t.row_from(entry) }}
{% endfor %}
{{ t.footer() }}
```

Or as a single call for simple cases:

```jinja
{{ table_render(entries, [
    {"name": "ID", "key": "id", "width": 8},
    {"name": "Name", "key": "name", "width": 20}
], border="light") }}
```

## 6. Width Resolution

### 6.1 Algorithm

```
Input: TabularSpec, available_width

1. overhead = (num_columns - 1) * separator_width + border_width
2. content_width = available_width - overhead

3. For each Fixed column:
     allocated[i] = column.width
     remaining -= column.width

4. For each Bounded column:
     if data available:
         content = max(cell_width for cell in column_data)
     else:
         content = column.min
     allocated[i] = clamp(content, column.min, column.max)
     remaining -= allocated[i]

5. total_fractions = sum(f for Fraction(f) columns) + count(Fill columns)
   unit = remaining / total_fractions

   For each Fill column:
       allocated[i] = unit
   For each Fraction(f) column:
       allocated[i] = unit * f

6. If no Fill/Fraction columns and remaining > 0:
     rightmost_bounded.allocated += remaining

Output: Vec<usize> of resolved widths
```

### 6.2 Anchor Resolution

Right-anchored columns are positioned after all left-anchored columns:

```
Input: columns with anchors, widths

1. left_columns = [c for c if c.anchor == Left]
2. right_columns = [c for c if c.anchor == Right]

3. Position left_columns from position 0
4. Position right_columns from right edge
5. Gap between = available - sum(left_widths) - sum(right_widths) - separators
```

## 7. Text Utilities

All utilities are ANSI-aware (preserve escape codes, don't count them in width).

### 7.1 Display Width

```rust
/// Returns display width in terminal columns
/// - ASCII: 1 column each
/// - CJK: 2 columns each
/// - Combining marks: 0 columns
/// - ANSI codes: 0 columns (but preserved in output)
pub fn display_width(s: &str) -> usize;
```

### 7.2 Truncation

```rust
pub fn truncate_end(s: &str, width: usize, marker: &str) -> String;
pub fn truncate_start(s: &str, width: usize, marker: &str) -> String;
pub fn truncate_middle(s: &str, width: usize, marker: &str) -> String;
```

### 7.3 Padding

```rust
pub fn pad_left(s: &str, width: usize) -> String;   // Right-align
pub fn pad_right(s: &str, width: usize) -> String;  // Left-align
pub fn pad_center(s: &str, width: usize) -> String;
```

### 7.4 Word Wrap

```rust
/// Wrap text to fit within width, breaking at word boundaries
/// Returns Vec of lines
pub fn wrap(s: &str, width: usize) -> Vec<String>;

/// Wrap with continuation indent
pub fn wrap_indent(s: &str, width: usize, indent: usize) -> Vec<String>;
```

Simple algorithm:
1. Split on whitespace
2. Accumulate words until line exceeds width
3. Start new line
4. Words longer than width are force-broken

## 8. Multi-line Cells

When cells wrap to multiple lines:

```
┌──────────┬──────────────────────┬────────┐
│ ID       │ Description          │ Status │
├──────────┼──────────────────────┼────────┤
│ abc123   │ This is a very long  │ active │
│          │ description that     │        │
│          │ wraps to three lines │        │
├──────────┼──────────────────────┼────────┤
│ def456   │ Short desc           │ done   │
└──────────┴──────────────────────┴────────┘
```

Implementation:
1. Format each cell, getting Vec<String> of lines
2. Find max lines in row
3. Pad shorter cells with empty lines (align top)
4. Zip lines across cells
5. Join with separator

```rust
impl TabularFormatter {
    pub fn row_lines(&self, values: &[&str]) -> Vec<String> {
        let cell_lines: Vec<Vec<String>> = values.iter()
            .zip(&self.columns)
            .map(|(v, col)| self.format_cell(v, col))
            .collect();

        let max_lines = cell_lines.iter().map(|c| c.len()).max().unwrap_or(1);

        (0..max_lines).map(|line_idx| {
            cell_lines.iter()
                .enumerate()
                .map(|(col_idx, lines)| {
                    lines.get(line_idx)
                        .map(|s| s.as_str())
                        .unwrap_or("")
                        .to_string()
                        // Pad to column width
                })
                .collect::<Vec<_>>()
                .join(&self.separator)
        }).collect()
    }
}
```

## 9. Style Integration

### 9.1 Static Style

```rust
Column::new("status").style("pending")
```

Wraps cell content: `[pending]content[/pending]`

### 9.2 Dynamic Style (via Template)

In template, apply style based on value:

```jinja
{{ table.row([entry.id, entry.name, "[" ~ entry.status ~ "]" ~ entry.status ~ "[/" ~ entry.status ~ "]"]) }}
```

Or with a helper:

```jinja
{{ table.row([entry.id, entry.name, entry.status | style_as(entry.status)]) }}
```

Where `style_as` filter wraps in style tags:
```jinja
{{ "active" | style_as("status") }}  →  [status]active[/status]
{{ "pending" | style_as("pending") }}  →  [pending]pending[/pending]
```

### 9.3 Value-Based Style in Spec

Column can specify that style = cell value:

```rust
Column::new("status").style_from_value()
```

Equivalent to:
```jinja
[{{ cell_value }}]{{ cell_value }}[/{{ cell_value }}]
```

So `"pending"` becomes `[pending]pending[/pending]`.

## 10. Configuration Formats

### 10.1 YAML Spec

```yaml
tabular:
  width: auto  # or fixed: 80, or {min: 80, max: 120}
  separator: "  "
  columns:
    - name: id
      width: 8

    - name: author
      width: {min: 10, max: 30}
      style: author

    - name: description
      width: fill
      overflow:
        wrap: {indent: 2}

    - name: date
      width: 10
      anchor: right
      align: right
```

### 10.2 JSON Spec

```json
{
  "tabular": {
    "width": "auto",
    "separator": "  ",
    "columns": [
      {"name": "id", "width": 8},
      {"name": "author", "width": {"min": 10, "max": 30}, "style": "author"},
      {"name": "description", "width": "fill", "overflow": {"wrap": {"indent": 2}}},
      {"name": "date", "width": 10, "anchor": "right", "align": "right"}
    ]
  }
}
```

## 11. Error Handling

### 11.1 Width Resolution Errors

- Total fixed widths exceed available: Use minimums, may overflow
- No columns defined: Return empty string
- Negative remaining after fixed: Proportionally shrink bounded

### 11.2 Cell Formatting Errors

- Null value: Use `null_repr`
- Value not extractable: Use `null_repr`, optionally warn
- Marker wider than column: Truncate marker itself

## 12. Migration from Current API

| Current | New | Notes |
|---------|-----|-------|
| `FlatDataSpec` | `TabularSpec` | Rename |
| `TableSpec` | `TabularSpec` | Remove alias |
| `TableFormatter` | `TabularFormatter` | Rename, add features |
| `Column::new(Width::Fixed(8))` | `Col::fixed(8)` | Shorthand |
| `col` filter | `col` filter | Unchanged |
| `pad_*` filters | `pad_*` filters | Unchanged |
| `truncate_at` filter | `truncate_at` filter | Unchanged |
| N/A | `tabular()` function | New |
| N/A | `Table` decorator | New |
| N/A | `Overflow::Wrap` | New |
| N/A | `Anchor::Right` | New |
| N/A | `Width::Fraction` | New |

## 13. Implementation Phases

### Phase 1: Core Refactoring
- Rename types (FlatDataSpec → TabularSpec, etc.)
- Add shorthand constructors (Col::fixed, etc.)
- Add Overflow::Wrap with simple word-wrap
- Add Anchor support

### Phase 2: Template Integration
- Implement `tabular()` global function
- Implement `row_from()` for struct extraction
- Add `style_as` filter

### Phase 3: Table Decorator
- Implement Table struct
- Header generation
- Border styles
- Row separators

### Phase 4: Advanced Features
- Width::Fraction
- YAML/JSON spec loading
- Data-driven width resolution improvements

## 14. Open Questions

1. **Config file format**: Should specs be loadable from external YAML/JSON files at runtime?

2. **Header source**: Auto-generate from column names, or always explicit?

3. **Empty table handling**: Show "No data" message, empty borders, or nothing?

4. **ANSI in wrapped text**: When wrapping styled text, should styles span lines or reset per line?
