//! Template registry for file-based and inline templates.
//!
//! This module provides [`TemplateRegistry`], which manages template resolution
//! from multiple sources: inline strings, filesystem directories, or embedded content.
//!
//! # Design
//!
//! The registry is a thin wrapper around [`FileRegistry<String>`](crate::file_loader::FileRegistry),
//! providing template-specific functionality while reusing the generic file loading infrastructure.
//!
//! The registry uses a two-phase approach:
//!
//! 1. Collection: Templates are collected from various sources (inline, directories, embedded)
//! 2. Resolution: A unified map resolves template names to their content or file paths
//!
//! This separation enables:
//! - Testability: Resolution logic can be tested without filesystem access
//! - Flexibility: Same resolution rules apply regardless of template source
//! - Hot reloading: File paths can be re-read on each render in development mode
//!
//! # Template Resolution
//!
//! Templates are resolved by name using these rules:
//!
//! 1. Inline templates (added via [`TemplateRegistry::add_inline`]) have highest priority
//! 2. File templates are searched in directory registration order (first directory wins)
//! 3. Names can be specified with or without extension: both `"config"` and `"config.jinja"` resolve
//!
//! # Supported Extensions
//!
//! Template files are recognized by extension, in priority order:
//!
//! | Priority | Extension | Description |
//! |----------|-----------|-------------|
//! | 1 (highest) | `.jinja` | Standard Jinja extension |
//! | 2 | `.jinja2` | Full Jinja2 extension |
//! | 3 | `.j2` | Short Jinja2 extension |
//! | 4 (lowest) | `.txt` | Plain text templates |
//!
//! If multiple files exist with the same base name but different extensions
//! (e.g., `config.jinja` and `config.j2`), the higher-priority extension wins.
//!
//! # Collision Handling
//!
//! The registry enforces strict collision rules:
//!
//! - Same-directory, different extensions: Higher priority extension wins (no error)
//! - Cross-directory collisions: Panic with detailed message listing conflicting files
//!
//! This strict behavior catches configuration mistakes early rather than silently
//! using an arbitrary winner.
//!
//! # Example
//!
//! ```rust,ignore
//! use standout::render::TemplateRegistry;
//!
//! let mut registry = TemplateRegistry::new();
//! registry.add_template_dir("./templates")?;
//! registry.add_inline("override", "Custom content");
//!
//! // Resolve templates
//! let content = registry.get_content("config")?;
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::file_loader::{
    self, build_embedded_registry, FileRegistry, FileRegistryConfig, LoadError, LoadedEntry,
    LoadedFile,
};

/// Recognized template file extensions in priority order.
///
/// When multiple files exist with the same base name but different extensions,
/// the extension appearing earlier in this list takes precedence.
///
/// # Priority Order
///
/// 1. `.jinja` - Standard Jinja extension
/// 2. `.jinja2` - Full Jinja2 extension
/// 3. `.j2` - Short Jinja2 extension
/// 4. `.txt` - Plain text templates
pub const TEMPLATE_EXTENSIONS: &[&str] = &[".jinja", ".jinja2", ".j2", ".txt"];

/// A template file discovered during directory walking.
///
/// This struct captures the essential information about a template file
/// without reading its content, enabling lazy loading and hot reloading.
///
/// # Fields
///
/// - `name`: The resolution name without extension (e.g., `"todos/list"`)
/// - `name_with_ext`: The resolution name with extension (e.g., `"todos/list.jinja"`)
/// - `absolute_path`: Full filesystem path for reading content
/// - `source_dir`: The template directory this file came from (for collision reporting)
///
/// # Example
///
/// For a file at `/app/templates/todos/list.jinja` with root `/app/templates`:
///
/// ```rust,ignore
/// TemplateFile {
///     name: "todos/list".to_string(),
///     name_with_ext: "todos/list.jinja".to_string(),
///     absolute_path: PathBuf::from("/app/templates/todos/list.jinja"),
///     source_dir: PathBuf::from("/app/templates"),
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateFile {
    /// Resolution name without extension (e.g., "config" or "todos/list")
    pub name: String,
    /// Resolution name with extension (e.g., "config.jinja" or "todos/list.jinja")
    pub name_with_ext: String,
    /// Absolute path to the template file
    pub absolute_path: PathBuf,
    /// The template directory root this file belongs to
    pub source_dir: PathBuf,
}

