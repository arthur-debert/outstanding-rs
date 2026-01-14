# outstanding

A CLI output framework that decouples application logic from terminal presentation. Outstanding provides template rendering with styled output, automatic terminal detection, and structured output modes.

## Why Outstanding?

Modern CLI applications need to produce output for multiple contexts:
- Humans reading in terminals (with colors, formatting)
- Scripts parsing output (plain text or JSON)
- Piped content (no ANSI codes)

Outstanding solves this by separating **what** you output from **how** it's rendered:

```
Command Logic → Structured Data → Template + Theme → Terminal Output
                     ↓
              (OutputMode::Json)
                     ↓
              Structured Output (JSON)
```

## Installation

```toml
[dependencies]
outstanding = "0.5"
```

For clap integration, also add:
```toml
outstanding-clap = "0.5"
```

## Quick Start

```rust
use outstanding::{render, Theme, ThemeChoice};
use console::Style;
use serde::Serialize;

#[derive(Serialize)]
struct Summary {
    title: String,
    total: usize,
}

let theme = Theme::new()
    .add("title", Style::new().bold())
    .add("count", Style::new().cyan());

let template = r#"
[title]{{ title }}[/title]
---------------------------
Total items: [count]{{ total }}[/count]
"#;

let output = render(
    template,
    &Summary { title: "Report".into(), total: 3 },
    ThemeChoice::from(&theme),
).unwrap();
println!("{}", output);
```

## Core Features

### Output Modes

Control how output is rendered:

```rust
use outstanding::{render_with_output, OutputMode};

// Auto-detect terminal capabilities (default)
render_with_output(template, &data, theme, OutputMode::Auto)?;

// Force ANSI colors
render_with_output(template, &data, theme, OutputMode::Term)?;

// Plain text, no colors
render_with_output(template, &data, theme, OutputMode::Text)?;

// Debug mode: [style]text[/style]
render_with_output(template, &data, theme, OutputMode::TermDebug)?;

// JSON output (skips template, serializes data directly)
render_or_serialize(template, &data, theme, OutputMode::Json)?;
```

### Adaptive Themes

Support light and dark terminals with automatic OS detection:

```rust
use outstanding::{Theme, AdaptiveTheme, ThemeChoice};

let light = Theme::new().add("accent", Style::new().blue());
let dark = Theme::new().add("accent", Style::new().cyan());
let adaptive = AdaptiveTheme::new(light, dark);

render(template, &data, ThemeChoice::Adaptive(&adaptive))?;
```

### Style Aliasing

Create maintainable layered styles:

```rust
let theme = Theme::new()
    // Visual layer (concrete)
    .add("muted", Style::new().dim())
    .add("accent", Style::new().cyan().bold())
    // Semantic layer (aliases)
    .add("timestamp", "muted")
    .add("command_name", "accent");
```

### Pre-compiled Templates

For repeated rendering:

```rust
use outstanding::Renderer;

let mut renderer = Renderer::new(theme)?;
renderer.add_template("row", r#"{{ label }}: {{ value }}"#)?;

for entry in entries {
    let output = renderer.render("row", &entry)?;
    println!("{}", output);
}
```

## Integration with Clap

For clap-based CLIs, the `outstanding-clap` crate provides:

- Command handler registration with templates
- Automatic `--output` flag injection
- Help topics system
- Pager support

```rust
use outstanding_clap::{Outstanding, CommandResult};

Outstanding::builder()
    .command("list", |_m, _ctx| {
        CommandResult::Ok(serde_json::json!({"items": ["a", "b"]}))
    }, "{{ items | join(', ') }}")
    .run_and_print(cmd, std::env::args());
```

See the [outstanding-clap documentation](../outstanding-clap/docs/using-with-clap.md) for details.

## Documentation

- [Styling Guide](../../docs/styling.md) - Themes, style aliasing, adaptive themes
- [Templates Guide](../../docs/templates.md) - MiniJinja syntax, filters, data structures
- [Architecture & Design](../../docs/proposals/fullapi-consolidated.md) - Deep dive for contributors and advanced users

## License

MIT
