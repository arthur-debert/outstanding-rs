# File System Resources

`standout-render` supports file-based templates and stylesheets that can be hot-reloaded during development and embedded into release binaries. This workflow combines the rapid iteration of interpreted languages with the distribution simplicity of compiled binaries.

---

## The Development Workflow

During development, you want to:

1. Edit a template or stylesheet
2. Re-run your program
3. See changes immediately

During release, you want:

1. A single binary with no external dependencies
2. No file paths to manage
3. No risk of missing assets

`standout-render` supports both modes with the same code.

---

## Hot Reload

In debug builds (`debug_assertions` enabled), file-based templates are re-read from disk on each render. This means:

- Edit `templates/report.jinja` → re-run → see changes
- No recompilation needed

```rust
use standout_render::{Renderer, Theme};

let mut renderer = Renderer::new(Theme::new())?;
renderer.add_template_dir("./templates")?;

// In debug: reads from disk each time
// In release: content was scanned once at registration
let output = renderer.render("report", &data)?;
```

### How It Works

`Renderer` tracks the source of each template name:

- **Inline** (`add_template`) and **embedded** (`with_embedded`, `with_embedded_source`) content: always cached, never re-read.
- **File-based** (`add_template_dir`): path recorded; in debug builds the file is re-read before each render, so edits are visible without recompiling.

---

## Supported Extensions

### Templates

| Extension | Priority |
| ----------- | ---------- |
| `.jinja` | 1 (highest) |
| `.jinja2` | 2 |
| `.j2` | 3 |
| `.stpl` | 4 |
| `.txt` | 5 (lowest) |

If both `report.jinja` and `report.txt` exist in the same directory, `report.jinja` is used. A lookup tries the given name exactly first; if that carries one of the extensions above, it also tries the name with that extension stripped.

### Stylesheets

| Extension | Format |
| ----------- | -------- |
| `.css` | CSS syntax |
| `.yaml` | YAML syntax |
| `.yml` | YAML syntax |

---

## Embedding Resources

For release builds, embed resources into the binary at compile time with `embed_templates!` / `embed_styles!`. Each macro reads matching files under the given directory and returns an `EmbeddedTemplates` / `EmbeddedStyles` value (both are `EmbeddedSource<R>`, differing only in the resource kind).

```rust
use standout_render::{embed_templates, embed_styles, Renderer, Theme};

let templates = embed_templates!("src/templates");
let styles = embed_styles!("src/styles");

let mut renderer = Renderer::new(Theme::new())?;
renderer.with_embedded_source(templates);
```

`EmbeddedSource::should_hot_reload()` is `true` in debug builds when the original source directory (recorded at compile time) still exists on disk — that's what lets a debug build behave like a file-based one even though the content is also embedded. `App::builder().templates(embedded)` / `.styles(embedded)` consume the same value; see the standout framework docs for that wiring.

### Hybrid Approach

Combine embedded defaults with an optional file directory that, for any name it defines, is only consulted when that name isn't already inline or embedded:

```rust
use standout_render::{embed_templates, Renderer, Theme};
use std::path::Path;

let embedded = embed_templates!("src/templates");

let mut renderer = Renderer::new(Theme::new())?;
renderer.with_embedded_source(embedded);

// Only reached for names not already resolved above
if Path::new("./templates").exists() {
    renderer.add_template_dir("./templates")?;
}
```

This pattern lets users override individual templates by placing a same-named file in `./templates`, without touching the binary — as long as that name isn't already inline or embedded, since those take priority (see below).

---

## Resolution Priority

`Renderer` resolves a name in two tiers:

1. **Inline and embedded** — `add_template` and `with_embedded`/`with_embedded_source` write into the same namespace. Whichever call registered a name last wins; there's no separate priority between "inline" and "embedded" content.
2. **File-based directories** (`add_template_dir`) — checked only for a name not already resolved in tier 1.

Registering the **same name from two different directories is a collision error**, not a silent override — file-based names must be unique across every directory you register.

```rust
renderer.with_embedded_source(embedded);   // Tier 1
renderer.add_template("report", "inline"); // Tier 1 — overwrites "report" if embedded also defined it
renderer.add_template_dir("./templates")?; // Tier 2 — only used for names tier 1 doesn't have
```

---

## Directory Structure

Recommended project layout:

```text
my-cli/
├── src/
│   ├── main.rs
│   ├── templates/           # Templates for embedding
│   │   ├── list.jinja
│   │   ├── detail.jinja
│   │   └── partials/
│   │       └── header.jinja
│   └── styles/              # Stylesheets for embedding
│       ├── default.css
│       └── colorblind.css
├── templates/               # Development overrides (gitignored)
└── styles/                  # Development overrides (gitignored)
```

In `main.rs`:

```rust
use std::path::Path;

let embedded_templates = embed_templates!("src/templates");

let mut renderer = Renderer::new(theme)?;
renderer.with_embedded_source(embedded_templates);

// In debug, also check local directories for overrides
#[cfg(debug_assertions)]
{
    if Path::new("./templates").exists() {
        renderer.add_template_dir("./templates")?;
    }
}
```

---

## Error Handling

### Missing Templates

```rust
match renderer.render("nonexistent", &data) {
    Ok(output) => println!("{}", output),
    Err(e) => {
        // Template not found in any source
        eprintln!("Template error: {}", e);
    }
}
```

### Name Collisions

Same-directory collisions use extension priority (`.jinja` beats `.txt`, etc. — see the table above).

Collisions across two different directories registered with `add_template_dir` are reported as `RegistryError::Collision`, not silently resolved by registration order.

### Invalid Content

Template syntax errors are reported with the template name and the underlying engine's message.

---

## API Reference

### Renderer

The primary entry point for most applications:

```rust
use standout_render::{Renderer, Theme};

let mut renderer = Renderer::new(Theme::new())?;

// Templates
renderer.add_template("name", "content")?;
renderer.add_template_dir("./templates")?;
renderer.with_embedded_source(embed_templates!("src/templates"));

// Render
let output = renderer.render("name", &data)?;
let count = renderer.template_count();
```

### TemplateRegistry

The lower-level registry `Renderer` builds on internally. Use it directly only when bypassing `Renderer`:

```rust
use standout_render::TemplateRegistry;

let mut registry = TemplateRegistry::new();

registry.add_inline("greeting", "Hello, {{ name }}!");
registry.add_embedded(embedded_map); // HashMap<String, String>

// Query — `get` returns the resolved source, not the raw content
let resolved = registry.get("greeting")?;   // Result<ResolvedTemplate, RegistryError>
let content = registry.get_content("greeting")?; // Result<String, RegistryError>
let names: Vec<&str> = registry.names().collect();
```

### StylesheetRegistry

```rust
use standout_render::StylesheetRegistry;

let mut registry = StylesheetRegistry::new();
registry.add_dir("./styles")?;
registry.add_embedded(embedded_themes); // HashMap<String, Theme>

let theme = registry.get("default")?;      // Result<Theme, StylesheetError>
let exists: bool = registry.contains("default");
let names: Vec<&str> = registry.names().collect();
```

### Embed Macros

```rust
use standout_render::{embed_templates, embed_styles};

// At compile time, reads all matching files and embeds their content
let templates = embed_templates!("path/to/templates");
let styles = embed_styles!("path/to/styles");
```
