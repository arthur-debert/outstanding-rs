# Proposal: Extract standout-dispatch

## Motivation

Standout does two fairly complicated things:

1. **Command Dispatch**: Binding command names to handlers, auto-dispatching parsed arguments, an execution pipeline with pre/post hooks.

2. **Rendering**: Template-based output with style tags, themes, adaptive styling, multiple output formats.

These domains are orthogonal. Keeping them in a single crate has led to:

- **Tight coupling** that makes interfaces less clean
- **Documentation sprawl** mixing dispatch and rendering concerns
- **All-or-nothing adoption** forcing users to take both even when they only want one

From user feedback, both value propositions resonate—but seldom with the same user. Some want dispatch without rendering complexity. Others want the rendering system in non-CLI contexts (servers, TUIs).

PR #44 extracted `standout-render` as a standalone crate. This proposal continues that work by extracting `standout-dispatch`.

## New Architecture

```text
standout-bbparser     (BBCode parser - standalone)
standout-macros       (embed macros - standalone)
standout-render       (rendering engine - standalone)
standout-dispatch     (command dispatch - standalone)  ← NEW
standout              (glue: dispatch + render)
```

Each library crate is independently useful. The `standout` crate becomes the integration layer for users who want the full framework.

## Per-Crate Responsibilities

### standout-dispatch

**Owns:**

- Command routing (name → handler mapping)
- Handler traits (`Handler`, `FnHandler`)
- Handler result types (`Output<T>`, `HandlerResult<T>`, `RunResult`)
- Execution context (`CommandContext`)
- Hook system (`Hooks`, `HookError`, `RenderedOutput`)
- App struct and builder (single-threaded, supports FnMut handlers)
- `OutputMode` enum (all variants: Auto, Term, TermDebug, Text, Json, Yaml, Csv, Xml)
- Auto mode detection (TTY capability check)
- Structured serialization (JSON, YAML, CSV, XML) — bypasses render entirely
- `--output` and `--output-file-path` flag injection
- Derive macro for dispatch (`#[derive(Dispatch)]`)
- `TextMode` enum for render functions (Styled, Plain, Debug)
- Default command support
- Command path extraction from ArgMatches

**Does NOT own:**

- Templates or template registries
- Themes or style processing
- BBCode/style tag parsing
- ANSI escape code generation
- Help topics system (stays in standout)

**Dependencies:**

- `clap` (argument parsing)
- `serde`, `serde_json`, `serde_yaml`, `csv`, `quick-xml` (serialization)
- `atty` or equivalent (TTY detection)
- `anyhow`, `thiserror` (error handling)

### standout-render

**Owns:**

- Template rendering (MiniJinja integration)
- Style tag processing (BBCode-like syntax via standout-bbparser)
- Theme system (CSS/YAML stylesheets)
- Adaptive styling (light/dark mode)
- ANSI escape code generation
- Template and stylesheet registries
- Context injection for templates
- The two-phase render pass (MiniJinja → BBParser)

**Does NOT own:**

- Command routing or dispatch
- Argument parsing
- Output mode selection
- TTY detection
- Structured serialization

### standout (glue crate)

**Owns:**

- Integration of standout-dispatch with standout-render
- `App` type alias with standout rendering
- Builder conveniences that wire templates/themes to dispatch
- Help topics system (requires both dispatch and render)
- Re-exports for ergonomic imports

## Output Mode Ownership

This is the key architectural decision: **dispatch owns output mode selection, render owns text formatting**.

### OutputMode (dispatch)

```rust
pub enum OutputMode {
    Auto,       // Dispatch decides Term vs Text based on TTY
    Term,       // Request styled output
    TermDebug,  // Request debug output (tags visible)
    Text,       // Request plain text (no styles)
    Json,       // Structured: dispatch serializes directly
    Yaml,       // Structured: dispatch serializes directly
    Csv,        // Structured: dispatch serializes directly
    Xml,        // Structured: dispatch serializes directly
}
```

### TextMode (passed to render function)

```rust
pub enum TextMode {
    Styled,  // Apply styles (ANSI escape codes)
    Plain,   // Strip style tags
    Debug,   // Keep tags as visible literals
}
```

### Dispatch Logic

