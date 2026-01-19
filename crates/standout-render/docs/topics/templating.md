# Templating

`standout-render` uses a two-pass templating system that combines the power of MiniJinja for logic and data binding with a custom BBCode-like syntax for styling. This separation keeps templates readable while providing full control over both content and presentation.

---

## Two-Pass Rendering Pipeline

Templates are processed in two distinct passes:

```text
Template + Data → [Pass 1: MiniJinja] → Text with style tags → [Pass 2: BBParser] → Final output
```

**Pass 1 - MiniJinja**: Standard template processing. Variables are substituted, control flow executes, filters apply.

**Pass 2 - BBParser**: Style tag processing. Bracket-notation tags are converted to ANSI escape codes (or stripped, depending on output mode).

### Example

```text
Template:     [title]{{ name }}[/title] has {{ count }} items
Data:         { name: "Report", count: 42 }

After Pass 1: [title]Report[/title] has 42 items
After Pass 2: \x1b[1;36mReport\x1b[0m has 42 items  (or plain: "Report has 42 items")
```

This separation means:

- Template logic (loops, conditionals) is handled by MiniJinja—a mature, well-documented engine
- Style application is a simple, predictable transformation
- You can debug each pass independently

---

## MiniJinja Basics

MiniJinja implements Jinja2 syntax, a widely-used templating language. Here's a quick overview:

### Variables

```jinja
{{ variable }}
{{ object.field }}
{{ list[0] }}
```

### Control Flow

```jinja
{% if condition %}
  Show this
{% elif other_condition %}
  Show that
{% else %}
  Default
{% endif %}

{% for item in items %}
  {{ loop.index }}. {{ item.name }}
{% endfor %}
```

### Filters

```jinja
{{ name | upper }}
{{ list | length }}
{{ value | default("N/A") }}
{{ text | truncate(20) }}
```

### Comments

```jinja
{# This is a comment and won't appear in output #}
```

