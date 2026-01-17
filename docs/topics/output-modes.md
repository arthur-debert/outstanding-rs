# Output Modes

Outstanding supports multiple output formats through a single handler because modern CLI tools serve two masters: human operators and machine automation.

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

Outstanding adds a global `--output` flag accepting these values:

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

**Term**: Always applies ANSI escape codes, even when piping:

```bash
myapp list --output=term > colored.txt
```

Useful when you want to preserve colors for later display (e.g., `less -R`).

**Text**: Never applies ANSI codes:

```bash
myapp list --output=text
```

Useful for clean output regardless of terminal capabilities, or when processing output with other tools.

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

CSV mode flattens nested JSON automatically. For more control, use `FlatDataSpec`.

See [Tabular Layout](tabular.md) for detailed CSV configuration.

```rust
let spec = FlatDataSpec::builder()
    .column(Column::new(Width::Fixed(10)).key("name").header("Name"))
    .column(Column::new(Width::Fixed(10)).key("meta.role").header("Role"))
    .build();

render_auto_with_spec(template, &data, &theme, OutputMode::Csv, Some(&spec))?
```

The `key` field uses dot notation for nested paths (`"meta.role"` extracts `data["meta"]["role"]`).

## File Output

The `--output-file-path` flag redirects output to a file:

```bash
myapp list --output-file-path=results.txt
myapp list --output=json --output-file-path=data.json
```

Behavior:

- Text output: written to file, nothing printed to stdout
- Binary output: written to file (same as without flag)
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

## Accessing OutputMode in Handlers

`CommandContext` carries the resolved output mode:

```rust
fn handler(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Data> {
    if ctx.output_mode.is_structured() {
        // Skip interactive prompts in JSON mode
    }

    if ctx.output_mode == OutputMode::Csv {
        // Maybe adjust data structure for flat output
    }

    Ok(Output::Render(data))
}
```

Helper methods:

```rust
ctx.output_mode.should_use_color()  // True for Term, depends on terminal for Auto
ctx.output_mode.is_structured()     // True for Json, Yaml, Xml, Csv
ctx.output_mode.is_debug()          // True for TermDebug
```

## Rendering Without CLI

For standalone rendering with explicit mode:

```rust
use outstanding::{render_auto, OutputMode};

// Renders template for Term/Text, serializes for Json/Yaml
let output = render_auto(template, &data, &theme, OutputMode::Json)?;
```

The "auto" in `render_auto` refers to template-vs-serialize dispatch, not color detection.

For full control over both output mode and color mode:

```rust
use outstanding::{render_with_mode, ColorMode};

let output = render_with_mode(
    template,
    &data,
    &theme,
    OutputMode::Term,
    ColorMode::Dark,
)?;
```
