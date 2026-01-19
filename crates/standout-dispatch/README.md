# standout-dispatch

Command dispatch with strict separation of logic and presentation for CLI applications.

```rust
use standout_dispatch::{Handler, Output, CommandContext, from_fn};

// Handler returns data, not strings
fn list_handler(_m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<Vec<Task>> {
    let tasks = db::fetch_tasks()?;
    Ok(Output::Render(tasks))
}

// Renderer is pluggable—you decide how to format
let render = from_fn(|data| Ok(serde_json::to_string_pretty(data)?));
```

## Why standout-dispatch?

CLI commands typically mix business logic with output formatting: database queries interleaved with `println!`, validation tangled with ANSI codes, error handling scattered across presentation. The result is code that's hard to test, hard to change, and impossible to reuse.

**standout-dispatch** enforces a clean separation:

```
CLI args → Handler (logic) → Data → Renderer (presentation) → Output
```

- **Handlers** receive parsed arguments, return serializable data
- **Renderers** are pluggable callbacks you provide
- **Hooks** intercept execution at defined points

This isn't just architectural nicety—it unlocks:

- **Testable handlers** — Pure functions with explicit inputs and outputs
- **Swappable renderers** — JSON, templates, plain text from the same handler
- **Cross-cutting concerns** — Auth, logging, transformation via hooks
- **Incremental adoption** — Migrate one command at a time

## Features

### Handler Traits

Thread-safe and local variants for different use cases:

```rust
// Thread-safe handler (Send + Sync, &self)
impl Handler for MyHandler {
    type Output = Data;
    fn handle(&self, matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Data> {
        Ok(Output::Render(self.db.query()?))
    }
}

// Local handler for mutable state (&mut self, no Send + Sync)
impl LocalHandler for MyCache {
    type Output = Data;
    fn handle(&mut self, matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Data> {
        self.invalidate();  // &mut self works
        Ok(Output::Render(self.get()?))
    }
}
```

### Pluggable Render Handlers

Dispatch doesn't know how rendering works—you provide a closure:

```rust
use standout_dispatch::from_fn;

// JSON output
let json_render = from_fn(|data| Ok(serde_json::to_string_pretty(data)?));

// Custom formatting
let custom_render = from_fn(|data| {
    let name = data["name"].as_str().unwrap_or("unknown");
    Ok(format!("Result: {}", name))
});

// Template-based (with standout-render or any engine)
let format = OutputFormat::from_cli_args(&matches);
let template_render = from_fn(move |data| {
    my_renderer::render(data, format)
});
```

This design means dispatch orchestrates execution without coupling to any rendering implementation.

### Hook System

Intercept execution at three points:

```rust
use standout_dispatch::{Hooks, HookError, RenderedOutput};

let hooks = Hooks::new()
    // Before handler: validation, auth
    .pre_dispatch(|matches, ctx| {
        if !is_authenticated() {
            return Err(HookError::pre_dispatch("auth required"));
        }
        Ok(())
    })
    // After handler, before render: transform data
    .post_dispatch(|_m, _ctx, mut data| {
        data["timestamp"] = json!(Utc::now().to_rfc3339());
        Ok(data)
    })
    // After render: transform output
    .post_output(|_m, _ctx, output| {
        if let RenderedOutput::Text(s) = output {
            Ok(RenderedOutput::Text(format!("{}\n-- footer", s)))
        } else {
            Ok(output)
        }
    });
```

Hooks chain—each receives the output of the previous.

### Command Routing Utilities

Extract and navigate clap's `ArgMatches`:

```rust
use standout_dispatch::{
    extract_command_path,
    get_deepest_matches,
    has_subcommand,
    insert_default_command,
};

// myapp db migrate --steps 5
let path = extract_command_path(&matches);  // ["db", "migrate"]
let deep = get_deepest_matches(&matches);   // ArgMatches for "migrate"

// Default command support
if !has_subcommand(&matches) {
    let args = insert_default_command(std::env::args(), "list");
    // Reparse with default command inserted
}
```

### Output Types

Handlers produce one of three outputs:

```rust
pub enum Output<T: Serialize> {
    Render(T),                              // Data for rendering
    Silent,                                 // No output (side-effect only)
    Binary { data: Vec<u8>, filename: String }, // File export
}
```

## Quick Start

```toml
[dependencies]
standout-dispatch = "2.1"
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
```

```rust
use standout_dispatch::{
    FnHandler, HandlerResult, Output, CommandContext,
    Hooks, from_fn, extract_command_path, path_to_string,
};
use clap::{Command, Arg};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize)]
struct ListResult { items: Vec<String> }

fn main() -> anyhow::Result<()> {
    // 1. Define clap command
    let cmd = Command::new("myapp")
        .subcommand(Command::new("list"));

    // 2. Create handler
    let list_handler = FnHandler::new(|_m, _ctx| {
        Ok(Output::Render(ListResult {
            items: vec!["task-1".into(), "task-2".into()],
        }))
    });

    // 3. Create render function
    let render = from_fn(|data| Ok(serde_json::to_string_pretty(data)?));

    // 4. Build registry and dispatch
    let matches = cmd.get_matches();
    let path = extract_command_path(&matches);

    if path_to_string(&path) == "list" {
        let ctx = CommandContext { command_path: path };
        let result = list_handler.handle(&matches, &ctx)?;

        if let Output::Render(data) = result {
            let json = serde_json::to_value(&data)?;
            let output = render(&json)?;
            println!("{}", output);
        }
    }

    Ok(())
}
```

## Documentation

### Guides
- [Introduction to Dispatch](docs/guides/intro-to-dispatch.md) — Complete dispatch tutorial

### Topics
- [Handler Contract](docs/topics/handler-contract.md) — Handler types, Output enum, testing
- [Execution Model](docs/topics/execution-model.md) — Pipeline, hooks, command routing
- [Partial Adoption](docs/topics/partial-adoption.md) — Incremental migration strategies

### Reference
- [API Documentation](https://docs.rs/standout-dispatch) — Full API reference

## Used By

This crate provides the dispatch foundation for the [standout](https://crates.io/crates/standout) CLI framework, which combines dispatch with [standout-render](https://crates.io/crates/standout-render) for a complete CLI solution. Use `standout-dispatch` directly when you want the separation pattern without the rendering layer.

## License

MIT
