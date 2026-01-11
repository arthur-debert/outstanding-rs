# File-Based Resources

Outstanding supports file-based configuration for templates and stylesheets, enabling a web-app-like development workflow for CLI applications.

## Problem

CLI applications built with Outstanding need to manage templates and stylesheets. Developers want:

1. **Separation of concerns** - Keep templates and styles in dedicated files, not embedded in Rust code
2. **Accessible to non-developers** - Designers or content authors can edit YAML/Jinja files without touching Rust
3. **Rapid iteration** - Changes to templates/styles should be visible immediately without recompilation
4. **Single-binary distribution** - Released applications should be self-contained with no external file dependencies

These requirements create a tension: development wants external files for flexibility, while release wants everything embedded for distribution.

## Solution

Outstanding's file loader provides a unified system that:

- **Development mode**: Reads files from disk, with hot reload on each access
- **Release mode**: Embeds all files into the binary at compile time via proc macros

### How It Works

#### Directory Structure

Organize resources in dedicated directories:

```
my-app/
├── src/
│   └── main.rs
├── templates/
│   ├── list.jinja
│   └── report/
│       └── summary.jinja
└── styles/
    ├── default.yaml
    └── themes/
        └── dark.yaml
```

#### Name Resolution

Files are referenced by their relative path from the root directory, without extension:

| File Path | Resolution Name |
|-----------|-----------------|
| `templates/list.jinja` | `"list"` |
| `templates/report/summary.jinja` | `"report/summary"` |
| `styles/default.yaml` | `"default"` |
| `styles/themes/dark.yaml` | `"themes/dark"` |

#### Development Usage

During development, register directories and access resources by name:

```rust
use outstanding::TemplateRegistry;
use outstanding::stylesheet::StylesheetRegistry;

// Templates
let mut templates = TemplateRegistry::new();
templates.add_dir("./templates")?;
let content = templates.get("report/summary")?;

// Stylesheets
let mut styles = StylesheetRegistry::new();
styles.add_dir("./styles")?;
let theme = styles.get("themes/dark")?;
```

Files are re-read from disk on each `get()` call, so edits are immediately visible without restarting the application.

#### Release Embedding

For release builds, use the embedding macros to bake all files into the binary:

```rust
use outstanding::embed_templates;
use outstanding::embed_styles;

// At compile time, walks the directory and embeds all files
embed_templates!("./templates");
embed_styles!("./styles");

// Same API as development - resources accessed by name
let content = templates.get("report/summary")?;
let theme = styles.get("themes/dark")?;
```

The macros:
1. Walk the directory at compile time
2. Read each file's content
3. Generate code that registers all resources with their derived names
4. Produce a single binary with no external file dependencies

### Conditional Compilation

A common pattern combines both modes:

```rust
#[cfg(debug_assertions)]
fn init_templates() -> TemplateRegistry {
    let mut reg = TemplateRegistry::new();
    reg.add_dir("./templates").expect("templates dir");
    reg
}

#[cfg(not(debug_assertions))]
fn init_templates() -> TemplateRegistry {
    embed_templates!("./templates")
}
```

Or use the convenience macro that handles this automatically:

```rust
// Uses file system in debug, embeds in release
let templates = outstanding::auto_templates!("./templates");
let styles = outstanding::auto_styles!("./styles");
```

## Supported Resource Types

| Resource | Extensions (priority order) | Registry Type |
|----------|----------------------------|---------------|
| Templates | `.jinja`, `.jinja2`, `.j2`, `.txt` | `TemplateRegistry` |
| Stylesheets | `.yaml`, `.yml` | `StylesheetRegistry` |

When multiple files share the same base name with different extensions, the higher-priority extension wins.

## Extension Points

The file loader infrastructure is generic and can support additional resource types:

```rust
use outstanding::file_loader::{FileRegistry, FileRegistryConfig};

// Custom configuration resource
let config = FileRegistryConfig {
    extensions: &[".toml", ".yaml"],
    transform: |content| parse_my_config(content),
};

let mut configs = FileRegistry::new(config);
configs.add_dir("./config")?;
```

## Error Handling

The system detects and reports:

- **Missing directories**: Clear error when a registered directory doesn't exist
- **Name collisions**: When the same name resolves from multiple directories
- **Parse errors**: When file content fails to parse (with file path context)
- **Missing resources**: When `get()` is called with an unknown name

## Summary

| Capability | Development | Release |
|------------|-------------|---------|
| File location | External directory | Embedded in binary |
| Hot reload | Yes | N/A |
| Single binary | No | Yes |
| API | `registry.get("name")` | `registry.get("name")` |

The file loader bridges development convenience with release simplicity, using the same API in both modes.
