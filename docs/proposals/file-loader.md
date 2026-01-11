# Generic File Loader

This document describes the design for a generic file loading infrastructure that can be shared across Outstanding features requiring file-based resources.

## Motivation

### Feature Parity Across File-Based Resources

Outstanding is introducing multiple file-based resource types:

| Feature | Extensions | Content Type | Transform |
|---------|------------|--------------|-----------|
| Templates | `.tmpl`, `.jinja2`, `.j2` | String | Identity (pass-through) |
| Stylesheets | `.yaml`, `.yml` | StyleDefinitions | YAML parsing |
| (Future) Configs | `.toml`, `.yaml` | Config structs | Format-specific parsing |

Each of these shares the same fundamental requirements:

1. **Directory registration**: Multiple root directories, searched in order
2. **Name derivation**: Relative path from root, sans extension → resource name
3. **Extension priority**: When multiple extensions exist, higher priority wins
4. **Collision detection**: Same name from different directories → error
5. **Dev mode**: Re-read from disk on each access (hot reload)
6. **Release mode**: Embed content at compile time (no filesystem dependency)

### The Problem with Separate Implementations

Without a shared foundation, each feature would implement this logic independently:

- **Code duplication**: Same directory walking, collision detection, name derivation
- **Behavioral divergence**: Subtle differences in how edge cases are handled
- **Testing burden**: Same patterns tested multiple times
- **Maintenance cost**: Bug fixes applied in multiple places
- **Cognitive load**: Developers learn different APIs for the same conceptual operation

### The Solution: Shared Infrastructure

A generic `FileRegistry<T>` that encapsulates:
- All the common file loading logic
- Parameterized by file extensions and content transform
- Consistent behavior guaranteed across all file-based features

This provides:
- **Code reuse**: Write once, use everywhere
- **Consistent mental model**: Same API patterns for templates, stylesheets, configs
- **Single test surface**: Core logic tested once, transforms tested separately
- **Predictable behavior**: Developers know how file loading works regardless of resource type

## Design

### Core Types

```rust
/// A file discovered during directory walking.
#[derive(Debug, Clone)]
pub struct LoadedFile {
    /// Resolution name without extension (e.g., "todos/list")
    pub name: String,
    /// Resolution name with extension (e.g., "todos/list.tmpl")
    pub name_with_ext: String,
    /// Absolute path to the file
    pub path: PathBuf,
    /// Source directory this file belongs to
    pub source_dir: PathBuf,
}

/// How a resource is stored - file path (dev) or content (release).
#[derive(Debug, Clone)]
pub enum LoadedEntry<T> {
    /// Path to read from disk (dev mode, enables hot reload)
    File(PathBuf),
    /// Pre-loaded/embedded content (release mode)
    Embedded(T),
}

/// Configuration for a file registry.
pub struct FileRegistryConfig<T> {
    /// Valid file extensions in priority order (first = highest)
    pub extensions: &'static [&'static str],
    /// Transform function: file content → typed value
    pub transform: fn(&str) -> Result<T, LoadError>,
}

/// Generic registry for file-based resources.
pub struct FileRegistry<T> {
    config: FileRegistryConfig<T>,
    dirs: Vec<PathBuf>,
    entries: HashMap<String, LoadedEntry<T>>,
    sources: HashMap<String, (PathBuf, PathBuf)>, // name → (path, source_dir)
}
```

### FileRegistry API

```rust
impl<T: Clone> FileRegistry<T> {
    /// Creates a new registry with the given configuration.
    pub fn new(config: FileRegistryConfig<T>) -> Self;

    /// Adds a directory to search for files.
    ///
    /// Directories are searched in registration order.
    /// Files in earlier directories take priority.
    pub fn add_dir<P: AsRef<Path>>(&mut self, path: P) -> Result<(), LoadError>;

    /// Adds pre-embedded content (for release builds).
    pub fn add_embedded(&mut self, name: &str, content: T);

    /// Initializes/refreshes the registry from registered directories.
    ///
    /// In dev mode, call this to pick up new files.
    /// Called automatically on first access if not initialized.
    pub fn refresh(&mut self) -> Result<(), LoadError>;

    /// Gets a resource by name, applying the transform if reading from disk.
    ///
    /// In dev mode: re-reads file and transforms on each call (hot reload).
    /// In release mode: returns embedded content directly.
    pub fn get(&self, name: &str) -> Result<T, LoadError>;

    /// Returns all registered names.
    pub fn names(&self) -> impl Iterator<Item = &str>;

    /// Returns the number of registered resources.
    pub fn len(&self) -> usize;

    /// Returns true if no resources are registered.
    pub fn is_empty(&self) -> bool;
}
```

### Extension Priority

Extensions are specified in priority order. When multiple files exist with the same base name:

```rust
// Config for templates
FileRegistryConfig {
    extensions: &[".tmpl", ".jinja2", ".j2"],  // .tmpl wins
    transform: |s| Ok(s.to_string()),
}
```

