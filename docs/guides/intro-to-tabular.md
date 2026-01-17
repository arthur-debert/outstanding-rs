# Introduction to Tabular

Polished terminal output requires good and smart formatting, which Outstanding gives you several tools for (see [Rendering System](rendering-system.md)). The other pillar is layouts. As a text-only, non-interactive output, that mostly means aligning things vertically and controlling how multiple, complex, and rich information fragments are presented.

Tabular works with a declarative set of columns. These can have powerful primitives for sizing (fixed, range, expand, expand fraction), position (anchor to right), truncation, overflow (clip, multi-line, truncate) as well as cell alignment, automated per-column styling, and more.

Tabular is not only about tables, however. Any listing where each item is further broken down and is better presented in alignment is a good candidate. For example, log entries listing where authors, timestamps, and messages are displayed. In fact, many if not most listing outputs benefit from the capabilities Tabular provides. That said, one can decorate a tabular declaration with headers, separators, and borders, and voila - now you've got a table.

Tabular is designed to free you from the grunt work as much as possible. That means it comes with a declarative API, a template-based syntax for in-template usage, and an easy way to annotate and link how your already existing data types are represented in these cases.

This ensures that even complex tables with complex types can be fully declaratively handled, with precise control over the layout and not a single line of code.

In this guide, we will walk our way up from a simpler table to a more complex one, exploring the available features of Tabular.

**See Also:**

- [Tabular Reference](tabular.md) - complete API reference
- [Rendering System](rendering-system.md) - templates and styles in depth

---

## Our Example: A Task Manager

We'll build the output for `tasks list`, a command that shows pending tasks. This is a perfect Tabular use case: each task has an ID, title, status, assignee, and due date. We want them aligned, readable, and visually clear at a glance.

Here's our data:

```rust
#[derive(Serialize)]
struct Task {
    id: String,
    title: String,
    status: String,      // "pending", "in_progress", "done", "blocked"
    assignee: String,
    due: String,
}

let tasks = vec![
    Task { id: "TSK-001", title: "Implement user authentication", status: "in_progress", assignee: "alice", due: "2024-01-20" },
    Task { id: "TSK-002", title: "Fix payment gateway timeout", status: "blocked", assignee: "bob", due: "2024-01-18" },
    Task { id: "TSK-003", title: "Update documentation for API v2", status: "pending", assignee: "carol", due: "2024-01-25" },
    Task { id: "TSK-004", title: "Review pull request #142", status: "done", assignee: "alice", due: "2024-01-15" },
];
```

Let's progressively build this from raw output to a polished, professional listing.

---

## Step 1: The Problem with Plain Output

Without any formatting, a naive approach might look like this:

```jinja
{% for task in tasks %}
{{ task.id }} {{ task.title }} {{ task.status }} {{ task.assignee }} {{ task.due }}
{% endfor %}
```

Output:

```text
TSK-001 Implement user authentication in_progress alice 2024-01-20
TSK-002 Fix payment gateway timeout blocked bob 2024-01-18
TSK-003 Update documentation for API v2 pending carol 2024-01-25
TSK-004 Review pull request #142 done alice 2024-01-15
```

This is barely readable. Fields run together, nothing aligns, and scanning the list requires mental parsing of each line. Let's fix that.

---

## Step 2: Basic Column Alignment with `col`

The simplest improvement is the `col` filter. It pads (or truncates) each value to a fixed width:

```jinja
{% for task in tasks %}
{{ task.id | col(8) }}  {{ task.title | col(35) }}  {{ task.status | col(12) }}  {{ task.assignee | col(8) }}  {{ task.due | col(10) }}
{% endfor %}
```

Output:

```text
TSK-001   Implement user authentication          in_progress   alice     2024-01-20
TSK-002   Fix payment gateway timeout            blocked       bob       2024-01-18
TSK-003   Update documentation for API v2        pending       carol     2024-01-25
TSK-004   Review pull request #142               done          alice     2024-01-15
```

Already much better. Each column aligns vertically, making it easy to scan. But we've hardcoded widths, and if a title is too long, it gets truncated with `…`.