impl TemplateFile {
    /// Creates a new template file descriptor.
    pub fn new(
        name: impl Into<String>,
        name_with_ext: impl Into<String>,
        absolute_path: impl Into<PathBuf>,
        source_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            name: name.into(),
            name_with_ext: name_with_ext.into(),
            absolute_path: absolute_path.into(),
            source_dir: source_dir.into(),
        }
    }

    /// Returns the extension priority (lower is higher priority).
    ///
    /// Returns `usize::MAX` if the extension is not recognized.
    pub fn extension_priority(&self) -> usize {
        for (i, ext) in TEMPLATE_EXTENSIONS.iter().enumerate() {
            if self.name_with_ext.ends_with(ext) {
                return i;
            }
        }
        usize::MAX
    }
}

impl From<LoadedFile> for TemplateFile {
    fn from(file: LoadedFile) -> Self {
        Self {
            name: file.name,
            name_with_ext: file.name_with_ext,
            absolute_path: file.path,
            source_dir: file.source_dir,
        }
    }
}

impl From<TemplateFile> for LoadedFile {
    fn from(file: TemplateFile) -> Self {
        Self {
            name: file.name,
            name_with_ext: file.name_with_ext,
            path: file.absolute_path,
            source_dir: file.source_dir,
        }
    }
}

/// How a template's content is stored or accessed.
///
/// This enum enables different storage strategies:
/// - `Inline`: Content is stored directly (for inline templates or embedded builds)
/// - `File`: Content is read from disk on demand (for hot reloading in development)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedTemplate {
    /// Template content stored directly in memory.
    ///
    /// Used for:
    /// - Inline templates added via `add_inline()`
    /// - Embedded templates in release builds
    Inline(String),

    /// Template loaded from filesystem on demand.
    ///
    /// The path is read on each render in development mode,
    /// enabling hot reloading without recompilation.
    File(PathBuf),
}

impl From<&LoadedEntry<String>> for ResolvedTemplate {
    fn from(entry: &LoadedEntry<String>) -> Self {
        match entry {
            LoadedEntry::Embedded(content) => ResolvedTemplate::Inline(content.clone()),
            LoadedEntry::File(path) => ResolvedTemplate::File(path.clone()),
        }
    }
}

/// Error type for template registry operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    /// Two template directories contain files that resolve to the same name.
    ///
    /// This is an unrecoverable configuration error that must be fixed
    /// by the application developer.
    Collision {
        /// The template name that has conflicting sources
        name: String,
        /// Path to the existing template
        existing_path: PathBuf,
        /// Directory containing the existing template
        existing_dir: PathBuf,
        /// Path to the conflicting template
        conflicting_path: PathBuf,
        /// Directory containing the conflicting template
        conflicting_dir: PathBuf,
    },

    /// Template not found in registry.
    NotFound {
        /// The name that was requested
        name: String,
    },

    /// Failed to read template file from disk.
    ReadError {
        /// Path that failed to read
        path: PathBuf,
        /// Error message
        message: String,
    },
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::Collision {
                name,
                existing_path,
                existing_dir,
                conflicting_path,
                conflicting_dir,
            } => {
                write!(
                    f,
                    "Template collision detected for \"{}\":\n  \
                     - {} (from {})\n  \
                     - {} (from {})",
                    name,
                    existing_path.display(),
                    existing_dir.display(),
                    conflicting_path.display(),
                    conflicting_dir.display()
                )
            }
            RegistryError::NotFound { name } => {
                write!(f, "Template not found: \"{}\"", name)
            }
            RegistryError::ReadError { path, message } => {
                write!(
                    f,
                    "Failed to read template \"{}\": {}",
                    path.display(),
                    message
                )
            }
        }
    }
}

impl std::error::Error for RegistryError {}

impl From<LoadError> for RegistryError {
    fn from(err: LoadError) -> Self {
        match err {
            LoadError::NotFound { name } => RegistryError::NotFound { name },
            LoadError::Io { path, message } => RegistryError::ReadError { path, message },
            LoadError::Collision {
                name,
                existing_path,
                existing_dir,
                conflicting_path,
                conflicting_dir,
            } => RegistryError::Collision {
                name,
                existing_path,
                existing_dir,
                conflicting_path,
                conflicting_dir,
            },
            LoadError::DirectoryNotFound { path } => RegistryError::ReadError {
                path: path.clone(),
                message: format!("Directory not found: {}", path.display()),
            },
            LoadError::Transform { name, message } => RegistryError::ReadError {
                path: PathBuf::from(&name),
                message,
            },
        }
    }
}

/// Creates the file registry configuration for templates.
fn template_config() -> FileRegistryConfig<String> {
    FileRegistryConfig {
        extensions: TEMPLATE_EXTENSIONS,
        transform: |content| Ok(content.to_string()),
    }
}

