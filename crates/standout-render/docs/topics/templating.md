# Templating

`standout-render` displays inserted values as text and applies styles written in templates or constructed with `FormattedText`. MiniJinja handles variables and control flow; the bracket parser turns deliberate styles into terminal output.

The default engine is MiniJinja (Jinja2-compatible), but alternative engines are available. See [Template Engines](template-engines.md) for options including a lightweight `SimpleEngine` for reduced binary size.

---

## Two-Pass Rendering Pipeline

Templates are processed in two distinct passes:

```text
Template + Data → [Pass 1: MiniJinja] → Text with style tags → [Pass 2: BBParser] → Final output
```

**Pass 1 - MiniJinja**: Values are inserted with their text escaped, control flow executes, and formatting operations preserve explicit styles.

**Pass 2 - BBParser**: Style tag processing. Bracket-notation tags are converted to ANSI escape codes (or stripped, depending on output mode).

### Pipeline Example

```text
Template:     [title]{{ name }}[/title] has {{ count }} items
Data:         { name: "Report", count: 42 }

After Pass 1: [title]Report[/title] has 42 items
After Pass 2: \x1b[1;36mReport\x1b[0m has 42 items  (or plain: "Report has 42 items")
```

## Text and Formatted Values

Ordinary `{{ value }}` displays brackets and backslashes literally. C0/C1
controls, including ESC, CR, NUL and DEL, become visible lowercase codepoint
spellings such as `\u{1b}` and `\u{d}`. Newline and tab remain layout whitespace.
The same policy applies to nested values, loop items, context values, table
cells, and incremental human output, including with color disabled. A filename
that must occupy one line needs a separate single-line presentation operation.

Construct deliberate formatting in CLI view data:

```rust
use standout_render::FormattedText;

let heading = FormattedText::text("Changed: ")
    .append(FormattedText::text("[draft].txt").styled("path")?)
    .styled("heading")?;
let colored = FormattedText::from_ansi_sgr("\x1b[31mred\x1b[0m");
```

`text` accepts a string; `append` accepts strings or formatted children.
`styled` wraps the existing children and validates the name: a lowercase ASCII
letter or `_`, then lowercase ASCII letters, digits, `_` or `-`.
Text children remain literal even inside a style. Define semantic styles in the
theme, and keep `FormattedText` in CLI view types rather than application
library models.

`plain_text()` concatenates the text children. JSON/YAML/CSV/NDJSON serialize a
formatted value as that string, with no style metadata; ordinary values retain
their original contents and shape. Human escape spellings are applied only when
displaying text. Explicit raw output and diagnostic payload APIs retain their
separate byte-preserving behavior.

Authored literal fragments between template expressions must contain complete
bracket tokens and paired backslash escapes. Escape literal brackets and
backslashes as `\[` and `\\`; use `value | style_as(name)` for dynamic styles.
This also applies inside branches, loops, captures, and included templates.

### Importing ANSI SGR

`from_ansi_sgr` accepts SGR introduced by `ESC [` or C1 CSI (`U+009B`) and
terminated by `m`. It supports semicolon-separated decimal parameters:

| Parameters | Formatting |
| --- | --- |
| `0` | Reset all attributes and colors |
| `1`, `2`, `3`, `4`, `5`, `7`, `8`, `9` | Bold, dim, italic, underline, blink, reverse, hidden, strikethrough |
| `22`, `23`, `24`, `25`, `27`, `28`, `29` | Reset bold/dim together, then the corresponding attributes above |
| `30–37`, `40–47`, `90–97`, `100–107` | Standard and bright foreground/background colors |
| `39`, `49` | Default foreground/background |
| `38;5;n`, `48;5;n` | Indexed foreground/background, `n` in `0–255` |
| `38;2;r;g;b`, `48;2;r;g;b` | RGB foreground/background, each channel in `0–255` |

An empty parameter means `0`. Each sequence permits at most 128 parameter
bytes and 32 parameters. Unsupported, malformed, incomplete, or oversized
sequences remain text in full; a partially supported sequence changes no style.
OSC, DCS and other control strings remain text as complete units, including any
SGR inside them. Supported SGR becomes metadata and disappears from the plain
projection; remaining controls become visible only during human rendering.

### Composition and String Operations

`append`, template `join`, `style_as`, tables, padding and `truncate_at` preserve
explicit formatting. Width operations measure the displayed text after control
escaping; style metadata has zero width. Truncation closes styles before the
following output.

`~`, `string`, `replace` and slicing use formatted values' plain-text projection
and return ordinary text. Macro and block captures preserve template-authored
styles and literal value children. Use `MiniJinjaEngine` for these captures;
a bare MiniJinja environment does not perform Standout's template preparation.

