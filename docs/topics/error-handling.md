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
Error: hook error (pre-dispatch): input `body`: validation failed: body must not be empty
```

A hook's own `Display` names its phase, which is why a hook line carries
`hook error ({phase}):` inside the framing. The wording of these diagnostics is
internal and may change in any release (ADR-0033); an application that must pin
its stderr bytes writes them itself through `AppFailure`, below.

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

`AppFailure` carries a status and bytes, and nothing else. It is not a
structured error type: the machine-readable error envelope belongs to the
parity program's machine contract, which will version the envelope this seam
feeds (ADR-0035).

## Preserving an authoritative external failure

Use `ExternalFailure` when *another* operation owns the status and diagnostic
contract, such as a delegated Git invocation — the application is relaying a
verdict rather than reaching one, which is the whole difference from
`AppFailure`. Construction rejects status `0`, and the diagnostic is a verbatim
stderr payload: Standout adds no `Error:` prefix and no trailing newline.

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

Neither escape hatch is an error-mapping registry. Wrapping an ordinary error
does not change its status, and neither declaration is recognized from
post-dispatch or post-output hooks. Attach an underlying cause with
`with_source` when one exists.

Capture callers match `result.outcome()` / `into_outcome()` as
`DispatchResult::Error`, then inspect the kind (`RunErrorKind::App` or
`RunErrorKind::External`), `error.exit_status()`, and `error.as_str()`. See
[Execution Outcomes](./execution-outcomes.md) and [Testing](./testing.md).
