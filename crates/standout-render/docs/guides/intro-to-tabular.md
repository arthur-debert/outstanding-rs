# Introduction to Tabular

Polished terminal output requires two things: good formatting (see [Rendering Introduction](intro-to-rendering.md)) and good layouts. For text-only, non-interactive output, layout mostly means aligning things vertically and controlling how multiple pieces of information are presented together.

Tabular provides a declarative column system with powerful primitives for sizing (fixed, range, fill, fractions), positioning (anchor to right), overflow handling (clip, wrap, truncate), cell alignment, and automated per-column styling.

Tabular is not only about tables. Any listing where items have multiple fields that benefit from vertical alignment is a good candidate—log entries with authors, timestamps, and messages; file listings with names, sizes, and dates; task lists with IDs, titles, and statuses. Add headers, separators, and borders to a tabular layout, and you have a table.

**Key capabilities:**

- **Flexible sizing**: Fixed widths, min/max ranges, fill remaining space, fractional proportions
- **Smart truncation**: Truncate at start, middle, or end with custom ellipsis
- **Word wrapping**: Wrap long content across multiple lines with proper alignment
- **Unicode-aware**: CJK characters, combining marks, and ANSI codes handled correctly
- **Dynamic styling**: Style columns or individual values based on content

In this guide, we will walk from a simple listing to a polished table, exploring the available features.

**See Also:**

- [Introduction to Rendering](intro-to-rendering.md) - templates and styles overview
- [Styling System](../topics/styling-system.md) - themes and adaptive styles

---

## Our Example: Task List

We'll build the output for a task list. This is a perfect Tabular use case: each task has an index, title, and status. We want them aligned, readable, and visually clear at a glance.

Here's our data:

```rust
use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status { Pending, Done }

#[derive(Clone, Serialize)]
struct Task {
    title: String,
    status: Status,
}

let tasks = vec![
    Task { title: "Implement user authentication".into(), status: Status::Pending },
    Task { title: "Fix payment gateway timeout".into(), status: Status::Pending },
    Task { title: "Update documentation for API v2".into(), status: Status::Done },
    Task { title: "Review pull request #142".into(), status: Status::Pending },
];
```

Let's progressively build this from raw output to a polished, professional listing.

---

## Step 1: The Problem with Plain Output

Without any formatting, a naive approach might look like this:

```jinja
{% for task in tasks %}
{{ loop.index }}. {{ task.title }} {{ task.status }}
{% endfor %}
```

Output:

```text
1. Implement user authentication pending
2. Fix payment gateway timeout pending
3. Update documentation for API v2 done
4. Review pull request #142 pending
```

This is barely readable. Fields run together, nothing aligns, and scanning the list requires mental parsing of each line. Let's fix that.

---

## Step 2: Basic Column Alignment with `col`

The simplest improvement is the `col` filter. It pads (or truncates) each value to a fixed width:

```jinja
{% for task in tasks %}
{{ loop.index | col(4) }}  {{ task.status | col(10) }}  {{ task.title | col(40) }}
{% endfor %}
```

Output:

```text
1.    pending     Implement user authentication
2.    pending     Fix payment gateway timeout
3.    done        Update documentation for API v2
4.    pending     Review pull request #142
```

Already much better. Each column aligns vertically, making it easy to scan. But we've hardcoded widths, and if a title is too long, it gets truncated with `...`.

> **Key insight:** The `col` filter handles Unicode correctly. CJK characters count as 2 columns, combining marks don't add width, and ANSI escape codes are preserved but not counted.

---

## Step 3: Structured Layout with `tabular()`

For more control, use the `tabular()` function. This creates a formatter that you configure once and use for all rows:

```jinja
{% set t = tabular([
    {"name": "index", "width": 4},
    {"name": "status", "width": 10},
    {"name": "title", "width": 40}
], separator="  ") %}

{% for task in tasks %}
{{ t.row([loop.index, task.status, task.title]) }}
{% endfor %}
```

The output looks the same, but now the column definitions are centralized. This becomes powerful when we start adding features.

---

## Step 4: Flexible Widths

Hardcoded widths are fragile. What if the terminal is wider or narrower? Tabular offers flexible width strategies:

| Width | Meaning |
| ----- | ------- |
| `8` | Exactly 8 columns (fixed) |
| `{"min": 10}` | At least 10, grows to fit content |
| `{"min": 10, "max": 30}` | Between 10 and 30 |
| `"fill"` | Takes all remaining space |
| `"2fr"` | 2 parts of remaining (proportional) |

Let's make the title column expand to fill available space:

```jinja
{% set t = tabular([
    {"name": "index", "width": 4},
    {"name": "status", "width": 10},
    {"name": "title", "width": "fill"}
], separator="  ") %}
```

