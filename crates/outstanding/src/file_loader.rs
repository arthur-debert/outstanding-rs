//! File-based resource loading for templates and stylesheets.
//!
//! Outstanding supports file-based configuration for templates and stylesheets,
//! enabling a web-app-like development workflow for CLI applications.
//!
//! # Problem
//!
//! CLI applications need to manage templates and stylesheets. Developers want:
//!
//! - **Separation of concerns** - Keep templates and styles in files, not Rust code
//! - **Accessible to non-developers** - Designers can edit YAML/Jinja without Rust
//! - **Rapid iteration** - Changes visible immediately without recompilation
//! - **Single-binary distribution** - Released apps should be self-contained
//!
//! These requirements create tension: development wants external files for flexibility,
//! while release wants everything embedded for distribution.
//!
//! # Solution
//!
//! The file loader provides a unified system that:
//!
//! - **Development mode**: Reads files from disk with hot reload on each access
//! - **Release mode**: Embeds all files into the binary at compile time via proc macros
//!
//! ## Directory Structure
//!
//! Organize resources in dedicated directories:
//!
//! ```text
//! my-app/
//! ├── templates/
//! │   ├── list.jinja
//! │   └── report/
//! │       └── summary.jinja
//! └── styles/
//!     ├── default.yaml
//!     └── themes/
//!         └── dark.yaml
//! ```
//!
//! ## Name Resolution
//!
//! Files are referenced by their relative path from the root, without extension:
//!
//! | File Path | Resolution Name |
//! |-----------|-----------------|
//! | `templates/list.jinja` | `"list"` |
//! | `templates/report/summary.jinja` | `"report/summary"` |
//! | `styles/themes/dark.yaml` | `"themes/dark"` |
//!
//! ## Development Usage
//!
//! Register directories and access resources by name:
//!
//! ```rust,ignore
//! use outstanding::file_loader::{FileRegistry, FileRegistryConfig};
//!
//! let config = FileRegistryConfig {
//!     extensions: &[".yaml", ".yml"],
//!     transform: |content| Ok(content.to_string()),
//! };
//!
//! let mut registry = FileRegistry::new(config);
//! registry.add_dir("./styles")?;
//!
//! // Re-reads from disk each call - edits are immediately visible
//! let content = registry.get("themes/dark")?;
//! ```
//!
//! ## Release Embedding
//!
//! For release builds, use the embedding macros to bake files into the binary:
//!
//! ```rust,ignore
//! // At compile time, walks directory and embeds all files
//! let styles = outstanding::embed_styles!("./styles");
//!
//! // Same API - resources accessed by name
//! let theme = styles.get("themes/dark")?;
//! ```
//!
//! The macros walk the directory at compile time, read each file, and generate
//! code that registers all resources with their derived names.
//!
//! See the [`outstanding_macros`] crate for detailed documentation on
//! [`embed_templates!`](outstanding_macros::embed_templates) and
//! [`embed_styles!`](outstanding_macros::embed_styles).
//!
//! # Extension Priority
//!
//! Extensions are specified in priority order. When multiple files share the same
//! base name, the extension appearing earlier wins for extensionless lookups:
//!
//! ```rust,ignore
//! // With extensions: [".yaml", ".yml"]
//! // If both default.yaml and default.yml exist:
//! registry.get("default")     // → default.yaml (higher priority)
//! registry.get("default.yml") // → default.yml (explicit)
//! ```
//!
//! # Collision Detection
//!
//! Cross-directory collisions (same name from different directories) cause a panic
//! with detailed diagnostics. This catches configuration mistakes early.
//!
//! Same-directory, different-extension scenarios are resolved by priority (not errors).
//!
//! # Supported Resource Types
//!
//! | Resource | Extensions | Transform |
//! |----------|------------|-----------|
//! | Templates | `.jinja`, `.jinja2`, `.j2`, `.txt` | Identity |
//! | Stylesheets | `.yaml`, `.yml` | YAML parsing |
//! | Custom | User-defined | User-defined |
//!
//! The registry is generic over content type `T`, enabling consistent behavior
//! across all resource types with type-specific parsing via the transform function.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A file discovered during directory walking.
///
/// This struct captures essential metadata about a file without reading its content,
/// enabling lazy loading and hot reloading.
///
/// # Fields
///
/// - `name`: The resolution name without extension (e.g., `"todos/list"`)
/// - `name_with_ext`: The resolution name with extension (e.g., `"todos/list.tmpl"`)
/// - `path`: Absolute filesystem path for reading content
/// - `source_dir`: The root directory this file came from (for collision reporting)
///
/// # Example
///
/// For a file at `/app/templates/todos/list.tmpl` with root `/app/templates`:
///
/// ```rust,ignore
/// LoadedFile {
///     name: "todos/list".to_string(),
///     name_with_ext: "todos/list.tmpl".to_string(),
///     path: PathBuf::from("/app/templates/todos/list.tmpl"),
///     source_dir: PathBuf::from("/app/templates"),
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedFile {
    /// Resolution name without extension (e.g., "config" or "todos/list").
    pub name: String,
    /// Resolution name with extension (e.g., "config.tmpl" or "todos/list.tmpl").
    pub name_with_ext: String,
    /// Absolute path to the file.
    pub path: PathBuf,
    /// The source directory this file belongs to.
    pub source_dir: PathBuf,
}

