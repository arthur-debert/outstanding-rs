//! Embedded resource source types for compile-time embedding with debug hot-reload.
//!
//! This module provides types that hold both embedded content (for release builds)
//! and source paths (for debug hot-reload). The macros `embed_templates!` and
//! `embed_styles!` return these types, and `App::builder()` consumes them.
//!
//! # Design
//!
//! The key insight is that we want:
//! - **Release builds**: Use embedded content, zero file I/O
//! - **Debug builds**: Hot-reload from disk if source path exists
//!
//! By storing both the embedded content AND the source path, we can make this
//! decision at runtime based on `cfg!(debug_assertions)` and path existence.
//!
//! # Example
//!
//! ```rust,ignore
//! use outstanding::{embed_templates, embed_styles};
//! use outstanding::cli::App;
//!
//! let app = App::builder()
//!     .templates(embed_templates!("src/templates"))
//!     .styles(embed_styles!("src/styles"))
//!     .build()?;
//!
//! // In debug: reads from "src/templates" if it exists
//! // In release: uses embedded content
//! let output = app.render("list", &data, OutputMode::Term)?;
//! ```

use std::marker::PhantomData;
use std::path::Path;

use crate::file_loader::{build_embedded_registry, walk_dir};
use crate::rendering::style::{StylesheetRegistry, STYLESHEET_EXTENSIONS};
use crate::rendering::template::{walk_template_dir, TemplateRegistry};
use crate::rendering::theme::Theme;

/// Marker type for template resources.
#[derive(Debug, Clone, Copy)]
pub struct TemplateResource;

/// Marker type for stylesheet resources.
#[derive(Debug, Clone, Copy)]
pub struct StylesheetResource;

/// Embedded resource source with optional debug hot-reload.
///
/// This type holds:
/// - Embedded entries (name, content) pairs baked in at compile time
/// - The source path for debug hot-reload
///
/// The type parameter `R` is a marker indicating the resource type
/// (templates or stylesheets).
#[derive(Debug, Clone)]
pub struct EmbeddedSource<R> {
    /// The embedded entries as (name_with_extension, content) pairs.
    /// This is `'static` because it's baked into the binary at compile time.
    pub entries: &'static [(&'static str, &'static str)],

    /// The source path used for embedding.
    /// In debug mode, if this path exists, files are read from disk instead.
    pub source_path: &'static str,

    /// Marker for the resource type.
    _marker: PhantomData<R>,
}

impl<R> EmbeddedSource<R> {
    /// Creates a new embedded source.
    ///
    /// This is typically called by the `embed_templates!` and `embed_styles!` macros.
    #[doc(hidden)]
    pub const fn new(
        entries: &'static [(&'static str, &'static str)],
        source_path: &'static str,
    ) -> Self {
        Self {
            entries,
            source_path,
            _marker: PhantomData,
        }
    }

    /// Returns the embedded entries.
    pub fn entries(&self) -> &'static [(&'static str, &'static str)] {
        self.entries
    }

    /// Returns the source path.
    pub fn source_path(&self) -> &'static str {
        self.source_path
    }

    /// Returns true if hot-reload should be used.
    ///
    /// Hot-reload is enabled when:
    /// - We're in debug mode (`debug_assertions` enabled)
    /// - The source path exists on disk
    pub fn should_hot_reload(&self) -> bool {
        cfg!(debug_assertions) && std::path::Path::new(self.source_path).exists()
    }
}

/// Type alias for embedded templates.
pub type EmbeddedTemplates = EmbeddedSource<TemplateResource>;

/// Type alias for embedded stylesheets.
pub type EmbeddedStyles = EmbeddedSource<StylesheetResource>;