Now on an 80-column terminal:

```text
1.    pending     Implement user authentication
2.    pending     Fix payment gateway timeout
3.    done        Update documentation for API v2
4.    pending     Review pull request #142
```

On a 120-column terminal, the title column automatically expands to use the extra space.

The layout adapts to the available space.

---

## Step 5: Right-Align Numbers

Numbers and indices look better right-aligned. Use the `align` option:

```jinja
{% set t = tabular([
    {"name": "index", "width": 4, "align": "right"},
    {"name": "status", "width": 10},
    {"name": "title", "width": "fill"}
], separator="  ") %}
```

Output:

```text
  1.  pending     Implement user authentication
  2.  pending     Fix payment gateway timeout
  3.  done        Update documentation for API v2
  4.  pending     Review pull request #142
```

The indices now align on the right edge of their column.

---

## Step 6: Anchoring Columns

Sometimes you want a column pinned to the terminal's right edge, regardless of how other columns resize. Use `anchor`:

```jinja
{% set t = tabular([
    {"name": "index", "width": 4},
    {"name": "title", "width": "fill"},
    {"name": "status", "width": 10, "anchor": "right"}
], separator="  ") %}
```

Now the status column is always at the right edge. If the terminal is 100 columns or 200, the status stays anchored. The fill column absorbs the extra space between fixed columns and anchored columns.

---

## Step 7: Handling Long Content

What happens when a title is longer than its column? By default, Tabular truncates at the end with `...`. But you have options:

### Truncate at Different Positions

```jinja
{"name": "title", "width": 30, "overflow": "truncate"}                        {# "Very long title th..." #}
{"name": "title", "width": 30, "overflow": {"truncate": {"at": "start"}}}     {# "...itle that is long" #}
{"name": "title", "width": 30, "overflow": {"truncate": {"at": "middle"}}}    {# "Very long...is long" #}
```

Middle truncation is perfect for file paths where both the start and end matter: `/home/user/.../important.txt`

### Wrap to Multiple Lines

For descriptions or messages, wrapping is often better than truncating:

```jinja
{% set t = tabular([
    {"name": "index", "width": 4},
    {"name": "title", "width": 40, "overflow": "wrap"},
    {"name": "status", "width": 10}
], separator="  ") %}
```

If a title exceeds 40 columns, it wraps:

```text
1.    Implement comprehensive error handling    pending
      for all API endpoints with proper
      logging and user feedback
2.    Quick fix                                 done
```

The wrapped lines are indented to align with the column.

---

## Step 8: Dynamic Styling Based on Values

Here's where Tabular shines for task lists. We want status colors: green for done, yellow for pending.

First, define styles in your theme:

```css
/* styles/default.css */
.done { color: green; }
.pending { color: yellow; }
```

Then use the `style_as` filter to apply styles based on the value itself:

```jinja
{% set t = tabular([
    {"name": "index", "width": 4},
    {"name": "status", "width": 10},
    {"name": "title", "width": "fill"}
], separator="  ") %}

{% for task in tasks %}
{{ t.row([loop.index, task.status | style_as(task.status), task.title]) }}
{% endfor %}
```

The `style_as` filter wraps the value in style tags: `[done]done[/done]`. The rendering system then applies the green color.

Output (with colors):

```text
1.    [yellow]pending[/yellow]   Implement user authentication
2.    [yellow]pending[/yellow]   Fix payment gateway timeout
3.    [green]done[/green]        Update documentation for API v2
4.    [yellow]pending[/yellow]   Review pull request #142
```

In the terminal, statuses appear in their respective colors, making it instantly clear which tasks need attention.

---

## Step 9: Column-Level Styles

Instead of styling individual values, you can style entire columns. This is useful for de-emphasizing certain information:

```jinja
{% set t = tabular([
    {"name": "index", "width": 4, "style": "muted"},
    {"name": "status", "width": 10},
    {"name": "title", "width": "fill"}
], separator="  ") %}
```

Now indices appear in a muted style (typically gray), while titles and statuses remain prominent. This creates visual hierarchy.

---

## Step 10: Automatic Field Extraction

Tired of manually listing `[task.title, task.status, ...]`? If your column names match your struct fields, use `row_from()`:

```jinja
{% set t = tabular([
    {"name": "title", "width": "fill"},
    {"name": "status", "width": 10}
]) %}

{% for task in tasks %}
{{ t.row_from(task) }}
{% endfor %}
```

Tabular extracts `task.title`, `task.status`, etc. automatically. For nested fields, use `key`:

```jinja
{"name": "Author", "key": "author.name", "width": 20}
{"name": "Email", "key": "author.email", "width": 30}
```

---

## Step 11: Adding Headers and Borders

