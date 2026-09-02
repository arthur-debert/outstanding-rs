# Stream entries under ndjson

`ndjson` joins `OutputMode` as the one stream mode: stdout is a sequence of one-line JSON objects rather than a single document. A handler writes entries of its own through `ctx.stream()`; the framework writes the result, the failure and the warnings as entries of its own. This records decision D3 of `docs/spec/parity-machine-contract.md` and the `ndjson` half of D2; the spec carries the reasoning, in particular why streaming stopped being a non-goal (tflike's diagnostic milestone asserts app-defined `version`, `planned_change` and `change_summary` entries).

## The stream

`CommandContext::stream()` returns an `EntryStream`. Under `ndjson` it is live over a `StreamSink` — the process's stdout, or the buffer `standout-test`'s harness injects through `App::run_with_sink` — and `emit(&value)` serializes the value as compact JSON, writes it as one line and flushes, so a consumer reading the pipe sees the entry when the handler produced it. Under every other mode the stream discards: `emit` neither serializes nor writes. A serialization or write failure is a `StreamError` the handler propagates; the run then fails with it, which under `ndjson` is itself a diagnostic entry after the lines already written. `EntryStream::is_live()` is the one thing a handler may ask about the mode: a handler whose whole result is its entries returns `Output::Silent` when the stream is live and `Output::Render` otherwise, so the human page still renders.

Nothing beyond line-per-value: no buffering, no backpressure, no async. A handler that wants ordering across entries has it, because each `emit` completes before the next statement runs.

## The framework's entries

`Output::Render(T)` under `ndjson` is the line `{"type":"result","data":<T>}`, written after the handler's entries where the other modes write their document. A failure is the D1 diagnostic serialized compact on one line, at the point in the stream where the run failed, through the same `emit_run_result` the other structured modes use; stderr carries nothing for it. Warnings are the one place `ndjson` diverges from the single-document modes (D2): each is a `severity: warning` diagnostic entry on stdout after the result or the failure, of kind `framework` — a kind outside the `RunErrorKind` projection, added because a warning is not a run failure and every kind D1 lists names one.

## Consequences

The harness observes the stream the way a process does: `TestHarness::run` hands the app a capturing sink, so `TestResult::stdout()` is the handler's entries followed by the result or diagnostic and the warning entries, and `TestResult::diagnostic()` reads the error-severity entry out of the stream through `standout::cli::parse_diagnostic`. `App::run_with` keeps its signature and streams to the process's stdout; `run_with_sink` is the injection point.

`ctx.stream()` is the one mode-aware member of `CommandContext`; the guidance that a handler returns the same data in every mode stands for everything else.
