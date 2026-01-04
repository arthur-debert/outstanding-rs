# outstanding

Outstanding is shell rendering library that allows your to deveop your application to be shell agnostic, being unit tested and easier to write and maintain. Likewise it decouples the rendetring from the model, giving you a interface that is easier to fine tune and update.

We've been pretty good at not mixing arg parsing and application logic for a while, with greate libs like clasp. Thankfully, you
won't see a logic three modules later thatn program execution parsing an ad hoc option from the input string.  That can't be said about the output, commonly integrmingled with logic, with prints to std out or std mid program and premature convertion of data types to strings.  This makes programs hard to test, maintain and design.

**Outstanding** is a library for rendering your application into terminal, be ir plain tech, richer formatting or textual or binary data that helps isolate logic and presentation. It support templates strings, template files and style sheets and is smart about gracefully degrading output to plain text when needed.

![alt text](assets/architecture.svg)

## Installation

```toml
[dependencies]
outstanding = "0.2.2"
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
