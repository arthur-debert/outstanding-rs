# The Handler Contract

Handlers are where your application logic lives. The handler contract is designed to be **explicit** rather than permissive. By enforcing serializable return types and clear ownership semantics, the library guarantees that your code remains testable and decoupled from output formatting.

---

## Quick Start: The `#[handler]` Macro

For most handlers, use the `#[handler]` macro to write pure functions:

```rust
use standout_macros::handler;

#[handler]
pub fn list(#[flag] all: bool, #[arg] limit: Option<usize>) -> Result<Vec<Item>, anyhow::Error> {
    storage::list(all, limit)
}

// Generates: list__handler(&ArgMatches, &CommandContext) -> HandlerResult<Vec<Item>>
```

The macro:
- Extracts CLI arguments from `ArgMatches` based on annotations
- Auto-wraps `Result<T, E>` in `Output::Render` via `IntoHandlerResult`
- Preserves the original function for direct testing

**Parameter Annotations:**

| Annotation | Type | Extraction |
|------------|------|------------|
| `#[flag]` | `bool` | `matches.get_flag("name")` |
| `#[flag(name = "x")]` | `bool` | `matches.get_flag("x")` |
| `#[arg]` | `T` | Required argument |
| `#[arg]` | `Option<T>` | Optional argument |
| `#[arg]` | `Vec<T>` | Multiple values |
| `#[arg(name = "x")]` | `T` | Argument with custom CLI name |
| `#[ctx]` | `&CommandContext` | Access to context |
| `#[matches]` | `&ArgMatches` | Raw matches (escape hatch) |

**Return Type Handling:**

| Return Type | Generated Wrapper |
|-------------|-------------------|
| `Result<T, E>` | Auto-wrapped in `Output::Render` |
| `Result<(), E>` | Wrapped in `Output::Silent` |

> **Testing:** The original function is preserved, so you can test directly: `list(true, Some(10))`.

---

## Handler Modes

`standout-dispatch` supports two handler modes:

| Aspect | `Handler` (default) | `LocalHandler` |
|--------|---------------------|----------------|
| Self reference | `&self` | `&mut self` |
| Closure type | `Fn` | `FnMut` |
| Thread bounds | `Send + Sync` | None |
| State mutation | Via interior mutability | Direct |
| Use case | Libraries, async, multi-threaded | Simple CLIs with mutable state |

Choose based on your needs:

- **`Handler`**: Default. Use when handlers are stateless or use interior mutability (`Arc<Mutex<_>>`). Required for potential multi-threading.

- **`LocalHandler`**: Use when your handlers need `&mut self` access without wrapper types. Ideal for single-threaded CLIs.

---

## The Handler Trait (Thread-safe)

```rust
pub trait Handler: Send + Sync {
    type Output: Serialize;
    fn handle(&self, matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Self::Output>;
}
```

Key constraints:

- **Send + Sync required**: Handlers may be called from multiple threads
- **Output must be Serialize**: Needed for JSON/YAML modes and template context
- **Immutable references**: Handlers cannot modify arguments or context

Implementing the trait directly is useful when your handler needs internal state—database connections, configuration, etc.

### Example: Struct Handler

```rust
use standout_dispatch::{Handler, Output, CommandContext, HandlerResult};
use clap::ArgMatches;
use serde::Serialize;

struct DbHandler {
    pool: DatabasePool,
    config: Config,
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

---

## Closure Handlers

Most handlers are simple closures using `FnHandler`:

```rust
use standout_dispatch::{FnHandler, Output, HandlerResult};

let handler = FnHandler::new(|matches, ctx| {
    let verbose = matches.get_flag("verbose");
    let items = storage::list()?;
    Ok(Output::Render(ListResult { items, verbose }))
});
```

The closure signature:

```rust
fn(&ArgMatches, &CommandContext) -> HandlerResult<T>
where T: Serialize + Send + Sync
```

Closures must be `Fn` (not `FnMut` or `FnOnce`) for thread safety.

---

## SimpleFnHandler (No Context Needed)

When your handler doesn't need `CommandContext`, use `SimpleFnHandler` for a cleaner signature:

```rust
use standout_dispatch::SimpleFnHandler;