impl LoadedFile {
    /// Creates a new loaded file descriptor.
    pub fn new(
        name: impl Into<String>,
        name_with_ext: impl Into<String>,
        path: impl Into<PathBuf>,
        source_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            name: name.into(),
            name_with_ext: name_with_ext.into(),
            path: path.into(),
            source_dir: source_dir.into(),
        }
    }

    /// Returns the extension priority for this file given a list of extensions.
    ///
    /// Lower values indicate higher priority. Returns `usize::MAX` if the file's
    /// extension is not in the list.
    pub fn extension_priority(&self, extensions: &[&str]) -> usize {
        extension_priority(&self.name_with_ext, extensions)
    }
}

// =============================================================================
// Shared helper functions for extension handling
// =============================================================================

/// Returns the extension priority for a filename (lower = higher priority).
///
/// Extensions are matched in order against the provided list. The index of the
/// first matching extension is returned. If no extension matches, returns `usize::MAX`.
///
/// # Example
///
/// ```rust
/// use outstanding::file_loader::extension_priority;
///
/// let extensions = &[".yaml", ".yml"];
/// assert_eq!(extension_priority("config.yaml", extensions), 0);
/// assert_eq!(extension_priority("config.yml", extensions), 1);
/// assert_eq!(extension_priority("config.txt", extensions), usize::MAX);
/// ```
pub fn extension_priority(name: &str, extensions: &[&str]) -> usize {
    for (i, ext) in extensions.iter().enumerate() {
        if name.ends_with(ext) {
            return i;
        }
    }
    usize::MAX
}

/// Strips a recognized extension from a filename.
///
/// Returns the base name without extension if a recognized extension is found,
/// otherwise returns the original name.
///
/// # Example
///
/// ```rust
/// use outstanding::file_loader::strip_extension;
///
/// let extensions = &[".yaml", ".yml"];
/// assert_eq!(strip_extension("config.yaml", extensions), "config");
/// assert_eq!(strip_extension("themes/dark.yml", extensions), "themes/dark");
/// assert_eq!(strip_extension("readme.txt", extensions), "readme.txt");
/// ```
pub fn strip_extension(name: &str, extensions: &[&str]) -> String {
    for ext in extensions {
        if let Some(base) = name.strip_suffix(ext) {
            return base.to_string();
        }
    }
    name.to_string()
}

