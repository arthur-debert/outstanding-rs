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
| Success with a status the handler declared | stdout, as any success | exact declared status |
| `--output-file-path` success | file only | `0` |
| Artifact written to a file | bytes to the file, report to stdout | `0` |
| Artifact written to stdout | bytes to stdout, report to stderr | `0` |
| Artifact with no selectable destination | stderr | `1` |
| Clap usage error | stderr | `2` |
| Handler, hook, render, pipe, or write failure | stderr | `1` |
| Application-declared external failure | stderr | exact declared nonzero status |

The framework names no exit code beyond `0`, `1` and `2`. A successful run that
wants to say more — "planned, with changes", "checked, with findings",
"listed nothing" — declares its own status, and that status is
success-with-signal, never an error: the result still goes to stdout, nothing
becomes a diagnostic, warnings flush as usual, and only the process status
differs. The Rust ecosystem has no convention for such codes (`ExitCode` knows
`0` and `1`, clap uses `2`, cargo's `101` marks a panic), so the meaning of a
declared status is the application's to document.

```rust,ignore
// A plan with changes exits 2, the way `terraform plan -detailed-exitcode` does.
let output = Output::Render(plan);
if detailed_exitcode && has_changes {
    return Ok(output.with_exit_status(ExitStatus::from(2)));
}
Ok(output)

// A list that found nothing exits 3; a non-empty list exits 0.
Ok(list_view(items).empty_exit_status(3).output())
```

`Output::with_exit_status(ExitStatus)` is the general form and applies to
`Output::Render` and `Output::Silent`; calling it again replaces the status. A
status declared on `Output::Binary` or `Output::Artifact` is a render error
(`1`), since those outcomes carry no status of their own.
`ListViewBuilder::empty_exit_status(n)` is the list case
([List View](./list-view.md#empty-lists-and-the-exit-status)). The
capture API (`App::run_command`) returns rendered output with no status (it
still reports the binary and artifact case as an error), so a declared status
is visible through `run`, `run_emitted`, `dispatch`, `run_with` and the test
harness, which report it through `exit_status()`.

The failure rows describe the human representation and its `term-debug`
diagnostic view; under a structured encoding a failure is the stdout document
instead, [below](#failures-under-a-structured-mode).

Framework warnings flush after the primary output and do not change its
status. A final write failure does replace a successful status with `1`
([below](#framework-owned-final-writes)).

## Failures under a structured mode

When the resolved representation is `json`, `yaml`, `csv` or `ndjson`, stdout
carries the result or the diagnostic, never both, and stderr carries nothing
the framework wrote for the failure. In the single-document modes that is one
document per run; under `ndjson` the diagnostic is one compact line at the
point in the stream where the run failed, after the entries the handler already
wrote ([Output Modes](./output-modes.md#ndjson-mode)). Statuses do not change.
An `App` or `External` failure still writes its verbatim bytes to stderr
and adds the stdout document, kind `app` or `external`, with the bytes as
`detail` and their first line as `summary`. Warnings stay prose on stderr in
the single-document modes,
because the document has one root; under `ndjson` each is a `severity:
warning` diagnostic entry of kind `framework` on stdout, after the result or
the failure.

The document is `Diagnostic`, serialized flat; `range` is present only when set:

```json
{
  "type": "diagnostic",
  "schema_version": 1,
  "severity": "error",
  "kind": "handler",
  "summary": "config line 2 does not parse",
  "detail": "expected `resource <name> <state>`",
  "range": { "filename": "main.tfl", "start": { "line": 2, "column": 1 } }
}
```

`kind` is a `DiagnosticKind`, the `RunErrorKind` projected onto the fixed wire
vocabulary: `clap-usage`, `default-command`, `handler`, `hook-pre-dispatch`,
`hook-post-dispatch`, `hook-post-output`, `render`, `final-write` (for every
`FinalWrite` payload), `external`, `app` — plus `framework`, the kind of a
warning entry, which no run failure produces. In `csv` the
document is one row whose header ends in `range_filename`, `range_line` and
`range_column`, empty when there is no range. `RunError::diagnostic()` is the
value, `emit_run_result` writes it, and `standout::cli::parse_diagnostic` reads
it back (under `ndjson`, out of the whole stream), as `TestResult::diagnostic()`
does. How an error fills `summary` and `detail` is in
[Error Handling](./error-handling.md#the-diagnostic-document).

A failure clap reports before a parse completes — an unknown flag, a
default-command miss, `--help` — takes its mode from a scan of the raw argv for
the output flag: the last occurrence wins, `--output json` and `--output=json`
alike, and nothing after `--` counts. A value that is not a mode
(`--output jsn`) scans as no mode at all, so the run stays in the fallback
mode: when the parse reaches the value, clap reports it as a usage error, prose
on stderr, exit 2.

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

`run_emitted` writes through `standout::cli::emit_run_result(&outcome,
output_mode, &mut stdout, &mut stderr)`, which is public: `standout-test`'s
harness writes its two streams with it, and an adopter that keeps its own
process edge can too. It returns `Ok(handled)` or the final-write failure whose
status replaces the run's own. Under `ndjson` the warnings follow through
`standout::cli::emit_warning_entries`; every other mode flushes them as stderr
prose, and so does a `NoMatch` handoff in every mode, since Standout then owns
no stdout. Events a handler emits through its `Results` channel reach stdout
while the handler runs, before either: `run_emitted` dispatches over a
`StreamSink` around the process's stdout and writes the result and the warning
entries through the same sink, so a `--output-file-path` that retargeted the
sink receives the whole run (events, result or diagnostic, warnings) and stdout
nothing. Nothing reads that run's values back, so it retains the summary alone
and writes each event without keeping it, and a long run costs no memory for
its length. `run_with` and `run_with_sink` capture the events instead and
return them as `CompletedRun::entries()`, alongside the values in
`CompletedRun::results()`.

`ProcessOutcome` has two public fields. `handled` is the `bool` that `run`
returns: `false` only for a `NoMatch` handoff. `status` is the final
`ExitStatus` after output errors, the one `run` would have exited with
([final writes](#framework-owned-final-writes)); a `NoMatch` handoff reports
`ExitStatus::SUCCESS` because Standout emitted nothing. Everything has been
written when the call returns, so a process-lifetime resource — a span
exporter, an audit log, a buffered writer — can close between the last byte of
output and the exit.

## Capturing typed metadata

`run_with` keeps output in-process and returns `CompletedRun`: the dispatch
outcome, any framework warnings collected during the run, and under `ndjson`
the lines the handler streamed (`entries()`, each with its newline), which a
process would have written before the result. `App::dispatch` captures the
same way; `run_command` takes the `StreamSink` as a parameter. `Deref` keeps
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
stderr in a human mode and to stdout as a document in a structured one, and
exits with the typed non-zero status when execution fails. The one
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

Capture APIs return the run's warnings instead of rendering them
(`CompletedRun::warnings()`, `TestResult::warnings()`).

## Compound artifacts

`Output::Artifact` extends the framework-owned write to commands that also have
something to say about it: Standout selects the destination, writes the bytes,
and emits the report; a failed write produces `FinalWrite(Artifact)` and no
report at all. The destination policy, the report envelope and the report
channel are in [Handler Contract](../crates/dispatch/topics/handler-contract.md#outputartifact).
