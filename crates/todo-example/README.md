# Todo: a production-shaped Standout example

This is the canonical worked Standout application. It deliberately uses two
Rust packages so the source tree teaches where reusable application behavior
ends and shell-specific behavior begins.

- [`todo-core`](todo-core/) is a CLI-free library. It owns the todo model,
  validation, filtering, state transitions, and concrete JSON persistence.
- [`tdoo`](tdoo/) is a binary-only CLI. It owns Clap, Standout, environment
  lookup, handlers, view models, templates, styles, hooks, and process output.

The user-visible application is unchanged: `tdoo add`, `tdoo list`, and
`tdoo done` operate on a JSON store and support Standout's structured output
modes. `tdoo export` adds the compound-artifact boundary: the core produces CSV
bytes and typed warnings, and Standout owns the destination and the write.

## Source map

```text
crates/todo-example/
├── README.md
├── todo-core/
│   ├── Cargo.toml             No clap or Standout dependencies
│   └── src/
│       ├── lib.rs             Small public library interface
│       ├── model.rs           Todo and TodoFilter
│       ├── export.rs          CSV bytes + typed warnings, no filesystem
│       └── store.rs           Validation, behavior, JSON persistence, tests
└── tdoo/
    ├── Cargo.toml             Binary package; depends on todo-core + Standout
    └── src/
        ├── main.rs            Resolve dependencies, build, run
        ├── cli.rs             Clap model and environment lookup
        ├── app.rs             Standout App, InputChain, hook, harness tests
        ├── handlers.rs        Thin adapters, view DTOs, typed-handler tests
        ├── templates/         MiniJinja views
        └── styles/            Semantic CSS theme
```

There is intentionally no `tdoo/src/lib.rs`. A library target created only so
integration tests can import `build_app` would blur the lesson. The binary's
module-local tests can call the same app builder and typed handlers directly.

## The core library interface

`todo-core` accepts its persistence path explicitly and exposes the behavior a
non-CLI caller would need:

```rust
use todo_core::{TodoFilter, TodoStore};

let store = TodoStore::load("todos.json")?;
let todo = store.add("write the docs")?;
store.mark_done(todo.id)?;
let pending = store.list(TodoFilter::Pending);
# Ok::<(), anyhow::Error>(())
```

The library has no knowledge of:

- command-line flags or argument parsing;
- Standout handlers, contexts, output modes, templates, or styles;
- environment variables or home-directory conventions;
- terminal wording or structured CLI response shapes.

That absence is an enforceable dependency rule, not merely a file-placement
preference. `todo-core/Cargo.toml` contains only ordinary Rust data and
persistence dependencies.

`TodoStore` is a deep module: validation, ID assignment, filtering, transition
rules, directory creation, serialization, and snapshot-before-save behavior
sit behind `load`, `add`, `list`, and `mark_done`. Callers and tests use the same
interface.

## The CLI package

The `tdoo` package adapts the shell to the core library.

### `cli.rs`: shell input and configuration

Clap's `Cli` and `Commands` types live here. So does `resolve_store_path`, which
maps `TODO_FILE` or the platform home directory (`$HOME`, then `%USERPROFILE%`)
into the explicit path required by `TodoStore::load`. The core never reads
process environment.

### `handlers.rs`: adapters, not application logic

Handlers translate CLI concepts into core concepts, then translate core values
into CLI-owned serializable view models:

```rust
#[handler]
pub(crate) fn list(
    #[flag] all: bool,
    #[ctx] ctx: &CommandContext,
) -> Result<Output<TodoListView>, anyhow::Error> {
    let store = ctx.app_state.get_required::<TodoStore>()?;
    let filter = if all { TodoFilter::All } else { TodoFilter::Pending };
    let todos: Vec<_> = store.list(filter).into_iter().map(TodoView::from).collect();
    let total = todos.len();
    Ok(Output::Render(TodoListView { todos, total }))
}
```

The `--all` flag is a CLI concern; pending-versus-all filtering behavior is a
core concern. `TodoView` is also owned by the CLI so changing the persisted
domain representation does not silently change `tdoo --output json`.

Handlers do not print or render. They return `Output::Render` data for Standout
to send through either a template or a structured serializer.

### `export`: the compound-artifact boundary

`tdoo export` is where "the core owns behavior, the shell owns the transaction"
becomes concrete for a command that writes a file. `todo-core` renders the CSV
bytes, suggests the name `todos.csv`, and returns typed `ExportWarning`s (for
example, completed todos the filter omitted). It writes nothing:
`store.export_csv(...)` returns a `CsvExport`, never a path.

The `export` handler maps that into a view model and states *which* destination
opt-ins apply — a suggested filename by default, or `allow_stdout()` under
`--stdout` — and returns `Output::Artifact`. It still opens no file and words no
success message. Standout then:

1. selects the destination (`--output-file-path` override, else the suggestion,
   else opted-in stdout);
2. performs the write, sharing one typed failure path (`FinalWrite(Artifact)`);
3. renders `export.jinja` *after* the write, with a `receipt` naming where the
   bytes actually landed.

