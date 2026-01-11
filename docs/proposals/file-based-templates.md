# File-Based Templates

This document describes the design for file-based template loading in Outstanding.

## Overview

File-based templates allow developers to organize templates as separate files in the filesystem, rather than embedding them as inline strings in Rust code. This enables:

- **Separation of concerns**: Templates live alongside other assets
- **Hot reloading in development**: Edit templates without recompiling
- **Familiar workflow**: Similar to web frameworks with template directories

## How It Works

### Template Directory Registration

Applications register one or more template directories:

```rust
let mut renderer = Renderer::new(theme)?;
renderer.add_template_dir("./templates")?;
renderer.add_template_dir("./plugin-templates")?;
```

### Template Resolution

Templates are resolved by their relative path from the template root, without extension:

```
templates/
  config.tmpl
  todos/
    list.tmpl
    detail.tmpl
```

With `templates/` registered:
- `"config"` → resolves to `templates/config.tmpl`
- `"todos/list"` → resolves to `templates/todos/list.tmpl`
- `"todos/detail"` → resolves to `templates/todos/detail.tmpl`

The extension is optional in lookups:
- `"config"` and `"config.tmpl"` both resolve correctly

### Supported Extensions

Templates are recognized by extension, in priority order:

1. `.tmpl` (highest priority)
2. `.jinja2`
3. `.j2` (lowest priority)

If multiple files exist with the same base name but different extensions (e.g., `config.tmpl` and `config.j2`), the higher-priority extension wins.

### Resolution Priority

When resolving a template name:

1. **Inline templates** (added via `add_template()`) have highest priority
2. **File templates** are searched in directory registration order (first dir wins)

This allows overriding file-based templates with inline versions when needed.

### Collision Handling

**Cross-directory collisions are errors.** If the same relative path exists in multiple template directories, Outstanding panics with a detailed error message listing the conflicting files.

Example error:
```
Template collision detected for "config":
  - /app/templates/config.tmpl
  - /app/plugins/templates/config.tmpl
```

This strict behavior catches configuration mistakes early.

## Dev vs Release Mode

### Development Mode

In development (`debug_assertions` enabled):
- Template **map** (name → path) is built on first render
- Template **content** is re-read from disk on each render
- This enables hot reloading without recompilation

### Release Mode

In release builds:
- Templates can be embedded at compile time using `embed_templates!()`
- The embedded content is used directly, no filesystem access needed
- This solves deployment path resolution issues

```rust
// Embed at compile time
let embedded = outstanding::embed_templates!("./templates");

// Use embedded templates
let mut renderer = Renderer::new(theme)?;
renderer.with_embedded(embedded);
```

## API

### Renderer Extensions

```rust
impl Renderer {
    /// Adds a directory to search for template files.
    ///
    /// Templates are resolved by relative path without extension.
    /// Multiple directories can be added; earlier directories take priority.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory doesn't exist or isn't readable.
    pub fn add_template_dir<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Error>;

    /// Forces a rebuild of the template resolution map.
    ///
    /// In development mode, this is called automatically on first render.
    /// Call manually if you've added templates after the first render.
    pub fn refresh(&mut self) -> Result<(), Error>;

    /// Loads pre-embedded templates (for release builds).
    pub fn with_embedded(&mut self, templates: HashMap<String, String>) -> &mut Self;
}
```

### TemplateRegistry (Internal)

The resolution logic is encapsulated in a `TemplateRegistry` that:
- Takes a list of `(name, content)` pairs for testability
- Builds a resolution map with collision detection
- Provides lookup by name

```rust
/// Internal template storage with resolution logic.
pub struct TemplateRegistry {
    templates: HashMap<String, ResolvedTemplate>,
}

enum ResolvedTemplate {
    /// Content stored directly (inline or embedded)
    Inline(String),
    /// Path to read from disk (file-based, dev mode)
    File(PathBuf),
}
```

## Implementation Notes

### Decoupled Directory Walking

The directory walking logic is separated from map generation:

```rust
// Directory walking returns simple data
fn walk_template_dir(path: &Path) -> Result<Vec<TemplateFile>, Error>;

struct TemplateFile {
    name: String,           // "config" or "todos/list"
    name_with_ext: String,  // "config.tmpl" or "todos/list.tmpl"
    absolute_path: PathBuf,
}

// Map generation takes the walked data
fn build_registry(files: Vec<TemplateFile>) -> Result<TemplateRegistry, Error>;
```

This separation enables:
- Easy unit testing without filesystem
- Injecting test data
- Reuse of resolution logic for embedded templates

### Extension Priority Implementation

When walking a directory, files are sorted by extension priority before insertion into the map. The first file for each base name wins.

### Collision Detection

During map building, if a name already exists from a different source directory, an error is raised with full paths for debugging.

## Examples

### Basic Usage

```rust
use outstanding::{Renderer, Theme};
use console::Style;

let theme = Theme::new()
    .add("title", Style::new().bold());

let mut renderer = Renderer::new(theme)?;
renderer.add_template_dir("./templates")?;

// Render a file-based template
let output = renderer.render("todos/list", &data)?;
```

### Mixed Inline and File-Based

```rust
let mut renderer = Renderer::new(theme)?;
renderer.add_template_dir("./templates")?;

// Override a specific template inline
renderer.add_template("config", "Custom: {{ value }}")?;

// "config" uses inline version
// Other templates use file-based versions
```

### Development Workflow

```bash
# Edit template file
vim templates/todos/list.tmpl

# Re-run command - sees updated template immediately
cargo run -- todos list
```

No recompilation needed during template iteration.

## Future Considerations

- **File watching**: Could add `notify`-based watching for automatic refresh
- **Template inheritance**: MiniJinja supports `{% extends %}`, file-based templates make this more practical
- **Theme files**: Similar pattern could apply to YAML-based theme definitions
