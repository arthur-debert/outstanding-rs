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

In debug builds (`debug_assertions` enabled), file-based resources are re-read from disk on each render. This means:

- Edit `templates/report.jinja` → re-run → see changes
- Edit `styles/theme.css` → re-run → see new styles
- No recompilation needed

```rust
use standout_render::Renderer;

let mut renderer = Renderer::new(theme)?;
renderer.add_template_dir("./templates")?;

// In debug: reads from disk each time
// In release: uses cached content
let output = renderer.render("report", &data)?;
```

### How It Works

The `Renderer` tracks the source of each template:
- **Inline**: Content provided as a string (always cached)
- **File-based**: Path recorded, content read on demand

In debug builds, file-based templates are re-read before each render. In release builds, content is cached after first load.

---

## File Registries

Both templates and stylesheets use a registry pattern: a map from names to content.

### Template Registry

```rust
use standout_render::TemplateRegistry;

let mut registry = TemplateRegistry::new();

// Add from directory
registry.add_dir("./templates")?;

// Add inline
registry.add("greeting", "Hello, {{ name }}!")?;

// Resolve by name
let content = registry.get("report")?;
```

**Name resolution from paths:**

```text
./templates/
├── greeting.jinja       → "greeting"
├── reports/
│   ├── summary.jinja    → "reports/summary"
│   └── detail.jinja     → "reports/detail"
└── partials/
    └── header.jinja     → "partials/header"
```

Names are relative paths without extensions.

### Stylesheet Registry

```rust
use standout_render::StylesheetRegistry;

let mut registry = StylesheetRegistry::new();
registry.add_dir("./styles")?;

let theme = registry.get("default")?;  // loads default.css or default.yaml
```

---

## Supported Extensions

### Templates

| Extension | Priority |
|-----------|----------|
| `.jinja` | 1 (highest) |
| `.jinja2` | 2 |
| `.j2` | 3 |
| `.txt` | 4 (lowest) |

If both `report.jinja` and `report.txt` exist, `report.jinja` is used.

### Stylesheets

| Extension | Format |
|-----------|--------|
| `.css` | CSS syntax |
| `.yaml` | YAML syntax |
| `.yml` | YAML syntax |

---

## Embedding Resources

For release builds, embed resources directly into the binary using the provided macros:

### Embedding Templates

```rust
use standout_render::{embed_templates, EmbeddedTemplates};

// Embed all .jinja files from a directory
let templates: EmbeddedTemplates = embed_templates!("src/templates");

// Use with Renderer
let mut renderer = Renderer::new(theme)?;
renderer.add_embedded_templates(templates)?;
```

### Embedding Stylesheets

```rust
use standout_render::{embed_styles, EmbeddedStyles};

// Embed all .css/.yaml files from a directory
let styles: EmbeddedStyles = embed_styles!("src/styles");

// Load a specific theme
let theme = styles.get("default")?;
```

### Hybrid Approach

Combine embedded defaults with optional file overrides:

```rust
use standout_render::{Renderer, embed_templates};

let embedded = embed_templates!("src/templates");

let mut renderer = Renderer::new(theme)?;

// Add embedded first (lower priority)
renderer.add_embedded_templates(embedded)?;

// Add file directory (higher priority, overrides embedded)
if Path::new("./templates").exists() {
    renderer.add_template_dir("./templates")?;
}
```

This pattern lets users customize templates without modifying the binary.

---

## Resolution Priority

When resolving a template or stylesheet name, sources are checked in priority order:

1. **Inline** (added via `add()` or `add_template()`)
2. **File-based directories** (in order added, later = higher priority)
3. **Embedded** (lowest priority)

Example:

```rust
renderer.add_embedded_templates(embedded)?;  // Priority 1 (lowest)
renderer.add_template_dir("./vendor")?;      // Priority 2
renderer.add_template_dir("./templates")?;   // Priority 3 (highest)
renderer.add_template("report", "inline")?;  // Priority 4 (always wins)
```

If "report" exists in all sources, the inline version is used.

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
let embedded_templates = embed_templates!("src/templates");
let embedded_styles = embed_styles!("src/styles");

let mut renderer = Renderer::new(theme)?;
renderer.add_embedded_templates(embedded_templates)?;

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

Same-directory collisions use extension priority (`.jinja` > `.txt`).

Cross-directory collisions are resolved by priority order (later directories win).

### Invalid Content

Template syntax errors are reported with line numbers:

```text
Template 'report' error at line 15:
  unexpected end of template, expected 'endif'
```

---

## API Reference

### TemplateRegistry

```rust
use standout_render::TemplateRegistry;

let mut registry = TemplateRegistry::new();

// Add sources
registry.add("name", "content")?;
registry.add_dir("./templates")?;
registry.add_embedded(embedded_templates)?;

// Query
let content: Option<&str> = registry.get("name");
let names: Vec<&str> = registry.names();
let exists: bool = registry.contains("name");
```

### StylesheetRegistry

```rust
use standout_render::StylesheetRegistry;

let mut registry = StylesheetRegistry::new();

// Add sources
registry.add_dir("./styles")?;
registry.add_embedded(embedded_styles)?;

// Get parsed theme
let theme: Theme = registry.get("default")?;
let names: Vec<&str> = registry.names();
```

### Embed Macros

```rust
use standout_render::{embed_templates, embed_styles};

// At compile time, reads all matching files and embeds content
let templates = embed_templates!("path/to/templates");
let styles = embed_styles!("path/to/styles");
```

### Renderer Integration

```rust
use standout_render::Renderer;

let mut renderer = Renderer::new(theme)?;

// Templates
renderer.add_template("name", "content")?;
renderer.add_template_dir("./templates")?;
renderer.add_embedded_templates(embedded)?;

// Render
let output = renderer.render("name", &data)?;
```