/// Builds a registry map from embedded entries with extension priority handling.
///
/// This is the core logic for creating registries from compile-time embedded resources.
/// It handles:
///
/// 1. **Extension priority**: Entries are sorted so higher-priority extensions are processed first
/// 2. **Dual registration**: Each entry is accessible by both base name and full name with extension
/// 3. **Transform**: Each entry's content is transformed via the provided function
///
/// # Arguments
///
/// * `entries` - Slice of `(name_with_ext, content)` pairs
/// * `extensions` - Extension list in priority order (first = highest)
/// * `transform` - Function to transform content into target type
///
/// # Returns
///
/// A `HashMap<String, T>` where each entry is accessible by both its base name
/// (without extension) and its full name (with extension).
///
/// # Example
///
/// ```rust,ignore
/// use outstanding::file_loader::build_embedded_registry;
///
/// let entries = &[
///     ("config.yaml", "key: value"),
///     ("config.yml", "other: data"),
///     ("themes/dark.yaml", "bg: black"),
/// ];
///
/// let registry = build_embedded_registry(
///     entries,
///     &[".yaml", ".yml"],
///     |content| Ok(content.to_string()),
/// )?;
///
/// // "config" resolves to config.yaml (higher priority)
/// // Both "config.yaml" and "config.yml" are accessible explicitly
/// ```
pub fn build_embedded_registry<T, E, F>(
    entries: &[(&str, &str)],
    extensions: &[&str],
    transform: F,
) -> Result<HashMap<String, T>, E>
where
    T: Clone,
    F: Fn(&str) -> Result<T, E>,
{
    let mut registry = HashMap::new();

    // Sort by extension priority so higher-priority extensions are processed first
    let mut sorted: Vec<_> = entries.iter().collect();
    sorted.sort_by_key(|(name, _)| extension_priority(name, extensions));

    let mut seen_base_names = std::collections::HashSet::new();

    for (name_with_ext, content) in sorted {
        let value = transform(content)?;
        let base_name = strip_extension(name_with_ext, extensions);

        // Register under full name with extension
        registry.insert((*name_with_ext).to_string(), value.clone());

        // Register under base name only if not already registered
        // (higher priority extension was already processed)
        if seen_base_names.insert(base_name.clone()) {
            registry.insert(base_name, value);
        }
    }

    Ok(registry)
}

/// How a resource is stored—file path (dev) or content (release).
///
/// This enum enables different storage strategies:
///
/// - [`File`](LoadedEntry::File): Store the path, read on demand (hot reload in dev)
/// - [`Embedded`](LoadedEntry::Embedded): Store content directly (no filesystem access)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadedEntry<T> {
    /// Path to read from disk (dev mode, enables hot reload).
    ///
    /// On each access, the file is re-read and transformed, picking up any changes.
    File(PathBuf),

    /// Pre-loaded/embedded content (release mode).
    ///
    /// Content is stored directly, avoiding filesystem access at runtime.
    Embedded(T),
}

/// Configuration for a file registry.
///
/// Specifies which file extensions to recognize and how to transform file content
/// into the target type.
///
/// # Example
///
/// ```rust,ignore
/// // For template files (identity transform)
/// FileRegistryConfig {
///     extensions: &[".tmpl", ".jinja2", ".j2"],
///     transform: |content| Ok(content.to_string()),
/// }
///
/// // For stylesheet files (YAML parsing)
/// FileRegistryConfig {
///     extensions: &[".yaml", ".yml"],
///     transform: |content| parse_style_definitions(content),
/// }
/// ```
pub struct FileRegistryConfig<T> {
    /// Valid file extensions in priority order (first = highest priority).
    ///
    /// When multiple files exist with the same base name but different extensions,
    /// the extension appearing earlier in this list wins for extensionless lookups.
    pub extensions: &'static [&'static str],

    /// Transform function: file content → typed value.
    ///
    /// Called when reading a file to convert raw string content into the target type.
    /// Return `Err(LoadError::Transform { .. })` for parse failures.
    pub transform: fn(&str) -> Result<T, LoadError>,
}

/// Error type for file loading operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// Directory does not exist or is not accessible.
    DirectoryNotFound {
        /// Path to the directory that was not found.
        path: PathBuf,
    },

    /// IO error reading a file.
    Io {
        /// Path that failed to read.
        path: PathBuf,
        /// Error message.
        message: String,
    },

    /// Resource not found in registry.
    NotFound {
        /// The name that was requested.
        name: String,
    },

    /// Cross-directory collision detected.
    ///
    /// Two directories contain files that resolve to the same name.
    /// This is a configuration error that must be fixed.
    Collision {
        /// The resource name that has conflicting sources.
        name: String,
        /// Path to the existing resource.
        existing_path: PathBuf,
        /// Directory containing the existing resource.
        existing_dir: PathBuf,
        /// Path to the conflicting resource.
        conflicting_path: PathBuf,
        /// Directory containing the conflicting resource.
        conflicting_dir: PathBuf,
    },

    /// Transform function failed.
    Transform {
        /// The resource name.
        name: String,
        /// Error message from the transform.
        message: String,
    },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::DirectoryNotFound { path } => {
                write!(f, "Directory not found: {}", path.display())
            }
            LoadError::Io { path, message } => {
                write!(f, "Failed to read \"{}\": {}", path.display(), message)
            }
            LoadError::NotFound { name } => {
                write!(f, "Resource not found: \"{}\"", name)
            }
            LoadError::Collision {
                name,
                existing_path,
                existing_dir,
                conflicting_path,
                conflicting_dir,
            } => {
                write!(
                    f,
                    "Collision detected for \"{}\":\n  \
                     - {} (from {})\n  \
                     - {} (from {})",
                    name,
                    existing_path.display(),
                    existing_dir.display(),
                    conflicting_path.display(),
                    conflicting_dir.display()
                )
            }
            LoadError::Transform { name, message } => {
                write!(f, "Failed to transform \"{}\": {}", name, message)
            }
        }
    }
}

