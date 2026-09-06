# Minimal single-crate example

This self-contained project is a compact way to see Standout's dispatch and
rendering pipeline. It keeps everything in one binary package for brevity; it
is not the recommended layout for an application with reusable logic.

For production-shaped ownership, continue to the
[two-package worked application](production-shaped-example.md), where a
CLI-free library owns application behavior and a binary owns all CLI concerns.

## File structure

```text
my-todo/
├── Cargo.toml
└── src/
    ├── main.rs
    ├── templates/list.jinja
    └── styles/default.css
```

## Cargo.toml

```toml
[package]
name = "my-todo"
version = "0.1.0"
edition = "2021"

[dependencies]
standout = "9"
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
anyhow = "1"
```

## `src/main.rs`

```rust
use clap::{CommandFactory, Parser, Subcommand};
use serde::Serialize;
use standout::cli::{App, CommandContext, Dispatch, Output};
use standout::{embed_styles, embed_templates, handler};

#[derive(Parser)]
#[command(name = "my-todo")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
enum Commands {
    /// List all todos. Running the binary with no command lists too.
    #[dispatch(pure, default)]
    List,
}

#[derive(Serialize)]
struct TodoResult {
    todos: Vec<TodoView>,
}

#[derive(Serialize)]
struct TodoView {
    title: String,
    status: String,
}

mod handlers {
    use super::*;

    // This is a CLI adapter returning view data. In a real application it
    // should call a CLI-free library rather than contain application behavior.
    #[handler]
    pub fn list(
        #[ctx] _ctx: &CommandContext,
    ) -> Result<Output<TodoResult>, anyhow::Error> {
        Ok(Output::Render(TodoResult {
            todos: vec![
                TodoView { title: "Write documentation".into(), status: "done".into() },
                TodoView { title: "Ship v1.0".into(), status: "pending".into() },
            ],
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

## `src/templates/list.jinja`

```jinja
[title]My Todos[/title]
{% for todo in todos %}
[index]{{ loop.index }}.[/index] {{ todo.title | style_as(todo.status) }}
{% endfor %}
```

## `src/styles/default.css`

```css
.title { color: cyan; font-weight: bold; }
.index { color: yellow; }
.done { color: gray; text-decoration: line-through; }
.pending { color: white; font-weight: bold; }

@media (prefers-color-scheme: light) {
    .pending { color: black; }
}
```

## Run it

```bash
cargo run
cargo run -- list
cargo run -- list --output json
cargo run -- list --color never
```

This demonstrates command dispatch, template rendering, structured output, hot
reload in debug builds, adaptive styles, and standout's themed `--help`, which
is on unless an application calls `.help_handling(false)`. It intentionally
does not teach package ownership or the testing pyramid; the
[production-shaped example](production-shaped-example.md) does.