Terminal templates reject `{% autoescape ... %}` blocks. The HTML filters
`safe`, `escape` and `e` leave values unchanged and confer no terminal formatting
privileges. MiniJinja safe-string metadata on supplied data is cleared at ingress.
The `verbatim` filter is removed: replace `{{ value | verbatim }}` with
`{{ value }}`. Callers outside templates can use
`standout_render::escape_control_characters(String)` for the same control policy;
ordinary template insertion already handles both controls and brackets.

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

### Booleans and None

Standout renders these the Rust way — `true`, `false`, `none` — not the Jinja2
way MiniJinja itself uses (`True`, `False`, `None`). This holds for
interpolation, loop and `set` bindings, `| string`, `| join`, sequence and map
literals, standout's own filters, and table cells:

```jinja
{{ flag }}                {# true #}
{{ missing }}             {# none #}
{{ flags }}               {# [true, false, none] #}
{{ flags | join(", ") }}  {# true, false, none #}
```

Two exceptions:

- The `~` concatenation operator formats inside MiniJinja's evaluator, which
  exposes no hook: `{{ "x" ~ flag }}` yields `xTrue`. Write `{{ "x" }}{{ flag }}`
  or `{{ "x" ~ flag | string }}`.
- Structured output (JSON, YAML, CSV, NDJSON) skips templates entirely, so
  those modes follow their format's own rules: JSON, YAML and CSV serialize
  your data as the document, and NDJSON writes it inside a
  `{"type":"result","data":…}` line.

If you build a `minijinja::Environment` yourself, use
`standout_render::template::new_environment()` — or call `register_filters` on
your own environment, which installs the same spelling.

---

## The Trailing-Newline Contract

Two things happen to the newline at the end of a template, and together they
are observable in the bytes a script reads, so they are stated here rather than
discovered by probing.

**The engine consumes exactly one final newline.** This is Jinja's rule and
MiniJinja keeps it. A template file ending in a single `\n` renders with no
trailing newline at all; a file ending in two renders with one.

| Template source | Rendered string |
| --- | --- |
| `{{ name }}` | `x` |
| `{{ name }}\n` | `x` |
| `{{ name }}\n\n` | `x\n` |
| `{{ name }}\n\n\n` | `x\n\n` |

**The process edge appends exactly one newline.** `App::run` writes a handled
command's text with `writeln!`, so what reaches stdout is the rendered string
plus one `\n` — whatever the template ended with.