/// Registry for template resolution from multiple sources.
///
/// The registry maintains a unified view of templates from:
/// - Inline strings (highest priority)
/// - Multiple filesystem directories
/// - Embedded content (for release builds)
///
/// # Resolution Order
///
/// When looking up a template name:
///
/// 1. Check inline templates first
/// 2. Check file-based templates in registration order
/// 3. Return error if not found
///
/// # Thread Safety
///
/// The registry is not thread-safe. For concurrent access, wrap in appropriate
/// synchronization primitives.
///
/// # Example
///
/// ```rust,ignore
/// let mut registry = TemplateRegistry::new();
///
/// // Add inline template (highest priority)
/// registry.add_inline("header", "{{ title }}");
///
/// // Add from directory
/// registry.add_template_dir("./templates")?;
///
/// // Resolve and get content
/// let content = registry.get_content("header")?;
/// ```
pub struct TemplateRegistry {
    /// The underlying file registry for directory-based file loading.
    inner: FileRegistry<String>,

    /// Inline templates (stored separately for highest priority).
    inline: HashMap<String, String>,

    /// File-based templates from add_from_files (maps name → path).
    /// These are separate from directory-based loading.
    files: HashMap<String, PathBuf>,

    /// Tracks source info for collision detection: name → (path, source_dir).
    sources: HashMap<String, (PathBuf, PathBuf)>,

    /// Framework templates (lowest priority fallback).
    /// These are provided by the standout framework and can be overridden
    /// by user templates with the same name.
    framework: HashMap<String, String>,
}

impl Default for TemplateRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateRegistry {
    /// Creates an empty template registry.
    pub fn new() -> Self {
        Self {
            inner: FileRegistry::new(template_config()),
            inline: HashMap::new(),
            files: HashMap::new(),
            sources: HashMap::new(),
            framework: HashMap::new(),
        }
    }

    /// Adds an inline template with the given name.
    ///
    /// Inline templates have the highest priority and will shadow any
    /// file-based templates with the same name.
    ///
    /// # Arguments
    ///
    /// * `name` - The template name for resolution
    /// * `content` - The template content
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// registry.add_inline("header", "{{ title | style(\"title\") }}");
    /// ```
    pub fn add_inline(&mut self, name: impl Into<String>, content: impl Into<String>) {
        self.inline.insert(name.into(), content.into());
    }

    /// Adds a template directory to search for files.
    ///
    /// Templates in the directory are resolved by their relative path without
    /// extension. For example, with directory `./templates`:
    ///
    /// - `"config"` → `./templates/config.jinja`
    /// - `"todos/list"` → `./templates/todos/list.jinja`
    ///
    /// # Errors
    ///
    /// Returns an error if the directory doesn't exist.
    pub fn add_template_dir<P: AsRef<Path>>(&mut self, path: P) -> Result<(), RegistryError> {
        self.inner.add_dir(path).map_err(RegistryError::from)
    }

    /// Adds templates discovered from a directory scan.
    ///
    /// This method processes a list of [`TemplateFile`] entries, typically
    /// produced by [`walk_template_dir`], and registers them for resolution.
    ///
    /// # Resolution Names
    ///
    /// Each file is registered under two names:
    /// - Without extension: `"config"` for `config.jinja`
    /// - With extension: `"config.jinja"` for `config.jinja`
    ///
    /// # Extension Priority
    ///
    /// If multiple files share the same base name with different extensions
    /// (e.g., `config.jinja` and `config.j2`), the higher-priority extension wins
    /// for the extensionless name. Both can still be accessed by full name.
    ///
    /// # Collision Detection
    ///
    /// If a template name conflicts with one from a different source directory,
    /// an error is returned with details about both files.
    ///
    /// # Arguments
    ///
    /// * `files` - Template files discovered during directory walking
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError::Collision`] if templates from different
    /// directories resolve to the same name.
    pub fn add_from_files(&mut self, files: Vec<TemplateFile>) -> Result<(), RegistryError> {
        // Sort by extension priority so higher-priority extensions are processed first
        let mut sorted_files = files;
        sorted_files.sort_by_key(|f| f.extension_priority());

        for file in sorted_files {
            // Check for cross-directory collision on the base name
            if let Some((existing_path, existing_dir)) = self.sources.get(&file.name) {
                // Only error if from different source directories
                if existing_dir != &file.source_dir {
                    return Err(RegistryError::Collision {
                        name: file.name.clone(),
                        existing_path: existing_path.clone(),
                        existing_dir: existing_dir.clone(),
                        conflicting_path: file.absolute_path.clone(),
                        conflicting_dir: file.source_dir.clone(),
                    });
                }
                // Same directory, different extension - skip (higher priority already registered)
                continue;
            }

            // Track source for collision detection
            self.sources.insert(
                file.name.clone(),
                (file.absolute_path.clone(), file.source_dir.clone()),
            );

            // Register the template under extensionless name
            self.files
                .insert(file.name.clone(), file.absolute_path.clone());

            // Register under name with extension (allows explicit access)
            self.files
                .insert(file.name_with_ext.clone(), file.absolute_path);
        }

        Ok(())
    }

    /// Adds pre-embedded templates (for release builds).
    ///
    /// Embedded templates are treated as inline templates, stored directly
    /// in memory without filesystem access.
    ///
    /// # Arguments
    ///
    /// * `templates` - Map of template name to content
    pub fn add_embedded(&mut self, templates: HashMap<String, String>) {
        for (name, content) in templates {
            self.inline.insert(name, content);
        }
    }

    /// Adds framework templates (lowest priority fallback).
    ///
    /// Framework templates are provided by the standout framework and serve as
    /// defaults that can be overridden by user templates with the same name.
    /// They are checked last during resolution.
    ///
    /// Framework templates typically use the `standout/` namespace to avoid
    /// accidental collision with user templates (e.g., `standout/list-view`).
    ///
    /// # Arguments
    ///
    /// * `name` - The template name (e.g., `"standout/list-view"`)
    /// * `content` - The template content
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// registry.add_framework("standout/list-view", include_str!("templates/list-view.jinja"));
    /// ```
    pub fn add_framework(&mut self, name: impl Into<String>, content: impl Into<String>) {
        self.framework.insert(name.into(), content.into());
    }

