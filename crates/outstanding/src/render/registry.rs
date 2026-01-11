//! Template registry for file-based and inline templates.
//!
//! This module provides [`TemplateRegistry`], which manages template resolution
//! from multiple sources: inline strings, filesystem directories, or embedded content.
//!
//! # Design
//!
//! The registry uses a two-phase approach:
//!
//! 1. **Collection**: Templates are collected from various sources (inline, directories, embedded)
//! 2. **Resolution**: A unified map resolves template names to their content or file paths
//!
//! This separation enables:
//! - **Testability**: Resolution logic can be tested without filesystem access
//! - **Flexibility**: Same resolution rules apply regardless of template source
//! - **Hot reloading**: File paths can be re-read on each render in development mode
//!
//! # Template Resolution
//!
//! Templates are resolved by name using these rules:
//!
//! 1. **Inline templates** (added via [`TemplateRegistry::add_inline`]) have highest priority
//! 2. **File templates** are searched in directory registration order (first directory wins)
//! 3. Names can be specified with or without extension: both `"config"` and `"config.tmpl"` resolve
//!
//! # Supported Extensions
//!
//! Template files are recognized by extension, in priority order:
//!
//! | Priority | Extension | Description |
//! |----------|-----------|-------------|
//! | 1 (highest) | `.tmpl` | Recommended extension |
//! | 2 | `.jinja2` | Jinja2 compatibility |
//! | 3 (lowest) | `.j2` | Short Jinja2 extension |
//!
//! If multiple files exist with the same base name but different extensions
//! (e.g., `config.tmpl` and `config.j2`), the higher-priority extension wins.
//!
//! # Collision Handling
//!
//! The registry enforces strict collision rules:
//!
//! - **Same-directory, different extensions**: Higher priority extension wins (no error)
//! - **Cross-directory collisions**: Error with detailed message listing conflicting files
//!
//! This strict behavior catches configuration mistakes early rather than silently
//! using an arbitrary winner.
//!
//! # Example
//!
//! ```rust,ignore
//! use outstanding::render::TemplateRegistry;
//!
//! // Build from collected template files
//! let files = vec![
//!     TemplateFile::new("config", "config.tmpl", "/app/templates/config.tmpl"),
//!     TemplateFile::new("todos/list", "todos/list.tmpl", "/app/templates/todos/list.tmpl"),
//! ];
//!
//! let mut registry = TemplateRegistry::new();
//! registry.add_from_files(files, "/app/templates")?;
//! registry.add_inline("override", "Custom content");
//!
//! // Resolve templates
//! let content = registry.get_content("config")?;
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Recognized template file extensions in priority order.
///
/// When multiple files exist with the same base name but different extensions,
/// the extension appearing earlier in this list takes precedence.
///
/// # Priority Order
///
/// 1. `.tmpl` - Recommended, unambiguous extension
/// 2. `.jinja2` - Full Jinja2 extension for compatibility
/// 3. `.j2` - Short Jinja2 extension
pub const TEMPLATE_EXTENSIONS: &[&str] = &[".tmpl", ".jinja2", ".j2"];

