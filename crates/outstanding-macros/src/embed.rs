//! Compile-time resource embedding macros.
//!
//! This module provides proc macros that walk directories at compile time and
//! embed file contents into the binary. The macros are intentionally "dumb" -
//! they only handle file discovery and content reading. All smart logic
//! (extension priority, name normalization, collision detection) lives in
//! the `outstanding` crate's registries.
//!
//! # Design
//!
//! The macros produce raw `(name_with_extension, content)` pairs and delegate
//! to `from_embedded_entries()` methods in the outstanding crate. This design:
//!
//! - **Avoids duplication**: Priority/collision logic lives in one place
//! - **Simplifies debugging**: Macros just read files, easier to troubleshoot
//! - **Ensures consistency**: Same logic for runtime and compile-time loading
//!
//! # Relationship to file_loader
//!
//! These macros are the compile-time counterpart to runtime file loading.
//! See [`outstanding::file_loader`] for the full file loading infrastructure
//! and [`outstanding::TemplateRegistry`] / [`outstanding::stylesheet::StylesheetRegistry`]
//! for the registry APIs that handle both runtime and embedded resources.
//!
//! # Example
//!
//! ```rust,ignore
//! use outstanding::embed_templates;
//!
//! // At compile time: walks directory, reads files
//! // At runtime: registry applies extension priority and normalization
//! let templates = embed_templates!("./templates");
//! let content = templates.get_content("report/summary")?;
//! ```

use proc_macro2::TokenStream;
use quote::quote;
use std::path::{Path, PathBuf};
use syn::LitStr;

/// Template file extensions (must match outstanding::render::registry::TEMPLATE_EXTENSIONS).
pub const TEMPLATE_EXTENSIONS: &[&str] = &[".jinja", ".jinja2", ".j2", ".txt"];

/// Stylesheet file extensions (must match outstanding::stylesheet::STYLESHEET_EXTENSIONS).
pub const STYLESHEET_EXTENSIONS: &[&str] = &[".yaml", ".yml"];

