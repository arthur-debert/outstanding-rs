# Hooks, inputs, and piping

Use these extension points to keep cross-cutting shell concerns out of handlers.

## Hooks

Hooks run in registration order and stop on the first `HookError`:

- `pre_dispatch(&ArgMatches, &mut CommandContext)` validates, authenticates, or inserts request state before the handler.
- `post_dispatch(&ArgMatches, &CommandContext, serde_json::Value)` transforms the serialized handler data before rendering.
- `post_output(&ArgMatches, &CommandContext, RenderedOutput)` transforms or observes text/binary/silent output after rendering.

Attach hooks through `command_with`, derive attributes, or
`.hooks(path, Hooks::new()...)`. Keep reusable behavior in the CLI-free library,
keep handlers as adapters, and use hooks only when the shell concern crosses
commands or pipeline phases.

## Declarative inputs

An `InputChain<T>` tries sources in order, validates the resolved value, and runs as pre-dispatch configuration:

```rust
use standout::cli::CommandContextInput;
use standout::input::{ArgSource, InputChain, StdinSource};

let chain = InputChain::<String>::new()
    .try_source(ArgSource::new("title"))
    .try_source(StdinSource::new())
    .validate(|s| !s.trim().is_empty(), "title cannot be empty");

let app = App::builder()
    .command_with("add", handlers::add__handler, |cfg| {
        cfg.input("title", chain).template("add.jinja")
    })?;

// In the handler:
let title: &String = ctx.input("title")?;
```

Sources include clap args/flags, environment, stdin, clipboard, defaults, editor, and interactive prompts. Test their process-global seams through `TestHarness`; script interactive sources with a `PromptResponder`.

## Piping

Pipes are post-output hooks for text output:

- `.pipe_to(command)` sends plain text to a command and preserves original output.
- `.pipe_through(command)` replaces output with the command's stdout.
- `.pipe_to_clipboard()` consumes output after copying it.

Multiple pipes chain in registration order. Binary and silent output pass through unchanged. Commands execute through the platform shell, so keep command strings fixed; never interpolate untrusted input.

Inspect `crates/standout-dispatch/src/hooks.rs`, `crates/standout-input/docs/topics/framework-integration.md`, and `crates/standout-pipe/docs/topics/piping.md` for detailed APIs.
