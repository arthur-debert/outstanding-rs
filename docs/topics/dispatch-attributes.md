# The `#[dispatch(…)]` and `#[handler]` Reference

Two macros carry the blessed idiom. `#[derive(Dispatch)]` on an enum binds
command names to handler functions; `#[handler]` on a function turns it into
something the dispatcher can call. This page is the complete list of what each
one accepts.

Both name mappings on this page — a variant name to a command name, and a
parameter name to a clap argument id — are contract, because they decide the
words a user types in a shell script. See
[What Is Contract](stability.md#4-the-two-name-mappings-a-user-types-on-the-command-line).

## `#[derive(Dispatch)]`

### The container attribute

One attribute goes on the enum, and it is required:

```rust,ignore
#[derive(Dispatch)]
#[dispatch(handlers = handlers)]
enum Commands { /* … */ }
```

`handlers = <module path>` names the module the derive looks each variant's
handler up in. Without it, expansion fails with `missing #[dispatch(handlers =
path)] attribute`.

### From a variant name to a command name

A variant registers under its **kebab-case** name — `ListUnits` becomes
`list-units` — which is the spelling clap's own derive gives the subcommand.
`#[dispatch(name = "…")]` renames one variant.

A `name` may not be empty and may not contain `.`: dispatch splits registration
paths on `.`, so such a name would register a nested path no clap subcommand
declares. Nesting is `#[dispatch(nested)]`, below.

A registered path that no clap subcommand can reach is a loud error naming the
path, raised by `App::run` and `App::verify_command` before dispatch.

### From a variant name to a handler function

The handler is looked up under the variant's **snake_case** name in the
`handlers` module — `ListUnits` calls `handlers::list_units`. Two attributes
change that:

- `#[dispatch(pure)]` appends `__handler`, so the derive calls
  `handlers::list_units__handler` — the wrapper `#[handler]` generated. Use it
  whenever the handler function carries `#[handler]`, which is the blessed
  style. The derive registers through a closure taking `HandlerResult<T>`, so a
  `pure` handler must be annotated `-> Result<Output<T>, E>` or
  `-> Result<(), E>`; a plain `-> Result<T, E>` fails expansion. See
  [what `#[handler]` generates](#what-handler-generates).
- `#[dispatch(handler = <path>)]` names the function outright and ignores the
  inferred name.

The two are mutually exclusive, and expansion rejects the pair with a message
naming both. Without `pure`, the derive calls the named function directly, so
that function must already have the dispatch signature.

### Every variant attribute

Several `#[dispatch(…)]` attributes may sit on one variant; they merge before
anything reads them.

| Attribute | Form | What it does |
| --- | --- | --- |
| `name` | `name = "…"` | Registers the command under this name instead of the variant's kebab-case name. Not empty, no `.`. |
| `handler` | `handler = <path>` | Calls this function instead of the inferred one. Excludes `pure`. |
| `pure` | flag | The handler carries `#[handler]`: append `__handler` to the inferred name. Excludes `handler` and `simple`. |
| `simple` | flag | The handler takes only `&ArgMatches`, with no `&CommandContext`. Excludes `pure`. |
| `nested` | flag | The variant is a subcommand group. Requires a single-field tuple variant wrapping another `Dispatch` enum. |
| `skip` | flag | Registers nothing for this variant. |
| `default` | flag | Runs this command on a naked invocation. At most one variant per enum. |
| `template_name` | `template_name = "…"` | Names the template registry entry instead of using the convention. Excludes `silent`, `binary` and `structured_only`. |
| `silent` | flag | The command renders nothing. Excludes `binary`, `structured_only`, `template_name`. |
| `binary` | flag | The command writes bytes rather than a rendered page. Same exclusions. |
| `structured_only` | flag | The command has output only in the structured modes. Same exclusions. |
| `pageable` | flag | The command's complete human output may reach the user through a pager ([Paging](./output-modes.md#paging)). |
| `list_view` | flag | Renders through the framework's built-in `standout/list-view` template, unless `template_name` also appears, and attaches a tabular spec to the handler's `Output::Render`. |
| `item_type` | `item_type = "…"` | Names the `Tabular`-implementing item type `list_view` builds its spec from. |
| `questionnaire` | `questionnaire = <path>` | Resolves this questionnaire type before the handler runs. |
| `pre_dispatch` | `pre_dispatch = <path>` | Runs this hook before the handler. |
| `post_dispatch` | `post_dispatch = <path>` | Runs this hook after the handler, before rendering. |
| `post_output` | `post_output = <path>` | Runs this hook after the output is produced. |
| `pipe_to` | `pipe_to = "…"` | Sends the output to this command and keeps the original. |
| `pipe_through` | `pipe_through = "…"` | Replaces the output with this command's stdout. |
| `pipe_to_clipboard` | flag | Sends the output to the clipboard. |

An unrecognized key fails expansion with a message listing every key above.

### Nested commands

`#[dispatch(nested)]` marks a variant that wraps another `Dispatch` enum:

```rust,ignore
#[derive(Dispatch)]
#[dispatch(handlers = handlers)]
enum Commands {
    #[dispatch(nested)]
    Pr(PrCommands),
}
```

The derive registers the group rather than a command, and the inner enum's own
variants register beneath it. Registration paths are dot-joined as the
recursion descends, so `pr checks list` registers the path `pr.checks.list`.

### How a command finds its template

With no `template_name` and no absence marker (`silent`, `binary`,
`structured_only`), the template name **is** the registration path with each
`.` replaced by `/`, and **no extension appended**:

| Registration path | Template name |
| --- | --- |
| `list` | `list` |
| `pr.checks.list` | `pr/checks/list` |

The registry then resolves that name against `.jinja`, `.jinja2`, `.j2`,
`.stpl` and `.txt`, in that priority order — so a nested `pr checks list`
command is rendered by `src/templates/pr/checks/list.jinja`. A name may be
looked up with or without an extension; the extension is stripped and the base
name retried.

`#[dispatch(template_name = "…")]` replaces that name with a registry entry you
choose, which is how two commands share one template.

## `#[handler]`

### Parameter attributes

Every typed parameter carries one of four attributes, references included: the
macro reads the value out of `ArgMatches` (or hands over the dispatcher's own
reference) by the attribute, never by the type, so a `&CommandContext` still
needs `#[ctx]` and an `&ArgMatches` still needs `#[matches]`.

| Attribute | Type | Where the value comes from |
| --- | --- | --- |
| `#[flag]` | `bool` | `matches.get_flag(id)` |
| `#[arg]` | `T` | a required argument |
| `#[arg]` | `Option<T>` | an optional argument |
| `#[arg]` | `Vec<T>` | a repeated argument |
| `#[ctx]` | `&CommandContext` | the dispatcher |
| `#[matches]` | `&ArgMatches` | the dispatcher, unparsed |

### From a parameter name to a clap argument id

**Underscores become hyphens.** A parameter named `no_legend` reads the
argument whose id is `no-legend`.

Clap's own derive ids an argument by the field name it comes from, so the two
disagree unless one of them says so. Either side can:

```rust,ignore
#[arg(id = "no-legend")]        // on the clap-derive field
no_legend: bool,

#[flag(name = "no_legend")]     // on the handler parameter
no_legend: bool,
```

`app.verify_command(&cmd)` reports the mismatch at build time rather than
leaving it to a runtime `get_flag` panic.

A parameter named with a raw identifier drops the `r#` first, the way clap's
derive drops it from a field name: `r#type` reads the argument id `type`.

### What `#[handler]` generates

For `fn list`, four items:

| Item | What it is |
| --- | --- |
| `list` | the original function, unchanged apart from the parameter attributes being removed — still directly callable from a test |
| `list__handler(&ArgMatches, &CommandContext)` | the wrapper that extracts the arguments and calls `list`. It returns **the function's own annotated return type**, verbatim |
| `list__expected_args() -> Vec<ExpectedArg>` | what `verify_command` reads |
| `list_Handler` | a unit struct implementing `Handler` — the registrable item |

The `Result<T, E>` to `Output::Render` wrap happens inside `Handler::handle`,
not inside `list__handler`, and that is what makes the two registration methods
accept different things. `AppBuilder::command_with` takes an `impl Handler`, so
`handlers::list_Handler` works for any of the three return shapes.
`GroupBuilder::command_with` — which is what `#[derive(Dispatch)]` reaches —
takes a closure returning `HandlerResult<T>`, so it accepts
`handlers::list__handler` only when the function was annotated
`-> Result<Output<T>, E>` or `-> Result<(), E>`. The un-suffixed
`handlers::list` is registrable through neither.

See the [handler contract](../crates/dispatch/topics/handler-contract.md) for
the return types themselves.

## A worked example

Every attribute below is exercised by
`crates/standout-fixtures/src/derive_surface.rs`, which compiles under
`deny(warnings)` against a single `standout` dependency.

```rust
use clap::{Arg, ArgAction, ArgMatches, Command};
use standout::cli::{App, CommandContext, Dispatch, Output};
use standout::{handler, EmbeddedTemplates};

#[derive(serde::Serialize)]
pub struct Units {
    pub names: Vec<String>,
}

pub mod handlers {
    use super::*;

    #[handler]
    pub fn list_units(#[flag] all: bool) -> Result<Output<Units>, anyhow::Error> {
        let mut names = vec!["ssh".to_string()];
        if all {
            names.push("cron".to_string());
        }
        Ok(Output::Render(Units { names }))
    }

    #[handler]
    pub fn about(#[ctx] _ctx: &CommandContext) -> Result<Output<Units>, anyhow::Error> {
        Ok(Output::Render(Units { names: vec!["unitctl".to_string()] }))
    }

    #[handler]
    pub fn reload(#[matches] _matches: &ArgMatches) -> Result<(), anyhow::Error> {
        Ok(())
    }
}

#[derive(Dispatch)]
#[dispatch(handlers = handlers)]
pub enum Commands {
    #[dispatch(pure, default)]
    ListUnits,
    #[dispatch(pure, name = "about-this")]
    About,
    #[dispatch(pure, silent)]
    Reload,
}

const TEMPLATES: &[(&str, &str)] = &[
    ("list-units", "{{ names | join(', ') }}"),
    ("about-this", "{{ names | join(', ') }}"),
];

/// The clap surface `Commands` is registered against. `Dispatch` connects a
/// variant to a handler; declaring the handler's arguments stays clap's job,
/// so `list-units` carries the `all` its handler reads.
fn command() -> Command {
    Command::new("unitctl")
        .subcommand(
            Command::new("list-units")
                .arg(Arg::new("all").long("all").action(ArgAction::SetTrue)),
        )
        .subcommand(Command::new("about-this"))
        .subcommand(Command::new("reload"))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app: App = App::builder()
        .templates(EmbeddedTemplates::new(TEMPLATES, ""))
        .commands(Commands::dispatch_config())?
        .build()?;
    app.verify_command(&command())?;
    Ok(())
}
```

`ListUnits` registers the command `list-units`, calls
`handlers::list_units__handler`, renders the registry entry `list-units`, and
runs on a naked invocation. Its `#[flag] all` parameter reads the clap argument
`all` that `command()` declares on that subcommand; `verify_command` is what
reports the two drifting apart, instead of leaving it to a `get_flag` panic on
the first invocation. `About` registers `about-this` and renders the
entry of the same name, because `name` changes the registration path and the
convention follows it. `Reload` registers `reload`, calls
`handlers::reload__handler` and renders nothing, so it needs no template.