impl std::error::Error for LoadError {}

/// Generic registry for file-based resources.
///
/// Manages loading and accessing resources from multiple directories with consistent
/// behavior for extension priority, collision detection, and dev/release modes.
///
/// # Type Parameter
///
/// - `T`: The content type. Must implement `Clone` for `get()` to return owned values.
///
/// # Example
///
/// ```rust,ignore
/// let config = FileRegistryConfig {
///     extensions: &[".yaml", ".yml"],
///     transform: |content| serde_yaml::from_str(content).map_err(|e| LoadError::Transform {
///         name: String::new(),
///         message: e.to_string(),
///     }),
/// };
///
/// let mut registry = FileRegistry::new(config);
/// registry.add_dir("./styles")?;
///
/// let definitions = registry.get("darcula")?;
/// ```
pub struct FileRegistry<T> {
    /// Configuration for this registry.
    config: FileRegistryConfig<T>,
    /// Registered source directories.
    dirs: Vec<PathBuf>,
    /// Map from name to loaded entry.
    entries: HashMap<String, LoadedEntry<T>>,
    /// Tracks source info for collision detection: name → (path, source_dir).
    sources: HashMap<String, (PathBuf, PathBuf)>,
    /// Whether the registry has been initialized from directories.
    initialized: bool,
}

impl<T: Clone> FileRegistry<T> {
    /// Creates a new registry with the given configuration.
    ///
    /// The registry starts empty. Call [`add_dir`](Self::add_dir) to register
    /// directories, then [`refresh`](Self::refresh) or access resources to
    /// trigger initialization.
    pub fn new(config: FileRegistryConfig<T>) -> Self {
        Self {
            config,
            dirs: Vec::new(),
            entries: HashMap::new(),
            sources: HashMap::new(),
            initialized: false,
        }
    }

    /// Adds a directory to search for files.
    ///
    /// Directories are searched in registration order. If files with the same name
    /// exist in multiple directories, a collision error is raised.
    ///
    /// # Lazy Initialization
    ///
    /// The directory is validated but not walked immediately. Walking happens on
    /// first access or explicit [`refresh`](Self::refresh) call.
    ///
    /// # Errors
    ///
    /// Returns [`LoadError::DirectoryNotFound`] if the path doesn't exist or
    /// isn't a directory.
    pub fn add_dir<P: AsRef<Path>>(&mut self, path: P) -> Result<(), LoadError> {
        let path = path.as_ref();

        if !path.exists() {
            return Err(LoadError::DirectoryNotFound {
                path: path.to_path_buf(),
            });
        }
        if !path.is_dir() {
            return Err(LoadError::DirectoryNotFound {
                path: path.to_path_buf(),
            });
        }

        self.dirs.push(path.to_path_buf());
        self.initialized = false;
        Ok(())
    }

    /// Adds pre-embedded content (for release builds).
    ///
    /// Embedded resources are stored directly in memory, avoiding filesystem
    /// access at runtime. Useful for deployment scenarios.
    ///
    /// # Note
    ///
    /// Embedded resources shadow file-based resources with the same name.
    pub fn add_embedded(&mut self, name: &str, content: T) {
        self.entries
            .insert(name.to_string(), LoadedEntry::Embedded(content));
    }

