# How To: Use Only the Rendering Layer

Outstanding's rendering layer is fully decoupled from its CLI integration (App, Clap, Dispatch). This means you can use the template engine, theme system, and structured output logic in any context—servers, TUI apps, or even other CLI frameworks.

This decoupling allows you to maintain consistent styling and logic across different parts of your ecosystem.

## When to Use This

- Adding styled output to an existing application
- Building a library that produces formatted terminal output
- Server-side rendering of CLI-style output
- Testing templates in isolation

## Basic Rendering

The simplest approach—auto-detect terminal capabilities:

```rust
use outstanding::{render, Theme};
use console::Style;
use serde::Serialize;

#[derive(Serialize)]
struct Report {
    title: String,
    items: Vec<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let theme = Theme::new()
        .add("title", Style::new().bold().cyan())
        .add("item", Style::new().green());

    let data = Report {
        title: "Status Report".into(),
        items: vec!["Task A: complete".into(), "Task B: pending".into()],
    };

    let output = render(
        r#"[title]{{ title }}[/title]
{% for item in items %}
  [item]•[/item] {{ item }}
{% endfor %}"#,
        &data,
        &theme,
    )?;

    println!("{}", output);
    Ok(())
}
```

## Explicit Output Mode

Control ANSI code generation:

```rust
use outstanding::{render_with_output, OutputMode};

// Force ANSI codes (even when piping)
let colored = render_with_output(template, &data, &theme, OutputMode::Term)?;

// Force plain text (no ANSI codes)
let plain = render_with_output(template, &data, &theme, OutputMode::Text)?;

// Debug mode (tags as literals)
let debug = render_with_output(template, &data, &theme, OutputMode::TermDebug)?;
```

## Auto-Dispatch: Template vs Serialization

`render_auto` chooses between template rendering and direct serialization:

```rust
use outstanding::render_auto;

fn format_output(data: &Report, mode: OutputMode) -> Result<String, Error> {
    render_auto(template, data, &theme, mode)
}

// Term/Text/Auto: renders template
format_output(&data, OutputMode::Term)?;

// Json/Yaml/Xml/Csv: serializes data directly
format_output(&data, OutputMode::Json)?;
```

Same function, same data—output format determined by mode.

## Full Control: Output Mode + Color Mode

For tests or when forcing specific behavior:

```rust
use outstanding::{render_with_mode, ColorMode};

// Force dark mode styling
let dark = render_with_mode(
    template,
    &data,
    &theme,
    OutputMode::Term,
    ColorMode::Dark,
)?;

// Force light mode styling
let light = render_with_mode(
    template,
    &data,
    &theme,
    OutputMode::Term,
    ColorMode::Light,
)?;
```

## Building Themes Programmatically

No YAML files needed:

```rust
use outstanding::Theme;
use console::{Style, Color};

let theme = Theme::new()
    // Simple styles
    .add("bold", Style::new().bold())
    .add("muted", Style::new().dim())
    .add("error", Style::new().red().bold())

    // With specific colors
    .add("info", Style::new().fg(Color::Cyan))
    .add("warning", Style::new().fg(Color::Yellow))

    // Aliases
    .add("disabled", "muted")
    .add("inactive", "muted")

    .add_adaptive(
        "panel",
        Style::new().bold(),
        Some(Style::new().fg(Color::Black)),  // Light mode
        Some(Style::new().fg(Color::White)),  // Dark mode
    );
```

### Theme Merging

You can layer themes using `merge`. This is useful for user overrides:

```rust
let base_theme = Theme::from_file("base.yaml")?;
let user_overrides = Theme::from_file("user-config.yaml")?;

// User styles overwrite base styles
let final_theme = base_theme.merge(user_overrides);
```

## Pre-Compiled Renderer

For repeated rendering with the same templates:

```rust
use outstanding::Renderer;

let theme = Theme::new()
    .add("title", Style::new().bold());

let mut renderer = Renderer::new(theme)?;

// Register templates
renderer.add_template("header", "[title]{{ title }}[/title]")?;
renderer.add_template("item", "  - {{ name }}: {{ value }}")?;

// Render multiple times
for record in records {
    let header = renderer.render("header", &record)?;
    println!("{}", header);

    for item in &record.items {
        let line = renderer.render("item", item)?;
        println!("{}", line);
    }
}
```

## Loading Templates from Files

```rust
let mut renderer = Renderer::new(theme)?;

// Add directory of templates
renderer.add_template_dir("./templates")?;

// Templates resolved by name (without extension)
let output = renderer.render("report", &data)?;
```

In debug builds, file-based templates are re-read on each render (hot reload).

## Template Validation

Catch style tag errors without producing output:

```rust
use outstanding::validate_template;

let result = validate_template(template, &sample_data, &theme);
match result {
    Ok(()) => println!("Template is valid"),
    Err(e) => {
        eprintln!("Template errors: {}", e);
        std::process::exit(1);
    }
}
```

Use at startup or in tests to fail fast on typos.

## Context Injection

### Simple Variables with `render_with_vars`

For adding simple key-value pairs to the template context:

```rust
use outstanding::{render_with_vars, Theme, OutputMode};
use std::collections::HashMap;

let theme = Theme::new();

let mut vars = HashMap::new();
vars.insert("version", "1.0.0");
vars.insert("app_name", "MyApp");

let output = render_with_vars(
    "{{ name }} - {{ app_name }} v{{ version }}",
    &data,
    &theme,
    OutputMode::Text,
    vars,
)?;
```

This is the recommended approach for most use cases.

### Full Context System

For dynamic context computed at render time:

```rust
use outstanding::{render_with_context, Theme, OutputMode};
use outstanding::context::{ContextRegistry, RenderContext};
use minijinja::Value;

let mut context = ContextRegistry::new();
context.add_static("version", Value::from("1.0.0"));
context.add_provider("timestamp", |_ctx: &RenderContext| {
    Value::from(chrono::Utc::now().to_rfc3339())
});

let render_ctx = RenderContext::new(
    OutputMode::Term,
    Some(80),
    &theme,
    &serde_json::to_value(&data)?,
);

let output = render_with_context(
    template,
    &data,
    &theme,
    OutputMode::Term,
    &context,
    &render_ctx,
)?;
```

## Structured Output Without Templates

For JSON/YAML output, templates are bypassed:

```rust
use outstanding::render_auto;

#[derive(Serialize)]
struct ApiResponse {
    status: String,
    data: Vec<Item>,
}

let response = ApiResponse { ... };

// Direct JSON serialization
let json = render_auto("unused", &response, &theme, OutputMode::Json)?;
println!("{}", json);

// Direct YAML serialization
let yaml = render_auto("unused", &response, &theme, OutputMode::Yaml)?;
```

The template parameter is ignored for structured modes.

## Minimal Example

Absolute minimum for styled output:

```rust
use outstanding::{render, Theme};
use console::Style;

let theme = Theme::new().add("ok", Style::new().green());
let output = render("[ok]Success[/ok]", &(), &theme)?;
println!("{}", output);
```

No files, no configuration—just a theme and a template string.