    /// Adds multiple framework templates from embedded entries.
    ///
    /// This is similar to [`from_embedded_entries`] but adds templates to the
    /// framework (lowest priority) tier instead of inline (highest priority).
    ///
    /// # Arguments
    ///
    /// * `entries` - Slice of `(name_with_ext, content)` pairs
    pub fn add_framework_entries(&mut self, entries: &[(&str, &str)]) {
        let framework: HashMap<String, String> =
            build_embedded_registry(entries, TEMPLATE_EXTENSIONS, |content| {
                Ok::<_, std::convert::Infallible>(content.to_string())
            })
            .unwrap(); // Safe: Infallible error type

        for (name, content) in framework {
            self.framework.insert(name, content);
        }
    }

    /// Clears all framework templates.
    ///
    /// This is useful when you want to disable all framework-provided defaults
    /// and require explicit template configuration.
    pub fn clear_framework(&mut self) {
        self.framework.clear();
    }

    /// Creates a registry from embedded template entries.
    ///
    /// This is the primary entry point for compile-time embedded templates,
    /// typically called by the `embed_templates!` macro.
    ///
    /// # Arguments
    ///
    /// * `entries` - Slice of `(name_with_ext, content)` pairs where `name_with_ext`
    ///   is the relative path including extension (e.g., `"report/summary.jinja"`)
    ///
    /// # Processing
    ///
    /// This method applies the same logic as runtime file loading:
    ///
    /// 1. Extension stripping: `"report/summary.jinja"` → `"report/summary"`
    /// 2. Extension priority: When multiple files share a base name, the
    ///    higher-priority extension wins (see [`TEMPLATE_EXTENSIONS`])
    /// 3. Dual registration: Each template is accessible by both its base
    ///    name and its full name with extension
    ///
    /// # Example
    ///
    /// ```rust
    /// use standout::TemplateRegistry;
    ///
    /// // Typically generated by embed_templates! macro
    /// let entries: &[(&str, &str)] = &[
    ///     ("list.jinja", "Hello {{ name }}"),
    ///     ("report/summary.jinja", "Report: {{ title }}"),
    /// ];
    ///
    /// let registry = TemplateRegistry::from_embedded_entries(entries);
    ///
    /// // Access by base name or full name
    /// assert!(registry.get("list").is_ok());
    /// assert!(registry.get("list.jinja").is_ok());
    /// assert!(registry.get("report/summary").is_ok());
    /// ```
    pub fn from_embedded_entries(entries: &[(&str, &str)]) -> Self {
        let mut registry = Self::new();

        // Use shared helper - infallible transform for templates
        let inline: HashMap<String, String> =
            build_embedded_registry(entries, TEMPLATE_EXTENSIONS, |content| {
                Ok::<_, std::convert::Infallible>(content.to_string())
            })
            .unwrap(); // Safe: Infallible error type

        registry.inline = inline;
        registry
    }

