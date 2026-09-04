# App Configuration

`AppBuilder` is the unified entry point for configuring your application. Instead of scattering configuration across multiple structs (`Standout`, `RenderSetup`, `Theme`), everything from command registration to theme selection happens in one fluent interface.

This design ensures that your application defines its entire environment—commands, styles, templates, and hooks—before the runtime starts, preventing configuration race conditions and simplifying testing.

This guide covers the full setup: embedding resources, registering commands, configuring themes, and customizing behavior.

See also:

- [Templating](../crates/render/topics/templating.md) and [Styling System](../crates/render/topics/styling-system.md) for templates and styles.
- [Topics System](topics-system.md) for help topics.

## Basic Setup

```rust
use standout::cli::{App, FnHandler};
use standout_macros::{embed_templates, embed_styles};

let app = App::builder()
    .templates(embed_templates!("src/templates"))
    .styles(embed_styles!("src/styles"))
    .default_theme("default")
    .command_with("list", FnHandler::new(list_handler), |config| config.template_name("list"))?
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

> **Custom template engines:** For advanced use cases, `standout-render` supports pluggable template engines. See the [Template Engines](../crates/render/topics/template-engines.md) topic for details on using `SimpleEngine` or implementing custom engines.

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

## Resources Read at Run Time

`templates_dir` and `styles_dir` add a directory read at run time, for
resources that live outside the crate source tree and so cannot be embedded:

```rust
App::builder()
    .templates(embed_templates!("src/templates"))
    .templates_dir("~/.myapp/templates")  // Adds names the binary does not embed
    .styles(embed_styles!("src/styles"))
    .styles_dir("~/.myapp/themes")        // Likewise for themes
```

These directories **add** names; they do not replace them. A registry resolves
an embedded name before it looks at any directory registered this way, so a
`~/.myapp/templates/list.jinja` sitting beside an embedded `list` never
renders — see [Resolution Priority](../crates/render/topics/file-system-resources.md#resolution-priority).
To let a user directory win, register only the directory, without the
`embed_templates!` call, when that directory exists.

## Theme Selection

### From Stylesheet Registry

```rust
    .styles(embed_styles!("src/styles"))
    .default_theme("dark")
```

`.default_theme(name)` names the theme `build()` loads from the stylesheet
registry; if that name isn't found, `build()` returns
`SetupError::ThemeNotFound`. With no `.default_theme(...)` call, `build()`
does not fall back to any conventional name — the application resolves to no
application theme, leaving the framework's own base styling.

### Explicit Theme

```rust
let theme = Theme::new()
    .add("title", Style::new().bold().cyan())
    .add("muted", Style::new().dim());

App::builder()
    .theme(theme)
```

`.theme(...)` sets the theme directly, bypassing the stylesheet registry.
Calling both `.styles(...)` and `.theme(...)` on the same builder is a
`SetupError` that names both calls — configure one path or the other, not
both.

## Command Registration

### Simple Commands

```rust
App::builder()
    .command_with("list", FnHandler::new(list_handler), |cfg| cfg)?
    .command_with("add", FnHandler::new(add_handler), |cfg| cfg)?
```

`AppBuilder::command_with` takes an `impl Handler`, not a bare function, so a
plain `fn(&ArgMatches, &CommandContext) -> HandlerResult<T>` is wrapped in
`FnHandler::new(...)` first; a `#[handler]`-annotated function registers as the
`name_Handler` struct the macro generates instead. (`GroupBuilder::command_with`
— the entry `.commands(...)` and `#[derive(Dispatch)]` reach — takes the bare
closure and wraps it for you, which is why the nested-group example below does
not name `FnHandler`.)

With no `.template_name(...)` set on the `CommandConfig`, the template
resolves by convention: the command path with `.` replaced by `/` (`list`,
`add`), matched against the registered templates using the extension list
`.jinja`, `.jinja2`, `.j2`, `.stpl`, `.txt`.

### With Configuration

```rust
App::builder()
    .command_with("delete", FnHandler::new(delete_handler), |cfg| cfg
        .template_name("delete")
        .pre_dispatch(require_confirmation)
        .post_dispatch(log_deletion))?
```

