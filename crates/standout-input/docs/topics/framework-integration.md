# Framework Integration

This page describes how `standout-input` plugs into the [`standout`](https://crates.io/crates/standout) CLI framework so that input chains become a declarative part of your command configuration. If you only want to use `standout-input` standalone, see [Introduction to Input](../guides/intro-to-input.md) — the framework integration is purely additive.

Questionnaire commands use the same pre-dispatch integration model but get a
larger generated surface: `questions`, `--answers`, `--yes`, typed filling, and
the attended confirmation gate. See
[Derived Questionnaires](../../../guides/derived-questionnaires.md) for that
path.

---

## The Picture

Without framework integration, a handler resolves chains imperatively. Pass the run's [`InputSources`](https://docs.rs/standout-input/latest/standout_input/struct.InputSources.html) so stdin, clipboard, and prompt responders match the invocation:

```rust
fn create(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Pad> {
    let body = InputChain::<String>::new()
        .try_source(ArgSource::new("body"))
        .try_source(StdinSource::new())
        .try_source(EditorSource::new())
        .resolve_from(matches, ctx.input_sources())?;          // <-- handler does this itself

    /* business logic ... */
}
```

That works, but the chain becomes invisible to anyone reading the command's registration: input rules are mixed in with logic, and you can't see at a glance "this command takes a body that may come from arg / stdin / editor".

With the integration, the chain is part of `CommandConfig`, just like `template`, `hooks`, and `pipe_through`:

```rust
use standout::cli::{App, CommandContextInput, Output};
use standout::input::{ArgSource, EditorSource, InputChain, StdinSource};

App::builder()
    .commands(|g| {
        g.command_with("create", create, |cfg| {
            cfg.template_name("create")
                .input("body", InputChain::<String>::new()
                    .try_source(ArgSource::new("body"))
                    .try_source(StdinSource::new())
                    .try_source(EditorSource::new()))
        })
    })?
    .build()?;

fn create(_m: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Value> {
    let body: &String = ctx.input("body")?;     // <-- already resolved
    /* business logic ... */
}
```

The chain runs in the **pre-dispatch** phase — before the handler is called — so handlers always see fully-resolved input. Errors during resolution (validation failure, editor cancelled, …) abort the request before any business logic runs.

---

## Where Resolution Happens

`standout`'s execution pipeline runs hooks in three phases:

```text
parsed CLI args → PRE-DISPATCH → handler → POST-DISPATCH → render → POST-OUTPUT
```

`.input(name, chain)` is sugar over `.pre_dispatch(...)` — the same hook used for auth checks, request-scoped state, etc. Each `.input(...)` call adds one pre-dispatch hook that:

1. Walks to the deepest subcommand's `ArgMatches` (so chains see the same args the handler does).
2. Calls `chain.resolve_from_with_source(matches, ctx.input_sources())`.
3. Stashes the result in an [`Inputs`](https://docs.rs/standout-input/latest/standout_input/struct.Inputs.html) bag on `ctx.extensions` under `name`.

If resolution returns an error, dispatch stops and the framework reports `` Hook error: input `body`: <error message> ``. The handler does not run.

---

## Reading Inputs in the Handler

Bring the [`CommandContextInput`](https://docs.rs/standout/latest/standout/cli/trait.CommandContextInput.html) extension trait into scope and call `.input::<T>(name)`:

```rust
use standout::cli::{CommandContextInput, Output};

fn create(_m: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Value> {
    let body: &String = ctx.input("body")?;
    let force: &bool = ctx.input("force")?;

    /* ... */
}
```

The lookup is by `(name, T)`. If the name was never registered, you get a [`MissingInput::NotRegistered`](https://docs.rs/standout-input/latest/standout_input/enum.MissingInput.html) error. If the registered type doesn't match `T`, you get `MissingInput::TypeMismatch`. The error type implements `std::error::Error` and converts cleanly with `?`.

### Inspecting the source

Sometimes you want to know *where* an input came from — for instance, to log "title was read from clipboard" or to alter behavior when input is piped vs. interactive:

```rust
match ctx.input_source("body") {
    Some(InputSourceKind::Editor) => log::info!("body composed in editor"),
    Some(InputSourceKind::Stdin) => log::info!("body piped from stdin"),
    Some(other) => log::debug!("body came from {other}"),
    None => unreachable!("body is registered, so it was resolved"),
}
```

### Iterating all inputs

For diagnostic output (like `--explain` flags) you can grab the whole bag:

```rust
if let Some(bag) = ctx.inputs() {
    for (name, source) in bag.iter_sources() {
        eprintln!("  {name}: {source}");
    }
}
```

---

## Multiple Inputs

`.input(...)` accumulates. A command can declare any number of named inputs of any types — including multiple inputs of the same type, which the `TypeId`-keyed `ctx.app_state` / raw `ctx.extensions` cannot disambiguate:

```rust
.commands(|g| {
    g.command_with("create", create, |cfg| {
        cfg.template_name("create")
            .input("title", InputChain::<String>::new()
                .try_source(ArgSource::new("title"))
                .default("untitled".to_string()))
            .input("body", InputChain::<String>::new()
                .try_source(ArgSource::new("body"))
                .try_source(StdinSource::new())
                .try_source(EditorSource::new()))
            .input("force", InputChain::<bool>::new()
                .try_source(FlagSource::new("force"))
                .default(false))
    })
})?
```

Each chain runs in registration order during pre-dispatch. They share the same `Inputs` bag on `ctx.extensions`, so two `String` inputs (`title`, `body`) coexist without colliding.

---

## Validation

Chain-level validation runs as part of `resolve_with_source`. If validation fails on a non-interactive source, the pre-dispatch hook returns an error and dispatch aborts:

```rust
.input("body", InputChain::<String>::new()
    .try_source(ArgSource::new("body"))
    .validate(|s| !s.trim().is_empty(), "body must not be empty"))
```

If the user runs `mycli create --body "   "`, the framework reports:

```text
Hook error: input `body`: validation failed: body must not be empty
```

For interactive sources (prompts, editor), validation failure re-prompts instead of aborting — the chain decides the loop. See [Backends](backends.md) for the full validation/retry semantics.

---

## Testing

The framework path composes naturally with `standout-test`:

```rust
use standout_test::TestHarness;

#[test]
fn create_uses_arg_when_provided() {
    let app = build_app();
    let cmd = my_clap_command();

    let result = TestHarness::new()
        .text_output()
        .run(&app, cmd, ["mycli", "create", "--body", "hello"]);

    result.assert_stdout_contains("hello");
}

#[test]
fn create_falls_back_to_stdin() {
    let app = build_app();
    let cmd = my_clap_command();

    let result = TestHarness::new()
        .piped_stdin("from pipe\n")
        .text_output()
        .run(&app, cmd, ["mycli", "create"]);

    result.assert_stdout_contains("from pipe");
}
```

The harness constructs [`InputSources`](https://docs.rs/standout-input/latest/standout_input/struct.InputSources.html) with `MockStdin` / `MockClipboard` and passes them into `App::run_with`, so `StdinSource::new()` and `ClipboardSource::new()` inside a `CommandConfig::input` chain see the mocks at resolve time.

For lower-level tests that don't need the harness, pass sources into [`InputChain::resolve_from`](https://docs.rs/standout-input/latest/standout_input/struct.InputChain.html#method.resolve_from).

---

## Re-exports and Feature Flags

`standout` re-exports `standout-input` as `standout::input`, so a single dependency on `standout` is enough:

```toml
[dependencies]
standout = "8"
```

```rust
use standout::input::{ArgSource, InputChain, StdinSource};
```

A default `standout` dependency only enables `standout-input`'s `simple-prompts` backend, which has no extra deps. The heavier backends are opt-in via these `standout` features:

| Feature         | Enables                                      | Adds deps                          |
| --------------- | -------------------------------------------- | ---------------------------------- |
| `input-editor`  | `EditorSource` (opens `$VISUAL` / `$EDITOR`) | `tempfile`, `which`, `shell-words` |
| `input-inquire` | The `Inquire*` rich TUI prompt sources       | `inquire` (~29 transitive)         |

```toml
[dependencies]
standout = { version = "8", features = ["input-editor"] }
```

You can still depend on `standout-input` directly if you want to bypass the `standout` re-export and pick features there.

---

## When NOT to Use the Builder Integration

The standalone `chain.resolve(matches)?` form is still the right tool when:

- Input shape depends on already-resolved values. If `--mode` decides which other inputs to ask for, you can't precompute a static chain.
- You're adopting `standout` incrementally and your handler isn't yet on the framework path.
- You're using `standout-input` outside the `standout` framework altogether.

In every other case, `.input(...)` keeps the command's input contract visible at registration time, alongside its template and hooks.
