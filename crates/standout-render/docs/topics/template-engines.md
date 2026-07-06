# Template Engines

`standout-render` uses a pluggable template engine architecture. While MiniJinja is the default (and recommended for most users), you can choose a lighter engine or implement your own.

---

## Available Engines

| Engine            | Syntax      | Features                               | Binary Size | Use When                             |
| ----------------- | ----------- | -------------------------------------- | ----------- | ------------------------------------ |
| `MiniJinjaEngine` | `{{ var }}` | Loops, conditionals, filters, includes | ~248KB      | Full template logic needed (default) |
| `SimpleEngine`    | `{var}`     | Variable substitution only             | ~5KB        | Simple output, minimal binary size   |

### Feature Comparison

| Feature                       | MiniJinjaEngine                | SimpleEngine       |
| ----------------------------- | ------------------------------ | ------------------ |
| Variable substitution         | `{{ name }}`                   | `{name}`           |
| Nested property access        | `{{ user.name }}`              | `{user.name}`      |
| Array index access            | `{{ items[0] }}`               | `{items.0}`        |
| Filters                       | `{{ name \| upper }}`          | -                  |
| Conditionals                  | `{% if %}...{% endif %}`       | -                  |
| Loops                         | `{% for %}...{% endfor %}`     | -                  |
| Template includes             | `{% include "file" %}`         | -                  |
| Macros                        | `{% macro %}...{% endmacro %}` | -                  |
| Comments                      | `{# comment #}`                | -                  |
| Escaped delimiters            | `{{ "{{" }}`                   | `{{` → `{`         |
| Context injection             | Yes                            | Yes                |
| Named templates               | Yes                            | Yes                |
| Style tags                    | Yes (pass-through)             | Yes (pass-through) |
| Hot reload                    | Yes                            | Yes                |
| Structured output (JSON/YAML) | Yes                            | Yes                |

### MiniJinjaEngine (Default)

Full-featured Jinja2-compatible engine. This is what you get by default.

```jinja
[title]{{ name | upper }}[/title]
{% for item in items %}
  {{ loop.index }}. {{ item.name }}
{% endfor %}
```

**Supports:**

- Variable substitution with filters: `{{ name | upper }}`
- Control flow: `{% if %}`, `{% for %}`, `{% macro %}`
- Template includes: `{% include "partial.jinja" %}`
- Custom filters and functions

**File extensions:** `.jinja`, `.jinja2`, `.j2`

### SimpleEngine

Lightweight engine using format-string style syntax. No loops, conditionals, or filters.

```text
[title]{name}[/title]
Status: {status}
Contact: {user.profile.email}
```

**Supports:**

- Simple variable substitution: `{name}`
- Nested property access: `{user.profile.email}`
- Array index access: `{items.0}`
- Escaped braces: `{{` renders as `{`

**Does NOT support:**

- Loops (`{% for %}`)
- Conditionals (`{% if %}`)
- Filters (`| upper`)
- Template includes

**File extension:** `.stpl`

---

## Choosing an Engine

### Use MiniJinjaEngine (default) when

- Templates need loops or conditionals
- You use filters for formatting
- Templates include other templates
- You're not concerned about binary size

### Use SimpleEngine when

- Templates only substitute variables
- Binary size is critical
- You want faster parsing (no template compilation)
- Templates are simple status messages or one-liners

---

## Using SimpleEngine

### With Renderer

```rust
use standout_render::{Renderer, Theme, OutputMode};
use standout_render::template::SimpleEngine;

let engine = Box::new(SimpleEngine::new());
let mut renderer = Renderer::with_output_and_engine(
    Theme::new(),
    OutputMode::Auto,
    engine,
)?;

renderer.add_template("status", "Status: {status}, Count: {count}")?;

let output = renderer.render("status", &data)?;
```

### With render_auto_with_engine

