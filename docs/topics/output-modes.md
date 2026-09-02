# Output Modes

Standout supports multiple output formats through a single handler because modern CLI tools serve two masters: human operators and machine automation.

The same handler logic produces styled terminal output for eyes, plain text for logs, or structured JSON for `jq` pipelines—controlled entirely by the user's `--output` flag. This frees you from writing separate "API" and "CLI" logic.

## The OutputMode Enum

```rust
pub enum OutputMode {
    Auto,       // Auto-detect terminal capabilities
    Term,       // Always use ANSI escape codes
    Text,       // Never use ANSI codes (plain text)
    TermDebug,  // Keep style tags as [name]...[/name]
    Json,       // Serialize as JSON (skip template)
    Yaml,       // Serialize as YAML (skip template)
    Xml,        // Serialize as XML (skip template)
    Csv,        // Serialize as CSV (skip template)
    Ndjson,     // One JSON object per line: a stream, not a document
}
```

Three categories:

**Templated modes** (Auto, Term, Text): Render the template, vary ANSI handling.

**Debug mode** (TermDebug): Render the template, keep tags as literals for inspection.

**Structured modes** (Json, Yaml, Xml, Csv, Ndjson): Skip the template entirely,
serialize handler data directly. `Ndjson` is the one stream mode among them,
[below](#ndjson-mode).

## Auto Mode

`Auto` is the default when `--output` is absent, and an application can change
that default with
[`output_mode_fallback(mode)`](./app-configuration.md#output-mode-fallback) — an
explicit `--output` still outranks it. `Auto` queries the terminal for color
support:

```rust
Term::stdout().features().colors_supported()
```

If colors are supported, Auto behaves like Term (ANSI codes applied). If not, Auto behaves like Text (tags stripped).

This detection happens at render time, not startup. Piping output to a file or another process typically disables color support, so:

```bash
myapp list              # Colors (if terminal supports)
myapp list > file.txt   # No colors (not a TTY)
myapp list | less       # No colors (pipe)
```

## The --output Flag

Standout adds a global `--output` flag accepting these values:

```bash
myapp list --output=auto        # Default
myapp list --output=term        # Force ANSI codes
myapp list --output=text        # Force plain text
myapp list --output=term-debug  # Show style tags
myapp list --output=json        # JSON serialization
myapp list --output=yaml        # YAML serialization
myapp list --output=xml         # XML serialization
myapp list --output=csv         # CSV serialization
myapp list --output=ndjson      # Newline-delimited JSON stream
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

## Term vs Text

**Term**: turns every resolved style tag into ANSI escape codes, including
when the destination is a pipe rather than a terminal:

```bash
myapp list --output=term > colored.txt
```

Useful when you want to preserve colors for later display (e.g., `less -R`).

A `term` request is unconditional, and the environment's color conventions do
not override it: `NO_COLOR=1 myapp list --output=term` still emits ANSI, the
same way `CLICOLOR_FORCE=1 myapp list --output=text` still emits none. `auto`
is the only mode the environment reaches, and it reaches it through one value:
the destination's reported color capability. `auto` resolves to `term` when
that capability is reported and to `text` when it is not. `NO_COLOR` and
`TERM=dumb` suppress the capability, so they turn `auto` plain;
`CLICOLOR_FORCE` is not part of that capability probe, so it never turns `auto`
into `term`.

**Text**: removes Standout's own style tags and adds no ANSI of its own:

```bash
myapp list --output=text
```

Useful for clean output regardless of terminal capabilities, or when processing output with other tools.

Neither `term` nor `text` touches ANSI bytes that a handler or template
writes literally into the rendered text — the framework does not sanitize
those bytes and does not promise to. A caller that needs them gone strips
them itself.

`term-debug` (which shows tags as `[name]...[/name]` rather than resolving
them) is internal: its tag vocabulary and exact spelling may change in any
release, so don't build automation against its output the way you might
against `term` or `text`.

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
check whether a tag has a matching style definition. No mode rewrites an
unknown tag to a `[unknown?]` marker — in `Term` and `Text` an unresolved tag
degrades to unstyled text and is recorded as a warning; run through `App::run`
that warning is written to stderr (see [Unknown Style
Tags](../crates/render/topics/styling-system.md#unknown-style-tags)).
Use `validate_template` when validation is required.

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

JSON, YAML, XML and NDJSON emit object keys in the order the handler declared
them, not alphabetically. In the example above, a reader sees `items` before `total`
because the struct lists `items` first. Field order in your `#[derive(Serialize)]`
struct — or key order in a `json!({ ... })` literal — is the output order.

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

Normal `App` dispatch flattens the serializable handler data automatically for
CSV. That is the same handler data used by the other structured modes; handlers
should not inspect the requested mode or return a CSV-specific shape.

The standalone rendering API also supports direct `FlatDataSpec` rendering when
a caller needs explicit columns and headers:

```rust
use standout::tabular::{Column, FlatDataSpec, Width};
use standout::OutputMode;
use standout_render::render_auto_with_spec;

let spec = FlatDataSpec::builder()
    .column(Column::new(Width::Fixed(10)).key("name").header("Name"))
    .column(Column::new(Width::Fixed(10)).key("meta.role").header("Role"))
    .build();

render_auto_with_spec(template, &data, &theme, OutputMode::Csv, Some(&spec))?
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

The projection applies only to CSV. Text and terminal modes still use the
template, while JSON, YAML, and XML serialize the canonical response. In the
pipeline, post-dispatch hooks run before projection and post-output hooks run
after it; `run`, `run_with`, output-file handling, and final emission
therefore all observe the same projected CSV.

See [Introduction to Tabular](../crates/render/guides/intro-to-tabular.md) for
tabular specifications and layout.

## NDJSON Mode

`--output ndjson` makes stdout a stream: every line is one JSON object, and the
run may write several. Nothing else changes — the same handler, the same
serializable data, the same exit statuses. This is the mode for a command whose
result accrues while it runs (a plan, an apply, a long listing) and for a
consumer that wants to react to entries before the process ends.

A stream is exactly line-per-value: each entry is written compact, one line,
and flushed when it is emitted. There is no buffering, no backpressure and no
async behind it.

### What a run writes

`Output::Render(data)` becomes one `result` entry where the other structured
modes write their document:

```text
{"type":"result","data":{"items":[...],"total":42}}
```

A failure is one `diagnostic` entry — the same flat document `json`, `yaml`
and `csv` write, [Execution Outcomes](./execution-outcomes.md#failures-under-a-structured-mode)
— at the point in the stream where the run failed, after whatever the handler
already emitted, and stderr carries nothing for it. A warning is a
`severity: warning` diagnostic entry of kind `framework` on stdout, after the
result or the failure, instead of the stderr prose the single-document modes
keep. `Output::Silent` writes nothing, so a handler whose entries are its whole
result leaves only those.

### Handler-emitted entries

`ctx.stream()` returns the run's `EntryStream`. `emit(&value)` writes the
value as one line under `ndjson` and does nothing in every other mode:

```rust
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Entry<'a> {
    Version { format_version: u32 },
    ApplyStart { resource: &'a str },
    ApplyComplete { resource: &'a str },
}

fn apply(_matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Summary> {
    let stream = ctx.stream();
    stream.emit(&Entry::Version { format_version: 1 })?;
    for change in plan()? {
        stream.emit(&Entry::ApplyStart { resource: &change.name })?;
        change.apply()?;                       // a failure here is a diagnostic entry
        stream.emit(&Entry::ApplyComplete { resource: &change.name })?;
    }
    Ok(Output::Render(summary))                // one more line: the result entry
}
```

```text
$ myapp apply --output ndjson
{"type":"version","format_version":1}
{"type":"apply_start","resource":"web"}
{"type":"apply_complete","resource":"web"}
{"type":"result","data":{"applied":1}}
```

The framework does not inspect an entry: its shape, including whether it
carries a `type` key, is the application's contract with its consumers.
`emit` fails with a `StreamError` when the value does not serialize or the
line cannot be written; propagate it with `?` and the run fails with it.

A handler whose entries *are* its result can skip the `result` line by
returning `Output::Silent` when the stream is live — `ctx.stream().is_live()`
is true only under `ndjson` — and `Output::Render` otherwise, so the human
modes still render their page. That is the one presentation branch a handler
is meant to take; see [Keep Output Mode Out of Handlers](#keep-output-mode-out-of-handlers).

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
- `ndjson`: the file is the stream. It is opened before the handler runs and
  receives the handler's entries as they are emitted, then the `result` or
  `diagnostic` entry, then the warning entries; stdout carries nothing.

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

## Keep Output Mode Out of Handlers

Output mode is a rendering concern and is deliberately absent from
`CommandContext`. A handler should return the same serializable data regardless
of whether the caller selected terminal, text, or structured output. If a
command's behavior genuinely differs, model that as an explicit command or
argument rather than an implicit presentation-mode branch.

The one mode-aware member of the context is `ctx.stream()`, which is live only
under `ndjson` ([NDJSON Mode](#ndjson-mode)). A handler emits its entries
unconditionally and lets the stream discard them elsewhere; the only branch it
takes on `is_live()` is whether to return `Output::Silent` in place of the
`result` entry.

## Rendering Without CLI

For standalone rendering with explicit mode:

```rust
use standout::{render_auto, OutputMode};

// Renders template for Term/Text, serializes for Json/Yaml
let output = render_auto(template, &data, &theme, OutputMode::Json)?;
```

The "auto" in `render_auto` refers to template-vs-serialize dispatch, not color detection.

For full control over both output mode and color mode:

```rust
use standout::{render_with_mode, ColorMode};

let output = render_with_mode(
    template,
    &data,
    &theme,
    OutputMode::Term,
    ColorMode::Dark,
)?;
```