> **Key insight:** The `col` filter handles Unicode correctly. CJK characters count as 2 columns, combining marks don't add width, and ANSI escape codes are preserved but not counted.

---

## Step 3: Structured Layout with `tabular()`

For more control, use the `tabular()` function. This creates a formatter that you configure once and use for all rows:

```jinja
{% set t = tabular([
    {"name": "id", "width": 8},
    {"name": "title", "width": 35},
    {"name": "status", "width": 12},
    {"name": "assignee", "width": 8},
    {"name": "due", "width": 10}
], separator="  ") %}

{% for task in tasks %}
{{ t.row([task.id, task.title, task.status, task.assignee, task.due]) }}
{% endfor %}
```

The output looks the same, but now the column definitions are centralized. This becomes powerful when we start adding features.

---

## Step 4: Flexible Widths

Hardcoded widths are fragile. What if the terminal is wider or narrower? Tabular offers flexible width strategies:

| Width | Meaning |
|-------|---------|
| `8` | Exactly 8 columns (fixed) |
| `{"min": 10}` | At least 10, grows to fit content |
| `{"min": 10, "max": 30}` | Between 10 and 30 |
| `"fill"` | Takes all remaining space |
| `"2fr"` | 2 parts of remaining (proportional) |

Let's make the title column expand to fill available space:

```jinja
{% set t = tabular([
    {"name": "id", "width": 8},
    {"name": "title", "width": "fill"},
    {"name": "status", "width": 12},
    {"name": "assignee", "width": 8},
    {"name": "due", "width": 10}
], separator="  ") %}
```

Now on an 80-column terminal:

```text
TSK-001   Implement user authentication                    in_progress   alice     2024-01-20
TSK-002   Fix payment gateway timeout                      blocked       bob       2024-01-18
TSK-003   Update documentation for API v2                  pending       carol     2024-01-25
TSK-004   Review pull request #142                         done          alice     2024-01-15
```

On a 120-column terminal, the title column automatically expands:

```text
TSK-001   Implement user authentication                                              in_progress   alice     2024-01-20
TSK-002   Fix payment gateway timeout                                                blocked       bob       2024-01-18
```

The layout adapts to the available space.

---

## Step 5: Right-Align Dates

Dates look better right-aligned. Numbers too. Use the `align` option:

```jinja
{% set t = tabular([
    {"name": "id", "width": 8},
    {"name": "title", "width": "fill"},
    {"name": "status", "width": 12},
    {"name": "assignee", "width": 8},
    {"name": "due", "width": 10, "align": "right"}
], separator="  ") %}
```

Output:

```text
TSK-001   Implement user authentication                    in_progress   alice     2024-01-20
TSK-002   Fix payment gateway timeout                      blocked       bob        2024-01-18
TSK-003   Update documentation for API v2                  pending       carol      2024-01-25
TSK-004   Review pull request #142                         done          alice      2024-01-15
```

The dates now align on the right edge of their column.

---

## Step 6: Anchoring Columns

Sometimes you want a column pinned to the terminal's right edge, regardless of how other columns resize. Use `anchor`:

```jinja
{% set t = tabular([
    {"name": "id", "width": 8},
    {"name": "title", "width": "fill"},
    {"name": "status", "width": 12},
    {"name": "assignee", "width": 8},
    {"name": "due", "width": 10, "align": "right", "anchor": "right"}
], separator="  ") %}
```

Now the due date column is always at the right edge. If the terminal is 100 columns or 200, the date stays anchored. The fill column absorbs the extra space between fixed columns and anchored columns.

---

## Step 7: Handling Long Content

What happens when a title is longer than its column? By default, Tabular truncates at the end with `…`. But you have options:

### Truncate at Different Positions

```jinja
{"name": "title", "width": 30, "overflow": "truncate"}                        {# "Very long title th…" #}
{"name": "title", "width": 30, "overflow": {"truncate": {"at": "start"}}}     {# "…itle that is long" #}
{"name": "title", "width": 30, "overflow": {"truncate": {"at": "middle"}}}    {# "Very long…is long" #}
```

Middle truncation is perfect for file paths where both the start and end matter: `/home/user/…/important.txt`

