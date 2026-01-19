# Introduction to Dispatch

CLI applications typically mix business logic with output formatting: database queries interleaved with `println!`, validation tangled with ANSI codes, error handling scattered across presentation. The result is code that's hard to test, hard to change, and impossible to reuse.

`standout-dispatch` enforces a clean separation:

```text
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

---

## The Problem

Here's a typical CLI command implementation:

```rust
fn list_command(matches: &ArgMatches) {
    let verbose = matches.get_flag("verbose");
    let items = storage::list().expect("failed to list");

    println!("\x1b[1;36mItems\x1b[0m");
    println!("──────");
    for item in &items {
        if verbose {
            println!("{}: {} (created: {})", item.id, item.name, item.created);
        } else {
            println!("{}: {}", item.id, item.name);
        }
    }
    println!("\n{} items total", items.len());
}
```

Problems with this approach:

1. **Testing is painful** — You have to capture stdout and parse it
2. **No format flexibility** — Want JSON output? Write a whole new function
3. **Error handling is crude** — `expect` or scattered error messages
4. **Logic and presentation intertwined** — Can't reuse the logic elsewhere
5. **Cross-cutting concerns require duplication** — Auth checks in every command

---

## The Solution: Handlers Return Data

With `standout-dispatch`, handlers focus purely on logic:

```rust
use standout_dispatch::{Handler, Output, CommandContext, HandlerResult};
use serde::Serialize;

#[derive(Serialize)]
struct ListResult {
    items: Vec<Item>,
    total: usize,
}

