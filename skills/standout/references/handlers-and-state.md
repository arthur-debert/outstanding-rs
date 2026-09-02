# Handlers and state

Use the `#[handler]` macro to keep clap extraction outside the typed function:

```rust
use serde::Serialize;
use standout::cli::{CommandContext, Output};
use standout::handler;
use todo_core::{Todo, TodoFilter, TodoStore};

#[derive(Serialize)]
pub struct TodoView {
    pub id: u32,
    pub title: String,
    pub done: bool,
}

impl From<Todo> for TodoView {
    fn from(todo: Todo) -> Self {
        Self { id: todo.id, title: todo.title, done: todo.done }
    }
}

#[derive(Serialize)]
pub struct TodoListView {
    pub todos: Vec<TodoView>,
    pub total: usize,
}

#[handler]
pub fn list(
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

Supported parameter annotations are `#[flag]` for booleans, `#[arg]` for typed required/optional/vector values, `#[ctx]`, and `#[matches]`. `name = "cli-name"` overrides the inferred flag or argument name.

The macro preserves `list(all, ctx)` and generates `list__handler(matches, ctx)` plus argument-verification metadata. Wire the generated wrapper; call the typed function in unit tests:

```rust
let Output::Render(result) = list(false, &ctx).unwrap() else {
    panic!("expected rendered data");
};
assert_eq!(result.total, result.todos.len());
```

Handlers are CLI adapters, not the home of reusable application behavior. A
handler may map a flag to a library type, obtain a dependency from app state,
call the library, and map the result to a CLI-owned view DTO. Validation,
filtering rules, and state transitions belong behind the library interface.

## Output contract

`Output<T>` is non-exhaustive; its shapes:

- `Output::Render(data)` renders a template or serializes `data` in a structured mode.
- `Output::Silent` completes without output.
- `Output::Binary { data, filename }` returns bytes and a suggested filename.
- `Output::Artifact(artifact)` returns bytes the framework writes plus a report rendered after the write.
- `Output::WithStatus`, built by `with_exit_status`, is a `Render` or `Silent` with a declared exit status.

Do not branch presentation in a handler. `CommandContext` contains `command_path`, `app_state`, per-dispatch `extensions`, and `stream`; it deliberately does **not** contain `output_mode`.

## State boundaries

Register long-lived values once with `.app_state(value)` and retrieve them by concrete type with `ctx.app_state.get_required::<T>()`. Use interior mutability when shared state must mutate.

Construct those dependencies before app assembly. Environment lookup and
configuration-file conventions belong in the CLI; pass explicit values such as
paths or URLs into the CLI-free library.

Inject request-only values in a pre-dispatch hook with `ctx.extensions.insert(value)` and retrieve them with `ctx.extensions.get_required::<T>()`. Declarative named inputs use a typed bag in extensions; access those through `CommandContextInput::input`, not directly.

For full signatures and verification behavior, inspect `crates/standout-dispatch/src/handler.rs`, `crates/standout-macros/src/handler.rs`, and `crates/standout/tests/handler_macro.rs`.
