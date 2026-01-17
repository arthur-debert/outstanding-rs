# The Handler Contract

Handlers are where your application logic lives. Outstanding's handler contract is designed to be **explicit** rather than permissive. By enforcing `Send + Sync` and serializable return types, the framework guarantees that your code remains testable, parallel-safe, and decoupled from output formatting.

Instead of fighting with generic `Any` types or global state, you work with a clear contract: inputs are immutable references, output is a `Result`.

See also:

- [Output Modes](output-modes.md) for how the output enum interacts with formats.

## The Handler Trait

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

Closures must be `Fn` (not `FnMut` or `FnOnce`) because Outstanding may call them multiple times in certain scenarios.

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

Errors become the command output—Outstanding formats and displays them appropriately.

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

The filename is used as a literal file path. Outstanding writes the bytes using `std::fs::write()` and prints a confirmation to stderr. The filename can be:

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

For subcommands, you receive the `ArgMatches` for your specific command, not the root. Outstanding navigates to the deepest match before calling your handler.

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