impl From<EmbeddedTemplates> for TemplateRegistry {
    /// Converts embedded templates into a TemplateRegistry.
    ///
    /// In debug mode, if the source path exists, templates are loaded from disk
    /// (enabling hot-reload). Otherwise, embedded content is used.
    fn from(source: EmbeddedTemplates) -> Self {
        if source.should_hot_reload() {
            // Debug mode with existing source path: load from filesystem
            // Use walk_template_dir + add_from_files for immediate loading
            // (add_template_dir uses lazy loading which doesn't work well here)
            let files = match walk_template_dir(source.source_path) {
                Ok(files) => files,
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to walk templates directory '{}', using embedded: {}",
                        source.source_path, e
                    );
                    return TemplateRegistry::from_embedded_entries(source.entries);
                }
            };

            let mut registry = TemplateRegistry::new();
            if let Err(e) = registry.add_from_files(files) {
                eprintln!(
                    "Warning: Failed to register templates from '{}', using embedded: {}",
                    source.source_path, e
                );
                return TemplateRegistry::from_embedded_entries(source.entries);
            }
            registry
        } else {
            // Release mode or missing source: use embedded content
            TemplateRegistry::from_embedded_entries(source.entries)
        }
    }
}

impl From<EmbeddedStyles> for StylesheetRegistry {
    /// Converts embedded styles into a StylesheetRegistry.
    ///
    /// In debug mode, if the source path exists, styles are loaded from disk
    /// (enabling hot-reload). Otherwise, embedded content is used.
    ///
    /// # Panics
    ///
    /// Panics if embedded YAML content fails to parse (should be caught in dev).
    fn from(source: EmbeddedStyles) -> Self {
        if source.should_hot_reload() {
            // Debug mode with existing source path: load from filesystem
            // Walk directory and load immediately (add_dir uses lazy loading which
            // doesn't work well for names() iteration)
            let files = match walk_dir(Path::new(source.source_path), STYLESHEET_EXTENSIONS) {
                Ok(files) => files,
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to walk styles directory '{}', using embedded: {}",
                        source.source_path, e
                    );
                    return StylesheetRegistry::from_embedded_entries(source.entries)
                        .expect("embedded stylesheets should parse");
                }
            };

            // Read file contents into (name_with_ext, content) pairs
            let entries: Vec<(String, String)> = files
                .into_iter()
                .filter_map(|file| match std::fs::read_to_string(&file.path) {
                    Ok(content) => Some((file.name_with_ext, content)),
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to read stylesheet '{}': {}",
                            file.path.display(),
                            e
                        );
                        None
                    }
                })
                .collect();

            // Build registry with extension priority handling
            let entries_refs: Vec<(&str, &str)> = entries
                .iter()
                .map(|(n, c)| (n.as_str(), c.as_str()))
                .collect();

            let inline =
                match build_embedded_registry(&entries_refs, STYLESHEET_EXTENSIONS, |yaml| {
                    Theme::from_yaml(yaml)
                }) {
                    Ok(map) => map,
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to parse stylesheets from '{}', using embedded: {}",
                            source.source_path, e
                        );
                        return StylesheetRegistry::from_embedded_entries(source.entries)
                            .expect("embedded stylesheets should parse");
                    }
                };

            let mut registry = StylesheetRegistry::new();
            registry.add_embedded(inline);
            registry
        } else {
            // Release mode or missing source: use embedded content
            StylesheetRegistry::from_embedded_entries(source.entries)
                .expect("embedded stylesheets should parse")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_source_new() {
        static ENTRIES: &[(&str, &str)] = &[("test.jinja", "content")];
        let source: EmbeddedTemplates = EmbeddedSource::new(ENTRIES, "src/templates");

        assert_eq!(source.entries().len(), 1);
        assert_eq!(source.source_path(), "src/templates");
    }

    #[test]
    fn test_should_hot_reload_nonexistent_path() {
        static ENTRIES: &[(&str, &str)] = &[];
        let source: EmbeddedTemplates = EmbeddedSource::new(ENTRIES, "/nonexistent/path");

        // Should be false because path doesn't exist
        assert!(!source.should_hot_reload());
    }
}