Inline command configuration can also attach a
`StructuredOutputProjection` for CSV shaping. The projection stays at the
presentation boundary: it sees post-dispatch data, and handlers remain
independent of the selected output mode. See [Output Modes](output-modes.md#csv-output).

### Nested Groups

```rust
App::builder()
    .commands(|g| g
        .group("db", |g| g
            .command("migrate", migrate_handler)
            .command("status", status_handler)
            .group("backup", |b| b
                .command("create", backup_create)
                .command("restore", backup_restore))))?
```

Creates command paths: `db.migrate`, `db.status`, `db.backup.create`,
`db.backup.restore`. Each resolves, by convention, to a template named after
its path (`db/migrate`, `db/status`, `db/backup/create`,
`db/backup/restore`); attach a `CommandConfig` to a group entry with
`command_with` instead of `command` when one needs `.template_name(...)` or
another `CommandConfig` setting.

### From Dispatch Macro

```rust
#[derive(Dispatch)]
#[dispatch(handlers = handlers)]
enum Commands {
    List,
    Add,
    #[dispatch(nested)]
    Db(DbCommands),
}

App::builder()
    .commands(Commands::dispatch_config())?
```

`#[dispatch(handlers = <module path>)]` on the enum is required: it names the
module `Dispatch` looks up each variant's handler function in (`handlers::list`
for `List`, `handlers::add` for `Add`). A nested variant's own type
(`DbCommands` here) needs its own `#[derive(Dispatch)]` with its own
`#[dispatch(handlers = ...)]` — the macro generates registration for all
variants, but a container attribute is scoped to the enum it's on.

## Default Command

When a CLI is invoked without a subcommand (a "naked" invocation like `myapp` or `myapp --verbose`), you can specify a default command to run:

```rust
App::builder()
    .default_command("list")
    .command("list", list_handler, "{{ items | length }} items")
    .command("add", add_handler, "Added {{ name }}")
```

With this configuration:

- `myapp` becomes `myapp list`
- `myapp --output=json` becomes `myapp list --output=json`
- `myapp add foo` stays as `myapp add foo` (explicit command takes precedence)

Default resolution applies to both the integrated dispatch path (`run`, `run_with`) and configured parsing (`get_matches_from`). If you parse first and build dispatch state afterwards, the matches you get back already name the resolved command.

### Invocation-Aware Defaults

A fixed name can't express "it depends". `default_command_with` chooses the default per invocation:

```rust
App::builder()
    .default_command_with(|ctx| {
        Some(if ctx.stdin_is_piped() { "add" } else { "list" }.to_string())
    })
    .command("list", list_handler, "{{ items | length }} items")
    .command("add", add_handler, "Added {{ name }}")
```

- `myapp` at a terminal becomes `myapp list`
- `cat notes.txt | myapp` becomes `myapp add`, which reads the pipe
- `myapp done 3` stays as `myapp done 3`

The resolver receives a `DefaultCommandContext` exposing only the facts needed to pick a command:

| Method | Fact |
| --- | --- |
| `matches()` | The parsed root `ArgMatches` — globals and root flags |
| `app_state::<T>()` | Read-only app state registered via `.app_state(...)` |
| `stdin_is_terminal()` / `stdin_is_piped()` | Whether stdin is redirected |

Plus `std::env` for env-derived facts. The matches are the root's, so global flags and root arguments are all there; there is no subcommand, because that is what makes the invocation naked.

**Stdin is never read during resolution.** The terminal check is the same non-consuming `StdinReader::is_terminal` seam the input system uses, so a handler's `InputChain` still consumes the pipe normally afterwards. This also means piped-but-empty stdin is a *pipe*, not a terminal — emptiness is only knowable by reading, which resolution never does. If empty input should be an error, that's the receiving command's `InputChain` policy, not the resolver's.

### Ordering guarantees

**Clap decides which command a line named**, and resolution reads that decision. A parse that selected a subcommand is not naked; a parse that selected none is.

- **Explicit and nested commands** short-circuit resolution — the resolver never runs.
- **`--help` / `--version`** are Clap's own displays: no default is inserted, so `myapp --help` renders the root's help rather than a default command's.
- **Invalid syntax** stays a Clap usage error (exit 2). If a default command is configured, a refused line is offered to it — `myapp --all` is a naked line at a root that has no `--all`, and becomes `myapp list --all` when `--all` belongs to `list` — and whatever the amended line parses to, success or failure, is what you get.
- **`--`**, option values, aliases, and short clusters mean exactly what they mean everywhere else, because the same parser reads them.

A root that requires a subcommand — what `#[command(subcommand)] command: Commands` produces — still accepts a naked invocation: the line is refused, the default is substituted, and the amended line parses. The field does not have to be `Option<Commands>`. See [ADR-0018](../adr/0018-let-the-parser-classify-the-command-line.md).

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
    .command_with("db.migrate", FnHandler::new(migrate_handler), |cfg| cfg)?
    .hooks("db.migrate", Hooks::new()
        .pre_dispatch(require_admin)
        .post_dispatch(add_timestamp)
        .post_output(log_result))