For comprehensive MiniJinja documentation, see the [MiniJinja documentation](https://docs.rs/minijinja).

---

## Style Tags

Style tags use BBCode-like bracket notation to apply named styles from your theme:

```jinja
[style-name]content to style[/style-name]
```

### Basic Usage

```jinja
[title]Report Summary[/title]
[error]Something went wrong![/error]
[muted]Last updated: {{ timestamp }}[/muted]
```

### Nesting

Tags can nest properly:

```jinja
[outer][inner]nested content[/inner][/outer]
```

### Spanning Lines

Tags can span multiple lines:

```jinja
[panel]
This is a multi-line
block of styled content
[/panel]
```

### With Template Logic

Style tags and MiniJinja work together seamlessly:

```jinja
[title]{% if custom_title %}{{ custom_title }}{% else %}Default Title{% endif %}[/title]

{% for task in tasks %}
[{{ task.status }}]{{ task.title }}[/{{ task.status }}]
{% endfor %}
```

The second example shows dynamic style names—the style applied depends on the value of `task.status`.

---

## Processing Modes

Pass 2 (BBParser) processes style tags differently based on the output mode:

| Mode | Behavior | Use Case |
|------|----------|----------|
| `Term` | Replace tags with ANSI escape codes | Rich terminal output |
| `Text` | Strip tags completely | Plain text, pipes, files |
| `TermDebug` | Keep tags as literal text | Debugging, testing |

### Example

Template: `[title]Hello[/title]`

- **Term**: `\x1b[1;36mHello\x1b[0m` (rendered as cyan bold)
- **Text**: `Hello`
- **TermDebug**: `[title]Hello[/title]`

### Setting the Mode

```rust
use standout_render::{render_with_output, OutputMode};

// Rich terminal
let output = render_with_output(template, &data, &theme, OutputMode::Term)?;

// Plain text
let output = render_with_output(template, &data, &theme, OutputMode::Text)?;

// Debug (tags visible)
let output = render_with_output(template, &data, &theme, OutputMode::TermDebug)?;

// Auto-detect based on TTY
let output = render_with_output(template, &data, &theme, OutputMode::Auto)?;
```

### Auto Mode

`OutputMode::Auto` detects the appropriate mode:

- If stdout is a TTY with color support → `Term`
- If stdout is a pipe or redirect → `Text`

> **For standout framework users:** The framework's `--output` CLI flag automatically sets the output mode. See standout documentation for details.

---

## Built-in Filters

Beyond MiniJinja's standard filters, `standout-render` provides formatting filters:

### Column Formatting

```jinja
{{ value | col(10) }}                              {# pad/truncate to 10 chars #}
{{ value | col(20, align="right") }}               {# right-align in 20 chars #}
{{ value | col(15, truncate="middle") }}           {# truncate in middle #}
{{ value | col(15, truncate="start", ellipsis="...") }}
```

### Padding

```jinja
{{ "42" | pad_left(8) }}      {# "      42" #}
{{ "hi" | pad_right(8) }}     {# "hi      " #}
{{ "hi" | pad_center(8) }}    {# "   hi   " #}
```

### Truncation

```jinja
{{ long_text | truncate_at(20) }}                   {# "Very long text th..." #}
{{ path | truncate_at(30, "middle", "...") }}      {# "/home/.../file.txt" #}
{{ text | truncate_at(20, "start") }}              {# "...end of the text" #}
```

### Display Width

```jinja
{% if value | display_width > 20 %}
  {{ value | truncate_at(20) }}
{% else %}
  {{ value }}
{% endif %}
```

Returns visual width (handles Unicode—CJK characters count as 2).

### Style Application

```jinja
{{ value | style_as("error") }}                    {# wraps in [error]...[/error] #}
{{ task.status | style_as(task.status) }}         {# dynamic: [pending]pending[/pending] #}
```

---

## Template Registry

When using the `Renderer` struct, templates are resolved by name through a registry:

```rust
use standout_render::Renderer;

let mut renderer = Renderer::new(theme)?;

// Add inline template
renderer.add_template("greeting", "Hello, [name]{{ name }}[/name]!")?;

// Add directory of templates
renderer.add_template_dir("./templates")?;

// Render by name
let output = renderer.render("greeting", &data)?;
```

### Resolution Priority

1. **Inline templates** (added via `add_template()`)
2. **Directory templates** (from `add_template_dir()`)

### File Extensions

Supported extensions (in priority order): `.jinja`, `.jinja2`, `.j2`, `.txt`

When you request `"report"`, the registry checks:
- Inline template named `"report"`
- `report.jinja` in registered directories
- `report.jinja2`, `report.j2`, `report.txt` (lower priority)

### Template Names

Template names are derived from relative paths:

```text
templates/
├── greeting.jinja       → "greeting"
├── reports/
│   └── summary.jinja    → "reports/summary"
└── errors/
    └── 404.jinja        → "errors/404"
```

---

## Including Templates

Templates can include other templates using MiniJinja's include syntax:

```jinja
{# main.jinja #}
[title]{{ title }}[/title]

{% include "partials/header.jinja" %}

{% for item in items %}
  {% include "partials/item.jinja" %}
{% endfor %}

{% include "partials/footer.jinja" %}
```

This enables reusable components across your application.

---

## Context Variables

Beyond your data, you can inject additional context into templates:

```rust
use standout_render::{render_with_vars, OutputMode};
use std::collections::HashMap;

let mut vars = HashMap::new();
vars.insert("version", "1.0.0");
vars.insert("app_name", "MyApp");

let output = render_with_vars(
    "{{ app_name }} v{{ version }}: {{ message }}",
    &data,
    &theme,
    OutputMode::Term,
    vars,
)?;
```

When handler data and context variables have the same key, **handler data wins**. Context is supplementary.

---

## Structured Output

For machine-readable output (JSON, YAML, CSV), templates are bypassed entirely:

```rust
use standout_render::{render_auto, OutputMode};

// Template is used for Term/Text modes
// Data is serialized directly for Json/Yaml/Csv
let output = render_auto(template, &data, &theme, OutputMode::Json)?;
```

| Mode | Behavior |
|------|----------|
| `Term` | Render template, apply styles |
| `Text` | Render template, strip styles |
| `TermDebug` | Render template, keep style tags |
| `Json` | `serde_json::to_string_pretty(data)` |
| `Yaml` | `serde_yaml::to_string(data)` |
| `Csv` | Flatten and format as CSV |

This means your serializable data types automatically support structured output without additional code.

---

## Validation

Check templates for unknown style tags before deploying:

```rust
use standout_render::validate_template;

let errors = validate_template(template, &sample_data, &theme);
if !errors.is_empty() {
    for error in &errors {
        eprintln!("Unknown style tag: [{}]", error.tag_name);
    }
}
```

Validation catches:
- Misspelled style names
- References to undefined styles
- Mismatched opening/closing tags

---

## API Reference

### Render Functions

```rust
use standout_render::{
    render,                  // Basic: template + data + theme
    render_with_output,      // With explicit output mode
    render_with_mode,        // With output mode + color mode
    render_with_vars,        // With extra context variables
    render_auto,             // Auto-dispatch template vs serialize
    render_auto_with_context,
};

// Basic
let output = render(template, &data, &theme)?;

// With output mode
let output = render_with_output(template, &data, &theme, OutputMode::Term)?;

// With color mode override (for testing)
let output = render_with_mode(template, &data, &theme, OutputMode::Term, ColorMode::Dark)?;

// Auto (template for text modes, serialize for structured)
let output = render_auto(template, &data, &theme, OutputMode::Json)?;
```

### Renderer Struct

```rust
use standout_render::Renderer;

let mut renderer = Renderer::new(theme)?;
renderer.add_template("name", "content")?;
renderer.add_template_dir("./templates")?;

let output = renderer.render("name", &data)?;
let output = renderer.render_with_mode("name", &data, OutputMode::Text)?;
```
