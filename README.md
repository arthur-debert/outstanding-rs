# outstanding

Styled CLI template rendering with automatic terminal detection.

Outstanding lets you render rich CLI output from templates while keeping all
presentation details (colors, bold, underline, layout) outside of your
application logic. It layers [minijinja](https://docs.rs/minijinja) (templates)
with the [console](https://docs.rs/console) crate (terminal styling) and handles:

- Clean templates: no inline `\x1b` escape codes
- Shared style definitions across multiple templates
- Automatic detection of terminal capabilities (TTY vs. pipes, `CLICOLOR`, etc.)
- Optional light/dark mode via `AdaptiveTheme`
- RGB helpers that convert `#rrggbb` values to the nearest ANSI color

## Installation

```toml
[dependencies]
outstanding = "0.2"
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
{{ title | style("title") }}
---------------------------
Total items: {{ total | style("count") }}
"#;

let output = render(
    template,
    &Summary { title: "Report".into(), total: 3 },
    ThemeChoice::from(&theme),
).unwrap();
println!("{}", output);
```

## Concepts

- **Theme**: Named collection of `console::Style` values (e.g., `"header"` → bold cyan)
- **AdaptiveTheme**: Pair of themes (light/dark) with OS detection (powered by `dark-light`)
- **ThemeChoice**: Pass either a theme or an adaptive theme to `render`
- **style filter**: `{{ value | style("name") }}` inside templates applies the registered style
- **Renderer**: Compile templates ahead of time if you render them repeatedly

## Adaptive Themes (Light & Dark)

```rust
use outstanding::{AdaptiveTheme, Theme, ThemeChoice};
use console::Style;

let light = Theme::new().add("tone", Style::new().green());
let dark  = Theme::new().add("tone", Style::new().yellow().italic());
let adaptive = AdaptiveTheme::new(light, dark);

// Automatically renders with the user's OS theme
let banner = outstanding::render_with_color(
    r#"Mode: {{ "active" | style("tone") }}"#,
    &serde_json::json!({}),
    ThemeChoice::Adaptive(&adaptive),
    true,
).unwrap();
```

## Pre-compiled Templates with Renderer

```rust
use outstanding::{Renderer, Theme};
use console::Style;
use serde::Serialize;

#[derive(Serialize)]
struct Entry { label: String, value: i32 }

let theme = Theme::new()
    .add("label", Style::new().bold())
    .add("value", Style::new().green());

let mut renderer = Renderer::new(theme);
renderer.add_template("row", r#"{{ label | style("label") }}: {{ value | style("value") }}"#).unwrap();

let rendered = renderer.render("row", &Entry { label: "Count".into(), value: 42 }).unwrap();
```

## Honoring --no-color Flags

```rust
use clap::Parser;
use outstanding::{render_with_color, Theme, ThemeChoice};

#[derive(Parser)]
struct Cli {
    #[arg(long)]
    no_color: bool,
}

let cli = Cli::parse();
let output = render_with_color(
    template,
    &data,
    ThemeChoice::from(&theme),
    !cli.no_color,  // explicit color control
).unwrap();
```

## License

MIT