### Wrap to Multiple Lines

For descriptions or messages, wrapping is often better than truncating:

```jinja
{% set t = tabular([
    {"name": "id", "width": 8},
    {"name": "title", "width": 40, "overflow": "wrap"},
    {"name": "status", "width": 12}
], separator="  ") %}
```

If a title exceeds 40 columns, it wraps:

```text
TSK-005   Implement comprehensive error handling    pending
          for all API endpoints with proper
          logging and user feedback
TSK-006   Quick fix                                 done
```

The wrapped lines are indented to align with the column.

---

## Step 8: Dynamic Styling Based on Values

Here's where Tabular shines for task lists. We want status colors: green for done, yellow for pending, red for blocked, blue for in progress.

First, define styles in your theme:

```yaml
# theme.yaml
styles:
  done: { fg: green }
  pending: { fg: yellow }
  blocked: { fg: red }
  in_progress: { fg: blue }
```

Then use the `style_as` filter to apply styles based on the value itself:

```jinja
{% set t = tabular([
    {"name": "id", "width": 8},
    {"name": "title", "width": "fill"},
    {"name": "status", "width": 12},
    {"name": "assignee", "width": 8},
    {"name": "due", "width": 10, "align": "right"}
], separator="  ") %}

{% for task in tasks %}
{{ t.row([task.id, task.title, task.status | style_as(task.status), task.assignee, task.due]) }}
{% endfor %}
```

The `style_as` filter wraps the value in style tags: `[done]done[/done]`. Outstanding's rendering system then applies the green color.

Output (with colors):

```text
TSK-001   Implement user authentication          [blue]in_progress[/blue]   alice     2024-01-20
TSK-002   Fix payment gateway timeout            [red]blocked[/red]         bob       2024-01-18
TSK-003   Update documentation for API v2        [yellow]pending[/yellow]   carol     2024-01-25
TSK-004   Review pull request #142               [green]done[/green]        alice     2024-01-15
```

In the terminal, statuses appear in their respective colors, making it instantly clear which tasks need attention.

---

## Step 9: Column-Level Styles

Instead of styling individual values, you can style entire columns. This is useful for de-emphasizing certain information:

```jinja
{% set t = tabular([
    {"name": "id", "width": 8, "style": "muted"},
    {"name": "title", "width": "fill"},
    {"name": "status", "width": 12},
    {"name": "assignee", "width": 8, "style": "muted"},
    {"name": "due", "width": 10, "align": "right", "style": "muted"}
], separator="  ") %}
```

Now IDs, assignees, and dates appear in a muted style (typically gray), while titles and statuses remain prominent. This creates visual hierarchy.

---

## Step 10: Automatic Field Extraction

Tired of manually listing `[task.id, task.title, ...]`? If your column names match your struct fields, use `row_from()`:

```jinja
{% set t = tabular([
    {"name": "id", "width": 8},
    {"name": "title", "width": "fill"},
    {"name": "status", "width": 12},
    {"name": "assignee", "width": 8},
    {"name": "due", "width": 10, "align": "right"}
]) %}

{% for task in tasks %}
{{ t.row_from(task) }}
{% endfor %}
```

Tabular extracts `task.id`, `task.title`, etc. automatically. For nested fields, use `key`:

```jinja
{"name": "Author", "key": "author.name", "width": 20}
{"name": "Email", "key": "author.email", "width": 30}
```

---

## Step 11: Adding Headers and Borders

For a proper table with headers, switch from `tabular()` to `table()`:

```jinja
{% set t = table([
    {"name": "ID", "key": "id", "width": 8},
    {"name": "Title", "key": "title", "width": "fill"},
    {"name": "Status", "key": "status", "width": 12},
    {"name": "Assignee", "key": "assignee", "width": 10},
    {"name": "Due Date", "key": "due", "width": 10, "align": "right"}
], border="rounded", header_style="bold") %}

{{ t.header_row() }}
{{ t.separator_row() }}
{% for task in tasks %}
{{ t.row_from(task) }}
{% endfor %}
{{ t.bottom_border() }}
```

Output:

```text
╭──────────┬─────────────────────────────────────┬──────────────┬────────────┬────────────╮
│ ID       │ Title                               │ Status       │ Assignee   │   Due Date │
├──────────┼─────────────────────────────────────┼──────────────┼────────────┼────────────┤
│ TSK-001  │ Implement user authentication       │ in_progress  │ alice      │ 2024-01-20 │
│ TSK-002  │ Fix payment gateway timeout         │ blocked      │ bob        │ 2024-01-18 │
│ TSK-003  │ Update documentation for API v2     │ pending      │ carol      │ 2024-01-25 │
│ TSK-004  │ Review pull request #142            │ done         │ alice      │ 2024-01-15 │
╰──────────┴─────────────────────────────────────┴──────────────┴────────────┴────────────╯
```

### Border Styles

Choose from six border styles:

| Style | Look |
|-------|------|
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
┌──────────┬────────────────────┐
│ ID       │ Title              │
├──────────┼────────────────────┤
│ TSK-001  │ Auth system        │
├──────────┼────────────────────┤
│ TSK-002  │ Payment fix        │
└──────────┴────────────────────┘
```

---

## Step 12: The Complete Example

Putting it all together, here's our polished task list:

```jinja
{% set t = table([
    {"name": "ID", "key": "id", "width": 8, "style": "muted"},
    {"name": "Title", "key": "title", "width": "fill", "overflow": {"truncate": {"at": "middle"}}},
    {"name": "Status", "key": "status", "width": 12},
    {"name": "Assignee", "key": "assignee", "width": 10},
    {"name": "Due", "key": "due", "width": 10, "align": "right", "anchor": "right", "style": "muted"}
], border="rounded", header_style="bold", separator=" │ ") %}

{{ t.header_row() }}
{{ t.separator_row() }}
{% for task in tasks %}
{{ t.row([task.id, task.title, task.status | style_as(task.status), task.assignee, task.due]) }}
{% endfor %}
{{ t.bottom_border() }}
```

Output (80 columns, with styling):

```text
╭──────────┬────────────────────────────────┬──────────────┬────────────┬────────────╮
│ ID       │ Title                          │ Status       │ Assignee   │        Due │
├──────────┼────────────────────────────────┼──────────────┼────────────┼────────────┤
│ TSK-001  │ Implement user authentication  │ in_progress  │ alice      │ 2024-01-20 │
│ TSK-002  │ Fix payment gateway timeout    │ blocked      │ bob        │ 2024-01-18 │
│ TSK-003  │ Update documentation for API…  │ pending      │ carol      │ 2024-01-25 │
│ TSK-004  │ Review pull request #142       │ done         │ alice      │ 2024-01-15 │
╰──────────┴────────────────────────────────┴──────────────┴────────────┴────────────╯
```

Features in use:
- **Rounded borders** for a modern look
- **Muted styling** on ID and date columns for visual hierarchy
- **Fill width** on title to use available space
- **Middle truncation** for titles that exceed the column
- **Dynamic status colors** via `style_as`
- **Right-aligned, right-anchored** due dates
- **Automatic field extraction** for clean template code

---

## Using Tabular from Rust

Everything shown in templates is also available in Rust:

```rust
use outstanding::tabular::{TabularSpec, Col, Table, BorderStyle};

let spec = TabularSpec::builder()
    .column(Col::fixed(8).header("ID").style("muted"))
    .column(Col::fill().header("Title").truncate_middle())
    .column(Col::fixed(12).header("Status"))
    .column(Col::fixed(10).header("Assignee"))
    .column(Col::fixed(10).header("Due").right().anchor_right().style("muted"))
    .separator(" │ ")
    .build();

let table = Table::new(spec, 80)
    .header_from_columns()
    .header_style("bold")
    .border(BorderStyle::Rounded);

// Render the full table
let output = table.render(&data);

// Or render parts manually for custom logic
println!("{}", table.header_row());
println!("{}", table.separator_row());
for task in &tasks {
    println!("{}", table.row(&[&task.id, &task.title, &task.status, &task.assignee, &task.due]));
}
println!("{}", table.bottom_border());
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

For complete API details, see the [Tabular Reference](tabular.md).
