# App Configuration

`AppBuilder` is the unified entry point for configuring your application. Instead of scattering configuration across multiple structs (`Outstanding`, `RenderSetup`, `Theme`), everything from command registration to theme selection happens in one fluent interface.

This design ensures that your application defines its entire environment—commands, styles, templates, and hooks—before the runtime starts, preventing configuration race conditions and simplifying testing.

This guide covers the full setup: embedding resources, registering commands, configuring themes, and customizing behavior.

See also:

- [Rendering System](rendering-system.md) for details on templates and styles.
- [Topics System](topics-system.md) for help topics.

## Basic Setup

```rust
use outstanding::cli::App;
use outstanding_macros::{embed_templates, embed_styles};

let app = App::builder()
    .templates(embed_templates!("src/templates"))
    .styles(embed_styles!("src/styles"))
    .default_theme("default")
    .command("list", list_handler, "list.j2")
    .build()?;

app.run(Cli::command(), std::env::args());
```

## Embedding Resources

### Templates

`embed_templates!` embeds template files at compile time:

```rust
.templates(embed_templates!("src/templates"))
```

Collects files matching: `.jinja`, `.jinja2`, `.j2`, `.txt` (in priority order).

Directory structure:

```text
src/templates/
  list.j2
  add.j2
  db/
    migrate.j2
    status.j2
```

Templates are referenced by path without extension: `"list"`, `"db/migrate"`.

### Styles

`embed_styles!` embeds stylesheet files:

```rust
.styles(embed_styles!("src/styles"))
```

Collects files matching: `.yaml`, `.yml`.

```text
src/styles/
  default.yaml
  dark.yaml
  light.yaml
```

Themes are referenced by filename without extension: `"default"`, `"dark"`.

### Hot Reloading

In debug builds, embedded resources are re-read from disk on each render—edit without recompiling. In release builds, embedded content is used directly.

This is automatic when the source path exists on disk.

## Runtime Overrides

Users can override embedded resources with local files:

```rust
App::builder()
    .templates(embed_templates!("src/templates"))
    .templates_dir("~/.myapp/templates")  // Overrides embedded
    .styles(embed_styles!("src/styles"))
    .styles_dir("~/.myapp/themes")        // Overrides embedded
```

Local directories take precedence. This enables user customization without recompiling.

## Theme Selection

### From Stylesheet Registry

```rust
    .styles(embed_styles!("src/styles"))
    // Optional: set explicit default name
    // If omitted, tries "default", "theme", then "base"
    .default_theme("dark")
```

If `.default_theme()` is not called, `AppBuilder` attempts to load a theme from the registry in this order:

1. `default`
2. `theme`
3. `base`

This allows you to provide a standard `base.yaml` or `theme.yaml` without requiring explicit configuration code. If the explicit theme isn't found, `build()` returns `SetupError::ThemeNotFound`.

### Explicit Theme

```rust
let theme = Theme::new()
    .add("title", Style::new().bold().cyan())
    .add("muted", Style::new().dim());

App::builder()
    .theme(theme)  // Overrides stylesheet registry
```

Explicit `.theme()` takes precedence over `.default_theme()`.

## Command Registration

### Simple Commands

```rust
App::builder()
    .command("list", list_handler, "list.j2")
    .command("add", add_handler, "add.j2")
```

Arguments: command name, handler function, template path.

### With Configuration

```rust
App::builder()
    .command_with("delete", delete_handler, |cfg| cfg
        .template("delete.j2")
        .pre_dispatch(require_confirmation)
        .post_dispatch(log_deletion))
```

Inline hook attachment without separate `.hooks()` call.

### Nested Groups

```rust
App::builder()
    .group("db", |g| g
        .command("migrate", migrate_handler, "db/migrate.j2")
        .command("status", status_handler, "db/status.j2")
        .group("backup", |b| b
            .command("create", backup_create, "db/backup/create.j2")
            .command("restore", backup_restore, "db/backup/restore.j2")))
```

Creates command paths: `db.migrate`, `db.status`, `db.backup.create`, `db.backup.restore`.

### From Dispatch Macro

```rust
#[derive(Dispatch)]
enum Commands {
    List,
    Add,
    #[dispatch(nested)]
    Db(DbCommands),
}

App::builder()
    .commands(Commands::dispatch_config())
```

The macro generates registration for all variants.

## Default Command

When a CLI is invoked without a subcommand (a "naked" invocation like `myapp` or `myapp --verbose`), you can specify a default command to run:

```rust
App::builder()
    .default_command("list")
    .command("list", list_handler, "list.j2")
    .command("add", add_handler, "add.j2")
```

With this configuration:

- `myapp` becomes `myapp list`
- `myapp --output=json` becomes `myapp list --output=json`
- `myapp add foo` stays as `myapp add foo` (explicit command takes precedence)

