# Standout

A CLI framework for Rust that enforces separation between logic and presentation.

**Test your data. Render your view.**

```rust
use standout::cli::{CommandContext, Output};
use standout::handler;
use serde::Serialize;

#[derive(Serialize)]
struct ListResult { items: Vec<String>, total: usize }

#[handler]
fn list(#[ctx] _ctx: &CommandContext) -> Result<Output<ListResult>, anyhow::Error> {
    let items = storage::list()?;
    Ok(Output::Render(ListResult { total: items.len(), items }))
}

// Test the handler directly—no stdout capture needed
#[test]
fn test_list() {
    let Output::Render(result) = list(&ctx).unwrap() else {
        panic!("expected rendered data");
    };
    assert_eq!(result.total, 3);
}
```

In a full application, `storage` and application behavior belong in a
CLI-free library. The handler belongs in the binary package and adapts library
results into CLI-owned serializable view models. See the
[production-shaped example](https://standout.magik.works/guides/production-shaped-example.html).

## What is Standout?

Standout combines two standalone libraries into a cohesive framework:

- **[standout-dispatch](https://crates.io/crates/standout-dispatch)** — Execution pattern where handlers return data, renderers produce output
- **[standout-render](https://crates.io/crates/standout-render)** — Terminal rendering with templates, themes, and adaptive styles

The framework provides the glue: clap integration, `--output` flag handling, auto-dispatch from derive macros, questionnaire-backed command surfaces, and the `AppBuilder` configuration API.

## Why Standout?

CLI code that mixes logic with `println!` is impossible to unit test. With Standout:

- **Handlers return structs**, not strings—test them like any other function
- **Multiple output modes** from the same handler: rich terminal, JSON, YAML, CSV
- **MiniJinja templates** with hot reload during development
- **CSS/YAML themes** with automatic light/dark mode support
- **Incremental adoption**—migrate one command at a time

## Quick Start

```toml
[dependencies]
standout = "9"
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
anyhow = "1"
```

The one `standout` dependency carries the macros; `standout-dispatch` and
`standout-render` are only needed by a project that uses them without the
framework.

```rust
use clap::{CommandFactory, Parser, Subcommand};
use serde::Serialize;
use standout::cli::{App, CommandContext, Dispatch, Output};
use standout::{embed_styles, embed_templates, handler};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum Commands {
    #[dispatch(pure)]
    List,
}

#[derive(Serialize)]
struct ListView {
    items: Vec<String>,
}

mod handlers {
    use super::*;

    #[handler]
    pub fn list(#[ctx] _ctx: &CommandContext) -> Result<Output<ListView>, anyhow::Error> {
        Ok(Output::Render(ListView {
            items: vec!["item-1".into(), "item-2".into()],
        }))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = App::builder()
        .version(env!("CARGO_PKG_VERSION"))
        .templates(embed_templates!("src/templates"))
        .styles(embed_styles!("src/styles"))
        .default_theme("default")
        .commands(Commands::dispatch_config())?
        .build()?;

    app.run(Cli::command(), std::env::args());
    Ok(())
}
```

The `List` variant registers the command under its kebab-case name, `list`;
`#[dispatch(pure)]` points it at the wrapper `#[handler]` generates,
`handlers::list__handler`; and with no template named, the convention renders
`src/templates/list.jinja`. Themed help is on by default.

```bash
myapp list                  # Rich terminal output
myapp list --output json    # JSON for scripting
```

## Documentation

- **Book**: [standout.magik.works](https://standout.magik.works/)

### Framework Topics

- [App Configuration](https://standout.magik.works/topics/app-configuration.html) — AppBuilder API
- [Derived Questionnaires](https://standout.magik.works/guides/derived-questionnaires.html) — Typed answer sheets and questionnaire command wiring
- [Output Modes](https://standout.magik.works/topics/output-modes.html) — --output flag and format handling

### Crate Documentation

- [standout-render](https://standout.magik.works/crates/render/guides/intro-to-rendering.html) — Templates, themes, tabular layouts
- [standout-dispatch](https://standout.magik.works/crates/dispatch/guides/intro-to-dispatch.html) — Handlers, hooks, command routing

### API Reference

- [API Documentation](https://docs.rs/standout) — Full API reference

## Standalone Crates

Each component can be used independently:

- **[standout-render](https://crates.io/crates/standout-render)** — Use the rendering system without the framework
- **[standout-dispatch](https://crates.io/crates/standout-dispatch)** — Use the execution pattern with your own renderer

## License

MIT
