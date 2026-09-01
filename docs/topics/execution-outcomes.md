# Execution Outcomes

Standout carries shell semantics as typed data from argument parsing through
dispatch, rendering, hooks, output files, and final writes. Applications can let
`App::run` own output without losing the distinction between a usage error and a
runtime failure.

## Status and streams

| Outcome | Stream used by `run()` and `run_emitted()` | Status |
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

Framework warning flushing happens after the primary output and does not replace
its status. Warnings cover non-fatal framework-owned setup, resource-loading,
and accepted-input diagnostics, including answer-sheet parse warnings that do
not reject a questionnaire submission. A final text or binary write failure does
replace a successful status with `1`, except that `BrokenPipe` while writing
final rendered command text to stdout is successful early consumer termination.

## Emitting without exiting

`run_emitted` is `run` up to the exit: it detects the destination, dispatches,
pages help, writes both streams, flushes warnings, and then returns a
`ProcessOutcome` instead of ending the process. `run` calls it and exits with
the status it reports, so the two never differ in what reaches stdout, stderr,
a file, or the pager.

```rust,ignore
let outcome = app.run_emitted(Cli::command(), std::env::args());
telemetry.shutdown();
std::process::exit(outcome.status.code().into());
```

`ProcessOutcome` has two public fields. `handled` is the `bool` that `run`
returns: `false` only for a `NoMatch` handoff. `status` is the final
`ExitStatus` after output errors, the one `run` would have exited with: a final
write failure has already replaced a successful status, `BrokenPipe` on final
rendered text has already been accepted as success, and a `NoMatch` handoff
reports `ExitStatus::SUCCESS` because Standout emitted nothing. Everything has
been written when the call returns, so a process-lifetime resource — a span
exporter, an audit log, a buffered writer — can close between the last byte of
output and the exit.

## Capturing typed metadata

`run_with` keeps output in-process and returns `CompletedRun`: the dispatch
outcome plus any framework warnings collected during the run. `Deref` keeps
string-oriented accessors and typed methods (`exit_status()`, `success_kind()`,
`error_kind()`) working on the wrapper. Pattern matching needs `outcome()` or
`into_outcome()`, because `CompletedRun` is not the variant enum.

```rust
use standout::cli::{
    CompletedRun, DispatchResult, ExitStatus, OutputKind, RunError, RunErrorKind, SuccessKind,
};
use standout::{InputSources, TargetProperties};

let result = app.run_with(
    command,
    args,
    TargetProperties::detect(),
    InputSources::from_process(),
);
let _ = result.warnings();

match result.outcome() {
    DispatchResult::Handled(output) => println!("{}", output),
    DispatchResult::Binary(bytes, filename) => consume(bytes, filename),
    DispatchResult::Artifact(run) => {
        use std::io::{self, Write};
        if run.destination().is_stdout() {
            let mut stdout = io::stdout();
            stdout.write_all(run.bytes()).and_then(|()| stdout.flush()).map_err(|error| {
                RunError::new(
                    format!("Error writing artifact stdout: {}", error),
                    RunErrorKind::FinalWrite(OutputKind::Artifact),
                )
            })?;
            if let Some(report) = run.report().filter(|r| !r.is_empty()) {
                let mut stderr = io::stderr();
                writeln!(stderr, "{}", report).and_then(|()| stderr.flush()).map_err(|error| {
                    RunError::new(
                        format!("Error writing artifact report: {}", error),
                        RunErrorKind::FinalWrite(OutputKind::Artifact),
                    )
                })?;
            }
        } else if let Some(report) = run.report().filter(|r| !r.is_empty()) {
            let mut stdout = io::stdout();
            writeln!(stdout, "{}", report).and_then(|()| stdout.flush()).map_err(|error| {
                RunError::new(
                    format!("Error writing artifact report: {}", error),
                    RunErrorKind::FinalWrite(OutputKind::Artifact),
                )
            })?;
        }
    }
    DispatchResult::Error(error) => eprintln!("{}", error),
    DispatchResult::NoMatch(_matches) => {}
    DispatchResult::Silent => {}
    _ => {}
}

assert_eq!(result.exit_status(), Some(ExitStatus::SUCCESS));
assert_eq!(result.success_kind(), Some(SuccessKind::Command));
assert_eq!(result.error_kind(), None);
```

Use `into_outcome()` when the fallback needs owned `ArgMatches`
(`DispatchResult::NoMatch(matches)`).

`RunOutput` and `RunError` dereference to `str`, implement `Display`, and expose
`as_str()` / `into_string()` for callers that used the tuple payloads as text.
`RunOutput::kind()` names a success the same way: `Command` for a handler's
output, `ClapHelp` / `ClapVersion` for a help or version display, and
`PagedHelp` for a `help --page` display — the text is identical, but the kind
tells a printing caller the user asked for a pager.
`RunError::kind()` identifies `ClapUsage`, `Handler`, `Hook(phase)`, `Render`, or
`FinalWrite(Text|Binary|Artifact)`. `External` identifies the narrow
application-declared external path; its `exit_status()` is the exact declared
nonzero status and its text is the verbatim diagnostic payload.

## No-match is a handoff, not an error

`DispatchResult::NoMatch` retains the parsed `ArgMatches` for partial adoption. It has
no framework exit status: `exit_status()` returns `None`, `run()` returns
`false`, `run_emitted()` reports `handled: false`, and Standout emits nothing. The fallback dispatcher still owns that
command and its eventual status.

The reverse direction never hands off. Before parsing, `run` and
`run_with` check every registered path against the clap `Command`: a
handler registered under a path the CLI declares no subcommand for is
unreachable — no invocation can name it and no fallback owns it, since the app
did register a handler. That returns `DispatchResult::Error` naming the
registered path, and the clap spelling too when the two differ only by `-`
versus `_` (`list_units` registered against a CLI declaring `list-units`).
`App::verify_command` reports the same mismatch at setup time.

That check reads canonical command names only. Clap resolves an alias to the
command it names before `ArgMatches` reports it, so dispatch never sees the
alias: a handler registered as `ls` against `Command::new("list").alias("ls")`
is reached by neither spelling and is reported as unreachable. Registering
`list` is what makes both `list` and `ls` run the handler.

## Framework-owned final writes

`run()` writes successful text and binary bytes to stdout, diagnostics to
stderr, and exits with the typed non-zero status when execution fails. The one
exception is a paged help display (`SuccessKind::PagedHelp`), which goes to the
pager instead; if no pager is available it falls back to stdout, so help is
never lost. A closed
downstream pipe is not an error only for final rendered command text:
`BrokenPipe` there means the consumer stopped reading early. Binary stdout
writes and artifact report writes keep their typed final-write failures. The
suggested filename on binary output remains available to capture callers; use
`--output-file-path` when the framework should write either text or binary to a
file instead of stdout.

Capture APIs do not perform the final stdout/stderr write, but file redirection
is part of dispatch and therefore reports typed `FinalWrite` failures directly.
External failures are never redirected to an output file: they remain stderr
diagnostics when `run()` performs the final write.

Capture-mode runs drain framework warnings instead of rendering them. The raw
capture path stores the batch in `standout-render`'s warning collector, and
`standout-test::TestHarness` exposes the batch through `TestResult::warnings()`
so tests can assert warning content deterministically.

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