fn list_handler(matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<ListResult> {
    let items = storage::list()?;  // Errors propagate naturally
    Ok(Output::Render(ListResult {
        total: items.len(),
        items,
    }))
}
```

The handler:
- Receives parsed arguments (`&ArgMatches`) and execution context
- Returns a `Result` with serializable data
- Contains zero presentation logic

Rendering is handled separately:

```rust
use standout_dispatch::from_fn;

// Simple JSON renderer
let render = from_fn(|data, _view| {
    Ok(serde_json::to_string_pretty(data)?)
});
```

Or use a full template engine:

```rust
let render = from_fn(move |data, view| {
    my_renderer::render_template(view, data, &theme)
});
```

---

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
    FnHandler, Output, CommandContext, HandlerResult,
    from_fn, extract_command_path, path_to_string,
};
use clap::{Command, Arg};
use serde::Serialize;

#[derive(Serialize)]
struct Greeting { message: String }

fn main() -> anyhow::Result<()> {
    // 1. Define clap command
    let cmd = Command::new("myapp")
        .subcommand(
            Command::new("greet")
                .arg(Arg::new("name").required(true))
        );

    // 2. Create handler
    let greet_handler = FnHandler::new(|matches, _ctx| {
        let name: &String = matches.get_one("name").unwrap();
        Ok(Output::Render(Greeting {
            message: format!("Hello, {}!", name),
        }))
    });

    // 3. Create render function
    let render = from_fn(|data, _view| {
        Ok(serde_json::to_string_pretty(data)?)
    });

    // 4. Parse and dispatch
    let matches = cmd.get_matches();
    let path = extract_command_path(&matches);

    if path_to_string(&path) == "greet" {
        let ctx = CommandContext { command_path: path };
        let result = greet_handler.handle(&matches, &ctx)?;

        if let Output::Render(data) = result {
            let json = serde_json::to_value(&data)?;
            let output = render(&json, "greet")?;
            println!("{}", output);
        }
    }

    Ok(())
}
```

---

## The Output Enum

Handlers return one of three output types:

```rust
pub enum Output<T: Serialize> {
    Render(T),          // Data for rendering
    Silent,             // No output (side-effect commands)
    Binary {            // Raw bytes (file exports)
        data: Vec<u8>,
        filename: String,
    },
}
```

### Output::Render(T)

The common case. Data is passed to your render function:

```rust
fn list_handler(_m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<Vec<Item>> {
    let items = storage::list()?;
    Ok(Output::Render(items))
}
```

### Output::Silent

For commands with side effects only:

```rust
fn delete_handler(matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
    let id: &String = matches.get_one("id").unwrap();
    storage::delete(id)?;
    Ok(Output::Silent)
}
```

### Output::Binary

For generating files:

```rust
fn export_handler(_m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
    let data = generate_report()?;
    let csv_bytes = format_as_csv(&data)?;

    Ok(Output::Binary {
        data: csv_bytes.into_bytes(),
        filename: "report.csv".into(),
    })
}
```

---

## Hooks: Cross-Cutting Concerns

Hooks let you intercept execution without modifying handler logic:

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
        if let Some(obj) = data.as_object_mut() {
            obj.insert("timestamp".into(), json!(Utc::now().to_rfc3339()));
        }
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

### Hook Phases

| Phase | Timing | Receives | Can |
|-------|--------|----------|-----|
| `pre_dispatch` | Before handler | ArgMatches, Context | Abort execution |
| `post_dispatch` | After handler, before render | ArgMatches, Context, Data | Transform data |
| `post_output` | After render | ArgMatches, Context, Output | Transform output |

### Hook Chaining

Multiple hooks per phase run sequentially:

```rust
Hooks::new()
    .post_dispatch(add_metadata)      // Runs first
    .post_dispatch(filter_sensitive)  // Receives add_metadata's output
```

---

## Handler Types

### Closure Handlers

Most handlers are simple closures:

```rust
let handler = FnHandler::new(|matches, ctx| {
    let name: &String = matches.get_one("name").unwrap();
    Ok(Output::Render(Data { name: name.clone() }))
});
```

### Trait Implementations

For handlers with internal state:

```rust
struct DbHandler {
    pool: DatabasePool,
}

impl Handler for DbHandler {
    type Output = Vec<Row>;

    fn handle(&self, matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<Vec<Row>> {
        let query: &String = matches.get_one("query").unwrap();
        let rows = self.pool.query(query)?;
        Ok(Output::Render(rows))
    }
}
```

### Local Handlers (Mutable State)

When handlers need `&mut self`:

```rust
impl LocalHandler for Cache {
    type Output = Data;

    fn handle(&mut self, matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<Data> {
        self.invalidate();  // &mut self works
        Ok(Output::Render(self.get()?))
    }
}
```

See [Handler Contract](../topics/handler-contract.md) for full details.

---

## Command Routing Utilities

Extract and navigate clap's `ArgMatches`:

```rust
use standout_dispatch::{
    extract_command_path,
    get_deepest_matches,
    has_subcommand,
    path_to_string,
};

// myapp db migrate --steps 5
let path = extract_command_path(&matches);  // ["db", "migrate"]
let path_str = path_to_string(&path);       // "db.migrate"
let deep = get_deepest_matches(&matches);   // ArgMatches for "migrate"
```

---

## Testing Handlers

Because handlers are pure functions, testing is straightforward:

```rust
#[test]
fn test_list_handler() {
    let cmd = Command::new("test")
        .arg(Arg::new("verbose").long("verbose").action(ArgAction::SetTrue));
    let matches = cmd.try_get_matches_from(["test", "--verbose"]).unwrap();

    let ctx = CommandContext {
        command_path: vec!["list".into()],
    };

    let result = list_handler(&matches, &ctx);

    assert!(result.is_ok());
    if let Ok(Output::Render(data)) = result {
        assert!(data.verbose);
    }
}
```

No mocking needed—construct `ArgMatches` with clap, call your handler, assert on the result.

---

## Summary

`standout-dispatch` provides:

1. **Clean separation** — Handlers return data, renderers produce output
2. **Pluggable rendering** — Use any output format without changing handlers
3. **Hook system** — Cross-cutting concerns without code duplication
4. **Testable design** — Handlers are pure functions with explicit contracts
5. **Incremental adoption** — Migrate one command at a time

For complete API details, see the [API documentation](https://docs.rs/standout-dispatch).

> **For standout framework users:** The framework provides full integration with templates and themes. See the standout documentation for the `App` and `AppBuilder` APIs that wire dispatch and render together automatically.
