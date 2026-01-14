# outstanding

Outstanding is a non-interactive CLI output framework that decouples your application logic from terminal presentation.

## Features

- **Template rendering** with MiniJinja + styled output
- **Themes** for named style definitions (colors, bold, etc.)
- **Automatic terminal capability detection** (TTY, CLICOLOR, etc.)
- **Output mode control** (Auto/Term/Text/TermDebug)
- **Help topics system** for extended documentation
- **Pager support** for long content

This crate is **CLI-agnostic** - it doesn't care how you parse arguments.
For easy integration with clap, see the `outstanding-clap` crate.

## Installation

```toml
[dependencies]
outstanding = "0.3"

# For clap integration:
outstanding-clap = "0.3"
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

## Output Modes

Control how output is rendered with `OutputMode`:

- `Auto` - Detect terminal capabilities (default)
- `Term` - Always use ANSI colors/styles
- `Text` - Plain text, no ANSI codes
- `TermDebug` - Render styles as `[name]text[/name]` for debugging

```rust
use outstanding::{render_with_output, OutputMode};

let output = render_with_output(template, &data, theme, OutputMode::Text).unwrap();
```

## Help Topics

The topics module provides extended documentation for CLI apps:

```rust
use outstanding::topics::{Topic, TopicRegistry, TopicType, render_topic};

let mut registry = TopicRegistry::new();
registry.add_topic(Topic::new(
    "Storage",
    "Notes are stored in ~/.notes/",
    TopicType::Text,
    Some("storage".to_string()),
));

// Load topics from files
registry.add_from_directory_if_exists("docs/topics").ok();

// Render a topic
if let Some(topic) = registry.get_topic("storage") {
    let output = render_topic(topic, None).unwrap();
    println!("{}", output);
}
```

## Clap Integration

For clap-based CLIs, use `outstanding-clap`:

```rust
use clap::Command;
use outstanding_clap::Outstanding;

// Simplest usage - all features enabled
let matches = Outstanding::run(Command::new("my-app"));

// With topics
let matches = Outstanding::builder()
    .topics_dir("docs/topics")
    .run(Command::new("my-app"));
```

See the [outstanding-clap README](crates/outstanding-clap/README.md) for more details.

## Documentation

- [Styling Guide](docs/styling.md) - Themes, style aliasing, adaptive themes, output modes
- [Templates Guide](docs/templates.md) - MiniJinja syntax, filters, data structures

## License

MIT
