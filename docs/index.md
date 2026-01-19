# Standout

**Test your data. Render your view.**

Standout is a CLI framework for Rust that enforces separation between logic and presentation. Your handlers return structs, not strings—making CLI logic as testable as any other code.

## The Problem

CLI code that mixes logic with `println!` statements is impossible to unit test:

```rust
// You can't unit test this—it writes directly to stdout
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

The only way to test this is regex on captured stdout. That's fragile, verbose, and couples your tests to presentation details.

## The Solution

With Standout, handlers return data. The framework handles rendering:

```rust
// This is unit-testable—it's a pure function that returns data
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
    let matches = /* mock ArgMatches with all=false */;
    let result = list_handler(&matches, &ctx).unwrap();
    assert!(result.todos.iter().all(|t| t.status == Status::Pending));
}
```

Because your logic returns a struct, you test the struct. No stdout capture, no regex, no brittleness.

## Standing Out

What Standout provides:

- Enforced architecture splitting data and presentation
- Logic is testable as any Rust code
- Boilerplateless: declaratively link your handlers to command names and templates, Standout handles the rest
- Autodispatch: save keystrokes with auto dispatch from the known command tree
- Free [output handling](topics/output-modes.md): rich terminal with graceful degradation, plus structured data (JSON, YAML, CSV)
- Finely crafted output:
  - File-based [templates](crates/render/topics/templating.md) for content and CSS for styling
  - Rich styling with [adaptive properties](crates/render/topics/styling-system.md) (light/dark modes), inheritance, and full theming
  - Powerful templating through [MiniJinja](https://github.com/mitsuhiko/minijinja), including partials (reusable, smaller templates for models displayed in multiple places)
  - [Hot reload](crates/render/topics/file-system-resources.md): changes to templates and styles don't require compiling
  - Declarative layout support for [tabular data](crates/render/guides/intro-to-tabular.md)

## Quick Start

### 1. Define Your Commands and Handlers

Use the `Dispatch` derive macro to connect commands to handlers. Handlers receive parsed arguments and return serializable data.

```rust
use standout::cli::{Dispatch, CommandContext, HandlerResult, Output};
use clap::{ArgMatches, Subcommand};
use serde::Serialize;

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]  // handlers are in the `handlers` module
pub enum Commands {
    List,
    Add { title: String },
}

#[derive(Serialize)]
struct TodoResult {
    todos: Vec<Todo>,
}

mod handlers {
    use super::*;

    // HandlerResult<T> wraps your data; Output::Render tells Standout to render it
    pub fn list(_m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<TodoResult> {
        let todos = storage::list()?;
        Ok(Output::Render(TodoResult { todos }))
    }

    pub fn add(m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<TodoResult> {
        let title: &String = m.get_one("title").unwrap();
        let todo = storage::add(title)?;
        Ok(Output::Render(TodoResult { todos: vec![todo] }))
    }
}
```

### 2. Define Your Presentation

Templates use MiniJinja with semantic style tags. Styles are defined separately in CSS or YAML.

```jinja
{# list.jinja #}
[title]My Todos[/title]
{% for todo in todos %}
  - {{ todo.title }} ([status]{{ todo.status }}[/status])
{% endfor %}
```

```css
/* styles/default.css */
.title { color: cyan; font-weight: bold; }
.status { color: yellow; }
```

### 3. Wire It Up

```rust
use standout::cli::App;
use standout::{embed_templates, embed_styles};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = App::builder()
        .commands(Commands::dispatch_config())  // Register handlers from derive macro
        .templates(embed_templates!("src/templates"))
        .styles(embed_styles!("src/styles"))
        .build()?;

    app.run(Cli::command(), std::env::args());
    Ok(())
}
```

Run it:

```bash
myapp list              # Rich terminal output with colors
myapp list --output json    # JSON for scripting
myapp list --output yaml    # YAML for config files
myapp list --output text    # Plain text, no ANSI codes
```

## Features

### Architecture

- Logic/presentation separation enforced by design
- Handlers return data; framework handles rendering
- Unit-testable CLI logic without stdout capture

### Output Modes

- Rich terminal output with colors and styles
- Automatic JSON, YAML, CSV serialization from the same handler
- Graceful degradation when terminal lacks capabilities

### Rendering

- [MiniJinja](https://github.com/mitsuhiko/minijinja) templates with semantic style tags
- CSS or YAML stylesheets with light/dark mode support
- Hot reload during development—edit templates without recompiling
- Tabular layouts with alignment, truncation, and Unicode support

### Integration

- Clap integration with automatic dispatch
- Declarative command registration via derive macros

## Installation

```bash
cargo add standout
```

## Migrating an Existing CLI

Already have a CLI? Standout supports incremental adoption. Standout handles matched commands automatically; unmatched commands return `ArgMatches` for your existing dispatch:

```rust
if let Some(matches) = app.run(Cli::command(), std::env::args()) {
    // Standout didn't handle this command, fall back to legacy
    your_existing_dispatch(matches);
}
```

See the [Partial Adoption Guide](crates/dispatch/topics/partial-adoption.md) for the full migration path.

## Next Steps

- **[Introduction to Standout](guides/intro-to-standout.md)** — Adopting Standout in a working CLI. Start here.
- [Introduction to Rendering](crates/render/guides/intro-to-rendering.md) — Creating polished terminal output
- [Introduction to Tabular](crates/render/guides/intro-to-tabular.md) — Building aligned, readable tabular layouts
- [All Topics](topics/index.md) — In-depth documentation for specific systems