```rust
fn produce_output(
    &self,
    data: serde_json::Value,
    render_fn: &RenderFn,
    output_mode: OutputMode,
) -> Result<String, Error> {
    match output_mode {
        // Structured: dispatch handles directly, no render function
        OutputMode::Json => Ok(serde_json::to_string_pretty(&data)?),
        OutputMode::Yaml => Ok(serde_yaml::to_string(&data)?),
        OutputMode::Csv  => Ok(serialize_csv(&data)?),
        OutputMode::Xml  => Ok(quick_xml::se::to_string(&data)?),

        // Text modes: dispatch detects/maps, delegates to render
        OutputMode::Auto => {
            let text_mode = if is_tty() { TextMode::Styled } else { TextMode::Plain };
            render_fn(&data, text_mode)
        }
        OutputMode::Term      => render_fn(&data, TextMode::Styled),
        OutputMode::TermDebug => render_fn(&data, TextMode::Debug),
        OutputMode::Text      => render_fn(&data, TextMode::Plain),
    }
}
```

### Render Function Contract

```rust
/// Per-command render function signature
pub type RenderFn = Arc<
    dyn Fn(&serde_json::Value, TextMode) -> Result<String, Error> + Send + Sync
>;
```

For dispatch-only users (no standout-render):

```rust
// TextMode is ignored since there are no styles to process
|data, _mode| Ok(format_my_output(data))
```

For standout users:

```rust
// standout constructs closures that call standout-render
// TextMode maps to how style tags are processed
```

## Derive Macro Design

The dispatch macro uses renderer-agnostic terminology:

```rust
#[derive(Dispatch)]
enum Commands {
    #[dispatch(handler = list_fn, view = "list")]
    List,

    #[dispatch(view = "status")]  // handler inferred as handlers::status
    Status,
}
```

- `handler`: The function that produces data
- `view`: Renderer-specific identifier (template name for standout-render, or arbitrary for custom renderers)

The standout crate can accept `template` as an alias for `view` for user convenience.

## Usage Examples

### Dispatch-only (no templates, no standout-render)

```rust
use standout_dispatch::{Dispatcher, Output, TextMode};

fn main() -> anyhow::Result<()> {
    Dispatcher::builder()
        .command("list", list_handler, |data, _mode| {
            // Imperative formatting - TextMode ignored
            let items: ListResult = serde_json::from_value(data.clone())?;
            Ok(format_list_output(&items))
        })
        .command("status", status_handler, |data, _mode| {
            Ok(format!("Status: {}", data["state"]))
        })
        .build()?
        .run(Cli::command(), std::env::args());
    Ok(())
}
```

**What dispatch-only users get:**

- Command routing
- Handler execution with `CommandContext`
- Pre/post dispatch hooks
- `--output json/yaml/csv/xml` for free
- `--output auto` with TTY detection
- No templates, no styles, no file-based assets

### Full standout (dispatch + render)

```rust
use standout::{App, embed_templates, embed_styles};

fn main() -> anyhow::Result<()> {
    App::builder()
        .templates(embed_templates!("src/templates"))
        .styles(embed_styles!("src/styles"))
        .theme("default")
        .command("list", list_handler, "list")
        .command("status", status_handler, "status")
        .build()?
        .run(Cli::command(), std::env::args());
    Ok(())
}
```

**What full standout users get:**

- Everything from dispatch
- Template-based rendering with MiniJinja
- Style tags with BBCode syntax
- Theme system with adaptive light/dark
- Hot reload in development

### Render-only (no dispatch)

```rust
use standout_render::{render, Theme, TextMode};

let theme = Theme::from_file("theme.yaml")?;
let output = render(
    "[title]Hello[/title], {{ name }}!",
    &json!({"name": "World"}),
    &theme,
    TextMode::Styled,
)?;
println!("{}", output);
```

## Migration Path

1. Extract `standout-dispatch` crate with all dispatch logic
2. Keep original code in `standout` temporarily for compatibility
3. Update `standout` to depend on `standout-dispatch`
4. Remove duplicated code from `standout`
5. Update documentation for the new architecture

## Help Topics System

The help topics system (`TopicRegistry`, help rendering) remains in `standout` because:

- It requires both dispatch (command handling) and render (styled output)
- It's a high-level feature, not a primitive
- Moving it to dispatch would re-introduce render coupling

## Summary

| Crate | Purpose | Dependencies |
| ------- | --------- | -------------- |
| `standout-bbparser` | BBCode parser | minimal |
| `standout-macros` | Embed macros | proc-macro |
| `standout-render` | Template + style rendering | minijinja, console |
| `standout-dispatch` | Command routing + hooks | clap, serde |
| `standout` | Full framework | dispatch + render |

This architecture enables:

- Dispatch-only adoption (simple, no templates)
- Render-only adoption (servers, TUIs)
- Full framework adoption (CLI apps with rich output)
- Clear documentation per concern
- Independent versioning and evolution