### With Dispatch Macro

Use the `#[dispatch(default)]` attribute to mark a variant as the default:

```rust
#[derive(Dispatch)]
#[dispatch(handlers = handlers)]
enum Commands {
    #[dispatch(default)]
    List,
    Add,
}

App::builder()
    .commands(Commands::dispatch_config())
```

Only one command can be marked as default. Multiple `#[dispatch(default)]` attributes will cause a compile error.

## Hooks

Attach hooks to specific command paths:

```rust
App::builder()
    .command("migrate", migrate_handler, "migrate.j2")
    .hooks("db.migrate", Hooks::new()
        .pre_dispatch(require_admin)
        .post_dispatch(add_timestamp)
        .post_output(log_result))
```

The path uses dot notation matching the command hierarchy.

## Context Injection

Add values available in all templates:

### Static Context

```rust
App::builder()
    .context("version", "1.0.0")
    .context("app_name", "MyApp")
```

### Dynamic Context

```rust
App::builder()
    .context_fn("terminal_width", |ctx| {
        Value::from(ctx.terminal_width.unwrap_or(80))
    })
    .context_fn("timestamp", |_ctx| {
        Value::from(chrono::Utc::now().to_rfc3339())
    })
```

Dynamic providers receive `RenderContext` with output mode, terminal width, and handler data.

## Topics

Add help topics:

```rust
App::builder()
    .topics_dir("docs/topics")
    .add_topic(Topic::new("auth", "Authentication...", TopicType::Text, None))
```

See [Topics System](topics-system.md) for details.

## Flag Customization

### Output Flag

```rust
App::builder()
    .output_flag(Some("format"))  // --format instead of --output
```

```rust
App::builder()
    .no_output_flag()  // Disable entirely
```

### File Output Flag

```rust
App::builder()
    .output_file_flag(Some("out"))  // --out instead of --output-file-path
```

```rust
App::builder()
    .no_output_file_flag()  // Disable entirely
```

## The App Struct

`build()` produces an `App`:

```rust
pub struct App {
    registry: TopicRegistry,
    output_flag: Option<String>,
    output_file_flag: Option<String>,
    output_mode: OutputMode,
    theme: Option<Theme>,
    command_hooks: HashMap<String, Hooks>,
    template_registry: Option<TemplateRegistry>,
    stylesheet_registry: Option<StylesheetRegistry>,
}
```

## Running the App

### Standard Execution

```rust
if let Some(matches) = app.run(Cli::command(), std::env::args()) {
    // Outstanding didn't handle this command, fall back to legacy
    legacy_dispatch(matches);
}
```

Parses args, dispatches to handler, prints output. Returns `Option<ArgMatches>`—`None` if handled, `Some(matches)` for fallback.

### Capture Output

For testing, post-processing, or when you need the output string:

```rust
match app.run_to_string(cmd, args) {
    RunResult::Handled(output) => { /* use output string */ }
    RunResult::Binary(bytes, filename) => { /* handle binary */ }
    RunResult::NoMatch(matches) => { /* fallback dispatch */ }
}
```

Returns `RunResult` instead of printing.

### Parse Only

```rust
let matches = app.parse_with(cmd);
// Use matches for manual dispatch
```

Parses with Outstanding's augmented command but doesn't dispatch.

## Build Validation

`build()` validates:

- Theme exists if `.default_theme()` was called
- Returns `SetupError::ThemeNotFound` if not found

What's NOT validated at build time:

- Templates (resolved lazily at render time)
- Command handlers
- Hook signatures (verified at registration)

## Complete Example

```rust
use outstanding::cli::{App, HandlerResult, Output};
use outstanding_macros::{embed_templates, embed_styles};
use clap::{Command, Arg};
use serde::Serialize;

#[derive(Serialize)]
struct ListOutput {
    items: Vec<String>,
}

fn list_handler(matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<ListOutput> {
    let items = vec!["one".into(), "two".into()];
    Ok(Output::Render(ListOutput { items }))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Command::new("myapp")
        .subcommand(Command::new("list").about("List items"));

    let app = App::builder()
        .templates(embed_templates!("src/templates"))
        .styles(embed_styles!("src/styles"))
        .default_theme("default")
        .context("version", env!("CARGO_PKG_VERSION"))
        .command("list", list_handler, "list.j2")
        .topics_dir("docs/topics")
        .build()?;

    app.run(cli, std::env::args());
    Ok(())
}
```

Template `src/templates/list.j2`:

```jinja
[header]Items[/header] ({{ items | length }} total)
{% for item in items %}
  - {{ item }}
{% endfor %}

[muted]v{{ version }}[/muted]
```

Style `src/styles/default.yaml`:

```yaml
header:
  fg: cyan
  bold: true
muted:
  dim: true
```
