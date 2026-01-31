# Execution Model

`standout-dispatch` manages a strict linear pipeline from CLI input to rendered output. This explicitly separated flow ensures that logic (handlers) remains decoupled from presentation (renderers) and side-effects (hooks).

---

## The Pipeline

```text
Clap Parsing → Pre-dispatch Hook → Handler → Post-dispatch Hook → Renderer → Post-output Hook → Output
```

Each stage has a clear responsibility:

**Clap Parsing**: Your `clap::Command` definition is parsed normally. `standout-dispatch` doesn't replace clap—it works with the resulting `ArgMatches`.

**Pre-dispatch Hook**: Runs before the handler. Can abort execution (e.g., auth checks).

**Handler**: Your logic function executes. It receives `ArgMatches` and `CommandContext`, returning a `HandlerResult<T>`—either data to render, a silent marker, or binary content. For simpler handlers, use the `#[handler]` macro to write pure functions that return `Result<T, E>` directly (see [Handler Contract](handler-contract.md)).

**Post-dispatch Hook**: Runs after the handler, before rendering. Can transform data.

**Renderer**: Your render function receives the data and produces output (string or binary).

**Post-output Hook**: Runs after rendering. Can transform the final output.

**Output**: The result is returned or written to stdout.

---

## Command Paths

A command path is a vector of strings representing the subcommand chain:

```bash
myapp db migrate --steps 5
```

The command path is `["db", "migrate"]`.

### Extracting Command Paths

```rust
use standout_dispatch::{extract_command_path, path_to_string, get_deepest_matches};

let matches = cmd.get_matches();

// Get the full path
let path = extract_command_path(&matches);  // ["db", "migrate"]

// Convert to dot notation
let path_str = path_to_string(&path);  // "db.migrate"

// Get ArgMatches for the deepest command
let deep = get_deepest_matches(&matches);  // ArgMatches for "migrate"
```

### Command Path Utilities

| Function | Purpose |
|----------|---------|
| `extract_command_path` | Get subcommand chain as `Vec<String>` |
| `path_to_string` | Convert path to dot notation (`"db.migrate"`) |
| `string_to_path` | Convert dot notation to path |
| `get_deepest_matches` | Get `ArgMatches` for deepest subcommand |
| `has_subcommand` | Check if any subcommand was invoked |

---

## State Injection

Handlers access state through `CommandContext`, which provides two mechanisms:

- **`app_state`**: Shared, immutable state configured at build time (database, config)
- **`extensions`**: Per-request, mutable state injected by hooks

```rust
fn handler(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<T> {
    // App state: shared resources
    let db = ctx.app_state.get_required::<Database>()?;

    // Extensions: per-request state
    let scope = ctx.extensions.get_required::<UserScope>()?;
    // ...
}
```

> For full details on state management, see [App State and Extensions](app-state.md).

---

## The Hooks System

Hooks are functions that run at specific points in the pipeline. They let you intercept, validate, or transform without touching handler logic—keeping concerns separated.

### Three Phases

**Pre-dispatch**: Runs before the handler. Can abort execution or inject per-request state.

Use for: authentication checks, input validation, logging start time, **injecting per-request state** via `extensions`.

Pre-dispatch hooks receive `&mut CommandContext`, allowing them to inject state via `ctx.extensions` that handlers can retrieve. They also have read access to `ctx.app_state` for shared resources:

```rust
use standout_dispatch::{Hooks, HookError};

// Per-request state types (injected by hooks)
struct UserSession { user_id: u64 }

Hooks::new()
    .pre_dispatch(|matches, ctx| {
        // Read from app_state (shared)
        let db = ctx.app_state.get_required::<Database>()?;

        // Validate and set up per-request state
        let token = std::env::var("API_TOKEN")
            .map_err(|_| HookError::pre_dispatch("API_TOKEN required"))?;

        let user_id = db.validate_token(&token)?;

        // Inject into extensions (per-request)
        ctx.extensions.insert(UserSession { user_id });
        Ok(())
    })
```

Handlers then use both app_state and extensions:

```rust
fn list_handler(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Vec<Item>> {
    // App state: shared across all requests
    let db = ctx.app_state.get_required::<Database>()?;

    // Extensions: per-request state from hooks
    let session = ctx.extensions.get_required::<UserSession>()?;

    let items = db.fetch_items(session.user_id)?;
    Ok(Output::Render(items))
}
```