    /// Initializes/refreshes the registry from registered directories.
    ///
    /// This walks all registered directories, discovers files, and builds the
    /// resolution map. Call this to:
    ///
    /// - Pick up newly added files (in dev mode)
    /// - Force re-initialization after adding directories
    ///
    /// # Panics
    ///
    /// Panics if a collision is detected (same name from different directories).
    /// This is intentional—collisions are configuration errors that must be fixed.
    ///
    /// # Errors
    ///
    /// Returns an error if directory walking fails.
    pub fn refresh(&mut self) -> Result<(), LoadError> {
        // Collect all files from all directories
        let mut all_files = Vec::new();
        for dir in &self.dirs {
            let files = walk_dir(dir, self.config.extensions)?;
            all_files.extend(files);
        }

        // Clear existing file-based entries (keep embedded)
        self.entries
            .retain(|_, v| matches!(v, LoadedEntry::Embedded(_)));
        self.sources.clear();

        // Sort by extension priority so higher-priority extensions are processed first
        all_files.sort_by_key(|f| f.extension_priority(self.config.extensions));

        // Process files
        for file in all_files {
            let entry = LoadedEntry::File(file.path.clone());

            // Check for cross-directory collision on the base name
            if let Some((existing_path, existing_dir)) = self.sources.get(&file.name) {
                if existing_dir != &file.source_dir {
                    panic!(
                        "{}",
                        LoadError::Collision {
                            name: file.name.clone(),
                            existing_path: existing_path.clone(),
                            existing_dir: existing_dir.clone(),
                            conflicting_path: file.path.clone(),
                            conflicting_dir: file.source_dir.clone(),
                        }
                    );
                }
                // Same directory, different extension—only register the explicit name with extension
                // (the extensionless name already points to higher-priority extension)
                // But only if there's no embedded entry for this explicit name
                if !self.entries.contains_key(&file.name_with_ext) {
                    self.entries.insert(file.name_with_ext.clone(), entry);
                }
                continue;
            }

            // Track source for collision detection
            self.sources.insert(
                file.name.clone(),
                (file.path.clone(), file.source_dir.clone()),
            );

            // Add under extensionless name (only if no embedded entry exists)
            if !self.entries.contains_key(&file.name) {
                self.entries.insert(file.name.clone(), entry.clone());
            }

            // Add under name with extension (only if no embedded entry exists)
            if !self.entries.contains_key(&file.name_with_ext) {
                self.entries.insert(file.name_with_ext.clone(), entry);
            }
        }

        self.initialized = true;
        Ok(())
    }

    /// Ensures the registry is initialized, doing so lazily if needed.
    fn ensure_initialized(&mut self) -> Result<(), LoadError> {
        if !self.initialized && !self.dirs.is_empty() {
            self.refresh()?;
        }
        Ok(())
    }

    /// Gets a resource by name, applying the transform if reading from disk.
    ///
    /// In dev mode (when using [`LoadedEntry::File`]): re-reads file and transforms
    /// on each call, enabling hot reload.
    ///
    /// In release mode (when using [`LoadedEntry::Embedded`]): returns embedded
    /// content directly.
    ///
    /// # Errors
    ///
    /// - [`LoadError::NotFound`] if the name doesn't exist
    /// - [`LoadError::Io`] if the file can't be read
    /// - [`LoadError::Transform`] if the transform function fails
    pub fn get(&mut self, name: &str) -> Result<T, LoadError> {
        self.ensure_initialized()?;

        match self.entries.get(name) {
            Some(LoadedEntry::Embedded(content)) => Ok(content.clone()),
            Some(LoadedEntry::File(path)) => {
                let content = std::fs::read_to_string(path).map_err(|e| LoadError::Io {
                    path: path.clone(),
                    message: e.to_string(),
                })?;
                (self.config.transform)(&content).map_err(|e| {
                    if let LoadError::Transform { message, .. } = e {
                        LoadError::Transform {
                            name: name.to_string(),
                            message,
                        }
                    } else {
                        e
                    }
                })
            }
            None => Err(LoadError::NotFound {
                name: name.to_string(),
            }),
        }
    }

    /// Returns a reference to the entry if it exists.
    ///
    /// Unlike [`get`](Self::get), this doesn't trigger initialization or file reading.
    /// Useful for checking if a name exists without side effects.
    pub fn get_entry(&self, name: &str) -> Option<&LoadedEntry<T>> {
        self.entries.get(name)
    }

