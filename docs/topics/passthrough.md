# Passthrough Commands

Every other way of registering a command — the `Dispatch` derive,
`command_with`, a `#[handler]` function — assumes the handler hands back
something serializable that Standout can render or serialize. A passthrough
command is the one registration shape without that assumption: the handler
writes its own bytes and returns `Result<(), anyhow::Error>`. ADR-0032 keeps
it as a secondary path for that reason: nothing else on the registration axis
accepts a signature with no serializable output and no render.

Reach for it when a command's job is genuinely "run this external process and
let it own the terminal" — streaming a subprocess's output live, driving an
interactive prompt library that writes ANSI itself, or wrapping a tool that
already produces its own formatted output — not for handlers that happen to
print instead of returning data.

## API

```rust
impl AppBuilder {
    pub fn command_passthrough<F>(self, path: &str, handler: F) -> Result<Self, SetupError>
    where
        F: FnMut(&ArgMatches, &CommandContext) -> Result<(), anyhow::Error> + 'static;
}
```

For a command declared inside `.commands(|g| ...)`, `GroupBuilder` has the
matching entry point:

```rust
impl GroupBuilder {
    pub fn passthrough<F>(self, name: &str, handler: F) -> Self
    where
        F: FnMut(&ArgMatches, &CommandContext) -> Result<(), anyhow::Error> + 'static;
}
```

Both take the same closure shape and register the command with no template:
there is nothing for a template to render.

## What it does not get

A passthrough command has no `Output` enum to return, so it gets none of the
things that come from having one:

- No template. The command is registered with its template reference set to
  "absent, silently" — there is no configured or conventional template to
  resolve, by design, not by omission.
- No render pass, so no theme, no style tags, no `Tabular` columns.
- No structured output modes. `Output::Render` is what `--output json` (and
  `yaml`/`xml`/`csv`) serializes; a passthrough handler never produces an
  `Output` value, so those modes have nothing to act on.
- No post-dispatch or post-output hooks run against its result, since both
  operate on a rendered or serializable value that never exists here.

Concretely, under `--output json` (or any other output mode) a passthrough
command runs exactly the same as under the default: the dispatch closure
backing `command_passthrough`/`passthrough` takes the output mode as a
parameter and ignores it. Standout's own output-writing pipeline resolves the
command to an empty string every time — whatever the handler wrote, it wrote
directly (to stdout, stderr, a file, wherever), outside that pipeline, and
`--output` has no way to reach it.

## What the handler is responsible for

Because nothing downstream will format or emit anything on the handler's
behalf, the handler owns:

- Writing everything it wants seen — to stdout, stderr, or elsewhere.
- Its own error reporting for anything it prints before returning `Err(...)`;
  the `anyhow::Error` becomes the command's failure, but text already written
  stays written.
- Respecting (or explicitly ignoring) the user's terminal — colors, width,
  paging — since none of the framework's rendering machinery runs.

## Worked example

```rust
use anyhow::Context;
use clap::ArgMatches;
use standout::cli::{App, CommandContext};
use std::process::Command;

fn run_migrations(_matches: &ArgMatches, _ctx: &CommandContext) -> Result<(), anyhow::Error> {
    let status = Command::new("./migrate.sh")
        .status()
        .context("failed to launch migrate.sh")?;
    if !status.success() {
        anyhow::bail!("migrate.sh exited with {status}");
    }
    Ok(())
}

let app = App::builder()
    .command_passthrough("migrate", run_migrations)?
    .build()?;
```

`migrate.sh`'s own stdout and stderr reach the terminal unchanged; Standout
neither captures nor reformats them.

## When not to use it

The blessed path is a `#[handler]` function returning `Result<T, E>`, wrapped
in `Output::Render` and rendered through a template — see
[Handler Contract](../crates/dispatch/topics/handler-contract.md). Reach for
that whenever the command has any data a caller might want as JSON, any
output a template could format, or any reason to support `--output`. Use
passthrough only when the handler's whole job is to hand control to
something else that already owns its own bytes.
