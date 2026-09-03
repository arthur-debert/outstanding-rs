# Output Modes

Standout supports multiple output formats through a single handler because modern CLI tools serve two masters: human operators and machine automation.

The same handler logic produces a rendered page for eyes or structured JSON for
`jq` pipelines. This frees you from writing separate "API" and "CLI" logic.

## Representation and style are two decisions

**What the run produces** is its representation. **Whether the rendered human
text carries escape sequences** is its style mode. They are separate types, and
no type combines them:

```rust
pub enum Representation {
    Human,      // Render the command's template: what a bare invocation produces
    TermDebug,  // Render it, keeping style tags as [name]...[/name]
    Json,       // Serialize as JSON (skip template)
    Yaml,       // Serialize as YAML (skip template)
    Csv,        // Serialize as CSV (skip template)
    Ndjson,     // One JSON object per line: a stream, not a document
}

pub enum StyleMode {
    Ansi,   // escape sequences applied
    Plain,  // style tags removed
    Debug,  // style tags kept as literals
}
```

The human representation has no `--output` name: it is what a bare invocation
renders. `term-debug` stays on the flag as the diagnostic view of the template's
style tags, outside the stability contract. `Ndjson` is the one stream
representation, [below](#ndjson-mode).

There is no XML representation. `--output xml` is a clap usage error like any
other value the flag does not accept, exit `2`.

## The style decision

The style mode is resolved per run from the representation, the run's
[`ColorPolicy`](#the-color-policy) and the destination's reported color
capability:

```rust
Term::stdout().features().colors_supported()
```

Under `ColorPolicy::Auto`, a capable destination renders with escape sequences
and an incapable one renders plain. Detection happens at render time, not
startup, so piping to a file or another process turns the page plain:

```bash
myapp list              # Colors (if the terminal supports them)
myapp list > file.txt   # No colors (not a TTY)
myapp list | less       # No colors (pipe)
```

`NO_COLOR` and `TERM=dumb` suppress the reported capability, so they turn a
bare run plain. `CLICOLOR_FORCE` is not part of that probe, so it does not turn
a plain destination colored. A structured encoding never carries escape
sequences, whatever the policy says.

## The color policy

`ColorPolicy` is `Auto`, `Always` or `Never`. In-process entry points name it —
`App::run_with_color`, `App::run_command`, `HelpConfig::color`,
`TopicRenderConfig::color`, and `TestHarness::color` — and everything else
resolves `Auto` against the destination.

## The --output Flag

Standout adds a global `--output` flag naming a structured encoding:

```bash
myapp list                      # the human template
myapp list --output=json        # JSON serialization
myapp list --output=yaml        # YAML serialization
myapp list --output=csv         # CSV serialization
myapp list --output=ndjson      # Newline-delimited JSON stream
myapp list --output=term-debug  # Show style tags
```

The flag is global—it applies to all subcommands. `--output` accepts a single
occurrence: passing it twice on one command line is a clap usage error
(`ArgumentConflict`), so an application cannot inject its own default by
appending a second `--output` to the arguments. To change the default output
mode, set it on the builder with
[`output_mode_fallback(mode)`](./app-configuration.md#output-mode-fallback)
rather than rewriting the command line.

The global property is not special to `--output`. Any flag an application
declares with clap's `.global(true)` is readable from the deepest `ArgMatches`,
which is the one a `#[flag]` handler parameter reads. A root-declared global
`--quiet`, for example, is visible as `#[flag] quiet: bool` in a handler for a
nested command such as `config set`.

## Literal escape bytes

No style mode touches ANSI bytes that a handler or template writes literally
into the rendered text — the framework does not sanitize those bytes and does
not promise to. A caller that needs them gone strips them itself.

`term-debug` is internal ([What Is Contract](./stability.md)): do not build
automation against its output.

## TermDebug Mode

TermDebug preserves style tags instead of converting them:

```text
Template: [title]Hello[/title]
Output:   [title]Hello[/title]
```

Use cases:

- Debugging template issues
- Verifying style tag placement
- Automated testing of template output

TermDebug keeps every tag as literal text and shows tag placement; it does not
check whether a tag has a matching style definition. What the other modes do
with an unknown tag is in [Unknown Style
Tags](../crates/render/topics/styling-system.md#unknown-style-tags). Use
`validate_template` when validation is required.

## Structured Modes

Structured modes bypass the template entirely. Handler data is serialized directly:

```rust
#[derive(Serialize)]
struct ListOutput {
    items: Vec<Item>,
    total: usize,
}

fn list_handler(...) -> HandlerResult<ListOutput> {
    Ok(Output::Render(ListOutput { items, total: items.len() }))
}
```

```bash
myapp list --output=json
```

```json
{
  "items": [...],
  "total": 42
}
```

Same handler, same types—different output format. This enables:

- Machine-readable output for scripts
- Integration with other tools (`jq`, etc.)
- API-like behavior from CLI apps

### Key ordering

JSON, YAML, CSV and NDJSON emit object keys in the order the handler declared
them, not alphabetically. In the example above, a reader sees `items` before `total`
because the struct lists `items` first. Field order in your `#[derive(Serialize)]`
struct — or key order in a `json!({ ... })` literal — is the output order, and
in CSV it is the column order.

This holds because Standout builds serde_json's `Value` with the `preserve_order`
feature on, so the intermediate map keeps insertion order instead of sorting. The
feature is process-wide: it governs every map serde_json builds in the process,
including any `HashMap` a handler serializes. A `HashMap` has no stable iteration
order, so a field of that type emits its keys in an order that varies from run to
run — `preserve_order` preserves that non-determinism rather than hiding it behind
a sort. For output whose key order must be stable, give the field a struct or an
order-preserving map type (for example `indexmap::IndexMap`); reach for a raw
`HashMap` only when the order genuinely does not matter.

### CSV Output

CSV takes flat records only. A flat record is a map whose values are scalars
(strings, numbers, booleans, null); the handler data must be one flat record or
an array of flat records. Each record is a row, the columns are the records'
keys in first-seen order, and a key a record lacks — or maps to null — is an
empty cell.

```rust
#[derive(Serialize)]
struct Row { name: String, count: usize }

// One row per element:   name,count
Ok(Output::Render(vec![Row { .. }, Row { .. }]))
```

Any nested value — an array or object inside a record, or a document that is
not a record at all — is a render error, exit `1`, whose message names the
value and points at `CsvProjection`:

```text
CSV output takes a flat record or an array of flat records, and `items` is an
array; declare the columns with a CsvProjection
```

Under `--output csv` that error is the stdout diagnostic document, kind
`render` ([Execution Outcomes](./execution-outcomes.md#failures-under-a-structured-mode)).
Nothing is flattened: there are no `items.0.name` columns and no JSON blobs in
cells. A command whose canonical response nests its rows declares the columns
with a `CsvProjection`, below. That is the same handler data used by the other
structured modes; handlers should not inspect the requested mode or return a
CSV-specific shape.

The standalone rendering API also supports direct `FlatDataSpec` rendering when
a caller needs explicit columns and headers:

```rust
use standout::tabular::{Column, FlatDataSpec, Width};
use standout::{ColorPolicy, Representation};
use standout_render::render_auto_with_spec;

let spec = FlatDataSpec::builder()
    .column(Column::new(Width::Fixed(10)).key("name").header("Name"))
    .column(Column::new(Width::Fixed(10)).key("meta.role").header("Role"))
    .build();

render_auto_with_spec(
    template,
    &data,
    &theme,
    Representation::Csv,
    ColorPolicy::Auto,
    Some(&spec),
)?
```

The `key` field uses dot notation for nested paths (`"meta.role"` extracts `data["meta"]["role"]`).

When a command's canonical response is an object containing the CSV rows,
attach a presentation-layer projection through `CommandConfig`:

```rust
use serde_json::json;
use standout::cli::FnHandler;
use standout::tabular::{Column, Width};
use standout::{CsvProjection, StructuredOutputProjection};

let projection = StructuredOutputProjection::csv(
    CsvProjection::builder("items")
        .column(Column::new(Width::default()).key("language").header("LANGUAGE"))
        .column(Column::new(Width::default()).key("code").header("CODE"))
        .derived_column(
            Column::new(Width::default()).header("NET"),
            |row, _root| json!(
                row["code"].as_i64().unwrap_or(0)
                    - row["comments"].as_i64().unwrap_or(0)
            ),
        )
        .synthetic_row(|root| json!({
            "language": "TOTAL",
            "code": root["totals"]["code"],
            "comments": root["totals"]["comments"]
        }))
        .conditional_row(|root| {
            (root["skipped"].as_u64().unwrap_or(0) > 0)
                .then(|| json!({ "language": "SKIPPED" }))
        })
        .build(),
);

App::builder().command_with("summary", FnHandler::new(summary_handler), |config| {
    config.structured_output_projection(projection)
})?;
```

Direct-column dot paths are resolved against each selected row. Derived
columns receive both the current row and the root response. Synthetic-row
callbacks receive the root response and run in registration order. Column
ordering, headers, and `null_repr` use the existing `FlatDataSpec` behavior.

The row source is a dot path into the response, or `.` for the response
itself. An array there is one row per element; a single record is one row,
which is how the framework's own diagnostic document becomes a CSV row: a
`CsvProjection` over `.` whose optional `range` is three columns,
`range_filename`, `range_line` and `range_column`, with `null_repr("")` so
they are empty when no range is set.

The projection applies only to CSV. The human representation still uses the
template, while JSON and YAML serialize the canonical response. In the
pipeline, post-dispatch hooks run before projection and post-output hooks run
after it; `run`, `run_with`, output-file handling, and final emission
therefore all observe the same projected CSV.

See [Introduction to Tabular](../crates/render/guides/intro-to-tabular.md) for
tabular specifications and layout.

## Incremental Commands

A command whose result accrues while it runs — a plan, an apply, a long listing
— produces a sequence of typed events and then a summary. It declares an event
type and takes the run's results channel as a third handler parameter:

```rust
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Event<'a> {
    ApplyStart { resource: &'a str },
    ApplyComplete { resource: &'a str },
}

fn apply(
    _matches: &ArgMatches,
    _ctx: &CommandContext,
    results: &mut Results<Event>,
) -> HandlerResult<Summary> {
    for change in plan()? {
        results.emit(Event::ApplyStart { resource: &change.name })?;
        change.apply()?;                  // a failure here follows the events
        results.emit(Event::ApplyComplete { resource: &change.name })?;
    }
    Ok(Output::Render(summary))           // the summary, after the last event
}
```

Register it with `EventsFnHandler::new(apply)`, or write the same three
parameters under `#[handler]`, which reads the `Results` parameter and derives
the command's event type from it. `emit` takes the value by value, returns once
it has been rendered or written, and fails when the value does not serialize,
does not render, or cannot be written; propagate with `?` and the run fails
with it. Standout reports that failure as a render error whether or not the
handler propagates it. The framework never inspects an event: its shape, including
whether it carries a `type` key, is the application's contract with its
consumers.

`Results` exposes `emit` and nothing else. A handler cannot ask which
representation is running or where the bytes go, and emits the same events
under every representation.

### The human representation

Standout renders each event from the command's template name with an `.event`
suffix — `apply.event` beside `apply` — resolved through the same directories
and theme. The template receives the event as `event` and branches on the
application's own discriminator, so one template covers every kind:

```jinja
{% if event.type == "apply_start" %}starting {{ event.resource }}…
{% elif event.type == "apply_complete" %}{{ "✔" | style("ok") }} {{ event.resource }}
{% endif %}
```

Each rendered event is one flushed write, on a terminal and in a pipe alike;
the summary follows from the command's own template. `.build()` requires the
`.event` template of every command that declares an event type, so a missing one
is a setup error rather than a failure on the first event. A command whose
events are its whole result returns `Output::Silent` as its summary and needs
only the `.event` template: the summary template is the one `.build()` lets it
skip, because which `Output` variant a handler returns is not something the
build can read.

```text
$ myapp apply
starting web…
✔ web
starting db…
✔ db
2 added, 0 removed
```

### Under a structured encoding

`--output ndjson` is the JSON record encoding plus line framing: each event is
written as the handler produced it, compact, on its own line and flushed, and
the summary is the `result` record the machine contract gives a batch value.
Standout adds no header, so a `version` line an application writes first is an
event like any other.

```text
$ myapp apply --output ndjson
{"type":"version","format_version":1}
{"type":"apply_start","resource":"web"}
{"type":"apply_complete","resource":"web"}
{"type":"result","data":{"applied":1}}
```

`json`, `yaml` and `csv` have no line framing, so they carry a command's events
as one document written when the command ends. Standout does not build that
document yet: an emitting command under those encodings is a render error,
decided from the handler's event type before the handler runs.

A failure after emitted events keeps them — the human representation has
rendered them, line framing has written them — and the diagnostic follows in
the shape [Execution Outcomes](./execution-outcomes.md#failures-under-a-structured-mode)
gives it. A reader that goes away is not one of those failures: when stdout
stops reading (`myapp apply --output ndjson | head -1`), Standout discards what
follows, lets the handler run to completion, and reports the command's own
status.

Binary and artifact output cannot follow events. A command with an event type
carries `Output::Render` and `Output::Silent` only, so either payload is a
render error under every representation — on the run that emitted nothing too,
since the refusal follows the type rather than the count. Under
`ndjson` a payload is a render error whether or not the command declares
events: a stream of JSON lines has no room for one.

## NDJSON Mode

`--output ndjson` makes stdout a stream: every line is one JSON object, and the
run may write several. Nothing else changes — the same handler, the same
serializable data, the same exit statuses.

`Output::Render(data)` becomes one `result` entry where the other structured
encodings write their document:

```text
{"type":"result","data":{"items":[...],"total":42}}
```

A failure is one `diagnostic` entry at the point in the stream where the run
failed, after whatever the handler already emitted, and a warning is a
`severity: warning` diagnostic entry of kind `framework` after the result or
the failure; the document itself and what each stream carries are in
[Execution Outcomes](./execution-outcomes.md#failures-under-a-structured-mode).
`Output::Silent` writes nothing, so a handler whose events are its whole result
leaves only those.

## File Output

The `--output-file-path` flag redirects output to a file:

```bash
myapp list --output-file-path=results.txt
myapp list --output=json --output-file-path=data.json
```

Behavior:

- Text output: written to file, nothing printed to stdout
- Binary output: written to the requested file instead of stdout
- Silent output: no-op
- An incremental command: the file is the whole run. It receives each event as
  it is emitted, then the summary or the diagnostic, then the warning entries
  under `ndjson`; stdout carries nothing.

After writing to file, stdout output is suppressed to prevent double-printing.

## Customizing Flags

Rename or disable the flags via `AppBuilder`:

```rust
App::builder()
    .output_flag(Some("format"))       // --format instead of --output
    .output_file_flag(Some("out"))     // --out instead of --output-file-path
    .build()?
```

```rust
App::builder()
    .no_output_flag()                  // Disable --output entirely
    .no_output_file_flag()             // Disable file output
    .build()?
```

## Keep the Representation Out of Handlers

The representation is a rendering concern and is deliberately absent from
`CommandContext`. A handler should return the same serializable data whether the
caller took the human page or a structured encoding. If a command's behavior
genuinely differs, model that as an explicit command or argument rather than an
implicit presentation branch.

A handler has nothing to branch on: the results channel exposes `emit` alone,
and an incremental command emits the same events under every representation.

## Rendering Without CLI

For standalone rendering with an explicit representation:

```rust
use standout::{render_auto, ColorPolicy, Representation};

// Renders the template for Human, serializes for Json/Yaml
let output = render_auto(template, &data, &theme, Representation::Json, ColorPolicy::Auto)?;
```

The "auto" in `render_auto` refers to template-vs-serialize dispatch, not color detection.

For control over the representation, the color policy and the color mode:

```rust
use standout::{render_with_mode, ColorMode, ColorPolicy, Representation};

let output = render_with_mode(
    template,
    &data,
    &theme,
    Representation::Human,
    ColorPolicy::Always,
    ColorMode::Dark,
)?;
```
