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

The framework provides the glue: clap integration, `--output` flag handling, auto-dispatch from derive macros, and the `AppBuilder` configuration API.

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
standout = "7"
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
```

```rust
use standout::cli::{App, Dispatch, CommandContext, HandlerResult, Output};
use standout::{embed_templates, embed_styles};
use clap::{ArgMatches, CommandFactory, Parser, Subcommand};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
pub enum Commands {
    List,
}

mod handlers {
    use super::*;

    pub fn list(_m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<Vec<String>> {
        Ok(Output::Render(vec!["item-1".into(), "item-2".into()]))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = App::builder()
        .commands(Commands::dispatch_config())?
        .templates(embed_templates!("src/templates"))
        .styles(embed_styles!("src/styles"))
        .build()?;

    app.run(Cli::command(), std::env::args());
    Ok(())
}
```

```bash
myapp list                  # Rich terminal output
myapp list --output json    # JSON for scripting
```

## Documentation

- **Book**: [standout.magik.works](https://standout.magik.works/)

### Framework Topics

- [App Configuration](https://standout.magik.works/topics/app-configuration.html) — AppBuilder API
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