/// A template file discovered during directory walking.
///
/// This struct captures the essential information about a template file
/// without reading its content, enabling lazy loading and hot reloading.
///
/// # Fields
///
/// - `name`: The resolution name without extension (e.g., `"todos/list"`)
/// - `name_with_ext`: The resolution name with extension (e.g., `"todos/list.tmpl"`)
/// - `absolute_path`: Full filesystem path for reading content
/// - `source_dir`: The template directory this file came from (for collision reporting)
///
/// # Example
///
/// For a file at `/app/templates/todos/list.tmpl` with root `/app/templates`:
///
/// ```rust,ignore
/// TemplateFile {
///     name: "todos/list".to_string(),
///     name_with_ext: "todos/list.tmpl".to_string(),
///     absolute_path: PathBuf::from("/app/templates/todos/list.tmpl"),
///     source_dir: PathBuf::from("/app/templates"),
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateFile {
    /// Resolution name without extension (e.g., "config" or "todos/list")
    pub name: String,
    /// Resolution name with extension (e.g., "config.tmpl" or "todos/list.tmpl")
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
/// // Add from directory scan
/// let files = walk_template_dir("./templates")?;
/// registry.add_from_files(files)?;
///
/// // Resolve and get content
/// let content = registry.get_content("header")?;
/// ```
#[derive(Debug, Clone, Default)]
pub struct TemplateRegistry {
    /// Map from template name to resolved template.
    ///
    /// Names are stored both with and without extension for flexible lookup.
    /// For example, "config.tmpl" creates entries for both "config" and "config.tmpl".
    templates: HashMap<String, ResolvedTemplate>,

    /// Tracks which source directory each template came from.
    ///
    /// Used for collision detection when adding templates from multiple directories.
    /// Key is the canonical name (without extension), value is (path, source_dir).
    sources: HashMap<String, (PathBuf, PathBuf)>,
}

impl TemplateRegistry {
    /// Creates an empty template registry.
    pub fn new() -> Self {
        Self::default()
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
        let name = name.into();
        let content = content.into();
        self.templates
            .insert(name, ResolvedTemplate::Inline(content));
    }

    /// Adds templates discovered from a directory scan.
    ///
    /// This method processes a list of [`TemplateFile`] entries, typically
    /// produced by [`walk_template_dir`], and registers them for resolution.
    ///
    /// # Resolution Names
    ///
    /// Each file is registered under two names:
    /// - Without extension: `"config"` for `config.tmpl`
    /// - With extension: `"config.tmpl"` for `config.tmpl`
    ///
    /// # Extension Priority
    ///
    /// If multiple files share the same base name with different extensions
    /// (e.g., `config.tmpl` and `config.j2`), the higher-priority extension wins
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

            // Register the template
            let resolved = ResolvedTemplate::File(file.absolute_path.clone());

            // Add under extensionless name
            self.templates.insert(file.name.clone(), resolved.clone());
            self.sources.insert(
                file.name.clone(),
                (file.absolute_path.clone(), file.source_dir.clone()),
            );

            // Add under name with extension (allows explicit access)
            self.templates.insert(file.name_with_ext.clone(), resolved);
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
            self.templates
                .insert(name, ResolvedTemplate::Inline(content));
        }
    }

    /// Looks up a template by name.
    ///
    /// Names can be specified with or without extension:
    /// - `"config"` resolves to `config.tmpl` (or highest-priority extension)
    /// - `"config.tmpl"` resolves to exactly that file
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError::NotFound`] if the template doesn't exist.
    pub fn get(&self, name: &str) -> Result<&ResolvedTemplate, RegistryError> {
        self.templates
            .get(name)
            .ok_or_else(|| RegistryError::NotFound {
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
            ResolvedTemplate::Inline(content) => Ok(content.clone()),
            ResolvedTemplate::File(path) => {
                std::fs::read_to_string(path).map_err(|e| RegistryError::ReadError {
                    path: path.clone(),
                    message: e.to_string(),
                })
            }
        }
    }

    /// Returns the number of registered templates.
    ///
    /// Note: This counts both extensionless and with-extension entries,
    /// so it may be higher than the number of unique template files.
    pub fn len(&self) -> usize {
        self.templates.len()
    }

    /// Returns true if no templates are registered.
    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }

    /// Returns an iterator over all registered template names.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.templates.keys().map(|s| s.as_str())
    }

    /// Clears all templates from the registry.
    pub fn clear(&mut self) {
        self.templates.clear();
        self.sources.clear();
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
    let root = root.as_ref();
    let root_canonical = root.canonicalize()?;
    let mut files = Vec::new();

    walk_dir_recursive(&root_canonical, &root_canonical, &mut files)?;

    Ok(files)
}

/// Recursive helper for directory walking.
fn walk_dir_recursive(
    current: &Path,
    root: &Path,
    files: &mut Vec<TemplateFile>,
) -> Result<(), std::io::Error> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            walk_dir_recursive(&path, root, files)?;
        } else if path.is_file() {
            // Check if this file has a recognized template extension
            if let Some(template_file) = try_parse_template_file(&path, root) {
                files.push(template_file);
            }
        }
    }

    Ok(())
}

/// Attempts to parse a file path as a template file.
///
/// Returns `None` if the file doesn't have a recognized template extension.
fn try_parse_template_file(path: &Path, root: &Path) -> Option<TemplateFile> {
    let path_str = path.to_string_lossy();

    // Find which extension this file has
    let extension = TEMPLATE_EXTENSIONS
        .iter()
        .find(|ext| path_str.ends_with(*ext))?;

    // Compute relative path from root
    let relative = path.strip_prefix(root).ok()?;
    let relative_str = relative.to_string_lossy();

    // Name with extension (using forward slashes for consistency)
    let name_with_ext = relative_str.replace(std::path::MAIN_SEPARATOR, "/");

    // Name without extension
    let name = name_with_ext.strip_suffix(extension)?.to_string();

    Some(TemplateFile::new(name, name_with_ext, path, root))
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // TemplateFile tests
    // =========================================================================

    #[test]
    fn test_template_file_extension_priority() {
        let tmpl = TemplateFile::new("config", "config.tmpl", "/a/config.tmpl", "/a");
        let jinja2 = TemplateFile::new("config", "config.jinja2", "/a/config.jinja2", "/a");
        let j2 = TemplateFile::new("config", "config.j2", "/a/config.j2", "/a");
        let unknown = TemplateFile::new("config", "config.txt", "/a/config.txt", "/a");

        assert_eq!(tmpl.extension_priority(), 0);
        assert_eq!(jinja2.extension_priority(), 1);
        assert_eq!(j2.extension_priority(), 2);
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
                "config.tmpl",
                "/templates/config.tmpl",
                "/templates",
            ),
            TemplateFile::new(
                "todos/list",
                "todos/list.tmpl",
                "/templates/todos/list.tmpl",
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
        assert!(registry.get("config.tmpl").is_ok());
        assert!(registry.get("todos/list.tmpl").is_ok());
    }

    #[test]
    fn test_registry_extension_priority() {
        let mut registry = TemplateRegistry::new();

        // Add files with different extensions for same base name
        // (j2 should be ignored because tmpl has higher priority)
        let files = vec![
            TemplateFile::new("config", "config.j2", "/templates/config.j2", "/templates"),
            TemplateFile::new(
                "config",
                "config.tmpl",
                "/templates/config.tmpl",
                "/templates",
            ),
        ];

        registry.add_from_files(files).unwrap();

        // Extensionless name should resolve to .tmpl
        let resolved = registry.get("config").unwrap();
        match resolved {
            ResolvedTemplate::File(path) => {
                assert!(path.to_string_lossy().ends_with("config.tmpl"));
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
                "config.tmpl",
                "/app/templates/config.tmpl",
                "/app/templates",
            ),
            TemplateFile::new(
                "config",
                "config.tmpl",
                "/plugins/templates/config.tmpl",
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
            "config.tmpl",
            "/templates/config.tmpl",
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
    fn test_error_display_not_found() {
        let err = RegistryError::NotFound {
            name: "missing".to_string(),
        };

        let display = err.to_string();
        assert!(display.contains("missing"));
    }
}
