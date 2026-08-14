# App Configuration

`AppBuilder` is the unified entry point for configuring your application. Instead of scattering configuration across multiple structs (`Standout`, `RenderSetup`, `Theme`), everything from command registration to theme selection happens in one fluent interface.

This design ensures that your application defines its entire environment—commands, styles, templates, and hooks—before the runtime starts, preventing configuration race conditions and simplifying testing.

This guide covers the full setup: embedding resources, registering commands, configuring themes, and customizing behavior.

See also:

- [Rendering System](rendering-system.md) for details on templates and styles.
- [Topics System](topics-system.md) for help topics.

## Basic Setup

```rust
use standout::cli::App;
use standout_macros::{embed_templates, embed_styles};

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

Collects files matching: `.jinja`, `.jinja2`, `.j2`, `.stpl`, `.txt` (in priority order).

> **Custom template engines:** For advanced use cases, `standout-render` supports pluggable template engines. See the [Template Engines](../crates/standout-render/docs/topics/template-engines.md) topic for details on using `SimpleEngine` or implementing custom engines.

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

Collects files matching: `.css` (and legacy `.yaml`, `.yml`).

```text
src/styles/
  default.css
  dark.css
  light.css
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

This allows you to provide a standard `base.css` or `theme.css` without requiring explicit configuration code. If the explicit theme isn't found, `build()` returns `SetupError::ThemeNotFound`.

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

Inline command configuration can also attach a
`StructuredOutputProjection` for CSV shaping. The projection stays at the
presentation boundary: it sees post-dispatch data, and handlers remain
independent of the selected output mode. See [Output Modes](output-modes.md#csv-output).

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
    .commands(Commands::dispatch_config())?
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

Default resolution applies to both the integrated dispatch path (`run`, `dispatch_from`, `run_to_string`) and configured parsing (`parse_from`, `get_matches_from`). If you parse first and build dispatch state afterwards, the matches you get back already name the resolved command.

### Invocation-Aware Defaults

A fixed name can't express "it depends". `default_command_with` chooses the default per invocation:

```rust
App::builder()
    .default_command_with(|ctx| {
        Some(if ctx.stdin_is_piped() { "add" } else { "list" }.to_string())
    })
    .command("list", list_handler, "list.j2")
    .command("add", add_handler, "add.j2")
```

- `myapp` at a terminal becomes `myapp list`
- `cat notes.txt | myapp` becomes `myapp add`, which reads the pipe
- `myapp done 3` stays as `myapp done 3`

The resolver receives a `DefaultCommandContext` exposing only the facts needed to pick a command:

| Method | Fact |
| --- | --- |
| `app_state::<T>()` | Read-only app state registered via `.app_state(...)` |
| `stdin_is_terminal()` / `stdin_is_piped()` | Whether stdin is redirected |

Plus `std::env` for env-derived facts, which never went through clap. There are deliberately no parse results here: resolution runs *before* parsing (see below), so there are none to hand it. A flag that decides which command a naked invocation means wants to be a command.

**Stdin is never read during resolution.** The terminal check is the same non-consuming `StdinReader::is_terminal` seam the input system uses, so a handler's `InputChain` still consumes the pipe normally afterwards. This also means piped-but-empty stdin is a *pipe*, not a terminal — emptiness is only knowable by reading, which resolution never does. If empty input should be an error, that's the receiving command's `InputChain` policy, not the resolver's.

### Ordering guarantees

Selection is **name-first**: the token in command position is read as a name before anything is parsed. If it names a command, that is what the line means; only a line that names none takes a default command. Clap stays authoritative for everything after selection.

- **Explicit and nested commands** are selected lexically — the resolver never runs, and the root's required arguments never fire before the name is understood.
- **`--help` / `--version`** are not naked invocations: Clap answers them, and no default is inserted (otherwise `myapp --help` would render *that command's* help).
- **Invalid syntax** stays a Clap usage error (exit 2), produced by the single authoritative parse. The resolver *does* run for such a line — its answer is a function of the command name alone — but its answer changes nothing about the diagnostic.
- **`--`** ends the options and hands the rest to the positionals, so nothing after it can name a command.

Selection reads command names off your clap definition, so a command Clap does not know is not a name a line can mean. Because the default is inserted *before* Clap sees the line, a root that requires a subcommand — what `#[command(subcommand)] command: Commands` produces — accepts a naked invocation; the field does not have to be `Option<Commands>`. See [ADR-0018](../adr/0018-select-the-command-lexically-before-parsing.md).

### Combining both

Both may be configured together. The resolver is consulted first; returning `None` declines to the static default:

```rust
App::builder()
    // Pipes mean `add`; everything else falls back to `list`.
    .default_command("list")
    .default_command_with(|ctx| ctx.stdin_is_piped().then(|| "add".to_string()))
```

Returning a name that isn't a command of your `clap::Command` fails the run with `RunErrorKind::DefaultCommand` (exit 1), carrying a diagnostic that names the offending resolver output and lists the valid commands. A resolver naming a command the CLI doesn't have is an application bug, so it's reported as one rather than reaching Clap as a usage error blaming the user. Return `None` to decline.

Validation is against your `clap::Command`'s names, not Standout's registered handlers — so partial adoption stays coherent: resolving to a Clap command Standout doesn't handle yields `NoMatch`, exactly as typing it explicitly would.

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
    .commands(Commands::dispatch_config())?
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
They also receive `ctx.ambiguous_width()`, the application's explicit
East Asian Ambiguous character-width policy. Configure it at the rendering
seam; narrow is the compatibility default and Standout does not infer a locale:

```rust
use standout::{AmbiguousWidth, cli::App};

let app = App::builder()
    .ambiguous_width(AmbiguousWidth::Wide)
    .build()?;
```

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
if !app.run(Cli::command(), std::env::args()) {
    // Standout did not handle this command; fall back to legacy dispatch.
    legacy_dispatch();
}
```

Parses args, dispatches to a handler, and performs the final write. It returns
`true` when Standout handled the command and `false` for an unmatched fallback.
Help/version and successes use stdout/status 0, usage errors use stderr/status
2, and runtime/write failures use stderr/status 1. An explicit
`ExternalFailure` is the sole exception: it preserves an authoritative external
operation's declared nonzero status and verbatim stderr payload.

### Capture Output

For testing, post-processing, or when you need the output string:

```rust
match app.run_to_string(cmd, args) {
    RunResult::Handled(output) => { /* use output string */ }
    RunResult::Binary(bytes, filename) => { /* handle binary */ }
    RunResult::Error(error) => { /* inspect error.kind() */ }
    RunResult::NoMatch(matches) => { /* fallback dispatch */ }
    _ => {}
}
```

Returns `RunResult` instead of printing. Use `exit_status()`, `success_kind()`,
and `error_kind()` for typed assertions; see [Execution
Outcomes](./execution-outcomes.md).

### Parse Only

```rust
let matches = app.parse_with(cmd);
// Use matches for manual dispatch
```

Parses with Standout's augmented command but doesn't dispatch.

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
use standout::cli::{App, HandlerResult, Output};
use standout_macros::{embed_templates, embed_styles};
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

Style `src/styles/default.css`:

```css
.header { color: cyan; font-weight: bold; }
.muted { opacity: 0.5; }
```
