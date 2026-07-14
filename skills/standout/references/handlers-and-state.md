# Handlers and state

Use the `#[handler]` macro to keep clap extraction outside the typed function:

```rust
use serde::Serialize;
use standout::cli::{CommandContext, Output};
use standout::handler;

#[derive(Serialize)]
pub struct ListResult {
    pub items: Vec<Item>,
    pub total: usize,
}

#[handler]
pub fn list(
    #[flag] all: bool,
    #[ctx] ctx: &CommandContext,
) -> Result<Output<ListResult>, anyhow::Error> {
    let store = ctx.app_state.get_required::<Store>()?;
    let items = store.list(all)?;
    let total = items.len();
    Ok(Output::Render(ListResult { items, total }))
}
```

Supported parameter annotations are `#[flag]` for booleans, `#[arg]` for typed required/optional/vector values, `#[ctx]`, and `#[matches]`. `name = "cli-name"` overrides the inferred flag or argument name.

The macro preserves `list(all, ctx)` and generates `list__handler(matches, ctx)` plus argument-verification metadata. Wire the generated wrapper; call the typed function in unit tests:

```rust
let Output::Render(result) = list(false, &ctx).unwrap() else {
    panic!("expected rendered data");
};
assert_eq!(result.total, result.items.len());
```

## Output contract

`Output<T>` has exactly these shapes:

- `Output::Render(data)` renders a template or serializes `data` in a structured mode.
- `Output::Silent` completes without output.
- `Output::Binary { data, filename }` returns bytes and a suggested filename.

Do not branch presentation in a handler. `CommandContext` contains `command_path`, `app_state`, and per-dispatch `extensions`; it deliberately does **not** contain `output_mode`.

## State boundaries

Register long-lived values once with `.app_state(value)` and retrieve them by concrete type with `ctx.app_state.get_required::<T>()`. Use interior mutability when shared state must mutate.

Inject request-only values in a pre-dispatch hook with `ctx.extensions.insert(value)` and retrieve them with `ctx.extensions.get_required::<T>()`. Declarative named inputs use a typed bag in extensions; access those through `CommandContextInput::input`, not directly.

For full signatures and verification behavior, inspect `crates/standout-dispatch/src/handler.rs`, `crates/standout-macros/src/handler.rs`, and `crates/standout/tests/handler_macro.rs`.