See the [Handler Contract](handler-contract.md#extensions) for full `Extensions` API documentation, and [App State](app-state.md) for details on the two-state model.

**Post-dispatch**: Runs after the handler, before rendering. Can transform data.

Use for: adding timestamps, filtering sensitive fields, data enrichment. The hook receives handler output as `serde_json::Value`, allowing generic transformations regardless of the handler's output type.

```rust
Hooks::new().post_dispatch(|_matches, _ctx, mut data| {
    if let Some(obj) = data.as_object_mut() {
        obj.insert("generated_at".into(), json!(Utc::now().to_rfc3339()));
    }
    Ok(data)
})
```

**Post-output**: Runs after rendering. Can transform the final string.

Use for: adding headers/footers, logging, metrics. The hook receives `RenderedOutput`—an enum of `Text(String)`, `Binary(Vec<u8>, String)`, or `Silent`.

```rust
use standout_dispatch::RenderedOutput;

Hooks::new().post_output(|_matches, _ctx, output| {
    match output {
        RenderedOutput::Text(s) => {
            Ok(RenderedOutput::Text(format!("{}\n-- Generated by MyApp", s)))
        }
        other => Ok(other),
    }
})
```

### Hook Chaining

Multiple hooks per phase are supported. Pre-dispatch hooks run sequentially—first error aborts. Post-dispatch and post-output hooks *chain*: each receives the output of the previous, enabling composable transformations.

```rust
Hooks::new()
    .post_dispatch(add_metadata)      // Runs first
    .post_dispatch(filter_sensitive)  // Receives add_metadata's output
```

Order matters: `filter_sensitive` sees the metadata that `add_metadata` inserted.

### Error Handling

When a hook returns `Err(HookError)`:

- Execution stops immediately
- Remaining hooks in that phase don't run
- For pre-dispatch: the handler never executes
- For post phases: the rendered output is discarded
- The error message is returned

```rust
use standout_dispatch::HookError;

// Create error with phase context
HookError::pre_dispatch("database connection failed")

// With source error for debugging
HookError::post_dispatch("transformation failed")
    .with_source(underlying_error)
```

---

## Render Handlers

The render handler is a pluggable callback that converts data to output:

```rust
use standout_dispatch::{from_fn, RenderFn};

// Simple JSON renderer
let render: RenderFn = from_fn(|data, _view| {
    Ok(serde_json::to_string_pretty(data)?)
});
```

### Render Function Signature

```rust
fn(&serde_json::Value, &str) -> Result<String, RenderError>
```

Parameters:
- `data`: The serialized handler output
- `view`: A view/template name hint (can be ignored)

### Using View Names

The `view` parameter enables template-based rendering:

```rust
let render = from_fn(move |data, view| {
    match view {
        "list" => format_as_list(data),
        "detail" => format_as_detail(data),
        _ => Ok(serde_json::to_string_pretty(data)?),
    }
});
```

> **For standout framework users:** The framework automatically maps view names to template files. See standout documentation for details.

### Local Render Functions

For render functions that need mutable state:

```rust
use standout_dispatch::{from_fn_mut, LocalRenderFn};

let render: LocalRenderFn = from_fn_mut(|data, view| {
    // Can capture and mutate state
    Ok(format_data(data))
});
```

---

## Default Command Support

Handle the case when no subcommand is specified:

```rust
use standout_dispatch::{has_subcommand, insert_default_command};

let matches = cmd.get_matches_from(args);

if !has_subcommand(&matches) {
    // Re-parse with default command inserted
    let args_with_default = insert_default_command(std::env::args(), "list");
    let matches = cmd.get_matches_from(args_with_default);
    // Now dispatch to "list"
}
```

`insert_default_command` inserts the command name after the binary name but before any flags.

---

## Putting It Together

A complete dispatch flow:

```rust
use standout_dispatch::{
    SimpleFnHandler, FnHandler, Output, CommandContext, Hooks, HookError,
    from_fn, extract_command_path, get_deepest_matches, path_to_string,
};

fn main() -> anyhow::Result<()> {
    // 1. Define clap command
    let cmd = Command::new("myapp")
        .subcommand(Command::new("list"))
        .subcommand(Command::new("delete").arg(Arg::new("id").required(true)));

    // 2. Create handlers
    // SimpleFnHandler: for handlers that don't need CommandContext
    let list_handler = SimpleFnHandler::new(|_m| {
        storage::list()  // Result<T, E> auto-wraps in Output::Render
    });

    // FnHandler: when you need CommandContext
    let delete_handler = FnHandler::new(|matches, _ctx| {
        let id: &String = matches.get_one("id").unwrap();
        storage::delete(id)?;
        Ok(Output::Silent)
    });

    // 3. Create render function
    let render = from_fn(|data, _view| {
        Ok(serde_json::to_string_pretty(data)?)
    });

    // 4. Create hooks
    let hooks = Hooks::new()
        .pre_dispatch(|_m, _ctx| {
            println!("Starting command...");
            Ok(())
        });

    // 5. Parse and dispatch
    let matches = cmd.get_matches();
    let path = extract_command_path(&matches);
    let mut ctx = CommandContext {
        command_path: path.clone(),
        ..Default::default()
    };

    // Run pre-dispatch hooks (may inject state via ctx.extensions)
    hooks.run_pre_dispatch(&matches, &mut ctx)?;

    // Dispatch based on command
    let result = match path_to_string(&path).as_str() {
        "list" => {
            let output = list_handler.handle(&matches, &ctx)?;
            if let Output::Render(data) = output {
                let json = serde_json::to_value(&data)?;
                let rendered = render(&json, "list")?;
                println!("{}", rendered);
            }
        }
        "delete" => {
            let deep = get_deepest_matches(&matches);
            delete_handler.handle(deep, &ctx)?;
            println!("Deleted.");
        }
        _ => eprintln!("Unknown command"),
    };

    Ok(())
}
```

---

## Summary

The execution model provides:

1. **Clear pipeline** — Each stage has defined inputs and outputs
2. **Hook points** — Intercept before, after handler, and after render
3. **Command routing** — Utilities for navigating subcommand hierarchies
4. **Pluggable rendering** — Render functions are separate from handlers
5. **Testable stages** — Each component can be tested in isolation