    /// Returns an iterator over all registered names.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(|s| s.as_str())
    }

    /// Returns the number of registered resources.
    ///
    /// Note: This counts both extensionless and with-extension entries,
    /// so it may be higher than the number of unique files.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if no resources are registered.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clears all entries from the registry.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.sources.clear();
        self.initialized = false;
    }

    /// Returns the registered directories.
    pub fn dirs(&self) -> &[PathBuf] {
        &self.dirs
    }
}

/// Walks a directory recursively and collects files with recognized extensions.
///
/// # Arguments
///
/// - `root`: The directory to walk
/// - `extensions`: Recognized file extensions
///
/// # Returns
///
/// A vector of [`LoadedFile`] entries, one for each discovered file.
pub fn walk_dir(root: &Path, extensions: &[&str]) -> Result<Vec<LoadedFile>, LoadError> {
    let root_canonical = root.canonicalize().map_err(|e| LoadError::Io {
        path: root.to_path_buf(),
        message: e.to_string(),
    })?;

    let mut files = Vec::new();
    walk_dir_recursive(&root_canonical, &root_canonical, extensions, &mut files)?;
    Ok(files)
}

/// Recursive helper for directory walking.
fn walk_dir_recursive(
    current: &Path,
    root: &Path,
    extensions: &[&str],
    files: &mut Vec<LoadedFile>,
) -> Result<(), LoadError> {
    let entries = std::fs::read_dir(current).map_err(|e| LoadError::Io {
        path: current.to_path_buf(),
        message: e.to_string(),
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| LoadError::Io {
            path: current.to_path_buf(),
            message: e.to_string(),
        })?;
        let path = entry.path();

        if path.is_dir() {
            walk_dir_recursive(&path, root, extensions, files)?;
        } else if path.is_file() {
            if let Some(loaded_file) = try_parse_file(&path, root, extensions) {
                files.push(loaded_file);
            }
        }
    }

    Ok(())
}