Because the report renders only after a successful write, `tdoo export` can never
claim a file it did not produce, and the warning taxonomy stays the core's while
the wording stays the CLI's — visible in both the human report and
`--output json`. The report channel follows the bytes: for a file it prints on
stdout, and for `--stdout` it moves to stderr so it cannot corrupt the CSV.

### `cli.rs` and `app.rs`: declaration and assembly

`cli.rs` holds both halves of the command set: clap's derive declares the
`Command`, and `#[derive(Dispatch)]` on the same enum registers each variant
against the handler of the same name. `pure` points the registration at the
wrapper `#[handler]` generates, `inputs` names the function that declares the
command's input chains, and `post_dispatch` names the hook the mutation runs
through:

```rust
#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = crate::handlers)]
pub(crate) enum Commands {
    /// Add a new todo. Title comes from --title or piped stdin.
    #[dispatch(
        pure,
        inputs = crate::handlers::add_inputs,
        post_dispatch = crate::handlers::audit_hook
    )]
    Add {
        #[arg(short, long)]
        title: Option<String>,
    },
    // list, done and export omitted here; see tdoo/src/cli.rs
}
```

```rust
// handlers.rs
pub(crate) fn add_inputs<H>(config: CommandConfig<H>) -> CommandConfig<H> {
    config.input(
        "title",
        InputChain::<String>::new()
            .try_source(ArgSource::new("title"))
            .try_source(StdinSource::new())
            .validate(|title: &String| !title.trim().is_empty(), "title cannot be empty"),
    )
}
```

`app.rs` is then wiring and nothing else: application state, the version clap
reports, the invocation policy for a bare `tdoo`, the presentation assets, and
the one call that registers the whole command set.

```rust
App::builder()
    .app_state(store)
    .version(env!("CARGO_PKG_VERSION"))
    .default_command_with(|ctx| {
        Some(if ctx.stdin_is_piped() { "add" } else { "list" }.to_string())
    })
    .templates(embed_templates!("src/templates"))
    .styles(embed_styles!("src/styles"))
    .default_theme("todo")
    .commands(Commands::dispatch_config())?
    .build()?;
```

No command names its template: a command's template is the registry entry its
path resolves to, so `add` renders `src/templates/add.jinja`. The InputChain
provides early CLI feedback, while `TodoStore::add` repeats the essential
non-empty-title invariant for every caller. The `TODO_AUDIT_LOG` hook lives
beside the handlers it wraps, because environment-driven audit output is a
cross-cutting shell concern.

### Templates and styles: final presentation

MiniJinja templates turn the handler view models into semantic style tags.
`todo.css` controls their terminal appearance. Structured modes such as JSON
bypass templates and serialize the same view model directly.

Presentation files stay under `tdoo/src/` because they ship with the executable,
not with the reusable library.

## The testing shape

The example uses the smallest test seam for each behavior.

1. `todo-core/src/store.rs` contains fast tests for validation, ID assignment,
   filtering, state transitions, persistence, missing IDs, and failed saves.
2. `tdoo/src/handlers.rs` directly calls the typed functions preserved by
   `#[handler]` and checks adapter/view-model behavior.
3. `tdoo/src/app.rs` uses `TestHarness` only for the argv-to-output pipeline:
   templates, piped stdin, InputChain validation, structured output, and hooks.

The harness tests are internal `#[cfg(test)]` modules and use `#[serial]`
because their controlled environment seams are process-global. No subprocess is
needed; this example has no process-only behavior that the harness cannot model.

## Trying it

```bash
# Default store path is .todos.json under $HOME or %USERPROFILE%; override it per run.
TODO_FILE=/tmp/tdoo.json cargo run -p tdoo -- add --title "buy milk"
TODO_FILE=/tmp/tdoo.json cargo run -p tdoo -- list
TODO_FILE=/tmp/tdoo.json cargo run -p tdoo -- done 1
TODO_FILE=/tmp/tdoo.json cargo run -p tdoo -- list --all

# Input chain fallback.
echo "write tests" | TODO_FILE=/tmp/tdoo.json cargo run -p tdoo -- add

# Standout output modes.
TODO_FILE=/tmp/tdoo.json cargo run -p tdoo -- list --output json
TODO_FILE=/tmp/tdoo.json cargo run -p tdoo -- list --output yaml
TODO_FILE=/tmp/tdoo.json cargo run -p tdoo -- list --output text

# Compound artifact: Standout owns the destination and the write.
TODO_FILE=/tmp/tdoo.json cargo run -p tdoo -- export                 # writes ./todos.csv
TODO_FILE=/tmp/tdoo.json cargo run -p tdoo -- export --output-file-path /tmp/todos.csv
TODO_FILE=/tmp/tdoo.json cargo run -p tdoo -- export --stdout > /tmp/todos.csv  # report on stderr

# Test the reusable library and the CLI independently.
cargo test -p todo-core
cargo test -p tdoo
```

## Deliberate omissions

The example does not add an abstract repository trait: there is only one
persistence adapter, so that seam would be hypothetical. It also does not add a
third "application" package or a CLI library target. Those layers would add
interface without hiding meaningful complexity.

For a compact, single-package introduction, see the
[minimal single-crate example](../../docs/guides/minimal-single-crate.md). Use
this two-package application when deciding where production code should live.
