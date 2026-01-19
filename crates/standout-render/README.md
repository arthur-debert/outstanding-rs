# standout-render

Rich terminal output with templates, themes, and automatic light/dark mode support.

```rust
use standout_render::{render, Theme};
use console::Style;

let theme = Theme::new()
    .add("title", Style::new().cyan().bold())
    .add("count", Style::new().yellow());

let output = render(
    "[title]{{ name }}[/title]: [count]{{ count }}[/count] items",
    &json!({"name": "Tasks", "count": 42}),
    &theme,
)?;
```

## Why standout-render?

Terminal output is stuck in the 1970s: scattered `println!` statements, cryptic ANSI escape codes, and presentation logic tangled with business logic. Every change requires recompilation. Nobody bothers with polish because iteration is painful.

**standout-render** fixes this with ideas borrowed from web development:

- **Templates** — MiniJinja (Jinja2 syntax) for readable, declarative output
- **Style tags** — BBCode-like `[style]content[/style]` syntax, not escape codes
- **Themes** — Centralized style definitions in CSS or YAML
- **Hot reload** — Edit templates during development, see changes instantly
- **Graceful degradation** — Same template renders rich or plain based on terminal

The result: output that's easy to write, easy to change, and looks polished.

## Features

### Two-Pass Rendering Pipeline

```
Template + Data → MiniJinja → Text with style tags → BBParser → ANSI output
```

1. **Pass 1**: Variable substitution, loops, conditionals (MiniJinja)
2. **Pass 2**: Style tag replacement with ANSI codes (or stripping for plain text)

### Adaptive Themes

Automatic OS detection for light/dark terminals:

```yaml
# theme.yaml
title:
  fg: cyan
  bold: true
panel:
  light:
    bg: "#f5f5f5"
  dark:
    bg: "#1a1a1a"
```

Or CSS syntax:

```css
.title { color: cyan; font-weight: bold; }

@media (prefers-color-scheme: dark) {
    .panel { background: #1a1a1a; }
}
```

### Multiple Output Modes

One template, many formats:

```rust
// Rich terminal output
render_with_output(template, &data, &theme, OutputMode::Term)?;

// Plain text (pipes, redirects)
render_with_output(template, &data, &theme, OutputMode::Text)?;

// Structured data (skip template entirely)
render_auto(template, &data, &theme, OutputMode::Json)?;
render_auto(template, &data, &theme, OutputMode::Yaml)?;
```

### Tabular Layout

Sophisticated column formatting with world-class alignment and layout:

```jinja
{% set t = tabular([
    {"name": "id", "width": 8},
    {"name": "name", "width": "fill"},
    {"name": "status", "width": 10, "anchor": "right"}
], separator="  ") %}
{% for entry in entries %}
{{ t.row([entry.id, entry.name, entry.status]) }}
{% endfor %}
```

Features: flexible truncation (start/middle/end), expanding columns, word wrapping, multi-line row alignment, justification, variable width, fractional sizing—all Unicode-aware and ANSI-safe.

### Hot Reload

During development, templates reload from disk on each render:

```rust
let mut renderer = Renderer::new(theme)?;
renderer.add_template_dir("./templates")?;

// Edit templates/report.jinja, re-run, see changes immediately
let output = renderer.render("report", &data)?;
```

In release builds, templates embed into the binary—no runtime file access.

## Quick Start

```toml
[dependencies]
standout-render = "2.1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

```rust
use standout_render::{render, Theme};
use serde::Serialize;

#[derive(Serialize)]
struct Report { title: String, items: Vec<String> }

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let theme = Theme::from_yaml(r#"
        title: { fg: cyan, bold: true }
        item: green
        count: { dim: true }
    "#)?;

    let report = Report {
        title: "Status".into(),
        items: vec!["server-1".into(), "server-2".into()],
    };

    let template = r#"
[title]{{ title }}[/title]
{% for item in items %}
  [item]{{ item }}[/item]
{% endfor %}
[count]{{ items | length }} items[/count]
"#;

    let output = render(template, &report, &theme)?;
    print!("{}", output);
    Ok(())
}
```

## Documentation

### Guides
- [Introduction to Rendering](docs/guides/intro-to-rendering.md) — Complete rendering tutorial
- [Introduction to Tabular](docs/guides/intro-to-tabular.md) — Column layouts and tables

### Topics
- [Styling System](docs/topics/styling-system.md) — Themes, adaptive styles, CSS syntax
- [Templating](docs/topics/templating.md) — MiniJinja, style tags, processing modes
- [File System Resources](docs/topics/file-system-resources.md) — Hot reload, registries, embedding

### Reference
- [API Documentation](https://docs.rs/standout-render) — Full API reference

## Used By

This crate provides the rendering foundation for the [standout](https://crates.io/crates/standout) CLI framework, which adds command dispatch, hooks, and clap integration. Use `standout-render` directly when you want rendering without the framework.

## License

MIT