```rust
use standout_render::{render_auto_with_engine, Theme, OutputMode};
use standout_render::template::SimpleEngine;
use standout_render::context::{ContextRegistry, RenderContext};

let engine = SimpleEngine::new();
let theme = Theme::new();
let data = serde_json::json!({"name": "World"});

let registry = ContextRegistry::new();
let render_ctx = RenderContext::new(OutputMode::Text, Some(80), &theme, &data);

let output = render_auto_with_engine(
    &engine,
    "Hello, {name}!",
    &data,
    &theme,
    OutputMode::Text,
    &registry,
    &render_ctx,
)?;
```

---

## File Extension Mapping

When loading templates from files, the extension determines the intended engine:

| Priority | Extension | Engine          |
| -------- | --------- | --------------- |
| 1        | `.jinja`  | MiniJinjaEngine |
| 2        | `.jinja2` | MiniJinjaEngine |
| 3        | `.j2`     | MiniJinjaEngine |
| 4        | `.stpl`   | SimpleEngine    |
| 5        | `.txt`    | (generic)       |

When multiple files share the same base name, higher-priority extensions win for extensionless lookups.

**Note:** The registry resolves templates by name, but doesn't automatically select the engine. You must configure the appropriate engine when creating the `Renderer`.

---

## Implementing a Custom Engine

To create your own template engine, implement the `TemplateEngine` trait:

```rust
use standout_render::template::TemplateEngine;
use standout_render::RenderError;
use std::collections::HashMap;

pub struct MyEngine {
    templates: HashMap<String, String>,
}

impl TemplateEngine for MyEngine {
    fn render_template(
        &self,
        template: &str,
        data: &serde_json::Value,
    ) -> Result<String, RenderError> {
        // Your rendering logic here
        Ok(format!("Rendered: {}", template))
    }

    fn add_template(&mut self, name: &str, source: &str) -> Result<(), RenderError> {
        self.templates.insert(name.to_string(), source.to_string());
        Ok(())
    }

    fn render_named(
        &self,
        name: &str,
        data: &serde_json::Value,
    ) -> Result<String, RenderError> {
        let template = self.templates.get(name)
            .ok_or_else(|| RenderError::TemplateNotFound(name.to_string()))?;
        self.render_template(template, data)
    }

    fn has_template(&self, name: &str) -> bool {
        self.templates.contains_key(name)
    }

    fn render_with_context(
        &self,
        template: &str,
        data: &serde_json::Value,
        context: HashMap<String, serde_json::Value>,
    ) -> Result<String, RenderError> {
        // Merge context with data and render
        self.render_template(template, data)
    }

    fn supports_includes(&self) -> bool { false }
    fn supports_filters(&self) -> bool { false }
    fn supports_control_flow(&self) -> bool { false }
}
```

### Trait Methods

| Method                | Purpose                                  |
| --------------------- | ---------------------------------------- |
| `render_template`     | Render an inline template string         |
| `add_template`        | Register a named template                |
| `render_named`        | Render a previously registered template  |
| `has_template`        | Check if a template exists               |
| `render_with_context` | Render with additional context variables |
| `supports_*`          | Feature flags for capability discovery   |

---

## API Reference

### Engine Types

```rust
use standout_render::template::{
    TemplateEngine,      // Trait for all engines
    MiniJinjaEngine,     // Default, full-featured
    SimpleEngine,        // Lightweight alternative
};
```

### Renderer with Custom Engine

```rust
use standout_render::{Renderer, Theme, OutputMode};

// Default (MiniJinja)
let renderer = Renderer::new(theme)?;

// With explicit engine
let engine = Box::new(SimpleEngine::new());
let renderer = Renderer::with_output_and_engine(theme, mode, engine)?;
```

### Standalone Rendering

```rust
use standout_render::{
    render_auto_with_engine,  // Render with custom engine
};
```

---

## Migration Notes

If you're upgrading from a version before pluggable engines:

- **No changes required** - MiniJinja remains the default
- **Error type changed** - `minijinja::Error` is now `RenderError`
- **New capability** - You can now inject custom engines via `with_output_and_engine()`

```rust
// Before
use minijinja::Error;

// After
use standout_render::RenderError;
```