    /// Looks up a template by name.
    ///
    /// Names can be specified with or without extension:
    /// - `"config"` resolves to `config.jinja` (or highest-priority extension)
    /// - `"config.jinja"` resolves to exactly that file
    ///
    /// # Resolution Priority
    ///
    /// Templates are resolved in this order:
    /// 1. Inline templates (highest priority)
    /// 2. File-based templates from `add_from_files`
    /// 3. Directory-based templates from `add_template_dir`
    /// 4. Framework templates (lowest priority)
    ///
    /// This allows user templates to override framework defaults.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError::NotFound`] if the template doesn't exist.
    pub fn get(&self, name: &str) -> Result<ResolvedTemplate, RegistryError> {
        // Check inline first (highest priority)
        if let Some(content) = self.inline.get(name) {
            return Ok(ResolvedTemplate::Inline(content.clone()));
        }

        // Check file-based templates from add_from_files
        if let Some(path) = self.files.get(name) {
            return Ok(ResolvedTemplate::File(path.clone()));
        }

        // Check directory-based file registry
        if let Some(entry) = self.inner.get_entry(name) {
            return Ok(ResolvedTemplate::from(entry));
        }

        // Check framework templates (lowest priority)
        if let Some(content) = self.framework.get(name) {
            return Ok(ResolvedTemplate::Inline(content.clone()));
        }

        Err(RegistryError::NotFound {
            name: name.to_string(),
        })
    }

    /// Gets the content of a template, reading from disk if necessary.
    ///
    /// For inline templates, returns the stored content directly.
    /// For file templates, reads the file from disk (enabling hot reload).
    ///
    /// # Errors
    ///
    /// Returns an error if the template is not found or cannot be read from disk.
    pub fn get_content(&self, name: &str) -> Result<String, RegistryError> {
        let resolved = self.get(name)?;
        match resolved {
            ResolvedTemplate::Inline(content) => Ok(content),
            ResolvedTemplate::File(path) => {
                std::fs::read_to_string(&path).map_err(|e| RegistryError::ReadError {
                    path,
                    message: e.to_string(),
                })
            }
        }
    }

    /// Refreshes the registry from registered directories.
    ///
    /// This re-walks all registered template directories and rebuilds the
    /// resolution map. Call this if:
    ///
    /// - You've added template directories after the first render
    /// - Template files have been added/removed from disk
    ///
    /// # Panics
    ///
    /// Panics if a collision is detected (same name from different directories).
    pub fn refresh(&mut self) -> Result<(), RegistryError> {
        self.inner.refresh().map_err(RegistryError::from)
    }

    /// Returns the number of registered templates.
    ///
    /// Note: This counts both extensionless and with-extension entries,
    /// so it may be higher than the number of unique template files.
    pub fn len(&self) -> usize {
        self.inline.len() + self.files.len() + self.inner.len() + self.framework.len()
    }

    /// Returns true if no templates are registered.
    pub fn is_empty(&self) -> bool {
        self.inline.is_empty()
            && self.files.is_empty()
            && self.inner.is_empty()
            && self.framework.is_empty()
    }

    /// Returns an iterator over all registered template names.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.inline
            .keys()
            .map(|s| s.as_str())
            .chain(self.files.keys().map(|s| s.as_str()))
            .chain(self.inner.names())
            .chain(self.framework.keys().map(|s| s.as_str()))
    }

    /// Clears all templates from the registry.
    pub fn clear(&mut self) {
        self.inline.clear();
        self.files.clear();
        self.sources.clear();
        self.inner.clear();
        self.framework.clear();
    }

    /// Returns true if the registry has framework templates.
    pub fn has_framework_templates(&self) -> bool {
        !self.framework.is_empty()
    }

    /// Returns an iterator over framework template names.
    pub fn framework_names(&self) -> impl Iterator<Item = &str> {
        self.framework.keys().map(|s| s.as_str())
    }
}