let handler = SimpleFnHandler::new(|matches| {
    let verbose = matches.get_flag("verbose");
    let items = storage::list()?;
    Ok(ListResult { items, verbose })
});
```

The closure signature:

```rust
fn(&ArgMatches) -> Result<T, E>
where T: Serialize + Send + Sync, E: Into<anyhow::Error>
```

`SimpleFnHandler` automatically wraps the result in `Output::Render` via `IntoHandlerResult`.

For `LocalHandler` equivalent, use `LocalSimpleFnHandler`:

```rust
use standout_dispatch::LocalSimpleFnHandler;

let handler = LocalSimpleFnHandler::new(|matches| {
    // No Send + Sync required
    Ok(items)
});
```

---

## IntoHandlerResult Trait

The `IntoHandlerResult` trait enables handlers to return `Result<T, E>` directly instead of `HandlerResult<T>`:

```rust
use standout_dispatch::IntoHandlerResult;

// Before: explicit Output wrapping
fn list(_m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<Vec<Item>> {
    let items = storage::list()?;
    Ok(Output::Render(items))
}

// After: automatic conversion
fn list(_m: &ArgMatches, _ctx: &CommandContext) -> impl IntoHandlerResult<Vec<Item>> {
    storage::list()  // Result<Vec<Item>, Error> auto-converts
}
```

The trait is implemented for:
- `Result<T, E>` where `E: Into<anyhow::Error>` → wraps `Ok(t)` in `Output::Render(t)`
- `HandlerResult<T>` → passes through unchanged

This is used internally by `SimpleFnHandler` and the `#[handler]` macro.

---

## The LocalHandler Trait (Mutable State)

When your handlers need `&mut self` access—common with database connections, file caches, or in-memory indices:

```rust
pub trait LocalHandler {
    type Output: Serialize;
    fn handle(&mut self, matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Self::Output>;
}
```

Key differences from `Handler`:

- **No Send + Sync**: Handlers don't need to be thread-safe
- **Mutable self**: `&mut self` allows direct state modification
- **FnMut closures**: Captured variables can be mutated

### When to Use LocalHandler

Use `LocalHandler` when:

- Your API uses `&mut self` methods (common for file/database operations)
- You want to avoid `Arc<Mutex<_>>` wrappers
- Your CLI is single-threaded (the typical case)

### Example: LocalHandler with Cache

```rust
use standout_dispatch::{LocalHandler, Output, CommandContext, HandlerResult};

struct CachingDatabase {
    connection: Connection,
    cache: HashMap<String, Record>,
}

impl CachingDatabase {
    fn query_with_cache(&mut self, sql: &str) -> Result<Vec<Row>, Error> {
        if let Some(cached) = self.cache.get(sql) {
            return Ok(cached.clone());
        }
        let result = self.connection.execute(sql)?;
        self.cache.insert(sql.to_string(), result.clone());
        Ok(result)
    }
}

impl LocalHandler for CachingDatabase {
    type Output = Vec<Row>;

    fn handle(&mut self, matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<Vec<Row>> {
        let query: &String = matches.get_one("query").unwrap();
        let rows = self.query_with_cache(query)?;
        Ok(Output::Render(rows))
    }
}
```

### Local Closure Handlers

`LocalFnHandler` accepts `FnMut` closures:

```rust
use standout_dispatch::LocalFnHandler;

let mut counter = 0;

let handler = LocalFnHandler::new(move |_matches, _ctx| {
    counter += 1;  // Mutation works!
    Ok(Output::Render(counter))
});
```

---

## HandlerResult

`HandlerResult<T>` is a standard `Result` type:

```rust
pub type HandlerResult<T> = Result<Output<T>, anyhow::Error>;
```

The `?` operator works naturally for error propagation:

```rust
fn list_handler(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Items> {
    let items = storage::load()?;           // Propagates errors
    let filtered = filter_items(&items)?;   // Propagates errors
    Ok(Output::Render(Items { filtered }))
}
```

---

## The Output Enum

`Output<T>` represents what a handler produces:

```rust
pub enum Output<T: Serialize> {
    Render(T),
    Silent,
    Binary { data: Vec<u8>, filename: String },
}
```

### Output::Render(T)

The common case. Data is passed to the render function:

```rust
#[derive(Serialize)]
struct ListResult {
    items: Vec<Item>,
    total: usize,
}

fn list_handler(_m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<ListResult> {
    let items = storage::list()?;
    Ok(Output::Render(ListResult {
        total: items.len(),
        items,
    }))
}
```

### Output::Silent

No output produced. Useful for commands with side effects only:

```rust
fn delete_handler(matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
    let id: &String = matches.get_one("id").unwrap();
    storage::delete(id)?;
    Ok(Output::Silent)
}
```

Silent behavior:
- Post-output hooks still receive `RenderedOutput::Silent`
- Render function is not called
- Nothing prints to stdout

### Output::Binary

Raw bytes for file output:

```rust
fn export_handler(matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
    let data = generate_report()?;
    let pdf_bytes = render_to_pdf(&data)?;

    Ok(Output::Binary {
        data: pdf_bytes,
        filename: "report.pdf".into(),
    })
}
```

Binary output bypasses the render function entirely.

---

## CommandContext

`CommandContext` provides execution environment information and state access:

```rust
pub struct CommandContext {
    pub command_path: Vec<String>,
    pub app_state: Arc<Extensions>,
    pub extensions: Extensions,
}
```

**command_path**: The subcommand chain as a vector, e.g., `["db", "migrate"]`. Useful for logging or conditional logic.

**app_state**: Shared, immutable state configured at app build time via `AppBuilder::app_state()`. Wrapped in `Arc` for cheap cloning. Use for database connections, configuration, API clients.

**extensions**: Per-request, mutable state injected by pre-dispatch hooks. Use for user sessions, request IDs, computed values.

> For comprehensive coverage of state management, see [App State and Extensions](app-state.md).

---

## State Access: App State vs Extensions

Handlers access state through two distinct mechanisms with different semantics:

| Aspect | `ctx.app_state` | `ctx.extensions` |
|--------|-----------------|------------------|
| **Mutability** | Immutable (`&`) | Mutable (`&mut`) |
| **Lifetime** | App lifetime | Per-request |
| **Set by** | `AppBuilder::app_state()` | Pre-dispatch hooks |
| **Use for** | Database, Config, API clients | User sessions, request IDs |

### App State (Shared Resources)

Configure long-lived resources at build time:

```rust
App::builder()
    .app_state(Database::connect()?)
    .app_state(Config::load()?)
    .command("list", list_handler, template)
    .build()?
```

Access in handlers via `ctx.app_state`:

```rust
fn list_handler(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Vec<Item>> {
    let db = ctx.app_state.get_required::<Database>()?;
    let config = ctx.app_state.get_required::<Config>()?;

    let items = db.query_items(config.max_results)?;
    Ok(Output::Render(items))
}
```

### Extensions (Per-Request State)

Pre-dispatch hooks inject request-scoped state:

```rust
use standout_dispatch::{Hooks, HookError};

struct UserScope { user_id: String, permissions: Vec<String> }

let hooks = Hooks::new()
    .pre_dispatch(|matches, ctx| {
        // Can read app_state to set up per-request state
        let db = ctx.app_state.get_required::<Database>()?;

        let user_id = matches.get_one::<String>("user").unwrap().clone();
        let permissions = db.get_permissions(&user_id)?;

        ctx.extensions.insert(UserScope { user_id, permissions });
        Ok(())
    });
```

Handlers retrieve from extensions:

```rust
fn list_handler(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Vec<Item>> {
    let db = ctx.app_state.get_required::<Database>()?;       // shared
    let scope = ctx.extensions.get_required::<UserScope>()?;  // per-request

    let items = db.list_for_user(&scope.user_id)?;
    Ok(Output::Render(items))
}
```

### Extensions API

Both `app_state` and `extensions` use the same `Extensions` type with these methods:

| Method | Description |
|--------|-------------|
| `insert<T>(value)` | Insert a value, returns previous if any |
| `get<T>()` | Get immutable reference, returns `Option<&T>` |
| `get_required<T>()` | Get reference or return error if missing |
| `get_mut<T>()` | Get mutable reference, returns `Option<&mut T>` |
| `remove<T>()` | Remove and return value |
| `contains<T>()` | Check if type exists |
| `len()` | Number of stored values |
| `is_empty()` | True if no values stored |
| `clear()` | Remove all values |

Use `get_required` for mandatory dependencies (fails fast with clear error), `get` for optional ones.

### When to Use Which

**Use App State for:**
- Database connections — expensive to create, should be pooled
- Configuration — loaded once at startup
- API clients — shared HTTP clients with connection pooling

**Use Extensions for:**
- User context — current user, session, permissions
- Request metadata — request ID, timing, correlation ID
- Transient state — data computed by one hook, used by handler

### The Two-State Pattern

The separation exists because:

1. **Closure capture doesn't work with `#[derive(Dispatch)]`** — macro-generated dispatch calls handlers with a fixed signature
2. **App-level resources shouldn't be created per-request** — database pools and config are expensive
3. **Per-request state needs mutable injection** — hooks compute values at runtime

```rust
// App state: configured once at build time
App::builder()
    .app_state(Database::connect()?)  // Shared via Arc
    .hooks("users.list", Hooks::new()
        .pre_dispatch(|matches, ctx| {
            // Extensions: computed per-request, can use app_state
            let db = ctx.app_state.get_required::<Database>()?;
            let user = authenticate(matches, db)?;
            ctx.extensions.insert(user);
            Ok(())
        }))
```

> For comprehensive coverage of state management patterns, see [App State and Extensions](app-state.md).

---

## Accessing CLI Arguments

The `ArgMatches` parameter provides access to parsed arguments through clap's standard API:

```rust
fn handler(matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<Data> {
    // Flags
    let verbose = matches.get_flag("verbose");

    // Required options
    let name: &String = matches.get_one("name").unwrap();

    // Optional values
    let limit: Option<&u32> = matches.get_one("limit");

    // Multiple values
    let tags: Vec<&String> = matches.get_many("tags")
        .map(|v| v.collect())
        .unwrap_or_default();

    Ok(Output::Render(Data { ... }))
}
```

For subcommands, you work with the `ArgMatches` for your specific command level.

---

## Testing Handlers

Because handlers are pure functions with explicit inputs and outputs, they're straightforward to test:

```rust
#[test]
fn test_list_handler() {
    let cmd = Command::new("test")
        .arg(Arg::new("verbose").long("verbose").action(ArgAction::SetTrue));
    let matches = cmd.try_get_matches_from(["test", "--verbose"]).unwrap();

    let ctx = CommandContext {
        command_path: vec!["list".into()],
        ..Default::default()
    };

    let result = list_handler(&matches, &ctx);

    assert!(result.is_ok());
    if let Ok(Output::Render(data)) = result {
        assert!(data.verbose);
    }
}
```

No mocking frameworks needed—construct `ArgMatches` with clap, create a `CommandContext`, call your handler, assert on the result.

### Testing with App State

When handlers depend on app_state, inject test fixtures:

```rust
#[test]
fn test_handler_with_app_state() {
    use std::sync::Arc;

    // Create test fixtures
    let mock_db = MockDatabase::with_items(vec![
        Item { id: "1", name: "Test" }
    ]);

    // Build app_state with test data
    let mut app_state = Extensions::new();
    app_state.insert(mock_db);

    let ctx = CommandContext {
        command_path: vec!["list".into()],
        app_state: Arc::new(app_state),
        extensions: Extensions::new(),
    };

    let cmd = Command::new("test");
    let matches = cmd.try_get_matches_from(["test"]).unwrap();

    let result = list_handler(&matches, &ctx);
    assert!(result.is_ok());
}
```

### Testing LocalHandlers

`LocalHandler` tests work the same way, but use `&mut self`:

```rust
#[test]
fn test_local_handler_state_mutation() {
    struct Counter { count: u32 }

    impl LocalHandler for Counter {
        type Output = u32;
        fn handle(&mut self, _m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<u32> {
            self.count += 1;
            Ok(Output::Render(self.count))
        }
    }

    let mut handler = Counter { count: 0 };
    let cmd = Command::new("test");
    let matches = cmd.try_get_matches_from(["test"]).unwrap();
    let ctx = CommandContext {
        command_path: vec!["count".into()],
        ..Default::default()
    };

    // State accumulates across calls
    let _ = handler.handle(&matches, &ctx);
    let _ = handler.handle(&matches, &ctx);
    let result = handler.handle(&matches, &ctx);

    assert!(matches!(result, Ok(Output::Render(3))));
}
```

---

## Choosing Between Handler and LocalHandler

| Your situation | Use |
|----------------|-----|
| Stateless handlers | `FnHandler` + closures |
| State with `Arc<Mutex<_>>` already | `Handler` trait |
| API with `&mut self` methods | `LocalHandler` trait |
| Building a library | `Handler` (consumers might need thread safety) |
| Simple single-threaded CLI | Either works; `LocalHandler` avoids wrapper types |

The key insight: CLIs are fundamentally single-threaded (parse → run one handler → output → exit). The `Send + Sync` requirement in `Handler` is conventional, not strictly necessary. `LocalHandler` removes this requirement for simpler code when thread safety isn't needed.