```

The path uses dot notation matching the command hierarchy.

### Hook order, and where a questionnaire sits in it

Pre-dispatch hooks run in the order they were registered — `Hooks` keeps them
in a list and `run_pre_dispatch` walks it front to back. The same holds for
post-dispatch and post-output.

`CommandConfig::questionnaire::<T>()` is a pre-dispatch hook, so it takes its
place in that same order: a `.pre_dispatch(f)` written *before* it runs before
the answers are resolved and cannot read them, and one written *after* it runs
with `ctx.questionnaire::<T>()` already populated.

```rust,ignore
// `check_permissions` runs first, then the questionnaire resolves,
// then `audit` runs and can read the answers.
CommandConfig::new(handler)
    .pre_dispatch(check_permissions)
    .questionnaire::<ProvisionAnswers>()
    .pre_dispatch(audit)
```

One trap goes with that: `CommandConfig::hooks(hooks)` *replaces* the config's
hook set rather than appending to it, so calling `.hooks(…)` after
`.questionnaire::<T>()` discards the questionnaire's own hook and the answers
never resolve. Register per-phase with `.pre_dispatch(…)` when a questionnaire
is involved.

Registering the same phase for one path through both `CommandConfig` and
`AppBuilder::hooks` is a configuration error naming the path and the phase,
rather than one hook set silently replacing the other.

## Context Injection

Add values available in all templates:

### Static Context

`context` takes a `minijinja::Value`, so string and number literals need
`.into()`:

```rust
App::builder()
    .context("version", "1.0.0".into())
    .context("app_name", "MyApp".into())
```

### Dynamic Context

```rust
use minijinja::Value;
use standout::context::RenderContext;

App::builder()
    .context_fn("terminal_width", |ctx: &RenderContext| {
        Value::from(ctx.terminal_width.unwrap_or(80))
    })
    .context_fn("doubled_count", |ctx: &RenderContext| {
        let count = ctx.data.get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        Value::from(count * 2)
    })
    .context_fn("timestamp", |_ctx: &RenderContext| {
        Value::from(chrono::Utc::now().to_rfc3339())
    })
```

The parameter type annotation is required: `context_fn` is generic over the
`ContextProvider` trait, so the compiler cannot infer it from the closure alone.
`test_context_fn_uses_handler_data` in `standout`'s builder tests exercises the
`doubled_count` shape against the live API.

A provider reads the serialized data for the request being rendered through
`ctx.data`, a `&serde_json::Value`. For an ordinary command that is the handler's
`Output::Render` payload — the `doubled_count` provider above reads `count` from
it — so a provider can derive a template value from handler data even when the
template never names that data directly. The same registry also runs while
rendering help, an artifact's report, or a direct render call, and each of those
supplies its own request-specific shape rather than a handler payload, so a
provider that assumes particular fields should treat them as optional.

Dynamic providers receive a `RenderContext` carrying the output mode
(`ctx.output_mode`), terminal width (`ctx.terminal_width`), theme (`ctx.theme`),
and the handler payload (`ctx.data`).
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

## Version

Application version metadata belongs on the builder, next to the rest of the
app's configuration:

```rust
App::builder()
    .version(env!("CARGO_PKG_VERSION"))
```

Standout applies the value to the root command wherever it augments and parses
it, so every entry point — `run`, `run_with`, `get_matches_from`, and
`TestHarness` — answers `myapp --version` the same way:
Clap's own display, on stdout, exit status 0, typed as
`SuccessKind::ClapVersion` (see [Execution
Outcomes](./execution-outcomes.md)).

Clap keeps owning the spelling and formatting of that output and the display
short-circuit; the builder only says what the version is. Leave `.version()`
unset and the supplied `clap::Command` is untouched, including a version
configured on Clap directly.

This is separate from `.context("version", …)`, which puts a value in *templates*
(`{{ version }}`); an app that wants both says both.

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

### Output Mode Fallback

The representation used when the flag is absent from the command line. It
defaults to `Representation::Human`; an application that wants a structured
encoding by default sets it here. A default the user keeps in a configuration file is `[term] output`
([Configuration Files](./config-files.md#the-term-section)), which fills this
same slot once the file is resolved and outranks the value given here.

```rust
App::builder()
    .output_mode_fallback(Representation::Json)
```

An explicit `--output` always
wins, so this sets the default rather than overriding the user. Whether the
human page carries escape sequences is a separate axis and is not what this call
does.

`app --help` renders in the fallback even when the command line
does carry an `--output` — the help flags never read the flag ([Help](./standout-help.md#output-modes)).

For the same reason the `[default: ...]` that `--help` shows for `--output` is
this fallback, never the configured `[term] output`.

### File Output Flag

```rust
App::builder()
    .output_file_flag(Some("out"))  // --out instead of --output-file-path
```

```rust
App::builder()
    .no_output_file_flag()  // Disable entirely
