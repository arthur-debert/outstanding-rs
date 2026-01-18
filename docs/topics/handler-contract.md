# The Handler Contract

Handlers are where your application logic lives. Standout's handler contract is designed to be **explicit** rather than permissive. By enforcing serializable return types and clear ownership semantics, the framework guarantees that your code remains testable and decoupled from output formatting.

Instead of fighting with generic `Any` types or global state, you work with a clear contract: inputs are references, output is a `Result`.

See also:

- [Output Modes](output-modes.md) for how the output enum interacts with formats.

## Handler Modes

Standout supports two handler modes to accommodate different use cases:

| Aspect | `Handler` (default) | `LocalHandler` |
|--------|---------------------|----------------|
| App type | `App` | `LocalApp` |
| Self reference | `&self` | `&mut self` |
| Closure type | `Fn` | `FnMut` |
| Thread bounds | `Send + Sync` | None |
| State mutation | Via interior mutability | Direct |
| Use case | Libraries, async, multi-threaded | Simple CLIs with mutable state |

Choose based on your needs:

- **`App` with `Handler`**: Default. Use when handlers are stateless or use interior mutability (`Arc<Mutex<_>>`). Required for potential multi-threading.

- **`LocalApp` with `LocalHandler`**: Use when your handlers need `&mut self` access without wrapper types. Ideal for single-threaded CLIs.

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

Implementing the trait directly is useful when your handler needs internal state—database connections, configuration, etc. For stateless logic, closure handlers are more convenient.

## Closure Handlers

Most handlers are simple closures:

```rust
App::builder()
    .command("list", |matches, ctx| {
        let verbose = matches.get_flag("verbose");
        let items = storage::list()?;
        Ok(Output::Render(ListResult { items, verbose }))
    }, "list.j2")
```

The closure signature:

```rust
fn(&ArgMatches, &CommandContext) -> HandlerResult<T>
where T: Serialize + Send + Sync
```

Closures must be `Fn` (not `FnMut` or `FnOnce`) because Standout may call them multiple times in certain scenarios.

## The LocalHandler Trait (Mutable State)

When your handlers need `&mut self` access—common with database connections, file caches, or in-memory indices—use `LocalHandler` with `LocalApp`:

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

```rust
use standout::cli::{LocalApp, LocalHandler, Output, HandlerResult, CommandContext};

struct Database {
    connection: Connection,
    cache: HashMap<String, Record>,
}

impl Database {
    fn query_mut(&mut self, sql: &str) -> Result<Vec<Row>, Error> {
        // Needs &mut self because it updates the cache
        if let Some(cached) = self.cache.get(sql) {
            return Ok(cached.clone());
        }
        let result = self.connection.execute(sql)?;
        self.cache.insert(sql.to_string(), result.clone());
        Ok(result)
    }
}

impl LocalHandler for Database {
    type Output = Vec<Row>;

    fn handle(&mut self, matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<Vec<Row>> {
        let query = matches.get_one::<String>("query").unwrap();
        let rows = self.query_mut(query)?;
        Ok(Output::Render(rows))
    }
}
```

### Local Closure Handlers

`LocalApp::builder().command()` accepts `FnMut` closures:

```rust
let mut db = Database::connect()?;

LocalApp::builder()
    .command("query", |matches, ctx| {
        let sql = matches.get_one::<String>("sql").unwrap();
        let rows = db.query_mut(sql)?;  // &mut db works!
        Ok(Output::Render(rows))
    }, "{{ rows }}")
    .build()?
    .run(cmd, args);
```

This is the primary use case: capturing mutable references in closures without interior mutability wrappers.

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

Errors become the command output—Standout formats and displays them appropriately.

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

The common case. Data is serialized to JSON, passed to the template engine, and rendered with styles:

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

In structured output modes (`--output json`), the template is skipped and data serializes directly—same handler code, different output format.

### Output::Silent

No output produced. Useful for commands with side effects only:

```rust
fn delete_handler(matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
    let id: &String = matches.get_one("id").unwrap();
    storage::delete(id)?;
    Ok(Output::Silent)
}
```

Silent behavior in the pipeline:

- Post-output hooks still receive `RenderedOutput::Silent` (they can transform it)
- If `--output-file` is set, nothing is written
- Nothing prints to stdout

The type parameter for `Output::Silent` is often `()` but can be any `Serialize` type—it's never used.

### Output::Binary

Raw bytes written to a file. Useful for exports, archives, or generated files:

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

The filename is used as a literal file path. Standout writes the bytes using `std::fs::write()` and prints a confirmation to stderr. The filename can be:

- Relative: `"output/report.pdf"`
- Absolute: `"/tmp/report.pdf"`
- Dynamic: `format!("report-{}.pdf", timestamp)`

Binary output bypasses the template engine entirely.

## CommandContext

`CommandContext` provides execution environment information:

```rust
pub struct CommandContext {
    pub output_mode: OutputMode,
    pub command_path: Vec<String>,
}
```

**output_mode**: The resolved output format (Term, Text, Json, etc.). Handlers can inspect this to adjust behavior—for example, skipping interactive prompts in JSON mode:

```rust
fn interactive_handler(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Data> {
    let confirmed = if ctx.output_mode.is_structured() {
        true  // Non-interactive in JSON mode
    } else {
        prompt_user("Continue?")?
    };
    // ...
}
```

**command_path**: The subcommand chain as a vector, e.g., `["db", "migrate"]`. Useful for logging or conditional logic.

See [Execution Model](execution-model.md) for more on command paths.

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

For subcommands, you receive the `ArgMatches` for your specific command, not the root. Standout navigates to the deepest match before calling your handler.

## The #[dispatch] Macro

For applications with many commands, the `#[dispatch]` attribute macro generates registration from an enum:

```rust
#[derive(Dispatch)]
enum Commands {
    List,
    Add,
    Remove,
}
```

This generates a `dispatch_config()` method that registers handlers. Variant names are converted to snake_case command names:

- `List` → `"list"`
- `ListAll` → `"list_all"`

The macro expects handler functions named after the variant:

```rust
fn list(_m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<ListOutput> { ... }
fn add(_m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<AddOutput> { ... }
fn remove(_m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<RemoveOutput> { ... }
```

Variant attributes for customization:

```rust
#[derive(Dispatch)]
enum Commands {
    #[dispatch(handler = custom_list_fn)]  // Override handler function
    List,

    #[dispatch(template = "custom/add.j2")]  // Override template path
    Add,

    #[dispatch(pre_dispatch = validate_auth)]  // Add hook
    Remove,

    #[dispatch(skip)]  // Don't register this variant
    Internal,

    #[dispatch(nested)]  // This is a subcommand enum
    Db(DbCommands),
}
```

The `nested` attribute is required for subcommand enums—it's not inferred from tuple variants.

## Testing Handlers

Because handlers are pure functions with explicit inputs and outputs, they're straightforward to test:

```rust
#[test]
fn test_list_handler() {
    let cmd = Command::new("test").arg(Arg::new("verbose").long("verbose").action(ArgAction::SetTrue));
    let matches = cmd.try_get_matches_from(["test", "--verbose"]).unwrap();

    let ctx = CommandContext {
        output_mode: OutputMode::Term,
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
        output_mode: OutputMode::Term,
        command_path: vec!["count".into()],
    };

    // State accumulates across calls
    let _ = handler.handle(&matches, &ctx);
    let _ = handler.handle(&matches, &ctx);
    let result = handler.handle(&matches, &ctx);

    assert!(matches!(result, Ok(Output::Render(3))));
}
```

## Choosing Between Handler and LocalHandler

| Your situation | Use |
|----------------|-----|
| Stateless handlers | `App` + closures |
| State with `Arc<Mutex<_>>` already | `App` + `Handler` trait |
| API with `&mut self` methods | `LocalApp` + `LocalHandler` |
| Building a library | `App` (consumers might need thread safety) |
| Simple single-threaded CLI | Either works; `LocalApp` avoids wrapper types |

The key insight: CLIs are fundamentally single-threaded (parse → run one handler → output → exit). The `Send + Sync` requirement in `Handler` is conventional, not strictly necessary. `LocalHandler` removes this requirement for simpler code when thread safety isn't needed.