/// Generates code to create an EmbeddedTemplates source.
///
/// This function:
/// 1. Walks the directory at compile time
/// 2. Collects all files matching template extensions
/// 3. Generates an `EmbeddedSource<TemplateResource>` with entries and source path
///
/// The returned `EmbeddedSource` can be passed to `RenderSetup` or converted
/// to a `TemplateRegistry` via `into()`.
pub fn embed_templates_impl(input: LitStr) -> TokenStream {
    let source_path = input.value();
    let dir_path = resolve_path(&source_path);

    let files = match collect_files(&dir_path, TEMPLATE_EXTENSIONS) {
        Ok(files) => files,
        Err(e) => {
            return syn::Error::new(input.span(), e).to_compile_error();
        }
    };

    // Store the absolute path for runtime hot-reload to work correctly
    let absolute_path = dir_path.to_string_lossy().to_string();

    // Generate array of (name_with_ext, content) tuples
    let entries: Vec<_> = files
        .iter()
        .map(|(name, content)| {
            quote! { (#name, #content) }
        })
        .collect();

    quote! {
        {
            static ENTRIES: &[(&str, &str)] = &[
                #(#entries),*
            ];
            ::outstanding::EmbeddedSource::<::outstanding::TemplateResource>::new(
                ENTRIES,
                #absolute_path,
            )
        }
    }
}

/// Generates code to create an EmbeddedStyles source.
///
/// This function:
/// 1. Walks the directory at compile time
/// 2. Collects all files matching stylesheet extensions
/// 3. Generates an `EmbeddedSource<StylesheetResource>` with entries and source path
///
/// The returned `EmbeddedSource` can be passed to `RenderSetup` or converted
/// to a `StylesheetRegistry` via `into()`.
pub fn embed_styles_impl(input: LitStr) -> TokenStream {
    let source_path = input.value();
    let dir_path = resolve_path(&source_path);

    let files = match collect_files(&dir_path, STYLESHEET_EXTENSIONS) {
        Ok(files) => files,
        Err(e) => {
            return syn::Error::new(input.span(), e).to_compile_error();
        }
    };

    // Store the absolute path for runtime hot-reload to work correctly
    let absolute_path = dir_path.to_string_lossy().to_string();

    // Generate array of (name_with_ext, content) tuples
    let entries: Vec<_> = files
        .iter()
        .map(|(name, content)| {
            quote! { (#name, #content) }
        })
        .collect();

    quote! {
        {
            static ENTRIES: &[(&str, &str)] = &[
                #(#entries),*
            ];
            ::outstanding::EmbeddedSource::<::outstanding::StylesheetResource>::new(
                ENTRIES,
                #absolute_path,
            )
        }
    }
}

/// Resolves a path relative to the crate's manifest directory.
///
/// CARGO_MANIFEST_DIR is set during compilation to the directory containing
/// the Cargo.toml of the crate being compiled (not the proc-macro crate).
fn resolve_path(path: &str) -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR should be set during compilation");
    Path::new(&manifest_dir).join(path)
}

/// Collects all files from a directory with matching extensions.
///
/// Returns a vector of (name_with_ext, content) pairs where name_with_ext
/// is the relative path from root INCLUDING the extension (e.g., "themes/dark.yaml").
///
/// NO extension stripping or priority logic is done here - that's the registry's job.
fn collect_files(dir: &Path, extensions: &[&str]) -> Result<Vec<(String, String)>, String> {
    if !dir.exists() {
        return Err(format!("Directory not found: {}", dir.display()));
    }
    if !dir.is_dir() {
        return Err(format!("Path is not a directory: {}", dir.display()));
    }

    let mut files = Vec::new();
    collect_files_recursive(dir, dir, extensions, &mut files)?;

    // Sort for deterministic output (helps with reproducible builds)
    files.sort_by(|a, b| a.0.cmp(&b.0));

    Ok(files)
}

/// Recursively collects files from a directory.
fn collect_files_recursive(
    current: &Path,
    root: &Path,
    extensions: &[&str],
    files: &mut Vec<(String, String)>,
) -> Result<(), String> {
    let entries = std::fs::read_dir(current)
        .map_err(|e| format!("Failed to read {}: {}", current.display(), e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let path = entry.path();

        if path.is_dir() {
            collect_files_recursive(&path, root, extensions, files)?;
        } else if path.is_file() {
            let path_str = path.to_string_lossy();

            // Check if file has a recognized extension
            if extensions.iter().any(|ext| path_str.ends_with(ext)) {
                // Compute relative path from root (with extension)
                let relative = path.strip_prefix(root).map_err(|_| {
                    format!("Failed to compute relative path for {}", path.display())
                })?;

                let name_with_ext = relative
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "/");

                let content = std::fs::read_to_string(&path)
                    .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

                files.push((name_with_ext, content));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_file(dir: &Path, relative_path: &str, content: &str) {
        let full_path = dir.join(relative_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full_path, content).unwrap();
    }

    #[test]
    fn test_collect_files_preserves_extension() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "config.yaml", "key: value");

        let files = collect_files(temp_dir.path(), STYLESHEET_EXTENSIONS).unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].0, "config.yaml"); // Extension preserved
        assert_eq!(files[0].1, "key: value");
    }

    #[test]
    fn test_collect_files_nested_paths() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "themes/dark.yaml", "dark content");
        create_file(temp_dir.path(), "themes/light.yaml", "light content");

        let files = collect_files(temp_dir.path(), STYLESHEET_EXTENSIONS).unwrap();

        assert_eq!(files.len(), 2);
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"themes/dark.yaml"));
        assert!(names.contains(&"themes/light.yaml"));
    }

    #[test]
    fn test_collect_files_filters_extensions() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "good.yaml", "yaml content");
        create_file(temp_dir.path(), "bad.txt", "text content");

        let files = collect_files(temp_dir.path(), STYLESHEET_EXTENSIONS).unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].0, "good.yaml");
    }

    #[test]
    fn test_collect_files_multiple_extensions() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "a.yaml", "a");
        create_file(temp_dir.path(), "b.yml", "b");

        let files = collect_files(temp_dir.path(), STYLESHEET_EXTENSIONS).unwrap();

        assert_eq!(files.len(), 2);
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"a.yaml"));
        assert!(names.contains(&"b.yml"));
    }

    #[test]
    fn test_collect_files_same_name_different_ext() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "config.yaml", "yaml version");
        create_file(temp_dir.path(), "config.yml", "yml version");

        let files = collect_files(temp_dir.path(), STYLESHEET_EXTENSIONS).unwrap();

        // Both should be collected - registry handles priority
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_collect_files_directory_not_found() {
        let result = collect_files(Path::new("/nonexistent/path"), STYLESHEET_EXTENSIONS);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_collect_files_sorted_output() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "zebra.yaml", "z");
        create_file(temp_dir.path(), "alpha.yaml", "a");
        create_file(temp_dir.path(), "middle.yaml", "m");

        let files = collect_files(temp_dir.path(), STYLESHEET_EXTENSIONS).unwrap();

        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["alpha.yaml", "middle.yaml", "zebra.yaml"]);
    }
}
