# How To: Adopt Standout Alongside Existing Clap Code

Adopting a new framework shouldn't require rewriting your entire application. Standout is designed for **gradual adoption**, allowing you to migrate one command at a time without breaking existing functionality.

This guide shows how to run Standout alongside your existing manual dispatch or Clap loop.

## The Core Pattern

When Standout handles a command, it prints output and returns `None`. When no handler matches, it returns `Some(ArgMatches)` for your fallback:

```rust
if let Some(matches) = app.run(cli, std::env::args()) {
    // Standout didn't handle this command, fall back to legacy
    your_existing_dispatch(matches);
}
```

## Pattern 1: Standout First, Fallback Second

Try Standout dispatch first. If no match, use your existing code:

```rust
use standout::cli::App;
use clap::Command;

fn main() {
    let cli = Command::new("myapp")
        .subcommand(Command::new("list"))     // Standout handles
        .subcommand(Command::new("status"))   // Your existing code
        .subcommand(Command::new("config"));  // Your existing code

    // Build Standout for just the commands you want
    let app = App::builder()
        .command("list", list_handler, "list.j2")
        .build()
        .expect("Failed to build app");

    // Try Standout first
    if let Some(matches) = app.run(cli, std::env::args()) {
        // Fall back to your existing dispatch
        match matches.subcommand() {
            Some(("status", sub)) => handle_status(sub),
            Some(("config", sub)) => handle_config(sub),
            _ => eprintln!("Unknown command"),
        }
    }
}
```

## Pattern 2: Existing Code First, Standout Fallback

Your dispatch handles known commands, Standout handles new ones:

```rust
fn main() {
    let cli = build_cli();
    let matches = cli.clone().get_matches();

    // Your existing dispatch first
    match matches.subcommand() {
        Some(("legacy-cmd", sub)) => {
            handle_legacy(sub);
            return;
        }
        Some(("old-feature", sub)) => {
            handle_old_feature(sub);
            return;
        }
        _ => {
            // Not handled by existing code
        }
    }

    // Standout handles everything else
    let app = build_standout_app();
    app.run(cli, std::env::args());
}
```

## Pattern 3: Standout Inside Your Match

Call Standout for specific commands within your existing match:

```rust
fn main() {
    let cli = build_cli();
    let matches = cli.clone().get_matches();

    match matches.subcommand() {
        Some(("status", sub)) => handle_status(sub),
        Some(("config", sub)) => handle_config(sub),

        // Use Standout just for these
        Some(("list", _)) | Some(("show", _)) => {
            let app = build_standout_app();
            app.run(cli, std::env::args());
        }

        _ => eprintln!("Unknown command"),
    }
}
```

## Adding Standout to One Command

Minimal setup for a single command:

```rust
let app = App::builder()
    .command("list", |matches, ctx| {
        let items = fetch_items()?;
        Ok(Output::Render(ListOutput { items }))
    }, "{% for item in items %}- {{ item }}\n{% endfor %}")
    .build()?;
```

No embedded files required. The template is inline. No theme means style tags show `?` markers, but rendering still works.

## Sharing Clap Command Definition

Standout augments your `clap::Command` with `--output` and help. You can share the definition:

```rust
fn build_cli() -> Command {
    Command::new("myapp")
        .subcommand(Command::new("list").about("List items"))
        .subcommand(Command::new("status").about("Show status"))
}

fn main() {
    let cli = build_cli();

    let app = App::builder()
        .command("list", list_handler, "list.j2")
        .build()?;

    // Standout augments the command, then dispatches
    if let Some(matches) = app.run(cli, std::env::args()) {
        // matches from the augmented command (has --output, etc.)
        match matches.subcommand() {
            Some(("status", sub)) => handle_status(sub),
            _ => {}
        }
    }
}
```

## Gradual Migration Strategy

1. **Start with one command**: Pick a command with complex output. Add Standout for just that command.

2. **Keep existing tests passing**: Your dispatch logic stays the same for unhandled commands.

3. **Add more commands over time**: Register additional handlers as you refactor.

4. **Add themes when ready**: Start with inline templates, add YAML stylesheets later.

5. **Eventually remove legacy dispatch**: Once all commands are migrated, simplify to just `app.run()`.

## Using run() vs run_to_string()

`run()` prints output directly and returns `Option<ArgMatches>`:

```rust
if let Some(matches) = app.run(cli, args) {
    // Standout didn't handle, use matches for fallback
    legacy_dispatch(matches);
}
```

`run_to_string()` captures output instead of printing, returning `RunResult`:

```rust
match app.run_to_string(cli, args) {
    RunResult::Handled(output) => { /* process output string */ }
    RunResult::Binary(bytes, filename) => { /* handle binary */ }
    RunResult::NoMatch(matches) => { /* access matches */ }
}
```

Use `run_to_string()` when you need to:

- Capture output for testing
- Post-process the output string before printing
- Log or record what was generated

For normal partial adoption, `run()` is simpler and preferred.

## Accessing the --output Flag in Fallback

Standout adds `--output` globally. In fallback code, you can still access it:

```rust
if let Some(matches) = app.run(cli, std::env::args()) {
    // Get the output mode Standout parsed
    let mode = matches.get_one::<String>("_output_mode")
        .map(|s| s.as_str())
        .unwrap_or("auto");

    match matches.subcommand() {
        Some(("status", sub)) => {
            if mode == "json" {
                println!("{}", serde_json::to_string(&status_data)?);
            } else {
                print_status_text(&status_data);
            }
        }
        _ => {}
    }
}
```

## Disabling Standout's Flags

If `--output` conflicts with your existing flags:

```rust
App::builder()
    .no_output_flag()       // Don't add --output
    .no_output_file_flag()  // Don't add --output-file-path
    .command("list", handler, template)
    .build()?
```

## Example: Hybrid Application

Complete example with both Standout and manual handlers:

```rust
use standout::cli::{App, HandlerResult, Output, CommandContext};
use clap::{Command, ArgMatches};
use serde::Serialize;

#[derive(Serialize)]
struct ListOutput { items: Vec<String> }

fn list_handler(_m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<ListOutput> {
    Ok(Output::Render(ListOutput {
        items: vec!["one".into(), "two".into()],
    }))
}

fn handle_status(_matches: &ArgMatches) {
    println!("Status: OK");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Command::new("myapp")
        .subcommand(Command::new("list").about("List items (Standout)"))
        .subcommand(Command::new("status").about("Show status (legacy)"));

    let app = App::builder()
        .command("list", list_handler, "{% for i in items %}- {{ i }}\n{% endfor %}")
        .build()?;

    if let Some(matches) = app.run(cli, std::env::args()) {
        match matches.subcommand() {
            Some(("status", sub)) => handle_status(sub),
            _ => eprintln!("Unknown command"),
        }
    }

    Ok(())
}
```

Run it:

```bash
myapp list              # Standout handles, renders template
myapp list --output=json # Standout handles, JSON output
myapp status            # Fallback to handle_status()
```
