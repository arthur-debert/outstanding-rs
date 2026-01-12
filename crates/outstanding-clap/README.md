# outstanding-clap

Batteries-included integration of `outstanding` with `clap`. This crate provides styled CLI output with minimal setup.

## Installation

```toml
[dependencies]
outstanding-clap = "0.12"
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

### With Embedded Styles

Use YAML stylesheets with the `macros` feature:

```toml
[dependencies]
outstanding-clap = { version = "0.12", features = ["macros"] }
```

Create a stylesheet in `styles/default.yaml`:
```yaml
item:
  fg: cyan
header:
  fg: white
  bold: true
```

Then use it:

```rust
use clap::Command;
use outstanding_clap::{Outstanding, CommandResult, embed_styles};
use serde::Serialize;

#[derive(Serialize)]
struct ListOutput { items: Vec<String> }

fn main() {
    let cmd = Command::new("my-app")
        .subcommand(Command::new("list"));

    Outstanding::builder()
        .styles(embed_styles!("styles"))
        .default_theme("default")
        .command("list", |_m, _ctx| {
            CommandResult::Ok(ListOutput {
                items: vec!["apple".into(), "banana".into()],
            })
        }, "{% for item in items %}- {{ item | style(\"item\") }}\n{% endfor %}")
        .run_and_print(cmd, std::env::args());
}
```

### With Help Topics

Add extended documentation from markdown or text files:

```rust
Outstanding::builder()
    .topics_dir("docs/topics")
    .run(cmd);
```

Users access via:
```bash
my-app help topics           # List all topics
my-app help getting-started  # View specific topic
```

## Output Modes

Users control output format via `--output`:

```bash
my-app list                  # Rendered template (default)
my-app list --output=json    # JSON serialization
my-app list --output=yaml    # YAML serialization
my-app list --output=text    # Plain text (no ANSI codes)
```

## Command Handlers

Register handlers that return serializable data:

```rust
.command("list", |matches, ctx| {
    // matches: &ArgMatches - parsed arguments
    // ctx: &CommandContext - command path, output mode, etc.

    let items = fetch_items();
    CommandResult::Ok(ListOutput { items })
}, "{% for item in items %}{{ item }}\n{% endfor %}")
```

### CommandResult Variants

| Variant | Description |
|---------|-------------|
| `Ok(data)` | Success with serializable data |
| `Err(error)` | Error to display |
| `Silent` | No output |
| `Archive(bytes, filename)` | Binary file output |

## Hooks

Run custom code before/after command execution:

```rust
use outstanding_clap::Hooks;

Outstanding::builder()
    .command("export", handler, template)
    .hooks("export", Hooks::new()
        .pre_dispatch(|_m, ctx| {
            println!("Running: {:?}", ctx.command_path);
            Ok(())
        })
        .post_output(|_m, _ctx, output| {
            // Transform output, copy to clipboard, etc.
            Ok(output)
        }))
    .run_and_print(cmd, args);
```

## Embed Macros

The `embed_styles!` macro embeds stylesheets at compile time with debug hot-reload:

- **At compile time**: Walk directory and embed all `.yaml`/`.yml` files
- **In debug mode**: If source directory exists, read from disk (hot-reload)
- **In release mode**: Use embedded content (zero file I/O)

```rust
// Embeds all .yaml, .yml files from styles/
.styles(embed_styles!("styles"))
.default_theme("default")  // Use styles/default.yaml
```

## Documentation

- **[Using Outstanding with Clap](docs/using-with-clap.md)** - Complete guide
- **[Handler Hooks](docs/hooks.md)** - Pre/post execution hooks

## License

MIT
