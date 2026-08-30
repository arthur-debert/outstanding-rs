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
}
```

Three categories:

**Templated modes** (Auto, Term, Text): Render the template, vary ANSI handling.

**Debug mode** (TermDebug): Render the template, keep tags as literals for inspection.

**Structured modes** (Json, Yaml, Xml, Csv): Skip the template entirely, serialize handler data directly.

## Auto Mode

`Auto` is the default. It queries the terminal for color support:

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
```

The flag is global—it applies to all subcommands.

## Term vs Text

**Term**: turns every resolved style tag into ANSI escape codes, including
when the destination is a pipe rather than a terminal:

```bash
myapp list --output=term > colored.txt
```

Useful when you want to preserve colors for later display (e.g., `less -R`).

That said, a `term` request does not unconditionally reach ANSI: under a
never-color policy (for example `NO_COLOR` set in the environment), a `term`
request resolves to `text` instead. `auto` is the mode that inspects the
destination — it resolves to `term` when the destination reports color
capability and to `text` when it does not.

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

Unlike Term mode, unknown tags don't get the `?` marker in TermDebug.
TermDebug shows tag placement; it does not check whether a tag has a matching
style definition. Use `validate_template` when validation is required.

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

App::builder().command_with("summary", summary_handler, |config| {
    config.structured_output_projection(projection)
});
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
