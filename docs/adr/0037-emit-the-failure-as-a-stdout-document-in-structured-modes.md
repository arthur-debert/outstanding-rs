# Emit the failure as a stdout document in structured modes

Under `--output json`, `yaml` or `csv`, a run writes one document to stdout: the handler's result when the run succeeds, a diagnostic when it fails. Stderr carries nothing the framework wrote for the failure. Human modes are unchanged: prose on stderr, the `Error:` framing ADR-0035 describes. Exit statuses do not move. This records decisions D1, D2 and D6 of `docs/spec/implemented/parity-machine-contract.md`, whose reasoning is not repeated here.

## The document

`Diagnostic` (`standout::cli::Diagnostic`) is the one shape, serialized flat: `type` (always `"diagnostic"`), `schema_version`, `severity`, `kind`, `summary`, `detail`, and an optional `range` of `filename` and `start.line` / `start.column` (D1). `kind` is a `DiagnosticKind`, the `RunErrorKind` projected onto D1's fixed vocabulary: hook phases stay distinct (`hook-pre-dispatch`), every `FinalWrite` payload collapses to `final-write`, so the wire never learns a name the spec does not list. A handler returns a `Diagnostic` as its error to fill `detail` and `range`; any other error reaches the document with its `Display` text as `summary` and an empty `detail`. `RunError::diagnostic()` is where a failure becomes the value, so every kind the framework classifies has a document, and `emit_run_result` is the only writer.

`AppFailure` and `ExternalFailure` are not exceptions to the document but additions to it: their verbatim bytes still reach stderr in every mode (ADR-0035), and the structured modes add a stdout document of kind `app` or `external` whose `detail` is the bytes (D2). Warnings stay prose on stderr in these modes, because a single-document mode has one root.

## Which mode a pre-parse failure is in

A failure clap reports before a parse completes — an unknown flag, a default-command miss, `--help` — has no `ArgMatches` to read the mode from. The framework already scanned raw argv for the output flag for the clap-error path; `--help` and `-h` now consume the same scan (D6), which is what closes #295: `--output` reaches the flags exactly as it reaches the `help` word. A value that is not a mode (`--output jsn`) stays a clap usage error as prose, exit 2, whichever form asked: the mode is the thing that is unknown, so there is nothing to serialize the diagnostic in.

## Where the harness stands

`standout-test` no longer keeps its own copy of the emission rule. `TestHarness::run` writes both streams through `emit_run_result`, and `TestResult::diagnostic()` reads the stdout document back through the framework's own parser, so a test observes what a process writes and the two cannot drift. `TestResult::stdout()` keeps its meaning as the rendered text: the one newline `emit_run_result` terminates rendered text with is the difference between it and a process's stdout.

## Consequences

`xml` keeps the prose path: D7 deletes the mode, so it is not given a document shape first. `ndjson` (D3), the `ContractSurface` trait behind `schema_version` (D4), and the general CSV flat-record rule (D8) land in the epic's later workstreams; the diagnostic's CSV row — three `range_*` columns, empty when unset — is fixed here because the row is the diagnostic's own shape.
