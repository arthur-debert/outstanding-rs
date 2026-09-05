# PAR02: The Machine Contract

> **Implemented** by PAR02 (#475): WS01 #485 (ADR-0037), WS02 #487 (ADR-0038),
> WS03 #488 (ADR-0039), WS04 #486 (ADR-0040). As built, where the text below
> differs:
>
> - D6: a `--output` value that is not a mode is a usage error where clap reaches
>   it. `myapp --output jsn --help` and `myapp help --output jsn` exit 2 with
>   prose; `myapp --help --output jsn` renders the help page, because clap stops
>   at `--help` before it validates the value.
> - D3: `Output::Binary` and `Output::Artifact` are render errors under `ndjson`,
>   decided before anything is written; a stream has no room for a payload.
> - D9: the help document exists under `json` and `yaml`; `--help --output ndjson`
>   renders the human page. `short` and `long` in the document are the tokens as
>   typed (`--tree`), not the bare word the example shows.
> - D4: the envelope is an application opt-in, so brewlike's
>   `list-json-payload-carries-the-schema-version` passes for an implementation
>   built from the archetype spec (mirrored in
>   `crates/standout-test/tests/contract_surfaces.rs`).
> - `ctx.output_mode()` in the examples does not exist; `ctx.stream().is_live()`
>   is the one mode predicate a handler has. `DiagnosticKind` has one name beyond
>   D1's list, `framework`, the kind of a warning entry.
> - The streaming example below is superseded. A handler emits typed events
>   through the `Results<E>` channel it is handed, and neither `ctx.stream()` nor
>   `is_live()` survives: see
>   [Typed Command Output](./typed-command-output.md) and
>   [ADR-0041](../../adr/0041-hand-the-handler-a-typed-results-sink.md).

First epic of the capability-parity program to execute. The program's order is PAR02,
then PAR01 (config layering), then the
terminal-behavior epic that `docs/spec/implemented/typed-command-output.md` has since replaced, then
PAR05 (named configuration sets). PAR02 depends on nothing
unfinished: the composition-contracts work (ADRs 0025 to 0035) already put every
failure through one function, `emit_run_result` in
`crates/standout/src/cli/builder/execution.rs`, and already scans raw argv for
`--output` (`last_unparsed_flag_value` in `crates/standout/src/cli/builder/mod.rs`).

## Problem

Standout's pitch is that a handler returns data and the framework renders it, so a
machine can drive any standout app with `--output json`. Four things break that pitch
today.

**A failure is prose in every mode.** `emit_run_result` writes the error string to
stderr without looking at the resolved output mode. A script running
`myapp list --output json` gets either a JSON document or nothing on stdout, and a
sentence on stderr, and has to guess which. Downstream apps work around it in code:
padz strips the `Error:` prefix standout writes by string match
(`crates/padz/src/cli/commands.rs:165-173`) and notes that a `PadzError` "is flattened
to text at the handler boundary, anything that wants to look at the error's structure
must do it now or not at all" (`crates/padz/src/cli/errors.rs:17-27`). comitia filed
issue #430 for the same reason.

**There is no way to run the framework's process edge without exiting.** `App::run` is
the only function that pages, emits, flushes warnings and sets the exit status, and it
ends in `process::exit`. `run_with` returns before any of that. dodot
(`crates/dodot-cli/src/main.rs:146-203`), rustloc (`crates/rustloc/src/main.rs:892-937`)
both reimplement the edge by hand, and
rustloc's copy exits 2 for every failure, including handler failures. comitia filed
issue #458.

**A successful command cannot declare an exit status.** `ExitStatus` is 0, 1 and 2 plus
the verbatim status of `AppFailure`/`ExternalFailure`. dodot keeps a
`PENDING_EXIT_CODE: AtomicI32` side channel (`crates/dodot-cli/src/handlers.rs:24-37`)
because "succeeded, with findings" has no framework spelling. The tflike archetype needs
`plan -detailed-exitcode` to exit 2 when the plan has changes.

**Structured output has no stability or shape rules.** `--output json` serializes the
handler's view struct as-is, so a renamed Rust field is a silent break for every script
downstream. CSV flattens nested data into `items.0.name` columns (rustloc adapts to flat
rows by hand, `crates/rustloc/src/main.rs:445-452`). XML wraps every non-struct in a
`<data>` root and drops rows whose keys collide after sanitization (#409), through a
quick-xml version with two RustSec advisories (#408). No client project uses XML.

## What the user gets

### A CLI user or script author

```text
$ myapp list --output json
{"schema_version":1,"data":[{"name":"a"},{"name":"b"}]}

$ myapp list --bogus-flag --output json ; echo "exit $?"
{"type":"diagnostic","schema_version":1,"severity":"error","kind":"clap-usage","summary":"unexpected argument '--bogus-flag'","detail":"..."}
exit 2

$ myapp apply --output ndjson
{"type":"version","format_version":1}
{"type":"apply_start","resource":"web"}
{"type":"diagnostic","schema_version":1,"severity":"error","kind":"handler","summary":"web: refused","detail":"..."}
exit 1

$ myapp deps --help --output json
{"schema_version":1,"name":"deps","path":["myapp","deps"],"usage":"myapp deps [OPTIONS]","about":"...","args":[{"name":"tree","long":"tree","short":null,"help":"...","required":false,"value_name":null,"default":null,"possible_values":[]}],"subcommands":[]}
```

In a structured mode (`json`, `yaml`, `csv`, `ndjson`), stdout is always one document
or one stream of that mode. A failure makes the document a diagnostic. `--output`
applies to `--help`, `-h` and usage errors exactly as it applies to a command, which
closes #295. `xml` is no longer a mode; passing it is a usage error.

### An app author

```rust
// A view marked as a versioned contract surface.
#[derive(Serialize, ContractSurface)]
#[contract(schema_version = 1)]
struct Listing { items: Vec<Item> }

#[handler]
fn list(#[ctx] ctx: &CommandContext) -> Result<Output<Envelope<Listing>>, anyhow::Error> {
    let listing = load()?;
    if listing.items.is_empty() {
        // A successful run that wants a non-zero status declares it verbatim.
        return Ok(Output::Render(listing.envelope()).with_exit_status(ExitStatus::from(3)));
    }
    Ok(Output::Render(listing.envelope()))
}

// A streaming command: entries go out one line at a time in ndjson mode.
#[handler]
fn apply(#[ctx] ctx: &CommandContext) -> Result<Output<()>, anyhow::Error> {
    let stream = ctx.stream();
    stream.emit(&Version { format_version: 1 })?;
    for change in plan()? {
        stream.emit(&ApplyStart { resource: &change.name })?;
        change.apply()?;                       // a failure here becomes a diagnostic entry
        stream.emit(&ApplyComplete { resource: &change.name })?;
    }
    Ok(Output::Silent)
}

// Diagnostics an app produces itself, with a source range.
Err(Diagnostic::error("config line 2 does not parse")
    .detail("expected `resource <name> <state>`")
    .range("main.tfl", 2, 1)
    .into())
```

`ctx.stream()` writes in `ndjson` mode and discards in every other mode; the handler
branches on `ctx.output_mode()` when it wants human output too. `ListView` takes the
same opt-in for its empty case: `ListView::new(items).empty_exit_status(3)`.

The non-exiting entry point (#458):

```rust
let outcome: ProcessOutcome = app.run_emitted(Cli::command(), std::env::args());
// outcome.handled: bool, outcome.status: ExitStatus; everything was written.
std::process::exit(outcome.status.code().into());
```

`App::run` becomes `run_emitted` followed by `process::exit`.

### A test author

```rust
let result = harness.run(["list", "--output", "json"]);
result.assert_schema_snapshot("list.json");      // keys and types, not values
let failed = harness.run(["list", "--output", "json", "--bogus"]);
assert_eq!(failed.diagnostic().unwrap().kind, RunErrorKind::ClapUsage);
```

`standout-test` reads the diagnostic document back into the `Diagnostic` struct, and
its `render_stderr` copy of the emission rule is deleted in favor of calling the one
framework function.

## Decisions

**D1. One diagnostic struct, serialized flat.** Fields: `type` (always
`"diagnostic"`), `schema_version`, `severity` (`error` or `warning`), `kind` (the
`RunErrorKind` variant in kebab-case: `clap-usage`, `default-command`, `handler`,
`hook-pre-dispatch` and the other hook phases, `render`, `final-write`, `external`,
`app`), `summary`, `detail`, and an optional `range` of `filename`, `start.line`,
`start.column`. The tflike gap suite asserts this shape, so it is not open. A handler
returns a `Diagnostic` as its error to fill `range` and `detail`; any other error type
becomes `summary` from `Display` with an empty `detail`.

**D2. Stdout in every structured mode.** The document on stdout is the result or the
diagnostic, never both, and stderr carries nothing framework-emitted for a
framework-owned failure. In `ndjson` a warning is a `severity: warning` entry; in the
single-document modes warnings stay on stderr as prose, because the document has one
root. The alternative, diagnostics on stderr as kubectl and gh do, keeps stdout empty on
failure but gives a consumer two streams to parse; one parse path wins. An `AppFailure`
or `ExternalFailure` keeps its verbatim stderr bytes (ADR-0035 is unchanged) and also
produces a stdout diagnostic of kind `app` or `external` whose `detail` is the bytes,
lossily decoded.

**D3. An `ndjson` mode with handler-emitted entries.** `OutputMode::Ndjson` joins the
enum. `ctx.stream()` returns a handle whose `emit(&impl Serialize)` writes one line in
`ndjson` mode and does nothing otherwise. `Output::Render(T)` in `ndjson` mode writes
one line `{"type":"result","data":<T>}`. The earlier spec text listed streaming as a
non-goal; tflike's diagnostic milestone asserts app-defined `version`,
`planned_change` and `change_summary` entries, so that non-goal contradicted the exit
criterion. Nothing beyond line-per-value: no buffering, no backpressure, no async.

**D4. Version in the document, marker as a trait.** `ContractSurface` is a trait with
`const SCHEMA_VERSION: u32`, derived by `#[derive(ContractSurface)]` with
`#[contract(schema_version = N)]`. `Envelope<T: ContractSurface>` serializes as
`{"schema_version":N,"data":<T>}`, and `T::envelope(self)` constructs it. Framework
documents (`ListViewResult`, the help document, the diagnostic) carry `schema_version`
as a top-level key beside their own fields, because the framework owns their shape.
There is no `--format-version` flag: brewlike's cases pin the key, and a key needs no
parsing. Adding the key to `ListViewResult` is the one breaking change, released as a
major. `standout-test` gains `assert_schema_snapshot`, which compares key names and JSON
value types against a stored file and ignores values.

**D5. Exit statuses stay 0, 1, 2; success-with-signal is app-supplied.** The Rust
ecosystem has no convention for "succeeded, found nothing": `std::process::ExitCode`
defines 0 and 1, clap uses 2 for usage errors (standout already matches), cargo's 101
marks a panic. So the framework names no new code. `Output::with_exit_status(ExitStatus)`
lets a successful handler declare any status, written verbatim like `AppFailure`'s.
`ListView::empty_exit_status(n)` is the opt-in for the list case. The earlier text
wanted exactly one framework code; tflike needs 2 for "changes" and dodot needs
"findings", and no single code covers both.

**D6. The argv scan decides the mode for pre-parse failures; a malformed value is prose.**
`last_unparsed_flag_value` already exists with last-occurrence-wins, `=` and space
forms, and `--` termination. `render_help_for_display_help_error` and the clap-error
path start consuming it, which is the whole of #295. A malformed `--output` value
(`--output jsn`) is a clap usage error with prose on stderr and exit 2, like any bad
flag value: the mode is the thing that is unknown, so there is nothing to serialize
the diagnostic in. The earlier text asked for a structured diagnostic in that case.

**D7. XML is deleted.** `OutputMode::Xml`, `serialize_to_xml`, `sanitize_xml_keys` and
the quick-xml dependency go. No client project uses the mode; deleting it closes the
three XML issues #107, #408 and #409 at once. The change is a line in the major's release notes.

**D8. CSV takes flat records only.** A flat record is a map whose values are scalars.
CSV accepts one flat record or an array of flat records; any nested value is a
`RenderError` naming `CsvProjection` as the way to declare columns. The indexed
`items.0.name` flattening is deleted. This closes #108 by replacing silent flattening
with a loud error. The framework's own documents obey the same rule. The diagnostic
(D1) carries its own `CsvProjection`: one row whose `range` is the three columns
`range_filename`, `range_line` and `range_column`, empty when no range is set. The help
document (D9) has no projection, so `--help --output csv` is this `RenderError`, emitted
as a diagnostic row of kind `render`.

**D9. Help is a versioned document under json and yaml.** Fields: `schema_version`,
`name`, `path`, `usage`, `about`, `args` (each with `name`, `short`, `long`,
`value_name`, `required`, `help`, `default`, `possible_values`) and `subcommands` (each
`name` and `about`). `usage` names the full path to the command, which fixes #453 for
the human page as well. ADR-0029 held this back until it could be versioned; it is
versioned here.

**D10. `run_emitted` before everything else.** `App::run` splits into
`run_emitted(cmd, args) -> ProcessOutcome { handled, status }`, which does everything
`run` does today except `process::exit`, and `run` calls it. This lands as the first
PR because D1 and D2 change the function it extracts, and because it is what dodot,
and rustloc replace their hand-written edges with.

## Workstreams

**WS01: The process edge and the diagnostic document.** `run_emitted` (D10); the
`Diagnostic` struct; `emit_run_result` takes the output mode and serializes the
diagnostic in json, yaml and csv; the argv scan feeds usage errors and `--help` (D6);
`standout-test` calls the framework function instead of its `render_stderr` copy. Files:
`crates/standout/src/cli/builder/execution.rs`, `crates/standout/src/cli/builder/mod.rs`,
`crates/standout-dispatch/src/handler.rs`, `crates/standout-test/src/lib.rs`,
`docs/topics/execution-outcomes.md`, `docs/topics/error-handling.md`. Done when a usage
error, a handler error, a hook error and a render error each produce a diagnostic
document under `--output json` in the harness and in `run_process`, and #295's pinning
test is flipped.

**WS02: The `ndjson` mode and `ctx.stream()`.** `OutputMode::Ndjson`; the stream handle
in `CommandContext`; `Output::Render` as a `result` entry; diagnostics and warnings as
entries (D2, D3). Files: `crates/standout-render/src/output.rs`,
`crates/standout-dispatch/src/handler.rs`, `crates/standout/src/cli/builder/execution.rs`,
`docs/topics/output-modes.md`. Done when the four stream tests in
`tflike_diagnostic.rs` pass against a tflike built on the branch.

**WS03: Contract surfaces and versioned framework documents.** `ContractSurface`,
`Envelope`, the derive in `standout-macros`, `schema_version` on `ListViewResult` and
the diagnostic, `assert_schema_snapshot` (D4); the help document (D9). Files:
`crates/standout-macros/src`, `crates/standout/src/views/list_view.rs`,
`crates/standout/src/cli/help/data.rs`, `crates/standout-test/src`,
`docs/topics/stability.md`, `docs/topics/standout-help.md`, `docs/topics/list-view.md`.
Done when brewlike's two `schema-version` cases pass and a snapshot test in the
todo-example fails on a renamed field.

**WS04: Exit statuses and mode semantics.** `Output::with_exit_status`,
`ListView::empty_exit_status` (D5); XML deletion (D7); the CSV rule (D8). Files:
`crates/standout-dispatch/src/handler.rs`, `crates/standout-render/src/util.rs`,
`crates/standout-render/src/projection.rs`, `crates/standout-render/Cargo.toml`,
`crates/standout/Cargo.toml`, `docs/topics/output-modes.md`. Done when the three
`-detailed-exitcode` tests in `tflike_diagnostic.rs` pass, a nested value under
`--output csv` is a render error in the harness, and a ranged diagnostic under
`--output csv` is one row with the three range columns.

WS02, WS03 and WS04 start after WS01 merges and can run in parallel.

## Exit criteria

- The exit-code table test in `standout-test` covers success, app-declared success
  status, usage error and failure under `text`, `json` and `ndjson`.
- A release on the 10.0 line, since D4 and D7 are breaking.

## Issues

- Closed by this epic: #430, #295, #453, #458, #107, #108, #408, #409.
- Closed before it starts, outside the epic: #329 (already fixed by ADR-0030), #452
  (a one-file message fix in `crates/standout-render/src/error.rs`).

## Out of scope

Human-mode error wording, operational verbosity and the warning channel's levels (a
separate future feature), incremental result events (`docs/spec/implemented/typed-command-output.md`),
schema migration tooling, YAML or JSON streaming
beyond one value per line, a machine-readable form for `term` or `text`.
