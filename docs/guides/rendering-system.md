# The Rendering System

Outstanding's rendering layer separates presentation from logic by using a two-pass architecture. This allows you to use standard tools (MiniJinja) for structure while keeping styling strictly separated and easy to debug.

Instead of mixing ANSI codes into your logic or templates, you define *what* something is (semantic tags like `[error]`) and let the theme decide *how* it looks.


## Two-Pass Rendering

Templates are processed in two distinct passes:

**Pass 1 - MiniJinja**: Standard template processing. Variables are substituted, control flow executes, filters apply.

**Pass 2 - BBParser**: Style tag processing. Bracket-notation tags are converted to ANSI escape codes (or stripped, depending on output mode).

```
Template:     [title]{{ name }}[/title] has {{ count }} items
Data:         { name: "Report", count: 42 }

After Pass 1: [title]Report[/title] has 42 items
After Pass 2: \x1b[1;32mReport\x1b[0m has 42 items
```

This separation means:
- Template logic (loops, conditionals) is handled by MiniJinja—a mature, well-documented engine
- Style application is a simple, predictable transformation
- You can debug each pass independently

## Style Tags

Style tags use BBCode-like bracket notation:

```
[style-name]content to style[/style-name]
```

The style-name must match a style defined in the theme. Tags can:
- Nest: `[outer][inner]text[/inner][/outer]`
- Span multiple lines
- Contain template logic: `[title]{% if x %}{{ x }}{% endif %}[/title]`

The tag syntax was chosen over Jinja filters because it reads naturally and doesn't interfere with Jinja's own syntax.

### What Happens Based on OutputMode

- **Term**: Tags replaced with ANSI escape codes
- **Text**: Tags stripped, plain text remains
- **TermDebug**: Tags kept as literals (`[name]...[/name]`) for debugging
- **Structured** (JSON, etc.): Template not used—data serializes directly

### Unknown Style Tags

### Unknown Style Tags

When a tag references a style not in the theme, Outstanding prioritizes developer visibility without crashing production apps.

- **Term mode**: Unknown tags get a `?` marker: `[unknown?]text[/unknown?]`

- **Text mode**: Tags stripped like any other
- **TermDebug mode**: Tags preserved as-is

The `?` marker helps catch typos during development. For production, use `validate_template()` at startup:

```rust
let result = validate_template(template, &sample_data, &theme);
if let Err(e) = result {
    eprintln!("Template errors: {}", e);
    std::process::exit(1);
}
```

## Themes and Styles

## Themes and Styles

A `Theme` is a named collection of styles mapping style names to console formatting.

See [App Configuration](app-configuration.md) for how to embed and load themes.


### Programmatic Themes

```rust
let theme = Theme::new()
    .add("title", Style::new().bold().cyan())
    .add("muted", Style::new().dim())
    .add("error", Style::new().red().bold())
    .add("disabled", "muted");  // Alias
```

### YAML Stylesheets

For file-based configuration:

```yaml
# Full attribute form
header:
  fg: cyan
  bold: true

# Shorthand
accent: cyan
emphasis: bold
warning: "yellow bold"

# Alias (references another style)
disabled: muted

# Adaptive (different in light/dark mode)
panel:
  fg: gray
  bold: true
  light:
    fg: black
  dark:
    fg: white
```

### CSS Stylesheets

If you prefer standard CSS syntax over YAML, Outstanding supports a subset of CSS Level 3 tailored for terminals:

```css
/* Selectors map to style names */
.panel {
  color: gray;
  font-weight: bold;
}

/* Shortcuts work as expected */
.error { color: red; font-weight: bold; }

/* Adaptive styles via media queries */
@media (prefers-color-scheme: light) {
  .panel { color: black; }
}

@media (prefers-color-scheme: dark) {
  .panel { color: white; }
}
```

This is ideal for developers who want to leverage existing knowledge and tooling (syntax highlighting, linters) for their CLI themes.


### Supported Attributes

Colors: `fg`, `bg`

Text attributes: `bold`, `dim`, `italic`, `underline`, `blink`, `reverse`, `hidden`, `strikethrough`

### Color Formats

```yaml
fg: red                  # Named (16 ANSI colors)
fg: bright_green         # Bright variants
fg: 208                  # 256-color palette
fg: "#ff6b35"            # RGB hex
fg: [255, 107, 53]       # RGB array
```

### Style Aliasing

Aliases let semantic names in templates resolve to visual styles:

```yaml
title:
  fg: cyan
  bold: true
commit-message: title    # Alias
section-header: title    # Another alias
```

Benefits:
- Templates use meaningful names (`[commit-message]`)
- Change one definition, update all aliases
- Styling stays flexible without template changes

Aliases can chain (`a` → `b` → `c` → concrete style). Cycles are detected and rejected.

### Adaptive Styles

Themes can respond to the OS light/dark mode:

```rust
theme.add_adaptive(
    "panel",
    Style::new().bold(),                    // base (shared)
    Some(Style::new().fg(Color::Black)),    // light mode override
    Some(Style::new().fg(Color::White)),    // dark mode override
)
```

In YAML:

```yaml
panel:
  bold: true
  light:
    fg: black
  dark:
    fg: white
```

The base provides shared attributes. Mode-specific overrides merge with base—`Some` replaces, `None` preserves.

Outstanding auto-detects the OS color scheme. Override for testing:

```rust
set_theme_detector(|| ColorMode::Dark);
```

## Template Filters

Beyond MiniJinja's built-ins, Outstanding adds formatting filters:

### Column Formatting

```jinja
{{ value | col(10) }}
{{ value | col(20, align='right') }}
{{ value | col(15, truncate='middle', ellipsis='...') }}
```

Arguments: `width`, `align` (left/right/center), `truncate` (end/start/middle), `ellipsis`

### Padding

```jinja
{{ "42" | pad_left(8) }}     {# "      42" #}
{{ "hi" | pad_right(8) }}    {# "hi      " #}
{{ "hi" | pad_center(8) }}   {# "   hi   " #}
```

### Truncation

```jinja
{{ long_text | truncate_at(20) }}
{{ path | truncate_at(30, 'middle', '...') }}
```

### Display Width

```jinja
{% if value | display_width > 20 %}...{% endif %}
```

Returns visual width (handles Unicode—CJK characters count as 2).

## Context Injection

Context injection adds values to the template beyond handler data.

### Static Context

Fixed values set at configuration time:

```rust
App::builder()
    .context("version", "1.0.0")
    .context("app_name", "MyApp")
```

### Dynamic Context

Computed at render time:

```rust
App::builder()
    .context_fn("terminal_width", |ctx| {
        Value::from(ctx.terminal_width.unwrap_or(80))
    })
    .context_fn("is_color", |ctx| {
        Value::from(ctx.output_mode.should_use_color())
    })
```

In templates:

```jinja
{{ app_name }} v{{ version }}
{% if terminal_width > 100 %}...{% endif %}
```

When handler data and context have the same key, **handler data wins**. Context is supplementary, not an override mechanism.

## Structured Output Modes

Structured modes (Json, Yaml, Xml, Csv) bypass template rendering entirely:

```
OutputMode::Json  → serde_json::to_string_pretty(data)
OutputMode::Yaml  → serde_yaml::to_string(data)
OutputMode::Xml   → quick_xml::se::to_string(data)
OutputMode::Csv   → flatten and format as CSV
```

This means:
- Template content is ignored
- Style tags never apply
- Context injection is skipped
- What you serialize is what you get

Same handler code, same data types—just different output format based on `--output`.

## Render Functions

For using the rendering layer without CLI integration:

### Basic Rendering

```rust
use outstanding::{render, Theme};

let theme = Theme::new().add("ok", Style::new().green());

let output = render(
    "[ok]{{ message }}[/ok]",
    &Data { message: "Success".into() },
    &theme,
)?;
```

### With Output Mode

```rust
use outstanding::{render_with_output, OutputMode};

// Honor --output flag value
let output = render_with_output(template, &data, &theme, OutputMode::Text)?;
```

### With Extra Variables

```rust
use outstanding::{render_with_vars, OutputMode};
use std::collections::HashMap;

// Inject simple key-value pairs into the template context
let mut vars = HashMap::new();
vars.insert("version", "1.0.0");

let output = render_with_vars(
    "{{ name }} v{{ version }}",
    &data, &theme, OutputMode::Text, vars,
)?;
```

### Auto-Dispatch (Template vs Serialize)

```rust
use outstanding::render_auto;

// For Term/Text: renders template
// For Json/Yaml/etc: serializes data directly
let output = render_auto(template, &data, &theme, OutputMode::Json)?;
```

### Full Control

```rust
use outstanding::{render_with_mode, ColorMode};

// Explicit output mode AND color mode (for tests)
let output = render_with_mode(
    template, &data, &theme,
    OutputMode::Term,
    ColorMode::Dark,
)?;
```

## Rendering Prelude

For convenient imports when using the rendering layer standalone:

```rust
use outstanding::rendering::prelude::*;

let theme = Theme::new()
    .add("title", Style::new().bold());

let output = render("[title]{{ name }}[/title]", &data, &theme)?;
```

The prelude includes: `render`, `render_auto`, `render_with_output`, `render_with_mode`, `render_with_vars`, `Theme`, `ColorMode`, `OutputMode`, `Renderer`, and `Style`.

## Hot Reloading

In debug builds, file-based templates are re-read from disk on each render. Edit templates without recompiling.

In release builds, templates are cached after first load.

This is automatic—no configuration needed. The `Renderer` struct tracks which templates are inline vs file-based and handles them appropriately.

## Template Registry

Templates are resolved by name with priority:

1. **Inline templates** (added via `add_template()`)
2. **Embedded templates** (from `embed_templates!`)
3. **File templates** (from `.templates_dir()`)

Supported extensions (in priority order): `.jinja`, `.jinja2`, `.j2`, `.txt`

When you request `"config"`, the registry checks:
- Inline template named `"config"`
- `config.jinja` in registered directories
- `config.jinja2`, `config.j2`, `config.txt` (lower priority)

Cross-directory collisions (same name in multiple dirs) raise an error. Same-directory collisions use extension priority.
