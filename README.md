# Standout

[![Crates.io](https://img.shields.io/crates/v/standout.svg)](https://crates.io/crates/standout)
[![Documentation](https://img.shields.io/badge/docs-book-blue)](https://arthur-debert.github.io/standout/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**Test your data. Render your view.**

Standout is a CLI framework for Rust that enforces separation between logic and presentation. Your handlers return structs, not strings—making CLI logic as testable as any other code.

## The Problem

CLI code that mixes logic with `println!` statements is impossible to unit test:

```rust
fn list_command(show_all: bool) {
    let todos = storage::list().unwrap();
    println!("Your Todos:");
    for todo in todos.iter() {
        if show_all || todo.status == Status::Pending {
            println!("  {} {}", if todo.done { "[x]" } else { "[ ]" }, todo.title);
        }
    }
}
```

## The Solution

With Standout, handlers return data. The framework handles rendering:

```rust
fn list_handler(matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<TodoResult> {
    let show_all = matches.get_flag("all");
    let todos = storage::list()?
        .into_iter()
        .filter(|t| show_all || t.status == Status::Pending)
        .collect();
    Ok(Output::Render(TodoResult { todos }))
}

#[test]
fn test_list_filters_completed() {
    let result = list_handler(&matches, &ctx).unwrap();
    assert!(result.todos.iter().all(|t| t.status == Status::Pending));
}
```

Because your logic returns a struct, you test the struct. No stdout capture, no regex, no brittleness.

## Features

- **Testable by design** — Handlers return data; framework handles rendering
- **Multiple output modes** — Rich terminal, JSON, YAML, CSV from the same handler
- **MiniJinja templates** — Familiar syntax with partials, filters, and hot reload
- **CSS/YAML styling** — Semantic styles with light/dark mode support
- **Tabular layouts** — Declarative columns with alignment, truncation, wrapping
- **Clap integration** — Automatic dispatch via derive macros
- **Incremental adoption** — Migrate one command at a time

## Installation

```bash
cargo add standout
```

## Quick Example

```rust
use standout::cli::{App, Dispatch, CommandContext, HandlerResult, Output};
use standout::{embed_templates, embed_styles};

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
pub enum Commands {
    List,
}

let app = App::builder()
    .commands(Commands::dispatch_config())
    .templates(embed_templates!("src/templates"))
    .styles(embed_styles!("src/styles"))
    .build()?;

app.run(Cli::command(), std::env::args());
```

```bash
myapp list                  # Rich terminal output
myapp list --output json    # JSON for scripting
```

## Documentation

You can find comprehensive documentation in our book: **[arthur-debert.github.io/standout](https://arthur-debert.github.io/standout/)**

- [Introduction to Standout](https://arthur-debert.github.io/standout/guides/intro-to-standout.html) — Start here
- [Rendering System](https://arthur-debert.github.io/standout/topics/rendering-system.html) — Templates and styles
- [Tabular Layouts](https://arthur-debert.github.io/standout/topics/tabular.html) — Tables and alignment
- [All Topics](https://arthur-debert.github.io/standout/topics/index.html) — Complete reference

## Contributing

Contributions welcome. Use the [issue tracker](https://github.com/arthur-debert/standout/issues) for bugs and feature requests.

## License

MIT