Given files `config.tmpl` and `config.j2`:
- `"config"` → resolves to `config.tmpl` (higher priority)
- `"config.tmpl"` → resolves to `config.tmpl` (explicit)
- `"config.j2"` → resolves to `config.j2` (explicit)

### Collision Detection

Cross-directory collisions are errors:

```
templates/
  config.tmpl          → name "config"
plugins/templates/
  config.tmpl          → name "config" ← COLLISION!
```

Same-directory, different-extension is resolved by priority (not an error).

### Dev vs Release Mode

```rust
impl<T: Clone> FileRegistry<T> {
    pub fn get(&self, name: &str) -> Result<T, LoadError> {
        match self.entries.get(name) {
            Some(LoadedEntry::Embedded(content)) => {
                // Release: return pre-loaded content
                Ok(content.clone())
            }
            Some(LoadedEntry::File(path)) => {
                // Dev: read from disk and transform
                let content = std::fs::read_to_string(path)?;
                (self.config.transform)(&content)
            }
            None => Err(LoadError::NotFound { name: name.to_string() }),
        }
    }
}
```

## Usage Examples

### Templates

```rust
/// Template-specific configuration
pub fn template_config() -> FileRegistryConfig<String> {
    FileRegistryConfig {
        extensions: &[".tmpl", ".jinja2", ".j2"],
        transform: |content| Ok(content.to_string()),  // Identity
    }
}

// Usage
let mut templates = FileRegistry::new(template_config());
templates.add_dir("./templates")?;

let content = templates.get("todos/list")?;  // Returns template string
```

### Stylesheets

```rust
/// Stylesheet-specific configuration
pub fn stylesheet_config() -> FileRegistryConfig<StyleDefinitions> {
    FileRegistryConfig {
        extensions: &[".yaml", ".yml"],
        transform: |content| parse_style_definitions(content),  // YAML parsing
    }
}

// Usage
let mut stylesheets = FileRegistry::new(stylesheet_config());
stylesheets.add_dir("./styles")?;

let definitions = stylesheets.get("darcula")?;  // Returns parsed StyleDefinitions
```

### Embedded (Release Mode)

```rust
// At build time or via macro
let mut templates = FileRegistry::new(template_config());
templates.add_embedded("config", include_str!("../templates/config.tmpl").to_string());
templates.add_embedded("todos/list", include_str!("../templates/todos/list.tmpl").to_string());

// No filesystem access needed at runtime
let content = templates.get("config")?;
```

## Error Types

```rust
#[derive(Debug)]
pub enum LoadError {
    /// Directory does not exist or is not readable
    DirectoryNotFound { path: PathBuf },

    /// IO error reading file
    Io { path: PathBuf, source: std::io::Error },

    /// Resource not found in registry
    NotFound { name: String },

    /// Cross-directory collision detected
    Collision {
        name: String,
        existing_path: PathBuf,
        existing_dir: PathBuf,
        conflicting_path: PathBuf,
        conflicting_dir: PathBuf,
    },

    /// Transform function failed
    Transform { name: String, message: String },
}
```

## Migration: Existing Template Loader

The current `TemplateRegistry` and `walk_template_dir()` will be refactored to use `FileRegistry<String>`:

### Before

```rust
// Current implementation
pub struct TemplateRegistry {
    templates: HashMap<String, ResolvedTemplate>,
    sources: HashMap<String, (PathBuf, PathBuf)>,
}

pub fn walk_template_dir(root: &Path) -> Result<Vec<TemplateFile>, std::io::Error>;
```

### After

```rust
// Thin wrapper around FileRegistry
pub struct TemplateRegistry {
    inner: FileRegistry<String>,
}

impl TemplateRegistry {
    pub fn new() -> Self {
        Self {
            inner: FileRegistry::new(template_config()),
        }
    }

    pub fn add_template_dir<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Error> {
        self.inner.add_dir(path).map_err(into_minijinja_error)
    }

    pub fn get_content(&self, name: &str) -> Result<String, Error> {
        self.inner.get(name).map_err(into_minijinja_error)
    }
}
```

The public API remains compatible; only the implementation changes.

## Benefits Summary

| Benefit | Description |
|---------|-------------|
| **Single implementation** | Directory walking, collision detection, name derivation written once |
| **Consistent behavior** | Templates and stylesheets work identically |
| **Predictable API** | Developers learn one pattern, apply everywhere |
| **Reduced testing** | Core logic tested once; only transforms need separate tests |
| **Future-proof** | New file-based features get all capabilities for free |
| **Maintainability** | Bug fixes and improvements apply universally |

## Implementation Notes

1. **Decoupled from filesystem**: Core logic takes `Vec<LoadedFile>` for testability
2. **Generic over content type**: `FileRegistry<T>` works with any `T: Clone`
3. **Lazy initialization**: Directory walking happens on first access or explicit `refresh()`
4. **Thread safety**: Consider `Arc<RwLock<...>>` wrapper for concurrent access (future)
