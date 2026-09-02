# Error Handling

Standout owns the shell adapter: handlers and hooks return errors as data, and
`App::run` performs the final stderr write and process exit. Ordinary handler,
hook, render, pipe, and final-write failures use status `1`; Clap usage failures
use status `2`.

## The handler diagnostic framing

One framing covers every diagnostic Standout writes on an application's behalf:
a fixed `Error:` prefix, then the error's own `Display` text, then a newline.
Handler failures and hook failures both use it, so a reader sees one shape:

```text
Error: could not read /etc/myapp.toml
Error: hook error (pre-dispatch): input `body`: Validation failed: body must not be empty
```

A hook's own `Display` names its phase, which is why a hook line carries
`hook error ({phase}):` inside the framing. The wording of these diagnostics is
internal and may change in any release (ADR-0033); an application that must pin
its stderr bytes writes them itself through `AppFailure`, below.

## The diagnostic document

Under `--output json`, `yaml`, `csv` or `ndjson` the framing above does not
apply: the failure is the stdout document, and stderr carries nothing for it.
Under `ndjson` the document is one line in the stream, written where the run
failed, after the entries the handler emitted before it. The document
is `Diagnostic` (`standout::cli::Diagnostic`): `type`, `schema_version`,
`severity`, `kind`, `summary`, `detail`, and an optional `range`. An ordinary
error becomes `summary` from its `Display` with an empty `detail`; a handler
that has more to say returns a `Diagnostic` as its error:

```rust
use standout::cli::{Diagnostic, HandlerResult};

fn handler(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<View> {
    Err(Diagnostic::error("config line 2 does not parse")
        .detail("expected `resource <name> <state>`")
        .range("main.tfl", 2, 1)
        .into())
}
```

In a human mode the same value is prose under the framing:
`Error: main.tfl:2:1: config line 2 does not parse`, the detail on the next
line. A hook failure reaches the document with the `HookError`'s `message` as
`summary`; its phase is the `kind` (`hook-pre-dispatch`, `hook-post-dispatch`,
`hook-post-output`). The shape per mode, the `kind` vocabulary and the argv
scan that picks the mode for a pre-parse failure are in
[Execution Outcomes](./execution-outcomes.md#failures-under-a-structured-mode).

## Ordinary application errors

Return ordinary errors through `HandlerResult` with `?`. Standout applies the
handler diagnostic framing, reports the failure under `RunErrorKind::Handler`,
and exits with status `1`:

```rust
fn handler(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<View> {
    let view = load_view()?;
    Ok(Output::Render(view))
}
```

Do not print or call `process::exit` from handlers. This keeps capture APIs,
`TestHarness`, output ownership, and real process behavior on the same seam.

## An application-owned status and diagnostic

`AppFailure` is the seam for a domain error whose exit status and stderr bytes
the application's own specification pins. It carries any nonzero `u8` and a
verbatim stderr payload: Standout adds no `Error:` prefix and no trailing
newline, and the status rides to the process exit. Construction rejects status
`0`, so a domain error can never report shell success.

```rust
use standout::cli::{AppFailure, HandlerResult};

fn handler(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<View> {
    let Some(repo) = find_repo()? else {
        return Err(AppFailure::new(1, "ghlike: repository not found: demo/gamma\n")?.into());
    };
    Ok(Output::Render(to_view(repo)))
}
```

A pre-dispatch guard reaches the same seam through
`HookError::pre_dispatch_app`. Capture callers see `RunErrorKind::App`.

`AppFailure` carries a status and bytes, and nothing else. Under a structured
mode the bytes still reach stderr verbatim, and stdout carries a diagnostic of
kind `app` whose `detail` is the bytes and whose `summary` is their first line
(ADR-0037).

## Preserving an authoritative external failure

Use `ExternalFailure` when *another* operation owns the status and diagnostic
contract, such as a delegated Git invocation — the application is relaying a
verdict rather than reaching one, which is the whole difference from
`AppFailure`. Construction rejects status `0` and validates nothing else: an
empty diagnostic is accepted, which is what makes a nonzero exit with no output
on either stream expressible. The diagnostic is a verbatim stderr payload:
Standout adds no `Error:` prefix and no trailing newline.

```rust
use standout::cli::{ExternalFailure, HandlerResult};

fn handler(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<View> {
    let output = run_git()?;
    if !output.status.success() {
        let status = output.status.code().and_then(|code| u8::try_from(code).ok()).unwrap_or(1);
        let diagnostic = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(ExternalFailure::new(status, diagnostic)?.into());
    }
    Ok(Output::Render(to_view(output)))
}
```

A supported pre-dispatch check uses the same `ExternalFailure` interface:

```rust
Hooks::new().pre_dispatch(|_matches, _ctx| {
    let failure = ExternalFailure::new(128, "fatal: repository not found\n")
        .expect("128 is nonzero");
    Err(HookError::pre_dispatch_external(failure))
})
```

Under a structured mode an `ExternalFailure` behaves as `AppFailure` does: the
bytes reach stderr verbatim and stdout carries a diagnostic of kind `external`.

Neither escape hatch is an error-mapping registry. Wrapping an ordinary error
does not change its status, and neither declaration is recognized from
post-dispatch or post-output hooks. Attach an underlying cause with
`with_source` when one exists.

Capture callers match `result.outcome()` / `into_outcome()` as
`DispatchResult::Error`, then inspect the kind (`RunErrorKind::App` or
`RunErrorKind::External`), `error.exit_status()`, and `error.as_str()`. See
[Execution Outcomes](./execution-outcomes.md) and [Testing](./testing.md).
