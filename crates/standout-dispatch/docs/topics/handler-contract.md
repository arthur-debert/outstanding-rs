# The Handler Contract

Handlers are where your application logic lives. The handler contract is designed to be **explicit** rather than permissive. By enforcing serializable return types and clear ownership semantics, the library guarantees that your code remains testable and decoupled from output formatting.

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

`CommandContext` provides execution environment information:

```rust
pub struct CommandContext {
    pub command_path: Vec<String>,
}
```

**command_path**: The subcommand chain as a vector, e.g., `["db", "migrate"]`. Useful for logging or conditional logic.

`CommandContext` is intentionally minimal. Application-specific context (config, connections) should be captured in struct handlers or closures:

```rust
struct MyHandler {
    db: DatabasePool,
    config: AppConfig,
}

impl Handler for MyHandler {
    type Output = Data;

    fn handle(&self, matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Data> {
        let result = self.db.query(...)?;
        Ok(Output::Render(result))
    }
}
```

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
    };

    let result = list_handler(&matches, &ctx);

    assert!(result.is_ok());
    if let Ok(Output::Render(data)) = result {
        assert!(data.verbose);
    }
}
```

No mocking frameworks needed—construct `ArgMatches` with clap, create a `CommandContext`, call your handler, assert on the result.

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
