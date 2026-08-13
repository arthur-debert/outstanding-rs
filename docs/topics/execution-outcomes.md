# Execution Outcomes

Standout carries shell semantics as typed data from argument parsing through
dispatch, rendering, hooks, output files, and final writes. Applications can let
`App::run` own output without losing the distinction between a usage error and a
runtime failure.

## Status and streams

| Outcome | Stream used by `run()` | Status |
| --- | --- | --- |
| Help or version | stdout | `0` |
| Successful rendered text or binary | stdout | `0` |
| `Output::Silent` | none | `0` |
| `--output-file-path` success | file only | `0` |
| Artifact written to a file | bytes to the file, report to stdout | `0` |
| Artifact written to stdout | bytes to stdout, report to stderr | `0` |
| Artifact with no selectable destination | stderr | `1` |
| Clap usage error | stderr | `2` |
| Handler, hook, render, pipe, or write failure | stderr | `1` |
| Application-declared external failure | stderr | exact declared nonzero status |

Warning flushing happens after the primary output and does not replace its
status. A final text or binary write failure does replace a successful status
with `1`, except that `BrokenPipe` while writing text stdout is successful early
consumer termination.

## Capturing typed metadata

`run_to_string` keeps output in-process and returns `RunResult`. Existing
string-oriented accessors remain available, while typed methods expose the
semantics:

```rust
use standout::cli::{ExitStatus, RunResult, SuccessKind};

let result = app.run_to_string(command, args);

match &result {
    RunResult::Handled(output) => println!("{}", output),
    RunResult::Binary(bytes, filename) => consume(bytes, filename),
    RunResult::Artifact(run) => {
        // The framework already wrote a file destination; the receipt says where.
        println!("{:?}: {:?}", run.destination(), run.report());
    }
    RunResult::Error(error) => eprintln!("{}", error),
    RunResult::NoMatch(matches) => legacy_dispatch(matches),
    RunResult::Silent => {}
    _ => {}
}

assert_eq!(result.exit_status(), Some(ExitStatus::SUCCESS));
assert_eq!(result.success_kind(), Some(SuccessKind::Command));
assert_eq!(result.error_kind(), None);
```

`RunOutput` and `RunError` dereference to `str`, implement `Display`, and expose
`as_str()` / `into_string()` for callers that used the tuple payloads as text.
`RunError::kind()` identifies `ClapUsage`, `Handler`, `Hook(phase)`, `Render`, or
`FinalWrite(Text|Binary|Artifact)`. `External` identifies the narrow
application-declared external path; its `exit_status()` is the exact declared
nonzero status and its text is the verbatim diagnostic payload.

## No-match is a handoff, not an error

`RunResult::NoMatch` retains the parsed `ArgMatches` for partial adoption. It has
no framework exit status: `exit_status()` returns `None`, `run()` returns
`false`, and Standout emits nothing. The fallback dispatcher still owns that
command and its eventual status.

## Framework-owned final writes

`run()` writes successful text and binary bytes to stdout, diagnostics to
stderr, and exits with the typed non-zero status when execution fails. A closed
downstream text pipe is not an error: `BrokenPipe` while writing rendered text
means the consumer stopped reading early. The suggested filename on binary
output remains available to capture callers; use `--output-file-path` when the
framework should write either text or binary to a file instead of stdout.

Capture APIs do not perform the final stdout/stderr write, but file redirection
is part of dispatch and therefore reports typed `FinalWrite` failures directly.
External failures are never redirected to an output file: they remain stderr
diagnostics when `run()` performs the final write.

## Compound artifacts

`Output::Artifact` extends the framework-owned write to commands that also have
something to say about it. The application returns bytes, an optional suggested
destination, and an optional report; Standout selects the destination, writes,
and only then renders the report with a receipt naming where the bytes landed.
Ordering is the guarantee: a failed write produces `FinalWrite(Artifact)` and no
report at all, so a success message can never promise a file that never
appeared.

See [Handler Contract](../crates/dispatch/topics/handler-contract.md) for the
destination policy, the report envelope, and the artifact-to-stdout report
channel.