/// Attempts to parse a file path as a loadable file.
///
/// Returns `None` if the file doesn't have a recognized extension.
fn try_parse_file(path: &Path, root: &Path, extensions: &[&str]) -> Option<LoadedFile> {
    let path_str = path.to_string_lossy();

    // Find which extension this file has
    let extension = extensions.iter().find(|ext| path_str.ends_with(*ext))?;

    // Compute relative path from root
    let relative = path.strip_prefix(root).ok()?;
    let relative_str = relative.to_string_lossy();

    // Name with extension (using forward slashes for consistency)
    let name_with_ext = relative_str.replace(std::path::MAIN_SEPARATOR, "/");

    // Name without extension
    let name = name_with_ext.strip_suffix(extension)?.to_string();

    Some(LoadedFile::new(name, name_with_ext, path, root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_file(dir: &Path, relative_path: &str, content: &str) {
        let full_path = dir.join(relative_path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut file = std::fs::File::create(&full_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    fn string_config() -> FileRegistryConfig<String> {
        FileRegistryConfig {
            extensions: &[".tmpl", ".jinja2", ".j2"],
            transform: |content| Ok(content.to_string()),
        }
    }

    // =========================================================================
    // LoadedFile tests
    // =========================================================================

    #[test]
    fn test_loaded_file_extension_priority() {
        let extensions = &[".tmpl", ".jinja2", ".j2"];

        let tmpl = LoadedFile::new("config", "config.tmpl", "/a/config.tmpl", "/a");
        let jinja2 = LoadedFile::new("config", "config.jinja2", "/a/config.jinja2", "/a");
        let j2 = LoadedFile::new("config", "config.j2", "/a/config.j2", "/a");
        let unknown = LoadedFile::new("config", "config.txt", "/a/config.txt", "/a");

        assert_eq!(tmpl.extension_priority(extensions), 0);
        assert_eq!(jinja2.extension_priority(extensions), 1);
        assert_eq!(j2.extension_priority(extensions), 2);
        assert_eq!(unknown.extension_priority(extensions), usize::MAX);
    }

    // =========================================================================
    // FileRegistry basic tests
    // =========================================================================

    #[test]
    fn test_registry_new_is_empty() {
        let registry = FileRegistry::new(string_config());
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_add_embedded() {
        let mut registry = FileRegistry::new(string_config());
        registry.add_embedded("test", "content".to_string());

        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());

        let content = registry.get("test").unwrap();
        assert_eq!(content, "content");
    }

    #[test]
    fn test_registry_embedded_overwrites() {
        let mut registry = FileRegistry::new(string_config());
        registry.add_embedded("test", "first".to_string());
        registry.add_embedded("test", "second".to_string());

        let content = registry.get("test").unwrap();
        assert_eq!(content, "second");
    }

    #[test]
    fn test_registry_not_found() {
        let mut registry = FileRegistry::new(string_config());
        let result = registry.get("nonexistent");
        assert!(matches!(result, Err(LoadError::NotFound { .. })));
    }

    // =========================================================================
    // Directory-based tests
    // =========================================================================

    #[test]
    fn test_registry_add_dir_nonexistent() {
        let mut registry = FileRegistry::new(string_config());
        let result = registry.add_dir("/nonexistent/path");
        assert!(matches!(result, Err(LoadError::DirectoryNotFound { .. })));
    }

    #[test]
    fn test_registry_add_dir_and_get() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "config.tmpl", "Config content");

        let mut registry = FileRegistry::new(string_config());
        registry.add_dir(temp_dir.path()).unwrap();

        let content = registry.get("config").unwrap();
        assert_eq!(content, "Config content");
    }

    #[test]
    fn test_registry_nested_directories() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "todos/list.tmpl", "List content");
        create_file(temp_dir.path(), "todos/detail.tmpl", "Detail content");

        let mut registry = FileRegistry::new(string_config());
        registry.add_dir(temp_dir.path()).unwrap();

        assert_eq!(registry.get("todos/list").unwrap(), "List content");
        assert_eq!(registry.get("todos/detail").unwrap(), "Detail content");
    }

    #[test]
    fn test_registry_access_with_extension() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "config.tmpl", "Content");

        let mut registry = FileRegistry::new(string_config());
        registry.add_dir(temp_dir.path()).unwrap();

        // Both with and without extension should work
        assert!(registry.get("config").is_ok());
        assert!(registry.get("config.tmpl").is_ok());
    }

    #[test]
    fn test_registry_extension_priority() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "config.j2", "From j2");
        create_file(temp_dir.path(), "config.tmpl", "From tmpl");

        let mut registry = FileRegistry::new(string_config());
        registry.add_dir(temp_dir.path()).unwrap();

        // Extensionless should resolve to .tmpl (higher priority)
        let content = registry.get("config").unwrap();
        assert_eq!(content, "From tmpl");

        // Explicit extension access still works
        assert_eq!(registry.get("config.j2").unwrap(), "From j2");
        assert_eq!(registry.get("config.tmpl").unwrap(), "From tmpl");
    }

    #[test]
    #[should_panic(expected = "Collision")]
    fn test_registry_collision_panics() {
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();

        create_file(temp_dir1.path(), "config.tmpl", "From dir1");
        create_file(temp_dir2.path(), "config.tmpl", "From dir2");

        let mut registry = FileRegistry::new(string_config());
        registry.add_dir(temp_dir1.path()).unwrap();
        registry.add_dir(temp_dir2.path()).unwrap();

        // This should panic due to collision
        registry.refresh().unwrap();
    }

    #[test]
    fn test_registry_embedded_shadows_file() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "config.tmpl", "From file");

        let mut registry = FileRegistry::new(string_config());
        registry.add_dir(temp_dir.path()).unwrap();
        registry.add_embedded("config", "From embedded".to_string());

        // Embedded should shadow file
        let content = registry.get("config").unwrap();
        assert_eq!(content, "From embedded");
    }

    #[test]
    fn test_registry_hot_reload() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "hot.tmpl", "Version 1");

        let mut registry = FileRegistry::new(string_config());
        registry.add_dir(temp_dir.path()).unwrap();

        // First read
        assert_eq!(registry.get("hot").unwrap(), "Version 1");

        // Modify file
        create_file(temp_dir.path(), "hot.tmpl", "Version 2");

        // Second read should see change (hot reload)
        assert_eq!(registry.get("hot").unwrap(), "Version 2");
    }

    #[test]
    fn test_registry_names_iterator() {
        let mut registry = FileRegistry::new(string_config());
        registry.add_embedded("a", "content a".to_string());
        registry.add_embedded("b", "content b".to_string());

        let names: Vec<&str> = registry.names().collect();
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));
    }

    #[test]
    fn test_registry_clear() {
        let mut registry = FileRegistry::new(string_config());
        registry.add_embedded("a", "content".to_string());

        assert!(!registry.is_empty());
        registry.clear();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_registry_refresh_picks_up_new_files() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "first.tmpl", "First content");

        let mut registry = FileRegistry::new(string_config());
        registry.add_dir(temp_dir.path()).unwrap();
        registry.refresh().unwrap();

        assert!(registry.get("first").is_ok());
        assert!(registry.get("second").is_err());

        // Add new file
        create_file(temp_dir.path(), "second.tmpl", "Second content");

        // Refresh to pick up new file
        registry.refresh().unwrap();

        assert!(registry.get("second").is_ok());
        assert_eq!(registry.get("second").unwrap(), "Second content");
    }

    // =========================================================================
    // Transform tests
    // =========================================================================

    #[test]
    fn test_registry_transform_success() {
        let config = FileRegistryConfig {
            extensions: &[".num"],
            transform: |content| {
                content
                    .trim()
                    .parse::<i32>()
                    .map_err(|e| LoadError::Transform {
                        name: String::new(),
                        message: e.to_string(),
                    })
            },
        };

        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "value.num", "42");

        let mut registry = FileRegistry::new(config);
        registry.add_dir(temp_dir.path()).unwrap();

        let value = registry.get("value").unwrap();
        assert_eq!(value, 42);
    }

    #[test]
    fn test_registry_transform_failure() {
        let config = FileRegistryConfig {
            extensions: &[".num"],
            transform: |content| {
                content
                    .trim()
                    .parse::<i32>()
                    .map_err(|e| LoadError::Transform {
                        name: String::new(),
                        message: e.to_string(),
                    })
            },
        };

        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "bad.num", "not a number");

        let mut registry = FileRegistry::new(config);
        registry.add_dir(temp_dir.path()).unwrap();

        let result = registry.get("bad");
        assert!(matches!(result, Err(LoadError::Transform { name, .. }) if name == "bad"));
    }

    // =========================================================================
    // walk_dir tests
    // =========================================================================

    #[test]
    fn test_walk_dir_empty() {
        let temp_dir = TempDir::new().unwrap();
        let files = walk_dir(temp_dir.path(), &[".tmpl"]).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_walk_dir_filters_extensions() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "good.tmpl", "content");
        create_file(temp_dir.path(), "bad.txt", "content");
        create_file(temp_dir.path(), "also_good.j2", "content");

        let files = walk_dir(temp_dir.path(), &[".tmpl", ".j2"]).unwrap();

        assert_eq!(files.len(), 2);
        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"good"));
        assert!(names.contains(&"also_good"));
    }

    #[test]
    fn test_walk_dir_nested() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "root.tmpl", "content");
        create_file(temp_dir.path(), "sub/nested.tmpl", "content");
        create_file(temp_dir.path(), "sub/deep/very.tmpl", "content");

        let files = walk_dir(temp_dir.path(), &[".tmpl"]).unwrap();

        assert_eq!(files.len(), 3);
        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"root"));
        assert!(names.contains(&"sub/nested"));
        assert!(names.contains(&"sub/deep/very"));
    }

    // =========================================================================
    // Error display tests
    // =========================================================================

    #[test]
    fn test_error_display_directory_not_found() {
        let err = LoadError::DirectoryNotFound {
            path: PathBuf::from("/missing"),
        };
        assert!(err.to_string().contains("/missing"));
    }

    #[test]
    fn test_error_display_not_found() {
        let err = LoadError::NotFound {
            name: "missing".to_string(),
        };
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn test_error_display_collision() {
        let err = LoadError::Collision {
            name: "config".to_string(),
            existing_path: PathBuf::from("/a/config.tmpl"),
            existing_dir: PathBuf::from("/a"),
            conflicting_path: PathBuf::from("/b/config.tmpl"),
            conflicting_dir: PathBuf::from("/b"),
        };

        let display = err.to_string();
        assert!(display.contains("config"));
        assert!(display.contains("/a/config.tmpl"));
        assert!(display.contains("/b/config.tmpl"));
    }

    #[test]
    fn test_error_display_transform() {
        let err = LoadError::Transform {
            name: "bad".to_string(),
            message: "parse error".to_string(),
        };
        let display = err.to_string();
        assert!(display.contains("bad"));
        assert!(display.contains("parse error"));
    }
}
