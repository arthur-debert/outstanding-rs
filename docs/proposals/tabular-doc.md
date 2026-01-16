# Tabular Layout - Documentation Draft

> This document shows what the user-facing documentation would look like.
> It drives the API design by focusing on user experience first.

---

# How To: Align Columns and Format Tables

Outstanding helps you create aligned, readable output for lists, logs, and tabular data.

**Choose your path:**
- [Quick Start](#quick-start-the-col-filter): Simple alignment with template filters
- [Structured Layout](#structured-layout): Multi-column specs for complex output
- [Full Tables](#tables-headers-and-borders): Headers, borders, and separators

## Quick Start: The `col` Filter

For simple alignment, use the `col` filter directly in templates:

```jinja
{% for entry in entries %}
{{ entry.id | col(8) }}  {{ entry.name | col(20) }}  {{ entry.status | col(10) }}
{% endfor %}
```

Output:
```
abc123    Alice Johnson         active
def456    Bob Smith             pending
ghi789    Carol Williams        done
```

The `col` filter:
- Pads short values to the specified width
- Truncates long values with `…`
- Handles Unicode correctly (CJK characters count as 2 columns)

### Alignment

```jinja
{{ value | col(10) }}                 {# Left-aligned (default) #}
{{ value | col(10, align="right") }}  {# Right-aligned #}
{{ value | col(10, align="center") }} {# Centered #}
```

```
left......
....right.
..center..
```

### Truncation Position

When content is too long, choose where to cut:

```jinja
{{ path | col(15) }}                        {# "Very long pa…" (default: end) #}
{{ path | col(15, truncate="start") }}      {# "…ng/path/file" #}
{{ path | col(15, truncate="middle") }}     {# "Very l…h/file" #}
```

Truncate `middle` is useful for paths where both start and end matter.

### Custom Ellipsis

```jinja
{{ value | col(10, ellipsis="...") }}   {# "Hello W..." instead of "Hello W…" #}
{{ value | col(10, ellipsis="→") }}     {# "Hello Wor→" #}
```

---

## Structured Layout

When you need consistent column widths across complex output, define a layout spec.

### Defining Columns in Templates

Use the `tabular()` function to create a formatter:

```jinja
{% set t = tabular([
    {"name": "id", "width": 8},
    {"name": "author", "width": 20},
    {"name": "message", "width": "fill"},
    {"name": "date", "width": 10, "align": "right"}
]) %}

{% for commit in commits %}
{{ t.row([commit.id, commit.author, commit.message, commit.date]) }}
{% endfor %}
```

Output:
```
a1b2c3d4  Alice Johnson         Add new login feature             2024-01-15
e5f6g7h8  Bob Smith             Fix authentication bug            2024-01-14
i9j0k1l2  Carol Williams        Update dependencies               2024-01-13
```

### Width Options

| Width | Meaning | Example |
|-------|---------|---------|
| `8` | Exactly 8 columns | IDs, short codes |
| `{"min": 10}` | At least 10, grows to fit | Names, titles |
| `{"min": 10, "max": 30}` | Between 10 and 30 | Bounded growth |
| `"fill"` | Takes remaining space | Descriptions |
| `"2fr"` | 2 parts of remaining (vs 1fr) | Proportional |

```jinja
{% set t = tabular([
    {"name": "id", "width": 8},                    {# Fixed #}
    {"name": "name", "width": {"min": 10}},        {# Grows to fit #}
    {"name": "desc", "width": "fill"},             {# Takes the rest #}
]) %}
```

### Anchoring Columns

Put columns at the right edge:

```jinja
{% set t = tabular([
    {"name": "name", "width": 20},
    {"name": "path", "width": "fill"},
    {"name": "size", "width": 8, "anchor": "right"},  {# Stays at right edge #}
]) %}
```

Output:
```
document.txt          /home/user/docs/                    1.2 MB
image.png             /home/user/photos/vacation/         4.5 MB
```

The `size` column is anchored to the right edge. The `path` column fills the gap.

### Handling Long Content

Choose what happens when content exceeds the column width:

```jinja
{% set t = tabular([
    {"name": "path", "width": 30, "overflow": "truncate"},  {# Default: "Very long…" #}
    {"name": "desc", "width": 30, "overflow": "wrap"},      {# Wrap to multiple lines #}
]) %}
```

#### Truncate (default)

```jinja
{"overflow": "truncate"}                      {# Truncate at end #}
{"overflow": {"truncate": {"at": "middle"}}}  {# Keep start and end #}
{"overflow": {"truncate": {"marker": "..."}}} {# Custom ellipsis #}
```

#### Wrap

Content wraps to multiple lines:

```
abc123  This is a very long       active
        description that wraps
        to multiple lines
def456  Short description         done
```

```jinja
{"name": "desc", "width": 25, "overflow": "wrap"}
{"name": "desc", "width": 25, "overflow": {"wrap": {"indent": 2}}}  {# Continuation indent #}
```

### Extracting Fields from Objects

When column names match struct fields, use `row_from()`:

```jinja
{% set t = tabular([
    {"name": "id", "width": 8},
    {"name": "title", "width": 30},
    {"name": "status", "width": 10}
]) %}

{% for item in items %}
{{ t.row_from(item) }}  {# Automatically extracts item.id, item.title, item.status #}
{% endfor %}
```

For nested fields, use `key`:

```jinja
{% set t = tabular([
    {"name": "Author", "key": "author.name", "width": 20},
    {"name": "Email", "key": "author.email", "width": 30}
]) %}
```

### Column Styles

Apply styles to entire columns:

```jinja
{% set t = tabular([
    {"name": "id", "width": 8, "style": "muted"},
    {"name": "name", "width": 20, "style": "bold"},
    {"name": "status", "width": 10}  {# No automatic style #}
]) %}
```

The `style` value wraps content in style tags: `[muted]abc123[/muted]`

For dynamic styles (style based on value):

```jinja
{% for item in items %}
{{ t.row([item.id, item.name, item.status | style_as(item.status)]) }}
{% endfor %}
```

This applies `[pending]pending[/pending]` or `[done]done[/done]` based on the actual status value.

---

## Defining Layout in Rust

For reusable layouts or when you need full control:

```rust
use outstanding::tabular::{TabularSpec, Col};

let spec = TabularSpec::builder()
    .column(Col::fixed(8).named("id"))
    .column(Col::min(10).named("name").style("author"))
    .column(Col::fill().named("description").wrap())
    .column(Col::fixed(10).named("status").anchor_right().right())
    .separator("  ")
    .build();
```

Pass to template context:

```rust
let formatter = TabularFormatter::auto(&spec);
ctx.insert("table", formatter);
```

### Shorthand Column Constructors

```rust
Col::fixed(8)          // Exactly 8 columns
Col::min(10)           // At least 10, grows to fit
Col::bounded(10, 30)   // Between 10 and 30
Col::fill()            // Takes remaining space
Col::fraction(2)       // 2 parts of remaining (2fr)

// Chained modifiers
Col::fixed(10)
    .named("status")       // Column name
    .right()               // Align right
    .center()              // Align center
    .anchor_right()        // Position at right edge
    .wrap()                // Overflow: wrap
    .clip()                // Overflow: hard cut
    .truncate_middle()     // Truncate in middle
    .style("pending")      // Apply style
    .null_repr("N/A")      // Display for missing values
```

---

## Tables: Headers and Borders

For output with explicit headers, separators, and borders:

```jinja
{% set t = table([
    {"name": "ID", "key": "id", "width": 8},
    {"name": "Author", "key": "author", "width": 20},
    {"name": "Message", "key": "message", "width": "fill"}
], border="rounded", header_style="bold") %}

{{ t.header() }}
{{ t.separator() }}
{% for commit in commits %}
{{ t.row_from(commit) }}
{% endfor %}
{{ t.footer() }}
```

Output:
```
╭──────────┬──────────────────────┬────────────────────────────────╮
│ ID       │ Author               │ Message                        │
├──────────┼──────────────────────┼────────────────────────────────┤
│ a1b2c3d4 │ Alice Johnson        │ Add new login feature          │
│ e5f6g7h8 │ Bob Smith            │ Fix authentication bug         │
│ i9j0k1l2 │ Carol Williams       │ Update dependencies            │
╰──────────┴──────────────────────┴────────────────────────────────╯
```

### Border Styles

```jinja
border="none"     {# No borders #}
border="ascii"    {# +--+--+  ASCII compatible #}
border="light"    {# ┌──┬──┐  Light box drawing #}
border="heavy"    {# ┏━━┳━━┓  Heavy box drawing #}
border="double"   {# ╔══╦══╗  Double lines #}
border="rounded"  {# ╭──┬──╮  Rounded corners #}
```

### Row Separators

Add lines between data rows:

```jinja
{% set t = table(columns, border="light", row_separator=true) %}
```

```
┌──────────┬──────────────────────┐
│ ID       │ Name                 │
├──────────┼──────────────────────┤
│ abc123   │ Alice                │
├──────────┼──────────────────────┤
│ def456   │ Bob                  │
└──────────┴──────────────────────┘
```

### Simple Table Rendering

For simple cases, render everything in one call:

```jinja
{{ table_render(commits, [
    {"name": "ID", "key": "id", "width": 8},
    {"name": "Author", "key": "author", "width": 20}
], border="light") }}
```

---

## In Rust: Full Table API

```rust
use outstanding::tabular::{Table, TabularSpec, Col, BorderStyle};

let spec = TabularSpec::builder()
    .column(Col::fixed(8).named("ID"))
    .column(Col::min(10).named("Author"))
    .column(Col::fill().named("Message"))
    .build();

let table = Table::from(spec)
    .header_from_columns()        // Use column names as headers
    .header_style("table-header")
    .border(BorderStyle::Rounded)
    .build();

// Render full table
let output = table.render(&data)?;
println!("{}", output);

// Or render parts manually
println!("{}", table.header());
println!("{}", table.separator());
for row in &data {
    println!("{}", table.row_from(row));
}
println!("{}", table.footer());
```

---

## Terminal Width

By default, Outstanding auto-detects terminal width. Override for testing or fixed-width output:

```jinja
{% set t = tabular(columns, width=80) %}  {# Fixed 80 columns #}
```

```rust
let formatter = TabularFormatter::new(&spec, 80);  // Fixed
let formatter = TabularFormatter::auto(&spec);     // Auto-detect
```

---

## Helper Filters

For simpler use cases, these filters work standalone:

### Padding

```jinja
{{ value | pad_right(10) }}   {# "hello     " - left align #}
{{ value | pad_left(10) }}    {# "     hello" - right align #}
{{ value | pad_center(10) }}  {# "  hello   " - center #}
```

### Truncation

```jinja
{{ path | truncate_at(20) }}                  {# End: "/very/long/path…" #}
{{ path | truncate_at(20, "middle") }}        {# Middle: "/very…/file" #}
{{ path | truncate_at(20, "start") }}         {# Start: "…long/path/file" #}
{{ path | truncate_at(20, "end", "...") }}    {# Custom marker #}
```

### Display Width

Check visual width (handles Unicode):

```jinja
{% if name | display_width > 20 %}
  {{ name | truncate_at(20) }}
{% else %}
  {{ name }}
{% endif %}
```

---

## Unicode and ANSI

The tabular system correctly handles:

- **CJK characters**: 日本語 counts as 6 columns (2 each)
- **Combining marks**: café is 4 columns (é combines)
- **ANSI codes**: Preserved in output, not counted in width

```jinja
{{ "Hello 日本" | col(12) }}  →  "Hello 日本  " (10 display columns + 2 padding)
```

Styled text maintains styles through truncation:

```jinja
{{ "[red]very long red text[/red]" | col(10) }}  →  "[red]very lon…[/red]"
```

---

## Missing Values

Set what displays for null/empty values:

```jinja
{% set t = tabular([
    {"name": "email", "width": 30, "null_repr": "N/A"}
]) %}
```

Or in templates with Jinja's default filter:

```jinja
{{ entry.email | default("N/A") | col(30) }}
```

---

## Complete Example

A git log-style output:

```jinja
{% set t = tabular([
    {"name": "hash", "width": 8, "style": "muted"},
    {"name": "author", "width": {"min": 15, "max": 25}, "style": "author"},
    {"name": "message", "width": "fill"},
    {"name": "date", "width": 10, "anchor": "right", "align": "right", "style": "date"}
], separator=" │ ") %}

{% for commit in commits %}
{{ t.row([commit.hash[:8], commit.author, commit.message, commit.date]) }}
{% endfor %}
```

Output (80 columns):
```
a1b2c3d4 │ Alice Johnson   │ Add new login feature with OAuth    │ 2024-01-15
e5f6g7h8 │ Bob Smith       │ Fix authentication bug              │ 2024-01-14
i9j0k1l2 │ Carol Williams  │ Update dependencies and refactor    │ 2024-01-13
```

With styling (in terminal):
```
[muted]a1b2c3d4[/muted] │ [author]Alice Johnson[/author]   │ Add new login feature with OAuth    │ [date]2024-01-15[/date]
```

---

## Summary

| Need | Solution |
|------|----------|
| Simple column alignment | `{{ value \| col(width) }}` |
| Multiple columns, same widths | `tabular([...])` with `t.row([...])` |
| Auto field extraction | `t.row_from(object)` |
| Headers and borders | `table([...])` |
| Right-edge columns | `anchor: "right"` |
| Long content wrapping | `overflow: "wrap"` |
| Proportional widths | `width: "2fr"` |

---

## Reference

### `col` Filter

```
{{ value | col(width, align=?, truncate=?, ellipsis=?) }}
```

| Param | Values | Default |
|-------|--------|---------|
| `width` | integer | required |
| `align` | "left", "right", "center" | "left" |
| `truncate` | "end", "start", "middle" | "end" |
| `ellipsis` | string | "…" |

### Column Spec

```json
{
  "name": "string",
  "key": "field.path",
  "width": 8 | {"min": 5} | {"min": 5, "max": 20} | "fill" | "2fr",
  "align": "left" | "right" | "center",
  "anchor": "left" | "right",
  "overflow": "truncate" | "wrap" | "clip" | {"truncate": {...}} | {"wrap": {...}},
  "style": "style-name",
  "null_repr": "-"
}
```

### `tabular()` Function

```jinja
{% set t = tabular(columns, separator=?, width=?) %}
{{ t.row([values]) }}
{{ t.row_from(object) }}
```

### `table()` Function

```jinja
{% set t = table(columns, border=?, header_style=?, row_separator=?) %}
{{ t.header() }}
{{ t.separator() }}
{{ t.row([values]) }}
{{ t.row_from(object) }}
{{ t.footer() }}
```

### Border Styles

| Value | Example |
|-------|---------|
| `"none"` | No borders |
| `"ascii"` | `+--+--+` |
| `"light"` | `┌──┬──┐` |
| `"heavy"` | `┏━━┳━━┓` |
| `"double"` | `╔══╦══╗` |
| `"rounded"` | `╭──┬──╮` |