For a proper table with headers, switch from `tabular()` to `table()`:

```jinja
{% set t = table([
    {"name": "#", "width": 4},
    {"name": "Status", "width": 10},
    {"name": "Title", "width": "fill"}
], border="rounded", header_style="bold") %}

{{ t.header_row() }}
{{ t.separator_row() }}
{% for task in tasks %}
{{ t.row([loop.index, task.status, task.title]) }}
{% endfor %}
{{ t.bottom_border() }}
```

Output:

```text
╭──────┬────────────┬────────────────────────────────────────╮
│ #    │ Status     │ Title                                  │
├──────┼────────────┼────────────────────────────────────────┤
│ 1    │ pending    │ Implement user authentication          │
│ 2    │ pending    │ Fix payment gateway timeout            │
│ 3    │ done       │ Update documentation for API v2        │
│ 4    │ pending    │ Review pull request #142               │
╰──────┴────────────┴────────────────────────────────────────╯
```

### Border Styles

Choose from six border styles:

| Style | Look |
| ----- | ---- |
| `"none"` | No borders |
| `"ascii"` | `+--+--+` (ASCII compatible) |
| `"light"` | `┌──┬──┐` |
| `"heavy"` | `┏━━┳━━┓` |
| `"double"` | `╔══╦══╗` |
| `"rounded"` | `╭──┬──╮` |

### Row Separators

For dense data, add lines between rows:

```jinja
{% set t = table(columns, border="light", row_separator=true) %}
```

```text
┌──────┬────────────────────────────────────╮
│ #    │ Title                              │
├──────┼────────────────────────────────────┤
│ 1    │ Implement user authentication      │
├──────┼────────────────────────────────────┤
│ 2    │ Fix payment gateway timeout        │
└──────┴────────────────────────────────────┘
```

---

## Step 12: The Complete Example

Putting it all together, here's a polished task list:

```jinja
{% set t = table([
    {"name": "#", "width": 4, "style": "muted"},
    {"name": "Status", "width": 10},
    {"name": "Title", "width": "fill", "overflow": {"truncate": {"at": "middle"}}}
], border="rounded", header_style="bold", separator=" | ") %}

{{ t.header_row() }}
{{ t.separator_row() }}
{% for task in tasks %}
{{ t.row([loop.index, task.status | style_as(task.status), task.title]) }}
{% endfor %}
{{ t.bottom_border() }}
```

Output (80 columns, with styling):

```text
╭──────┬────────────┬───────────────────────────────────────────────────────╮
│ #    │ Status     │ Title                                                 │
├──────┼────────────┼───────────────────────────────────────────────────────┤
│ 1    │ pending    │ Implement user authentication                         │
│ 2    │ pending    │ Fix payment gateway timeout                           │
│ 3    │ done       │ Update documentation for API v2                       │
│ 4    │ pending    │ Review pull request #142                              │
╰──────┴────────────┴───────────────────────────────────────────────────────╯
```

Features in use:

- **Rounded borders** for a modern look
- **Muted styling** on index column for visual hierarchy
- **Fill width** on title to use available space
- **Middle truncation** for titles that exceed the column
- **Dynamic status colors** via `style_as`

---

## Using Tabular from Rust

Everything shown in templates is also available in Rust:

```rust
use standout_render::tabular::{TabularFormatter, ColumnSpec, Overflow, Alignment};

let columns = vec![
    ColumnSpec::fixed(4).header("#").style("muted"),
    ColumnSpec::fixed(10).header("Status"),
    ColumnSpec::fill().header("Title").overflow(Overflow::truncate_middle()),
];

let formatter = TabularFormatter::new(columns)
    .separator(" | ")
    .terminal_width(80);

// Format individual rows
for (i, task) in tasks.iter().enumerate() {
    let row = formatter.format_row(&[
        &(i + 1).to_string(),
        &task.status.to_string(),
        &task.title,
    ]);
    println!("{}", row);
}
```

---

## Summary

Tabular transforms raw data into polished, scannable output with minimal effort:

1. **Start simple** - use `col` filter for quick alignment
2. **Structure with `tabular()`** - centralize column definitions
3. **Flex with widths** - use `fill`, bounded ranges, and fractions
4. **Align content** - right-align numbers and dates
5. **Anchor columns** - pin important data to edges
6. **Handle overflow** - truncate intelligently or wrap
7. **Add visual hierarchy** - style columns and values dynamically
8. **Extract automatically** - let `row_from()` pull fields from structs
9. **Decorate as tables** - add borders, headers, and separators

The declarative approach means your layout adapts to terminal width, handles Unicode correctly, and remains maintainable as your data evolves.

For complete API details, see the [API documentation](https://docs.rs/standout-render).
