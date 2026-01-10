# outstanding-clap

Batteries-included integration of `outstanding` with `clap`. This crate provides a complete solution for CLI output management:

- **Command handlers** - Map commands to handlers with automatic rendering
- **Styled help** - Beautiful help output using outstanding templates
- **Output modes** - `--output=<auto|term|text|json>` flag on all commands
- **Help topics** - Extended documentation system (`help <topic>`, `help topics`)
- **Pager support** - Automatic paging for long content

## Installation

```toml
[dependencies]
outstanding-clap = "0.5"
clap = "4"
serde = { version = "1", features = ["derive"] }
```

## Quick Start

### Simplest Usage

```rust
use clap::Command;
use outstanding_clap::Outstanding;

let matches = Outstanding::run(Command::new("my-app"));
```

Your CLI now has styled help and an `--output` flag.

### With Command Handlers

```rust
use clap::Command;
use outstanding_clap::{Outstanding, CommandResult};
use serde::Serialize;

#[derive(Serialize)]
struct ListOutput {
    items: Vec<String>,
}

fn main() {
    let cmd = Command::new("my-app")
        .subcommand(Command::new("list").about("List all items"));

    Outstanding::builder()
        .command("list", |_matches, _ctx| {
            CommandResult::Ok(ListOutput {
                items: vec!["apple".into(), "banana".into()],
            })
        }, "{% for item in items %}- {{ item }}\n{% endfor %}")
        .run_and_print(cmd, std::env::args());
}
```

Now your CLI supports:
```bash
my-app list              # Rendered template output
my-app list --output=json # JSON output
```

## Adoption Models

Outstanding supports three adoption levels:

### Full Adoption
Register all commands with handlers. Outstanding manages rendering.

### Partial Adoption
Register some commands, handle others manually:

```rust
match builder.dispatch_from(cmd, args) {
    RunResult::Handled(output) => println!("{}", output),
    RunResult::Unhandled(matches) => {
        // Handle legacy commands manually
    }
}
```

### Output-Only
Use Outstanding just for rendering in your existing code:

```rust
use outstanding::{render_or_serialize, OutputMode};

let output = render_or_serialize(template, &data, theme, mode)?;
```

## Help Topics

Add extended documentation:

```rust
Outstanding::builder()
    .topics_dir("docs/topics")  // Load .txt and .md files
    .run(cmd);
```

Users access via:
```bash
my-app help topics     # List all topics
my-app help <topic>    # View specific topic
```

## Handler Hooks

Run custom code before and after command execution:

```rust
use outstanding_clap::{Outstanding, Hooks, Output};
use serde_json::json;

Outstanding::builder()
    .command("export", handler, template)
    .hooks("export", Hooks::new()
        .pre_dispatch(|_m, ctx| {
            println!("Running: {:?}", ctx.command_path);
            Ok(())
        })
        .post_dispatch(|_m, _ctx, mut data| {
            // Modify data before rendering
            if let Some(obj) = data.as_object_mut() {
                obj.insert("processed".into(), json!(true));
            }
            Ok(data)
        })
        .post_output(|_m, _ctx, output| {
            // Copy to clipboard, log, transform, etc.
            if let Output::Text(ref text) = output {
                // clipboard::copy(text)?;
            }
            Ok(output)
        }))
    .run_and_print(cmd, args);
```

- **Pre-dispatch**: Run before handler, can abort execution
- **Post-dispatch**: Run after handler but before rendering, can modify data
- **Post-output**: Run after rendering, can transform output
- **Per-command**: Different hooks for different commands
- **Chainable**: Multiple hooks at the same phase run in order

See [docs/hooks.md](docs/hooks.md) for full documentation.

## Documentation

For comprehensive documentation, see:

- **[Using Outstanding with Clap](docs/using-with-clap.md)** - Complete guide covering:
  - All adoption models with examples
  - Command handlers and templates
  - Output modes and themes
  - Help topics configuration
  - Best practices

- **[Handler Hooks](docs/hooks.md)** - Pre/post command execution hooks for:
  - Logging and metrics
  - Clipboard operations
  - Output transformation
  - Validation and access control

- **[Architecture & Design](../../docs/proposals/fullapi-consolidated.md)** - Technical deep dive for contributors

## API Overview

### OutstandingBuilder

| Method | Description |
|--------|-------------|
| `.command(path, handler, template)` | Register command with closure |
| `.command_handler(path, handler, template)` | Register command with struct handler |
| `.hooks(path, hooks)` | Register hooks for a command |
| `.topics_dir(path)` | Load help topics from directory |
| `.theme(theme)` | Set custom theme |
| `.output_flag(Some("format"))` | Rename `--output` flag |
| `.no_output_flag()` | Disable output flag |

### Dispatch Methods

| Method | Returns | Use Case |
|--------|---------|----------|
| `.run_and_print(cmd, args)` | `bool` | Complete flow: parse, dispatch, print |
| `.dispatch_from(cmd, args)` | `RunResult` | Parse and dispatch, you handle output |
| `.dispatch(matches, mode)` | `RunResult` | You provide parsed matches |
| `.run_command(path, matches, handler, template)` | `Result<Output, HookError>` | Regular API with hooks |

### CommandResult Variants

| Variant | Description |
|---------|-------------|
| `Ok(data)` | Success with serializable data |
| `Err(error)` | Error to display |
| `Silent` | No output |
| `Archive(bytes, filename)` | Binary file output |

## License

MIT
