# Standout

**Test your data. Render your view.**

Standout is a CLI framework for Rust that enforces separation between logic and
presentation. Keep application behavior in a CLI-free library; handlers adapt
that behavior into serializable CLI view data instead of strings.

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

With Standout, the library owns behavior, handlers return CLI view data, and
the framework handles rendering:

```rust
#[handler]
fn list(
    #[flag] all: bool,
    #[ctx] ctx: &CommandContext,
) -> Result<Output<TodoListView>, anyhow::Error> {
    let store = ctx.app_state.get_required::<TodoStore>()?;
    let filter = if all { TodoFilter::All } else { TodoFilter::Pending };
    let todos = store.list(filter).into_iter().map(TodoView::from).collect();
    let total = todos.len();
    Ok(Output::Render(TodoListView { todos, total }))
}

#[test]
fn test_list_returns_pending_view() {
    let Output::Render(result) = list(false, &ctx).unwrap() else {
        panic!("expected rendered data");
    };
    assert!(result.todos.iter().all(|todo| !todo.done));
}
```

Test filtering and state transitions through the library interface. Test only
the mapping and returned view struct in the handler. No stdout capture, regex,
or template coupling. See the
[production-shaped application](guides/production-shaped-example.md).

## Standing Out

What Standout provides:

- Enforced architecture splitting data and presentation
- Logic is testable as any Rust code — and full CLI invocations are testable in-process via the [`standout-test`](guides/intro-to-testing.md) harness, without subprocess spawning or stdout parsing
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

Use the `Dispatch` derive macro to connect commands to typed handler adapters.

```rust
use standout::cli::{CommandContext, Dispatch, Output};
use standout::handler;
use clap::Subcommand;
use serde::Serialize;

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]  // handlers are in the `handlers` module
pub enum Commands {
    #[dispatch(pure)]
    List,
    #[dispatch(pure)]
    Add { title: String },
}

#[derive(Serialize)]
struct TodoResult {
    todos: Vec<Todo>,
}

mod handlers {
    use super::*;

    #[handler]
    pub fn list(#[ctx] ctx: &CommandContext) -> Result<Output<TodoResult>, anyhow::Error> {
        let core = ctx.app_state.get_required::<TodoStore>()?;
        Ok(Output::Render(TodoResult::from(core.list(TodoFilter::Pending))))
    }

    #[handler]
    pub fn add(
        #[arg] title: String,
        #[ctx] ctx: &CommandContext,
    ) -> Result<Output<TodoResult>, anyhow::Error> {
        let core = ctx.app_state.get_required::<TodoStore>()?;
        Ok(Output::Render(TodoResult::from(vec![core.add(title)?])))
    }
}
```

### 2. Define Your Presentation

Templates use MiniJinja with semantic style tags. Styles are defined separately in CSS.

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
    let store = TodoStore::load("todos.json")?;
    let app = App::builder()
        .app_state(store) // the handlers above read it back with `get_required`
        .templates(embed_templates!("src/templates"))
        .styles(embed_styles!("src/styles"))
        .default_theme("default")
        .commands(Commands::dispatch_config())? // Register handlers from derive macro
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

- CLI-free library separated from shell presentation
- Handlers adapt library results; framework handles rendering
- Core behavior and CLI adapters testable without stdout capture

### Output Modes

- Rich terminal output with colors and styles
- Automatic JSON, YAML, CSV serialization from the same handler
- Graceful degradation when terminal lacks capabilities

### Rendering

- [MiniJinja](https://github.com/mitsuhiko/minijinja) templates with semantic style tags
- CSS stylesheets with light/dark mode support
- Hot reload during development—edit templates without recompiling
- Tabular layouts with alignment, truncation, and Unicode support

### Integration

- Clap integration with automatic dispatch
- Declarative command registration via derive macros

## Installation

```bash
cargo add standout standout-dispatch
```

## Migrating an Existing CLI

Already have a CLI? Standout supports incremental adoption. `run` reports
whether Standout handled the command:

```rust
if !app.run(Cli::command(), std::env::args()) {
    your_existing_dispatch();
}
```

Use `run_to_string(...)` and match `DispatchResult::NoMatch(matches)` on
`into_outcome()` when the legacy dispatcher needs the unmatched `ArgMatches`.

See the [Partial Adoption Guide](crates/dispatch/topics/partial-adoption.md) for the full migration path.

## Next Steps

- **[Introduction to Standout](guides/intro-to-standout.md)** — Adopting Standout in a working CLI. Start here.
- **[Introduction to Testing](guides/intro-to-testing.md)** — Why Standout CLIs are testable by design, and how the `standout-test` harness replaces slow, brittle subprocess tests with fast in-process ones.
- [Introduction to Rendering](crates/render/guides/intro-to-rendering.md) — Creating polished terminal output
- [Introduction to Tabular](crates/render/guides/intro-to-tabular.md) — Building aligned, readable tabular layouts
- [All Topics](topics/index.md) — In-depth documentation for specific systems