```

### Color Flag

`--color auto|always|never` decides whether the human page carries escape
sequences, on its own and whatever `--output` names. It renames and disappears
through the same seam.

```rust
App::builder()
    .color_flag(Some("colour"))  // --colour instead of --color
```

```rust
App::builder()
    .no_color_flag()  // Disable entirely; an application that spells --color itself needs this
```

### Pager Flag

`--no-pager` writes a run's output straight to stdout instead of through a
pager. It renames and disappears through the same seam.

```rust
App::builder()
    .pager_flag(Some("plain"))  // --plain instead of --no-pager
```

```rust
App::builder()
    .no_pager_flag()  // Remove it; the user then has no way to decline paging
```

Which commands may page is a per-command declaration, not a builder call: see
[`pageable`](./dispatch-attributes.md#every-variant-attribute) and
[Paging](./output-modes.md#paging).

### The Application Name

```rust
App::builder()
    .name(env!("CARGO_PKG_NAME"))
```

The name Standout reads the application's own pager variable from:
`<NAME>_PAGER` before `PAGER`, upper-cased with every character outside
`A-Z0-9` written as `_`. An application that never names itself is paged by
`PAGER` alone. This is separate from clap's own `Command::name`, which owns the
spelling in usage lines.

### Configuration

`config(clapfig_builder)` registers the application's settings struct;
`term_settings(accessor)` tells the framework where its own `[term]` section
lives; `config_override_flag("set")` installs an opt-in `--set key=value`;
`no_config_command()` removes the injected `config` command. All four are
described in [Configuration Files](./config-files.md).

## The App Struct

`build()` produces an `App`, which holds everything the builder resolved: the
flag names, the output-mode fallback, the topic, template and stylesheet
registries, the hooks per command path, and the theme. The theme `build()`
merged is always present; `get_default_theme()` returns `&Theme`.

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
2, and runtime/write failures use stderr/status 1. The two owner-declared
failures are the exceptions: `AppFailure` carries the application's own nonzero
status and verbatim stderr payload, and `ExternalFailure` preserves an
authoritative external operation's. See [Error Handling](./error-handling.md).

### Capture Output

For tests, reach for `standout_test::TestHarness` (see [Testing](./testing.md)).
For post-processing, or any other embedding caller that needs the output
string, pass destination properties and input sources in explicitly:

```rust
let target = TargetProperties::detect();
let sources = InputSources::from_process();
let result = app.run_with(cmd, args, target, sources);
let _ = result.warnings();
match result.into_outcome() {
    DispatchResult::Handled(output) => { /* use output string */ }
    DispatchResult::Binary(bytes, filename) => { /* handle binary */ }
    DispatchResult::Error(error) => { /* inspect error.kind() */ }
    DispatchResult::NoMatch(matches) => { /* fallback dispatch */ }
    _ => {}
}
```

Returns `CompletedRun` instead of printing: a wrapper around `DispatchResult`
plus framework warnings. Use `exit_status()`, `success_kind()`, and
`error_kind()` for typed assertions; see [Execution
Outcomes](./execution-outcomes.md).

### Parse Only

```rust
match app.get_matches_from(cmd, std::env::args(), &InputSources::from_process()) {
    HelpResult::Matches(matches) => { /* use matches for manual dispatch */ }
    HelpResult::Help(text) => { /* the invocation asked for help */ }
    HelpResult::Error(e) => { /* a clap::Error: usage failure, or --version display */ }
}
```

Parses with Standout's augmented command, intercepting help display; returns
matches only when the invocation didn't trigger a help/usage/version display.

## Build Validation

`build()` validates:

- a theme registry exists and contains the theme named by `.default_theme(...)`
- named templates resolve through `.templates(...)` or `.templates_dir(...)`
- convention templates resolve when application templates are configured; without application templates, human-mode rendering reports the missing convention template at runtime
- registered templates compile
- framework templates only use tags defined by the resolved theme
- `command_groups`, topics, and `help_word(true)` are not combined with `.help_handling(false)`
- commands do not collide with the `help` word standout installs when help handling is on
- the same command path and hook phase are not configured through both `CommandConfig` and `AppBuilder::hooks`

What's NOT validated at build time:

- Command handlers
- Hook signatures (verified at registration)

## Complete Example

```rust
use standout::cli::{App, CommandContext, FnHandler, HandlerResult, Output};
use standout_macros::{embed_templates, embed_styles};
use clap::{Command, ArgMatches};
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
        .version(env!("CARGO_PKG_VERSION"))
        .context("version", env!("CARGO_PKG_VERSION").into())
        .command_with("list", FnHandler::new(list_handler), |config| {
            config.template_name("list")
        })?
        .topics_dir("docs/topics")?
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
