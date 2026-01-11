# Using Outstanding with Clap

This guide covers how to integrate Outstanding into your clap-based CLI application. Outstanding provides styled output, help topics, and a command handler system that cleanly separates your logic from presentation.

## Table of Contents

- [Quick Start](#quick-start)
- [Core Concepts](#core-concepts)
- [Adoption Models](#adoption-models)
- [Command Handlers](#command-handlers)
- [Output Modes](#output-modes)
- [Themes and Styling](#themes-and-styling)
- [Help Topics](#help-topics)
- [Templates](#templates)
- [Context Injection](#context-injection)

---

## Quick Start

The simplest integration adds styled help and an `--output` flag to your CLI:

```rust
use clap::Command;
use outstanding_clap::Outstanding;

fn main() {
    let matches = Outstanding::run(Command::new("my-app")
        .about("My awesome CLI")
        .subcommand(Command::new("list").about("List items")));

    // Handle your commands normally
    if let Some(_) = matches.subcommand_matches("list") {
        println!("Listing items...");
    }
}
```

Your CLI now has:
- `--output=<auto|term|text|term-debug|json>` flag on all commands
- `help` subcommand with topic support
- Styled help output

---

## Core Concepts

### The Rendering Pipeline

Outstanding follows a clear flow:

```
Command Logic → Structured Data → Template → Styled Output
                      ↓
              (--output=json)
                      ↓
                 JSON Output
```

1. Your command handler produces **structured data** (a Rust struct)
2. A **template** renders that data for human consumption
3. **Themes** apply colors and formatting
4. The **output mode** determines final format (terminal, plain text, JSON)

### OutputMode

Controls how output appears:

| Mode | Description |
|------|-------------|
| `auto` | Detect terminal capabilities (default) |
| `term` | Force ANSI colors |
| `text` | Plain text, no colors |
| `term-debug` | Bracket tags: `[style]text[/style]` |
| `json` | Machine-readable JSON |

Users control this via `--output`:
```bash
my-app list --output=json
my-app list --output=text
```

---

## Adoption Models

Outstanding supports three adoption models. Choose based on your needs.

### Model 1: Full Adoption

Register all commands with handlers. Outstanding manages the complete flow.

```rust
use clap::Command;
use outstanding_clap::{Outstanding, CommandResult};
use serde::Serialize;

#[derive(Serialize)]
struct ListOutput {
    items: Vec<String>,
    total: usize,
}

fn main() {
    let cmd = Command::new("my-app")
        .subcommand(Command::new("list").about("List all items"))
        .subcommand(Command::new("count").about("Count items"));

    Outstanding::builder()
        .command("list", |_matches, _ctx| {
            let items = vec!["apple".into(), "banana".into()];
            let total = items.len();
            CommandResult::Ok(ListOutput { items, total })
        }, "Items ({{ total }}):\n{% for item in items %}- {{ item }}\n{% endfor %}")

        .command("count", |_matches, _ctx| {
            CommandResult::Ok(serde_json::json!({"count": 42}))
        }, "Count: {{ count }}")

        .run_and_print(cmd, std::env::args());
}
```

**Benefits:**
- Clean separation of logic and presentation
- Automatic `--output=json` support
- Consistent error handling

### Model 2: Partial Command Adoption

Register only some commands. Others fall through for manual handling.

```rust
use clap::Command;
use outstanding_clap::{Outstanding, CommandResult, RunResult};

fn main() {
    let cmd = Command::new("my-app")
        .subcommand(Command::new("list"))
        .subcommand(Command::new("legacy"));

    let builder = Outstanding::builder()
        .command("list", |_m, _ctx| {
            CommandResult::Ok(serde_json::json!({"items": ["a", "b"]}))
        }, "{{ items | join(', ') }}");

    match builder.dispatch_from(cmd, std::env::args()) {
        RunResult::Handled(output) => {
            println!("{}", output);
        }
        RunResult::Binary(bytes, filename) => {
            std::fs::write(&filename, bytes).unwrap();
        }
        RunResult::Unhandled(matches) => {
            // Handle legacy commands manually
            if let Some(_) = matches.subcommand_matches("legacy") {
                println!("Legacy code path");
            }
        }
    }
}
```

**Benefits:**
- Gradual migration path
- Keep working code working
- Adopt Outstanding incrementally

### Model 3: Output-Only Adoption

Use Outstanding only for rendering. You handle command execution yourself.

```rust
use clap::{Command, ArgMatches};
use outstanding::{render_or_serialize, Theme, ThemeChoice, OutputMode};
use serde::Serialize;

#[derive(Serialize)]
struct Report {
    title: String,
    items: Vec<String>,
}

fn main() {
    let cmd = Command::new("my-app")
        .arg(clap::arg!(--output <MODE>).default_value("auto"));

    let matches = cmd.get_matches();

    // Determine output mode from flag
    let mode = match matches.get_one::<String>("output").map(|s| s.as_str()) {
        Some("json") => OutputMode::Json,
        Some("text") => OutputMode::Text,
        Some("term") => OutputMode::Term,
        _ => OutputMode::Auto,
    };

    // Your command logic produces data
    let report = Report {
        title: "Summary".into(),
        items: vec!["one".into(), "two".into()],
    };

    // Outstanding handles rendering
    let theme = Theme::new()
        .add("title", console::Style::new().bold())
        .add("item", console::Style::new().cyan());

    let template = r#"
{{ title | style("title") }}
{% for item in items %}- {{ item | style("item") }}
{% endfor %}"#;

    let output = render_or_serialize(
        template,
        &report,
        ThemeChoice::from(&theme),
        mode,
    ).unwrap();

    println!("{}", output);
}
```

**Benefits:**
- Maximum flexibility
- No changes to existing command structure
- Use Outstanding's rendering engine directly

---

## Command Handlers

### Handler Signature

Handlers receive `ArgMatches` and `CommandContext`, return `CommandResult`:

```rust
use clap::ArgMatches;
use outstanding_clap::{CommandContext, CommandResult};

fn my_handler(matches: &ArgMatches, ctx: &CommandContext) -> CommandResult<MyOutput> {
    // Access command arguments
    let name = matches.get_one::<String>("name");

    // Check output mode
    if ctx.output_mode.is_structured() {
        // Maybe include extra data for JSON consumers
    }

    // Return structured data
    CommandResult::Ok(MyOutput { /* ... */ })
}
```

### Struct Handlers

For stateful handlers (database connections, config), implement the `Handler` trait:

```rust
use outstanding_clap::{Handler, CommandContext, CommandResult};
use clap::ArgMatches;

struct ListHandler {
    db: DatabasePool,
}

impl Handler for ListHandler {
    type Output = Vec<Item>;

    fn handle(&self, matches: &ArgMatches, ctx: &CommandContext)
        -> CommandResult<Self::Output>
    {
        let items = self.db.list_all()?;
        CommandResult::Ok(items)
    }
}

// Register with command_handler instead of command
Outstanding::builder()
    .command_handler("list", ListHandler { db: pool }, template)
```

### Nested Commands

Use dot notation for subcommands:

```rust
Outstanding::builder()
    .command("config.get", get_config_handler, "{{ key }}: {{ value }}")
    .command("config.set", set_config_handler, "Set {{ key }}")
    .command("config.list", list_config_handler, "{% for k, v in items %}...")
```

Matches:
- `my-app config get`
- `my-app config set`
- `my-app config list`

### Binary Output

For file exports (PDFs, images), use `CommandResult::Archive`:

```rust
fn export_handler(matches: &ArgMatches, ctx: &CommandContext)
    -> CommandResult<()>
{
    let pdf_bytes = generate_pdf();
    CommandResult::Archive(pdf_bytes, "report.pdf".into())
}
```

---

## Output Modes

### Automatic JSON Support

When a handler is registered, `--output=json` automatically serializes the data:

```rust
#[derive(Serialize)]
struct Stats { count: usize, average: f64 }

// Template is used for terminal output
.command("stats", handler, "Count: {{ count }}, Avg: {{ average }}")
```

```bash
$ my-app stats
Count: 42, Avg: 3.14

$ my-app stats --output=json
{
  "count": 42,
  "average": 3.14
}
```

### Debug Mode

`--output=term-debug` shows style names as bracket tags:

```bash
$ my-app stats --output=term-debug
[label]Count:[/label] [value]42[/value]
```

Useful for debugging templates and verifying style application.

---

## Themes and Styling

### Creating Themes

```rust
use outstanding::Theme;
use console::Style;

let theme = Theme::new()
    .add("title", Style::new().bold().cyan())
    .add("error", Style::new().red())
    .add("success", Style::new().green())
    .add("muted", Style::new().dim());
```

### Style Aliasing

Create layered styles for maintainability:

```rust
let theme = Theme::new()
    // Visual layer (concrete styles)
    .add("dim_gray", Style::new().dim())
    .add("bright_cyan", Style::new().cyan().bold())

    // Presentation layer (aliases)
    .add("muted", "dim_gray")
    .add("accent", "bright_cyan")

    // Semantic layer (aliases to presentation)
    .add("timestamp", "muted")
    .add("command_name", "accent");
```

Change `"muted"` in one place, all semantic styles update.

### Adaptive Themes

Support light and dark terminals:

```rust
use outstanding::{Theme, AdaptiveTheme};

let light = Theme::new().add("accent", Style::new().blue());
let dark = Theme::new().add("accent", Style::new().cyan());

let adaptive = AdaptiveTheme::new(light, dark);
// Automatically selects based on OS theme preference
```

---

## Help Topics

Extended documentation beyond `--help`:

### Loading from Files

```rust
Outstanding::builder()
    .topics_dir("docs/topics")  // Load .txt and .md files
```

Topic file format:
```text
Storage Guide
=============

Notes are stored in ~/.notes/

Each note is a separate file...
```

### Programmatic Topics

```rust
use outstanding::topics::{Topic, TopicType};

Outstanding::builder()
    .add_topic(Topic::new(
        "Advanced Usage",
        "Detailed guide for power users...",
        TopicType::Markdown,
        Some("advanced".into()),  // slug for `help advanced`
    ))
```

### User Access

```bash
my-app help topics     # List all topics
my-app help storage    # View specific topic
my-app help --page X   # View with pager
```

---

## Templates

Templates use [MiniJinja](https://docs.rs/minijinja) syntax (Jinja2-compatible).

### Basic Syntax

```jinja
{{ variable }}
{{ object.field }}
{{ items | join(", ") }}
```

### Loops

```jinja
{% for item in items %}
- {{ item.name }}: {{ item.value }}
{% endfor %}
```

### Conditionals

```jinja
{% if count > 0 %}
Found {{ count }} items
{% else %}
No items found
{% endif %}
```

### Style Filter

Apply theme styles:

```jinja
{{ title | style("header") }}
{{ error_message | style("error") }}
```

### Filters

Built-in MiniJinja filters plus:
- `style(name)`: Apply a named style
- `nl`: Append newline

```jinja
{{ "Section" | style("header") | nl }}
{{ items | join(", ") }}
```

---

## Configuration Reference

### OutstandingBuilder Methods

| Method | Description |
|--------|-------------|
| `.command(path, handler, template)` | Register closure handler |
| `.command_handler(path, handler, template)` | Register struct handler |
| `.topics_dir(path)` | Load topics from directory |
| `.add_topic(topic)` | Add single topic |
| `.theme(theme)` | Set custom theme |
| `.output_flag(Some("format"))` | Rename output flag |
| `.no_output_flag()` | Disable output flag |
| `.context(name, value)` | Add static context value |
| `.context_fn(name, provider)` | Add dynamic context provider |
| `.hooks(path, hooks)` | Register hooks for command |

### Dispatch Methods

| Method | Description |
|--------|-------------|
| `.run_and_print(cmd, args)` | Parse, dispatch, print |
| `.dispatch_from(cmd, args)` | Parse, dispatch, return result |
| `.dispatch(matches, mode)` | Dispatch with parsed matches |

### CommandResult Variants

| Variant | Use Case |
|---------|----------|
| `Ok(data)` | Success with data to render |
| `Err(error)` | Error to display |
| `Silent` | No output needed |
| `Archive(bytes, filename)` | Binary file output |

---

## Context Injection

Beyond handler data, you can inject additional values into templates. This is useful for:
- Terminal information (width, TTY detection)
- App configuration
- Table formatters
- User preferences

### Static Context

Values that don't change between renders:

```rust
use outstanding_clap::Outstanding;
use minijinja::Value;

Outstanding::builder()
    .context("app_version", Value::from("1.0.0"))
    .context("config", Value::from_iter([
        ("debug", Value::from(true)),
        ("max_items", Value::from(100)),
    ]))
    .command("info", handler, "v{{ app_version }}, debug={{ config.debug }}")
```

### Dynamic Context

Values computed at render time using [`RenderContext`]:

```rust
use outstanding_clap::{Outstanding, RenderContext};
use minijinja::Value;

Outstanding::builder()
    .context_fn("terminal", |ctx: &RenderContext| {
        Value::from_iter([
            ("width", Value::from(ctx.terminal_width.unwrap_or(80))),
            ("is_tty", Value::from(ctx.output_mode == outstanding::OutputMode::Term)),
        ])
    })
    .command("info", handler, "Terminal width: {{ terminal.width }}")
```

### RenderContext Fields

Dynamic providers receive a `RenderContext` with:

| Field | Type | Description |
|-------|------|-------------|
| `output_mode` | `OutputMode` | Current output mode (Term, Text, Json, etc.) |
| `terminal_width` | `Option<usize>` | Terminal width in columns, if available |
| `theme` | `&Theme` | Theme being used for rendering |
| `data` | `&serde_json::Value` | Handler's output data |
| `extras` | `HashMap<String, String>` | Additional metadata |

### Table Formatters with Context

A common use case is injecting table formatters with resolved terminal width:

```rust
use outstanding_clap::{Outstanding, RenderContext};
use outstanding::table::{TableSpec, Column, Width, TableFormatter};
use minijinja::Value;

// Define table spec once
let log_spec = TableSpec::builder()
    .column(Column::new(Width::Fixed(7)))
    .column(Column::new(Width::Fill))
    .column(Column::new(Width::Fixed(10)))
    .separator("  ")
    .build();

Outstanding::builder()
    .context_fn("log_table", move |ctx: &RenderContext| {
        let formatter = TableFormatter::new(&log_spec, ctx.terminal_width.unwrap_or(80));
        Value::from_object(formatter)
    })
    .command("log", handler, r#"
{% for entry in entries %}
{{ log_table.row([entry.hash, entry.message, entry.date]) }}
{% endfor %}
    "#)
```

The `TableFormatter` implements `minijinja::value::Object`, providing:
- `row([values])`: Format a row with the given values
- `num_columns`: Get the number of columns
- `column_width(index)`: Get width of a specific column

### Context vs Data Precedence

When a context key conflicts with a data field, **data wins**:

```rust
// Context provides "version"
.context("version", Value::from("1.0"))

// Handler also returns "version"
|_m, _ctx| CommandResult::Ok(json!({"version": "2.0"}))

// Template sees "2.0" (data wins)
"Version: {{ version }}"
```

### JSON Mode and Context

When using `--output=json`, context is **not** included in the output.
Only handler data is serialized:

```bash
# Context has "extra_field" but it won't appear in JSON output
$ my-app info --output=json
{
  "name": "app",
  "version": "1.0"
}
```

---

## Best Practices

1. **Design data first**: Define your output structs before templates
2. **Keep templates simple**: Complex logic belongs in handlers
3. **Use semantic styles**: Name styles by meaning, not appearance
4. **Test with JSON mode**: Ensures your data is well-structured
5. **Gradual adoption**: Start with one command, expand as needed
6. **Use context for utilities**: Terminal info, formatters, configuration
