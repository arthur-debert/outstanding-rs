# Error Handling

Standout owns the shell adapter: handlers and hooks return errors as data, and
`App::run` performs the final stderr write and process exit. Ordinary handler,
hook, render, pipe, and final-write failures use status `1`; Clap usage failures
use status `2`.

## Ordinary application errors

Return ordinary errors through `HandlerResult` with `?`. Standout adds the
normal handler diagnostic framing and reports status `1`:

```rust
fn handler(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<View> {
    let view = load_view()?;
    Ok(Output::Render(view))
}
```

Do not print or call `process::exit` from handlers. This keeps capture APIs,
`TestHarness`, output ownership, and real process behavior on the same seam.

## Preserving an authoritative external failure

Use `ExternalFailure` only when another operation owns the status and diagnostic
contract, such as a delegated Git invocation. Construction rejects status `0`.
The diagnostic is a verbatim stderr payload: Standout adds no `Error:` prefix
and no trailing newline.

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

The escape hatch is not an error-mapping registry. Wrapping an ordinary error
does not change its status, and external declarations are not recognized from
post-dispatch or post-output hooks. Attach an underlying cause with
`ExternalFailure::with_source` when one exists.

Capture callers match `RunResult::Error`, then inspect
`RunErrorKind::External`, `error.exit_status()`, and `error.as_str()`. See
[Execution Outcomes](./execution-outcomes.md) and [Testing](./testing.md).