The practical consequence: a template that ends with one newline and a template
that ends with none produce identical bytes. To end a page with a blank line,
the template needs *two* trailing newlines. Every editor that adds a final
newline on save is therefore invisible here, which is the reason the rule is
worth stating.

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
{{ task.title | style_as(task.status) }}
{% endfor %}
```

The second example selects a validated style name from `task.status`. Templates
that assemble a tag name with `[{{ status }}]` are rejected; use `style_as`.

### Literal brackets

In literal template source, escape `[` as `\[`, `]` as `\]`, and a backslash
as `\\`. Inserted values need no manual escaping:

```jinja
Ready \[y/n\]
Range \[0, 100\]
```

renders as `Ready [y/n]` and `Range [0, 100]`. The escape works in every output
mode, and an escaped bracket is never treated as a tag, so it raises no
[unresolved-tag warning](styling-system.md#unresolved-tag-warning). A `[` that
does not begin a valid tag name (for example `[0, 100]`) is already left
literal, so escaping is only needed when the bracketed text would otherwise
parse as a tag.

A doubled backslash emits one backslash, so `\\[title]x[/title]` displays a
backslash followed by styled `x`. MiniJinja brace escaping remains separate
(see [Template Engines](template-engines.md)).

---

## Style Modes

Pass 2 (BBParser) processes style tags differently according to the `StyleMode`
the run resolves to:

| Style mode | Behavior | Use Case |
| ------ | ---------- | ---------- |
| `Ansi` | Replace tags with ANSI escape codes | Rich terminal output |
| `Plain` | Strip tags completely | Plain text, pipes, files |
| `Debug` | Keep tags as literal text | Debugging, testing |

`Ansi` and `Plain` are the two ways `Representation::Human` can render, chosen
by the run's `ColorPolicy` against the destination's color capability. `Debug`
is what `Representation::TermDebug` renders as.

### Style Modes Example

Template: `[title]Hello[/title]`

- **Ansi**: `\x1b[1;36mHello\x1b[0m` (rendered as cyan bold)
- **Plain**: `Hello`
- **Debug**: `[title]Hello[/title]`

The "strip tags completely" behavior applies to tags the active theme defines.
An *unknown* tag degrades differently, and an unbalanced unknown tag survives
verbatim even under a plain style mode — see [Unknown Style
Tags](styling-system.md#unknown-style-tags).

### Setting the Representation and the Color Policy

```rust
use standout_render::{render_with_output, ColorPolicy, Representation};

// Rich terminal
let output = render_with_output(template, &data, &theme, Representation::Human, ColorPolicy::Always)?;

// Plain text
let output = render_with_output(template, &data, &theme, Representation::Human, ColorPolicy::Never)?;

// Debug (tags visible)
let output = render_with_output(template, &data, &theme, Representation::TermDebug, ColorPolicy::Auto)?;

// Decide from the destination's color capability
let output = render_with_output(template, &data, &theme, Representation::Human, ColorPolicy::Auto)?;
```

### The style decision

`Representation::Human` renders with or without escape sequences according to
the run's `ColorPolicy` and the destination's color capability; the rule is in
[Output Modes](../../../topics/output-modes.md#the-style-decision).

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

Returns displayed width after control escaping; CJK characters count as 2.

### Style Application

```jinja
{{ value | style_as("error") }}
{{ task.status | style_as(task.status) }}
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

Supported extensions (in priority order): `.jinja`, `.jinja2`, `.j2`, `.stpl`, `.txt`

When you request `"report"`, the registry checks:

- Inline template named `"report"`
- `report.jinja` in registered directories
- `report.jinja2`, `report.j2`, `report.stpl`, `report.txt` (lower priority)

The `.stpl` extension is for SimpleEngine templates. See [Template Engines](template-engines.md) for details.

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
use standout_render::{render_with_vars, ColorPolicy, Representation};
use std::collections::HashMap;

let mut vars = HashMap::new();
vars.insert("version", "1.0.0");
vars.insert("app_name", "MyApp");

let output = render_with_vars(
    "{{ app_name }} v{{ version }}: {{ message }}",
    &data,
    &theme,
    Representation::Human,
    ColorPolicy::Always,
    vars,
)?;
```

When handler data and context variables have the same key, **handler data wins**. Context is supplementary.

---

## Structured Output

For machine-readable output (JSON, YAML, CSV, NDJSON), templates are bypassed entirely:

```rust
use standout_render::{render_auto, ColorPolicy, Representation};

// The template is used for the human representation
// Data is serialized directly for Json/Yaml/Csv; Ndjson wraps it in a result entry
let output = render_auto(template, &data, &theme, Representation::Json, ColorPolicy::Auto)?;
```

| Mode | Behavior |
| ------ | ---------- |
| `Human` | Render template; color policy selects styled or plain output |
| `TermDebug` | Render template, keep style tags |
| `Json` | `serde_json::to_string_pretty(data)` |
| `Yaml` | `serde_yaml::to_string(data)` |
| `Csv` | One row per flat record; a nested value is a render error |
| `Ndjson` | One compact line, `{"type":"result","data":…}` |

This means your serializable data types automatically support structured output without additional code.

---

## Validation

Check templates for unknown style tags before deploying:

```rust
use standout_render::validate_template;

validate_template(template, &sample_data, &theme)?;
```

The error lists unknown or unbalanced authored tags. Brackets in ordinary
values are literal and produce no style diagnostics.

---

## API Reference

### Render Functions

```rust
use standout_render::{
    ColorPolicy, Representation,
    render,                  // Basic: template + data + theme
    render_with_output,      // With explicit output mode
    render_with_mode,        // With representation + color policy + color mode
    render_with_vars,        // With extra context variables
    render_auto,             // Auto-dispatch template vs serialize
    render_auto_with_context,
};

// Basic
let output = render(template, &data, &theme)?;

// With a representation and a color policy
let output = render_with_output(template, &data, &theme, Representation::Human, ColorPolicy::Always)?;

// With color mode override (for testing)
let output = render_with_mode(
    template,
    &data,
    &theme,
    Representation::Human,
    ColorPolicy::Always,
    ColorMode::Dark,
)?;

// Auto (template for the human representation, serialize for structured)
let output = render_auto(template, &data, &theme, Representation::Json, ColorPolicy::Auto)?;
```

### Renderer Struct

```rust
use standout_render::{ColorPolicy, Renderer};

let mut renderer = Renderer::new(theme)?;
renderer.add_template("name", "content")?;
renderer.add_template_dir("./templates")?;

let output = renderer.render("name", &data)?;
renderer.set_color_policy(ColorPolicy::Never);
let output = renderer.render("name", &data)?;
```
