# Templates Guide

Outstanding uses [MiniJinja](https://docs.rs/minijinja) as its template engine. Templates let you define output structure separately from your data, with full access to styling.

## Basic Syntax

### Variable Output

Use `{{ }}` to output values:

```
Hello, {{ name }}!
Total: {{ count }}
```

### Expressions

Templates support expressions:

```
{{ price * quantity }}
{{ "Hello" ~ " " ~ "World" }}  {# String concatenation #}
{{ items | length }}
```

## Styling

Apply named styles using the `style` filter:

```
{{ title | style("heading") }}
{{ error_message | style("error") }}
```

Styles are defined in your theme. See [Styling](styling.md) for details.

### Styling Literals

You can style literal strings too:

```
{{ "Error:" | style("error") }} {{ message }}
{{ ">" | style("prompt") }}
```

## Control Structures

### Conditionals

```
{% if count > 0 %}
Items found: {{ count }}
{% else %}
No items found.
{% endif %}
```

### Loops

```
{% for item in items %}
- {{ item.name | style("item") }}: {{ item.value }}
{% endfor %}
```

Loop variables available inside `{% for %}`:

| Variable | Description |
|----------|-------------|
| `loop.index` | Current iteration (1-indexed) |
| `loop.index0` | Current iteration (0-indexed) |
| `loop.first` | True on first iteration |
| `loop.last` | True on last iteration |
| `loop.length` | Total number of items |

```
{% for item in items %}
{{ loop.index }}. {{ item.name }}{% if not loop.last %}, {% endif %}
{% endfor %}
```

## Built-in Filters

### MiniJinja Filters

MiniJinja provides many built-in filters:

| Filter | Description | Example |
|--------|-------------|---------|
| `upper` | Uppercase | `{{ name \| upper }}` |
| `lower` | Lowercase | `{{ name \| lower }}` |
| `title` | Title Case | `{{ name \| title }}` |
| `trim` | Remove whitespace | `{{ text \| trim }}` |
| `length` | Get length | `{{ items \| length }}` |
| `first` | First item | `{{ items \| first }}` |
| `last` | Last item | `{{ items \| last }}` |
| `join` | Join with separator | `{{ items \| join(", ") }}` |
| `default` | Default value | `{{ value \| default("N/A") }}` |
| `replace` | Replace substring | `{{ text \| replace("a", "b") }}` |

### Outstanding Filters

Outstanding adds these filters:

| Filter | Description | Example |
|--------|-------------|---------|
| `style` | Apply named style | `{{ text \| style("heading") }}` |
| `nl` | Append newline | `{{ text \| nl }}` |

### Filter Chaining

Filters can be chained:

```
{{ title | upper | style("heading") }}
{{ items | join(", ") | style("list") }}
```

## Whitespace Control

By default, templates preserve whitespace. Use `-` to trim:

```
{%- if show_header %}
Header
{%- endif %}
```

- `{%-` trims whitespace before the tag
- `-%}` trims whitespace after the tag

## Newline Control

The `nl` filter explicitly adds newlines:

```
{{ title | style("heading") | nl }}
{{ "" | nl }}  {# Blank line #}
{{ content }}
```

This gives you precise control over line breaks.

## Comments

Comments are enclosed in `{# #}`:

```
{# This is a comment #}
{{ name }}  {# Inline comment #}
```

## Data Structures

### Accessing Nested Data

Use dot notation:

```
{{ user.name }}
{{ config.settings.theme }}
```

### Accessing by Index

```
{{ items[0] }}
{{ matrix[1][2] }}
```

## Practical Examples

### List with Styling

```
{% for item in items %}
{{ item.name | style("item_name") }}:{{ item.padding }}{{ item.desc | style("item_desc") }}
{% endfor %}
```

### Conditional Formatting

```
{% if status == "error" %}
{{ message | style("error") }}
{% elif status == "warning" %}
{{ message | style("warning") }}
{% else %}
{{ message | style("info") }}
{% endif %}
```

### Table-like Output

```
{{ "Name" | style("header") }}    {{ "Value" | style("header") }}
{% for row in rows %}
{{ row.name }}    {{ row.value | style("value") }}
{% endfor %}
```

### Summary with Counts

```
{{ title | upper | style("title") | nl }}
{{ "-" * 40 | nl }}
Found {{ count | style("count") }} item{% if count != 1 %}s{% endif %}.
```

## Rendering

### Simple Rendering

```rust
use outstanding::{render, Theme, ThemeChoice};

let output = render(template, &data, ThemeChoice::from(&theme))?;
```

### With Output Mode Control

```rust
use outstanding::{render_with_output, OutputMode};

let output = render_with_output(
    template,
    &data,
    ThemeChoice::from(&theme),
    OutputMode::Text,  // Plain text, no ANSI codes
)?;
```

### Pre-compiled Templates

For repeated rendering, use `Renderer`:

```rust
use outstanding::Renderer;

let mut renderer = Renderer::new(theme)?;
renderer.add_template("summary", template)?;

// Render multiple times efficiently
let output1 = renderer.render("summary", &data1)?;
let output2 = renderer.render("summary", &data2)?;
```

## Data Serialization

Template data must implement `serde::Serialize`:

```rust
use serde::Serialize;

#[derive(Serialize)]
struct Report {
    title: String,
    items: Vec<Item>,
    count: usize,
}

#[derive(Serialize)]
struct Item {
    name: String,
    value: i32,
}
```

For ad-hoc data, use `serde_json::json!`:

```rust
let data = serde_json::json!({
    "title": "Report",
    "items": [
        {"name": "foo", "value": 42},
        {"name": "bar", "value": 17},
    ],
    "count": 2
});
```

## Error Handling

Template errors are returned as `minijinja::Error`:

```rust
match render(template, &data, theme) {
    Ok(output) => println!("{}", output),
    Err(e) => eprintln!("Template error: {}", e),
}
```

Common errors:
- Undefined variables
- Invalid filter usage
- Syntax errors
- Style validation failures (unresolved aliases, cycles)

## Best Practices

1. **Keep templates simple**: Complex logic belongs in Rust code, not templates

2. **Use semantic style names**: Reference styles by what they mean, not how they look. See [Styling](styling.md).

3. **Validate early**: Errors surface at render time. Test templates during development.

4. **Consider plain text**: Design templates that degrade gracefully to plain text (no styling)

5. **Use `Renderer` for repeated renders**: Pre-compile templates for better performance