/// Walks a template directory and collects template files.
///
/// This function traverses the directory recursively, finding all files
/// with recognized template extensions ([`TEMPLATE_EXTENSIONS`]).
///
/// # Arguments
///
/// * `root` - The template directory root to walk
///
/// # Returns
///
/// A vector of [`TemplateFile`] entries, one for each discovered template.
/// The vector is not sorted; use [`TemplateFile::extension_priority`] for ordering.
///
/// # Errors
///
/// Returns an error if the directory cannot be read or traversed.
///
/// # Example
///
/// ```rust,ignore
/// let files = walk_template_dir("./templates")?;
/// for file in &files {
///     println!("{} -> {}", file.name, file.absolute_path.display());
/// }
/// ```
pub fn walk_template_dir(root: impl AsRef<Path>) -> Result<Vec<TemplateFile>, std::io::Error> {
    let files = file_loader::walk_dir(root.as_ref(), TEMPLATE_EXTENSIONS)
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    Ok(files.into_iter().map(TemplateFile::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // TemplateFile tests
    // =========================================================================

    #[test]
    fn test_template_file_extension_priority() {
        let jinja = TemplateFile::new("config", "config.jinja", "/a/config.jinja", "/a");
        let jinja2 = TemplateFile::new("config", "config.jinja2", "/a/config.jinja2", "/a");
        let j2 = TemplateFile::new("config", "config.j2", "/a/config.j2", "/a");
        let txt = TemplateFile::new("config", "config.txt", "/a/config.txt", "/a");
        let unknown = TemplateFile::new("config", "config.xyz", "/a/config.xyz", "/a");

        assert_eq!(jinja.extension_priority(), 0);
        assert_eq!(jinja2.extension_priority(), 1);
        assert_eq!(j2.extension_priority(), 2);
        assert_eq!(txt.extension_priority(), 3);
        assert_eq!(unknown.extension_priority(), usize::MAX);
    }

    // =========================================================================
    // TemplateRegistry inline tests
    // =========================================================================

    #[test]
    fn test_registry_add_inline() {
        let mut registry = TemplateRegistry::new();
        registry.add_inline("header", "{{ title }}");

        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());

        let content = registry.get_content("header").unwrap();
        assert_eq!(content, "{{ title }}");
    }

    #[test]
    fn test_registry_inline_overwrites() {
        let mut registry = TemplateRegistry::new();
        registry.add_inline("header", "first");
        registry.add_inline("header", "second");

        let content = registry.get_content("header").unwrap();
        assert_eq!(content, "second");
    }

    #[test]
    fn test_registry_not_found() {
        let registry = TemplateRegistry::new();
        let result = registry.get("nonexistent");

        assert!(matches!(result, Err(RegistryError::NotFound { .. })));
    }

    // =========================================================================
    // File-based template tests (using synthetic data)
    // =========================================================================

    #[test]
    fn test_registry_add_from_files() {
        let mut registry = TemplateRegistry::new();

        let files = vec![
            TemplateFile::new(
                "config",
                "config.jinja",
                "/templates/config.jinja",
                "/templates",
            ),
            TemplateFile::new(
                "todos/list",
                "todos/list.jinja",
                "/templates/todos/list.jinja",
                "/templates",
            ),
        ];

        registry.add_from_files(files).unwrap();

        // Should have 4 entries: 2 names + 2 names with extension
        assert_eq!(registry.len(), 4);

        // Can access by name without extension
        assert!(registry.get("config").is_ok());
        assert!(registry.get("todos/list").is_ok());

        // Can access by name with extension
        assert!(registry.get("config.jinja").is_ok());
        assert!(registry.get("todos/list.jinja").is_ok());
    }

    #[test]
    fn test_registry_extension_priority() {
        let mut registry = TemplateRegistry::new();

        // Add files with different extensions for same base name
        // (j2 should be ignored because jinja has higher priority)
        let files = vec![
            TemplateFile::new("config", "config.j2", "/templates/config.j2", "/templates"),
            TemplateFile::new(
                "config",
                "config.jinja",
                "/templates/config.jinja",
                "/templates",
            ),
        ];

        registry.add_from_files(files).unwrap();

        // Extensionless name should resolve to .jinja
        let resolved = registry.get("config").unwrap();
        match resolved {
            ResolvedTemplate::File(path) => {
                assert!(path.to_string_lossy().ends_with("config.jinja"));
            }
            _ => panic!("Expected file template"),
        }
    }

    #[test]
    fn test_registry_collision_different_dirs() {
        let mut registry = TemplateRegistry::new();

        let files = vec![
            TemplateFile::new(
                "config",
                "config.jinja",
                "/app/templates/config.jinja",
                "/app/templates",
            ),
            TemplateFile::new(
                "config",
                "config.jinja",
                "/plugins/templates/config.jinja",
                "/plugins/templates",
            ),
        ];

        let result = registry.add_from_files(files);

        assert!(matches!(result, Err(RegistryError::Collision { .. })));

        if let Err(RegistryError::Collision { name, .. }) = result {
            assert_eq!(name, "config");
        }
    }

    #[test]
    fn test_registry_inline_shadows_file() {
        let mut registry = TemplateRegistry::new();

        // Add file-based template first
        let files = vec![TemplateFile::new(
            "config",
            "config.jinja",
            "/templates/config.jinja",
            "/templates",
        )];
        registry.add_from_files(files).unwrap();

        // Add inline with same name (should shadow)
        registry.add_inline("config", "inline content");

        let content = registry.get_content("config").unwrap();
        assert_eq!(content, "inline content");
    }

    #[test]
    fn test_registry_names_iterator() {
        let mut registry = TemplateRegistry::new();
        registry.add_inline("a", "content a");
        registry.add_inline("b", "content b");

        let names: Vec<&str> = registry.names().collect();
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));
    }

    #[test]
    fn test_registry_clear() {
        let mut registry = TemplateRegistry::new();
        registry.add_inline("a", "content");

        assert!(!registry.is_empty());
        registry.clear();
        assert!(registry.is_empty());
    }

    // =========================================================================
    // Error display tests
    // =========================================================================

    #[test]
    fn test_error_display_collision() {
        let err = RegistryError::Collision {
            name: "config".to_string(),
            existing_path: PathBuf::from("/a/config.jinja"),
            existing_dir: PathBuf::from("/a"),
            conflicting_path: PathBuf::from("/b/config.jinja"),
            conflicting_dir: PathBuf::from("/b"),
        };

        let display = err.to_string();
        assert!(display.contains("config"));
        assert!(display.contains("/a/config.jinja"));
        assert!(display.contains("/b/config.jinja"));
    }

    #[test]
    fn test_error_display_not_found() {
        let err = RegistryError::NotFound {
            name: "missing".to_string(),
        };

        let display = err.to_string();
        assert!(display.contains("missing"));
    }

    // =========================================================================
    // from_embedded_entries tests
    // =========================================================================

    #[test]
    fn test_from_embedded_entries_single() {
        let entries: &[(&str, &str)] = &[("hello.jinja", "Hello {{ name }}")];
        let registry = TemplateRegistry::from_embedded_entries(entries);

        // Should be accessible by both names
        assert!(registry.get("hello").is_ok());
        assert!(registry.get("hello.jinja").is_ok());

        let content = registry.get_content("hello").unwrap();
        assert_eq!(content, "Hello {{ name }}");
    }

    #[test]
    fn test_from_embedded_entries_multiple() {
        let entries: &[(&str, &str)] = &[
            ("header.jinja", "{{ title }}"),
            ("footer.jinja", "Copyright {{ year }}"),
        ];
        let registry = TemplateRegistry::from_embedded_entries(entries);

        assert_eq!(registry.len(), 4); // 2 base + 2 with ext
        assert!(registry.get("header").is_ok());
        assert!(registry.get("footer").is_ok());
    }

    #[test]
    fn test_from_embedded_entries_nested_paths() {
        let entries: &[(&str, &str)] = &[
            ("report/summary.jinja", "Summary: {{ text }}"),
            ("report/details.jinja", "Details: {{ info }}"),
        ];
        let registry = TemplateRegistry::from_embedded_entries(entries);

        assert!(registry.get("report/summary").is_ok());
        assert!(registry.get("report/summary.jinja").is_ok());
        assert!(registry.get("report/details").is_ok());
    }

    #[test]
    fn test_from_embedded_entries_extension_priority() {
        // .jinja has higher priority than .txt (index 0 vs index 3)
        let entries: &[(&str, &str)] = &[
            ("config.txt", "txt content"),
            ("config.jinja", "jinja content"),
        ];
        let registry = TemplateRegistry::from_embedded_entries(entries);

        // Base name should resolve to higher priority (.jinja)
        let content = registry.get_content("config").unwrap();
        assert_eq!(content, "jinja content");

        // Both can still be accessed by full name
        assert_eq!(registry.get_content("config.txt").unwrap(), "txt content");
        assert_eq!(
            registry.get_content("config.jinja").unwrap(),
            "jinja content"
        );
    }

    #[test]
    fn test_from_embedded_entries_extension_priority_reverse_order() {
        // Same test but with entries in reverse order to ensure sorting works
        let entries: &[(&str, &str)] = &[
            ("config.jinja", "jinja content"),
            ("config.txt", "txt content"),
        ];
        let registry = TemplateRegistry::from_embedded_entries(entries);

        // Base name should still resolve to higher priority (.jinja)
        let content = registry.get_content("config").unwrap();
        assert_eq!(content, "jinja content");
    }

    #[test]
    fn test_from_embedded_entries_names_iterator() {
        let entries: &[(&str, &str)] = &[("a.jinja", "content a"), ("nested/b.jinja", "content b")];
        let registry = TemplateRegistry::from_embedded_entries(entries);

        let names: Vec<&str> = registry.names().collect();
        assert!(names.contains(&"a"));
        assert!(names.contains(&"a.jinja"));
        assert!(names.contains(&"nested/b"));
        assert!(names.contains(&"nested/b.jinja"));
    }

    #[test]
    fn test_from_embedded_entries_empty() {
        let entries: &[(&str, &str)] = &[];
        let registry = TemplateRegistry::from_embedded_entries(entries);

        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_extensionless_includes_work() {
        // Simulates the user's report: {% include "_partial" %} should work
        // when the file is actually "_partial.jinja"
        let entries: &[(&str, &str)] = &[
            ("main.jinja", "Start {% include '_partial' %} End"),
            ("_partial.jinja", "PARTIAL_CONTENT"),
        ];
        let registry = TemplateRegistry::from_embedded_entries(entries);

        // Build MiniJinja environment the same way App.render() does
        let mut env = minijinja::Environment::new();
        for name in registry.names() {
            if let Ok(content) = registry.get_content(name) {
                env.add_template_owned(name.to_string(), content).unwrap();
            }
        }

        // Verify extensionless include works
        let tmpl = env.get_template("main").unwrap();
        let output = tmpl.render(()).unwrap();
        assert_eq!(output, "Start PARTIAL_CONTENT End");
    }

    #[test]
    fn test_extensionless_includes_with_extension_syntax() {
        // Also verify that {% include "_partial.jinja" %} works
        let entries: &[(&str, &str)] = &[
            ("main.jinja", "Start {% include '_partial.jinja' %} End"),
            ("_partial.jinja", "PARTIAL_CONTENT"),
        ];
        let registry = TemplateRegistry::from_embedded_entries(entries);

        let mut env = minijinja::Environment::new();
        for name in registry.names() {
            if let Ok(content) = registry.get_content(name) {
                env.add_template_owned(name.to_string(), content).unwrap();
            }
        }

        let tmpl = env.get_template("main").unwrap();
        let output = tmpl.render(()).unwrap();
        assert_eq!(output, "Start PARTIAL_CONTENT End");
    }

    // =========================================================================
    // Framework templates tests
    // =========================================================================

    #[test]
    fn test_framework_add_and_get() {
        let mut registry = TemplateRegistry::new();
        registry.add_framework("standout/list-view", "Framework list view");

        assert!(registry.has_framework_templates());
        let content = registry.get_content("standout/list-view").unwrap();
        assert_eq!(content, "Framework list view");
    }

    #[test]
    fn test_framework_lowest_priority() {
        let mut registry = TemplateRegistry::new();

        // Add framework template
        registry.add_framework("config", "framework content");

        // Add inline template with same name (should shadow)
        registry.add_inline("config", "inline content");

        // Inline should win
        let content = registry.get_content("config").unwrap();
        assert_eq!(content, "inline content");
    }

    #[test]
    fn test_framework_user_can_override() {
        let mut registry = TemplateRegistry::new();

        // Add framework template in standout/ namespace
        registry.add_framework("standout/list-view", "framework default");

        // User creates their own version
        registry.add_inline("standout/list-view", "user override");

        // User version should win
        let content = registry.get_content("standout/list-view").unwrap();
        assert_eq!(content, "user override");
    }

    #[test]
    fn test_framework_entries() {
        let mut registry = TemplateRegistry::new();

        let entries: &[(&str, &str)] = &[
            ("standout/list-view.jinja", "List view content"),
            ("standout/detail-view.jinja", "Detail view content"),
        ];

        registry.add_framework_entries(entries);

        // Should be accessible by both names
        assert!(registry.get("standout/list-view").is_ok());
        assert!(registry.get("standout/list-view.jinja").is_ok());
        assert!(registry.get("standout/detail-view").is_ok());
    }

    #[test]
    fn test_framework_names_iterator() {
        let mut registry = TemplateRegistry::new();
        registry.add_framework("standout/a", "content a");
        registry.add_framework("standout/b", "content b");

        let names: Vec<&str> = registry.framework_names().collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"standout/a"));
        assert!(names.contains(&"standout/b"));
    }

    #[test]
    fn test_framework_clear() {
        let mut registry = TemplateRegistry::new();
        registry.add_framework("standout/list-view", "content");

        assert!(registry.has_framework_templates());

        registry.clear_framework();

        assert!(!registry.has_framework_templates());
        assert!(registry.get("standout/list-view").is_err());
    }

    #[test]
    fn test_framework_included_in_len_and_names() {
        let mut registry = TemplateRegistry::new();
        registry.add_inline("user-template", "user content");
        registry.add_framework("standout/framework", "framework content");

        // Both should be counted
        assert_eq!(registry.len(), 2);

        let names: Vec<&str> = registry.names().collect();
        assert!(names.contains(&"user-template"));
        assert!(names.contains(&"standout/framework"));
    }

    #[test]
    fn test_framework_clear_all_clears_framework() {
        let mut registry = TemplateRegistry::new();
        registry.add_framework("standout/test", "content");

        registry.clear();

        assert!(registry.is_empty());
        assert!(!registry.has_framework_templates());
    }
}
