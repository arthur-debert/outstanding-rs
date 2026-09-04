# The Handler Contract

Handlers are shell adapters: they map parsed CLI input to application calls and
return serializable CLI-owned view data. Keep reusable behavior in a CLI-free
library. The handler contract is designed to be **explicit** rather than
permissive, so adapters remain testable and decoupled from output formatting.

---

## Quick Start: The `#[handler]` Macro

For most handlers, use the `#[handler]` macro to write typed adapter functions:

```rust,ignore
use standout_macros::handler;

#[handler]
pub fn list(#[flag] all: bool, #[arg] limit: Option<usize>) -> Result<Vec<Item>, anyhow::Error> {
    storage::list(all, limit)
}
```

The macro leaves `list` alone and adds three items beside it:

| Item | What it is |
| --- | --- |
| `list__handler(&ArgMatches, &CommandContext)` | reads the arguments out of `ArgMatches` and calls `list`. It returns **the annotated return type verbatim** — here `Result<Vec<Item>, anyhow::Error>`, not `HandlerResult<Vec<Item>>` |
| `list__expected_args() -> Vec<ExpectedArg>` | what `App::verify_command` reads |
| `list_Handler` | a unit struct implementing [`Handler`](#the-handler-trait) — **the registrable item** |

The `Result<T, E>` to `Output::Render` wrap happens inside `list_Handler`'s
`Handler::handle` (through `IntoHandlerResult`), never in `list__handler`
itself. That is the difference the next two tables spell out.

The un-suffixed `handlers::list` is not registrable — it has the wrong
signature by design, so that a test can call it directly. Which of the other
two items you register depends on the method:

| Method | What it takes | What to pass |
| --- | --- | --- |
| `AppBuilder::command_with` | `impl Handler` | `handlers::list_Handler` |
| `GroupBuilder::command` / `command_with`, and therefore `#[derive(Dispatch)]` | a closure returning `HandlerResult<T>` | `handlers::list__handler` |

That second row constrains the return type, and it is the one place the two
registration paths genuinely differ. `list__handler` returns the annotated type
verbatim, so it satisfies `HandlerResult<T>` only when the function was written
`-> Result<Output<T>, E>` (or `-> Result<(), E>`, whose wrapper returns
`HandlerResult<()>`). **A handler annotated `-> Result<T, E>` cannot be
registered through `#[derive(Dispatch)]`**: expansion fails with `expected
list__handler to return Result<Output<_>, Error>, but it returns Result<Items,
Error>`. Write it `-> Result<Output<T>, E>`, or register `list_Handler` through
`AppBuilder::command_with`, where `Handler::handle` applies the wrap for you.

The three return shapes, and the original functions still being callable:

```rust
use standout::cli::{CommandContext, Output};
use standout::handler;

#[derive(serde::Serialize)]
pub struct Items {
    pub names: Vec<String>,
}

/// `Handler::Output` is `Items`; `handle` wraps the value in `Output::Render`.
#[handler]
pub fn list(#[flag] all: bool) -> Result<Items, anyhow::Error> {
    let mut names = vec!["ssh".to_string()];
    if all {
        names.push("cron".to_string());
    }
    Ok(Items { names })
}

/// `Handler::Output` is `Items`; the `Output` passes through untouched.
#[handler]
pub fn about(#[ctx] _ctx: &CommandContext) -> Result<Output<Items>, anyhow::Error> {
    Ok(Output::Render(Items { names: vec!["unitctl".to_string()] }))
}

/// `Handler::Output` is `()`; `handle` produces `Output::Silent`.
#[handler]
pub fn reload(#[flag] _force: bool) -> Result<(), anyhow::Error> {
    Ok(())
}

fn main() {
    // No ArgMatches, no dispatcher: the annotated function is what a unit test calls.
    assert_eq!(list(true).unwrap().names, ["ssh", "cron"]);
    reload(false).unwrap();
}
```

Every `#[dispatch(…)]` and `#[handler]` attribute is listed in the
[`#[dispatch(…)]` and `#[handler]` reference](../../../topics/dispatch-attributes.md).

**Parameter Annotations:**

| Annotation            | Type              | Extraction                    |
| --------------------- | ----------------- | ----------------------------- |
| `#[flag]`             | `bool`            | `matches.get_flag("name")`    |
| `#[flag(name = "x")]` | `bool`            | `matches.get_flag("x")`       |
| `#[arg]`              | `T`               | Required argument             |
| `#[arg]`              | `Option<T>`       | Optional argument             |
| `#[arg]`              | `Vec<T>`          | Multiple values               |
| `#[arg(name = "x")]`  | `T`               | Argument with custom CLI name |
| `#[ctx]`              | `&CommandContext` | Access to context             |
| `#[matches]`          | `&ArgMatches`     | Raw matches (escape hatch)    |

Without `name = "x"`, the argument id is the parameter name with underscores
turned into hyphens: `no_legend` reads the argument id `no-legend`. Clap's own
derive ids an argument by the field name it comes from, so a clap-derive
`no_legend` field declares `#[arg(id = "no-legend")]` to meet the handler, or
the handler parameter takes the field's id with `#[flag(name = "no_legend")]`.
`app.verify_command(&cmd)` reports the mismatch instead of leaving it to a
runtime `get_flag` panic. A parameter named with a raw identifier drops the
`r#` first, the way clap's derive drops it from a field name: `r#type` reads
the argument id `type`.

**Return Type Handling:** the function must return `Result<T, E>`; the macro
rejects anything else with `handler must return Result<T, E>`. What `T` is
decides what `Handler::Output` becomes and whether anything is wrapped.

| Annotated return type | `Handler::Output` | What `handle` produces |
| --- | --- | --- |
| `Result<T, E>` | `T` | `Ok(value)` wrapped in `Output::Render(value)` |
| `Result<Output<T>, E>` (that is, `HandlerResult<T>`) | `T` | the `Output` you returned, unchanged |
| `Result<(), E>` | `()` | `Output::Silent` |

> **Testing:** The original function is preserved, so you can test directly: `list(true, Some(10))`.

---

## The Handler Trait

```rust,ignore
pub trait Handler {
    type Output: Serialize;
    fn handle(&mut self, matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Self::Output>;
}
```

Key characteristics:

- **Mutable self**: `&mut self` allows direct state modification
- **Output must be Serialize**: Needed for JSON/YAML modes and template context

Implementing the trait directly is useful when your handler needs internal state—database connections, configuration, caches, etc.

### Example: Struct Handler with State

```rust,ignore
use standout_dispatch::{Handler, Output, CommandContext, HandlerResult};
use clap::ArgMatches;
use serde::Serialize;

struct CachingDatabase {
    connection: Connection,
    cache: HashMap<String, Vec<Row>>,
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

impl Handler for CachingDatabase {
    type Output = Vec<Row>;

    fn handle(&mut self, matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<Vec<Row>> {
        let query: &String = matches.get_one("query").unwrap();
        let rows = self.query_with_cache(query)?;  // &mut self works!
        Ok(Output::Render(rows))
    }
}
```

---

## Closure Handlers

Most handlers are simple closures using `FnHandler`:

```rust,ignore
use standout_dispatch::{FnHandler, Output, HandlerResult};

let mut counter = 0;

let handler = FnHandler::new(move |_matches, _ctx| {
    counter += 1;  // Mutation works!
    Ok(Output::Render(counter))
});
```

The closure signature:

```rust,ignore
fn(&ArgMatches, &CommandContext) -> HandlerResult<T>
where T: Serialize
```

Closures are `FnMut`, allowing captured variables to be mutated.

---

## SimpleFnHandler (No Context Needed)

When your handler doesn't need `CommandContext`, use `SimpleFnHandler` for a cleaner signature:

```rust,ignore
use standout_dispatch::SimpleFnHandler;

let handler = SimpleFnHandler::new(|matches| {
    let verbose = matches.get_flag("verbose");
    let items = storage::list()?;
    Ok(ListResult { items, verbose })
});
```

The closure signature:

```rust,ignore
fn(&ArgMatches) -> Result<T, E>
where T: Serialize, E: Into<anyhow::Error>
```

`SimpleFnHandler` automatically wraps the result in `Output::Render` via `IntoHandlerResult`.

---

## IntoHandlerResult Trait

The `IntoHandlerResult` trait enables handlers to return `Result<T, E>` directly instead of `HandlerResult<T>`:

```rust,ignore
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

## HandlerResult

`HandlerResult<T>` is a standard `Result` type:

```rust,ignore
pub type HandlerResult<T> = Result<Output<T>, anyhow::Error>;
```

The `?` operator works naturally for error propagation:

```rust,ignore
fn list_handler(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Items> {
    let items = storage::load()?;           // Propagates errors
    let filtered = filter_items(&items)?;   // Propagates errors
    Ok(Output::Render(Items { filtered }))
}
```

### Owner-declared failures

`AppFailure` and `ExternalFailure` carry a nonzero status and a stderr payload
the framework writes verbatim, through the same `HandlerResult` seam
([Error Handling](../../../topics/error-handling.md#an-application-owned-status-and-diagnostic)):

```rust,ignore
// The application's own specification pins the status and the line.
Err(AppFailure::new(1, "ghlike: repository not found: demo/gamma\n")?.into())

// A delegated executable decided both, and the handler is relaying them.
Err(ExternalFailure::new(128, git_stderr)?.into())
```

A *successful* run declares a status through the output, not through an error:
`Output::Render(data).with_exit_status(ExitStatus::from(2))`
([Execution Outcomes](../../../topics/execution-outcomes.md#status-and-streams)).

---

## The Output Enum

`Output<T>` represents what a handler produces:

```rust,ignore
#[non_exhaustive]
pub enum Output<T: Serialize> {
    Render(T),
    Silent,
    Binary { data: Vec<u8>, filename: String },
    Artifact(Artifact<T>),
    WithStatus { output: Box<Output<T>>, status: ExitStatus },
}
```

`Output` is `#[non_exhaustive]`: matches on it need a `_` arm so later shapes
can be added without breaking downstream code. `WithStatus` is built by
`with_exit_status` and wraps a `Render` or `Silent` output together with the
exit status the handler chose; `split_exit_status()` takes it apart,
`exit_status()` reads it (`SUCCESS` when none was declared), `map_render(f)`
reaches the rendered value through it, and the `is_*` predicates answer for the
wrapped output. Declaring a status on `Binary` or `Artifact` is a render error.

### Output::Render(T)

The common case. Data is passed to the render function:

```rust,ignore
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

```rust,ignore
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

```rust,ignore
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

The filename is a **hint for the caller**, not permission to write. Without
`--output-file-path`, `run()` sends the bytes to stdout and touches no file. If
you want the framework to write the suggested destination, use `Output::Artifact`
— that opt-in is the whole difference between the two shapes.

### Output::Artifact

Owned bytes plus an application-owned report, for commands that produce a file
*and* have something to say about it. `Output::Binary` cannot carry a report,
and nothing renders after its write, so a command that wants to say "exported 12
rows to /tmp/report.csv (2 warnings)" would otherwise have to write the file
itself — pulling destination policy back into the application core.

```rust,ignore
use standout::cli::{Artifact, HandlerResult, Output};

#[derive(Serialize)]
struct ExportReport {
    exported: usize,
    warnings: Vec<Warning>,
}

fn export_handler(_m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<ExportReport> {
    let export = core::export_csv()?;   // bytes + facts, no filesystem

    Ok(Output::Artifact(
        Artifact::new(export.csv)
            .suggest_destination(export.suggested_filename)
            .with_report(ExportReport {
                exported: export.rows,
                warnings: export.warnings,
            }),
    ))
}
```

#### Artifact builder

`Artifact<T>` is `T`, the report type, plus a byte payload and two opt-in
destination hints:

| Method | Purpose |
| --- | --- |
| `Artifact::new(bytes: impl Into<Vec<u8>>)` | Construct with the payload bytes. An empty vector is legal — a zero-byte artifact writes an empty file (or nothing to stdout). |
| `.suggest_destination(path: impl Into<PathBuf>)` | Offer a default write path (destination policy step 2). |
| `.allow_stdout()` | Permit the framework to fall back to stdout (step 3). |
| `.with_report(report: T)` | Attach the report rendered after the write. |

When the report is non-empty, the framework appends a newline after it via
`writeln!`; it does not strip or collapse a newline the report already ends
with, so a report that ends in `\n` produces a blank line. An absent or empty
report emits nothing — no newline at all. A destination must still be selected first: an `Artifact` that suggests
none, does not `.allow_stdout()`, and gets no `--output-file-path` fails with a
stderr diagnostic rather than writing silently. Once a destination is selected
and the write succeeds, a zero-byte artifact with no report adds nothing to
either stream.

Who owns what:

| Concern | Owner |
| --- | --- |
| Artifact bytes | Application |
| Suggested destination | Application (a suggestion) |
| Semantic report and warning taxonomy | Application |
| Destination selection | Framework |
| The write and its failure | Framework |
| Receipt (completed destination) | Framework |

#### Destination policy

Standout selects the destination deterministically:

1. the explicit `--output-file-path` override;
2. the artifact's `suggest_destination(...)`, if the application opted in;
3. stdout, if the application opted in with `allow_stdout()`.

If none applies, the run fails with `FinalWrite(Artifact)` rather than inventing
a file or dropping the bytes. All three steps share that one failure path.

#### Write first, report second

Standout writes, then renders the report from a fixed envelope:

```json
{
  "report": { "exported": 12, "warnings": [] },
  "receipt": { "destination": "/tmp/report.csv", "stdout": false, "byte_count": 480 }
}
```

So a template can say what only the framework knows:

```jinja
Exported {{ report.exported }} rows to {{ receipt.destination }}
```

The envelope shape is fixed (`report` + `receipt`) whatever the report's type,
so no application key can collide with the receipt. Structured modes serialize
the same envelope. A failed write renders nothing: success cannot outrun the
write that justifies it.

#### The report channel

Mixing a report into the bytes would corrupt them, so the channel follows the
destination:

| Artifact destination | Report goes to |
| --- | --- |
| File | stdout |
| Stdout (`allow_stdout()`) | stderr |

#### Hooks and artifacts

Post-dispatch hooks see the report as ordinary handler data. Post-output hooks
see `RenderedOutput::Artifact` and can still transform the bytes or the report
via `as_artifact_mut()`. Hooks never perform the write — that stays framework-
owned, which is what keeps the failure path single and the report honest.

Bytes are owned; streaming is deliberately not part of this contract.

---

## Incremental commands

`Handler` carries two associated types. `Output` is the batch value or the
summary; `Event` is the type of the values the command produces while it runs.
A batch command sets `Event = NoEvents`, whose uninhabited type leaves `emit`
with no argument that can be constructed, and ignores the third parameter.

```rust,ignore
pub trait Handler {
    type Event: Serialize + 'static;
    type Output: Serialize;

    fn handle(
        &mut self,
        matches: &ArgMatches,
        ctx: &CommandContext,
        results: &mut Results<Self::Event>,
    ) -> HandlerResult<Self::Output>;
}
```

`emits_events::<H::Event>()` is how the consuming framework asks whether a
command is incremental: every event type but `NoEvents` says it is. The fact
comes from the handler's own signature rather than a declaration beside it, so
no handler can say one thing and do another, and the framework can decide
before the handler runs what counting one invocation's events could only tell
it afterwards. It decides at build time that the command needs its
`<name>.event` template; and it refuses an incremental command under an
encoding that carries a command's results as one document before the handler
runs.

`Event` is `'static` because an associated type carries no lifetime from
`handle`'s parameters: an event holding a borrow of something the handler was
passed has no way to name it here.

`Results::emit` takes the event by value, returns once the framework has
retained it and written it, and fails when the value does not serialize
(`EmitError::Serialize`), the destination cannot turn it into bytes
(`EmitError::Render`) or cannot write them (`EmitError::Write`). `Serialize` is
the whole bound `Results` adds: an event may hold an `Rc` or anything else that
does not cross threads.

`Results` is `&mut` and not `Clone`, so a handler cannot store it or keep
emitting past its own run, and it exposes `emit` and nothing else: a handler
cannot ask which representation is running or where the bytes go.

The adapter behind a two-argument closure (`FnHandler`) sets `Event = NoEvents`;
`EventsFnHandler` is the adapter behind a three-argument closure taking
`&mut Results<E>`, and that closure returns `Summary<T>` — `Render` or
`Silent`, with an exit status — rather than `Output<T>`, so a payload from a
command that declares events does not compile. `#[handler]` picks between the
two adapters, and between the two return types, from whether the function
declares a `Results` parameter.

`RunRecorder` is the framework's own retention of the same values, passed to
the dispatch closure rather than held on the context, so a handler cannot
record values the channel never saw. `EventSink` is what the consuming
framework implements to render or frame an event for a representation.

---

## CommandContext

`CommandContext` provides execution environment information and state access:

```rust,ignore
pub struct CommandContext {
    pub command_path: Vec<String>,
    pub app_state: Rc<Extensions>,
    pub extensions: Extensions,
}
```

**command_path**: The subcommand chain as a vector, e.g., `["db", "migrate"]`. Useful for logging or conditional logic.

**app_state**: Shared, immutable state configured at app build time via `AppBuilder::app_state()`. Held in an `Rc<Extensions>` for cheap cloning; the dispatch pipeline is single-threaded, so app state is not `Send`/`Sync`. Use for database connections, configuration, API clients.

**extensions**: Per-request, mutable state injected by pre-dispatch hooks. Use for user sessions, request IDs, computed values.

The context carries no presentation state. A command that produces its result
while it runs receives the typed results channel as the third parameter of
`Handler::handle`, not through the context; see
[Incremental commands](#incremental-commands).

> For comprehensive coverage of state management, see [App State and Extensions](app-state.md).

---

## State Access: App State vs Extensions

Handlers access state through two distinct mechanisms with different semantics:

| Aspect         | `ctx.app_state`               | `ctx.extensions`           |
| -------------- | ----------------------------- | -------------------------- |
| **Mutability** | Immutable (`&`)               | Mutable (`&mut`)           |
| **Lifetime**   | App lifetime                  | Per-request                |
| **Set by**     | `AppBuilder::app_state()`     | Pre-dispatch hooks         |
| **Use for**    | Database, Config, API clients | User sessions, request IDs |

### App State (Shared Resources)

Configure long-lived resources at build time:

```rust,ignore
App::builder()
    .app_state(Database::connect()?)
    .app_state(Config::load()?)
    .command("list", list_handler, template)?
    .build()?
```

Access in handlers via `ctx.app_state`:

```rust,ignore
fn list_handler(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Vec<Item>> {
    let db = ctx.app_state.get_required::<Database>()?;
    let config = ctx.app_state.get_required::<Config>()?;

    let items = db.query_items(config.max_results)?;
    Ok(Output::Render(items))
}
```

### Extensions (Per-Request State)

Pre-dispatch hooks inject request-scoped state:

```rust,ignore
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

```rust,ignore
fn list_handler(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Vec<Item>> {
    let db = ctx.app_state.get_required::<Database>()?;       // shared
    let scope = ctx.extensions.get_required::<UserScope>()?;  // per-request

    let items = db.list_for_user(&scope.user_id)?;
    Ok(Output::Render(items))
}
```

### Extensions API

Both `app_state` and `extensions` use the same `Extensions` type with these methods:

| Method              | Description                                     |
| ------------------- | ----------------------------------------------- |
| `insert<T>(value)`  | Insert a value, returns previous if any         |
| `get<T>()`          | Get immutable reference, returns `Option<&T>`   |
| `get_required<T>()` | Get reference or return error if missing        |
| `get_mut<T>()`      | Get mutable reference, returns `Option<&mut T>` |
| `remove<T>()`       | Remove and return value                         |
| `contains<T>()`     | Check if type exists                            |
| `len()`             | Number of stored values                         |
| `is_empty()`        | True if no values stored                        |
| `clear()`           | Remove all values                               |

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

```rust,ignore
// App state: configured once at build time
App::builder()
    .app_state(Database::connect()?)  // Shared via Rc
    .hooks("users.list", Hooks::new()
        .pre_dispatch(|matches, ctx| {
            // Extensions: computed per-request, can use app_state
            let db = ctx.app_state.get_required::<Database>()?;
            let user = authenticate(matches, db)?;
            ctx.extensions.insert(user);
            Ok(())
        }))?
```

> For comprehensive coverage of state management patterns, see [App State and Extensions](app-state.md).

---

## Accessing CLI Arguments

The `ArgMatches` parameter provides access to parsed arguments through clap's standard API:

```rust,ignore
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

Because handlers have explicit inputs and outputs, their adapter behavior is
straightforward to test directly. Test validation, filtering, and state
transitions through the CLI-free library instead:

```rust,ignore
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

```rust,ignore
#[test]
fn test_handler_with_app_state() {
    use std::rc::Rc;

    // Create test fixtures
    let mock_db = MockDatabase::with_items(vec![
        Item { id: "1", name: "Test" }
    ]);

    // Build app_state with test data
    let mut app_state = Extensions::new();
    app_state.insert(mock_db);

    let ctx = CommandContext {
        command_path: vec!["list".into()],
        app_state: Rc::new(app_state),
        ..Default::default()
    };

    let cmd = Command::new("test");
    let matches = cmd.try_get_matches_from(["test"]).unwrap();

    let result = list_handler(&matches, &ctx);
    assert!(result.is_ok());
}
```

### Testing Handlers with Mutable State

Handler tests can verify state mutation across calls:

```rust,ignore
#[test]
fn test_handler_state_mutation() {
    struct Counter { count: u32 }

    impl Handler for Counter {
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
