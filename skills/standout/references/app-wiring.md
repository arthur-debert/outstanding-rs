# App and command wiring

Build the whole environment before dispatch: state, embedded presentation assets, theme, commands, and hooks.

```rust
use standout::cli::App;
use standout::{embed_styles, embed_templates};

let app = App::builder()
    .app_state(store)
    .templates(embed_templates!("src/templates"))
    .styles(embed_styles!("src/styles"))
    .default_theme("todo")
    .command_with("list", handlers::list__handler, |cfg| {
        cfg.template("list.jinja")
    })?
    .build()?;
```

Use `command_with` when a command needs an explicit template, hook, input chain, or pipe. A plain `.command(path, handler, template)` is the direct builder form. Dot-separated paths register nested commands.

## Derive wiring

Prefer `#[derive(Dispatch)]` when clap variants map to handlers by convention:

```rust
#[derive(clap::Subcommand, standout::cli::Dispatch)]
#[dispatch(handlers = handlers)]
enum Commands {
    #[dispatch(pure)]
    List,
    #[dispatch(pure, template = "add.jinja")]
    Add,
}

let app = App::builder()
    .commands(Commands::dispatch_config())?
    .build()?;
```

By default, `List` maps to `handlers::list`. Mark a variant `#[dispatch(pure)]` when its function uses `#[handler]`; the derive then selects `handlers::list__handler`. Variant attributes also attach templates, hooks, nested dispatch, defaults, and pipes.

## Running and partial adoption

`App::run(command, args)` parses, dispatches, prints, and returns `true` when Standout handled the command. It returns `false` for an unmatched command:

```rust
if !app.run(Cli::command(), std::env::args()) {
    run_legacy_path();
}
```

Use `run_to_string` when code needs rendered output, errors, binary bytes, or the unmatched `ArgMatches`. Match `RunResult::{Handled, Binary, Silent, Error, NoMatch}` and include a wildcard because the enum is non-exhaustive.

Embedded resources are compile-time bundles; debug builds re-read the original source path when available. Explicit template names include their extension. Convention-based resolution uses the configured extension (`.j2` by default).

Evidence: `crates/todo-example/src/lib.rs`, `crates/standout/src/cli/builder/`, and `crates/standout-macros/src/dispatch.rs`.
