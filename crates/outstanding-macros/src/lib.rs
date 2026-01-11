//! Proc macros for compile-time resource embedding in Outstanding.
//!
//! These macros walk directories at compile time and embed all matching files
//! into the binary, enabling single-binary distribution without external file
//! dependencies.
//!
//! # Macros
//!
//! - [`embed_templates!`] - Embed template files (`.jinja`, `.jinja2`, `.j2`, `.txt`)
//! - [`embed_styles!`] - Embed stylesheet files (`.yaml`, `.yml`)
//!
//! # Example
//!
//! ```rust,ignore
//! use outstanding_macros::{embed_templates, embed_styles};
//!
//! // Embeds all templates from ./templates at compile time
//! let templates = embed_templates!("./templates");
//!
//! // Embeds all stylesheets from ./styles at compile time
//! let styles = embed_styles!("./styles");
//!
//! // Access by name, same as runtime API
//! let content = templates.get("report/summary")?;
//! let theme = styles.get("dark")?;
//! ```

use proc_macro::TokenStream;
use quote::quote;
use std::path::{Path, PathBuf};
use syn::{parse_macro_input, LitStr};

/// Template file extensions in priority order.
const TEMPLATE_EXTENSIONS: &[&str] = &[".jinja", ".jinja2", ".j2", ".txt"];

/// Stylesheet file extensions in priority order.
const STYLESHEET_EXTENSIONS: &[&str] = &[".yaml", ".yml"];

/// Embeds all template files from a directory at compile time.
///
/// This macro walks the specified directory at compile time, reads all files
/// with recognized template extensions, and generates code that creates a
/// `TemplateRegistry` with all templates pre-loaded.
///
/// # Extensions
///
/// Recognized extensions (in priority order):
/// - `.jinja`
/// - `.jinja2`
/// - `.j2`
/// - `.txt`
///
/// # Name Resolution
///
/// Files are named by their relative path from the root, without extension:
/// - `templates/list.jinja` → `"list"`
/// - `templates/report/summary.jinja` → `"report/summary"`
///
/// # Example
///
/// ```rust,ignore
/// use outstanding_macros::embed_templates;
///
/// let templates = embed_templates!("./templates");
/// let content = templates.get("report/summary")?;
/// ```
///
/// # Compile-Time Errors
///
/// - Directory doesn't exist
/// - Directory is not readable
/// - File content is not valid UTF-8
#[proc_macro]
pub fn embed_templates(input: TokenStream) -> TokenStream {
    let path_lit = parse_macro_input!(input as LitStr);
    let dir_path = resolve_path(&path_lit.value());

    let files = match collect_files(&dir_path, TEMPLATE_EXTENSIONS) {
        Ok(files) => files,
        Err(e) => {
            return syn::Error::new(path_lit.span(), e)
                .to_compile_error()
                .into();
        }
    };

    let entries: Vec<_> = files
        .iter()
        .map(|(name, content)| {
            quote! {
                registry.add_inline(#name, #content);
            }
        })
        .collect();

    let expanded = quote! {
        {
            let mut registry = ::outstanding::TemplateRegistry::new();
            #(#entries)*
            registry
        }
    };

    expanded.into()
}

/// Embeds all stylesheet files from a directory at compile time.
///
/// This macro walks the specified directory at compile time, reads all files
/// with recognized stylesheet extensions, and generates code that creates a
/// `StylesheetRegistry` with all stylesheets pre-loaded.
///
/// # Extensions
///
/// Recognized extensions (in priority order):
/// - `.yaml`
/// - `.yml`
///
/// # Name Resolution
///
/// Files are named by their relative path from the root, without extension:
/// - `styles/default.yaml` → `"default"`
/// - `styles/themes/dark.yaml` → `"themes/dark"`
///
/// # Example
///
/// ```rust,ignore
/// use outstanding_macros::embed_styles;
///
/// let styles = embed_styles!("./styles");
/// let theme = styles.get("dark")?;
/// ```
///
/// # Compile-Time Errors
///
/// - Directory doesn't exist
/// - Directory is not readable
/// - File content is not valid UTF-8
#[proc_macro]
pub fn embed_styles(input: TokenStream) -> TokenStream {
    let path_lit = parse_macro_input!(input as LitStr);
    let dir_path = resolve_path(&path_lit.value());

    let files = match collect_files(&dir_path, STYLESHEET_EXTENSIONS) {
        Ok(files) => files,
        Err(e) => {
            return syn::Error::new(path_lit.span(), e)
                .to_compile_error()
                .into();
        }
    };

    let entries: Vec<_> = files
        .iter()
        .map(|(name, content)| {
            quote! {
                registry.add_inline(#name, #content).expect("embedded stylesheet should parse");
            }
        })
        .collect();

    let expanded = quote! {
        {
            let mut registry = ::outstanding::stylesheet::StylesheetRegistry::new();
            #(#entries)*
            registry
        }
    };

    expanded.into()
}

/// Resolves a path relative to the crate's manifest directory.
fn resolve_path(path: &str) -> PathBuf {
    // CARGO_MANIFEST_DIR is set during compilation to the directory containing
    // the Cargo.toml of the crate being compiled (not the proc-macro crate).
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR should be set during compilation");

    Path::new(&manifest_dir).join(path)
}

/// Collects all files from a directory with matching extensions.
///
/// Returns a vector of (name, content) pairs where name is the extensionless
/// relative path.
fn collect_files(dir: &Path, extensions: &[&str]) -> Result<Vec<(String, String)>, String> {
    if !dir.exists() {
        return Err(format!("Directory not found: {}", dir.display()));
    }

    if !dir.is_dir() {
        return Err(format!("Path is not a directory: {}", dir.display()));
    }

    let mut files = Vec::new();
    collect_files_recursive(dir, dir, extensions, &mut files)?;

    // Sort by extension priority, then by name for deterministic output
    files.sort_by(|a, b| {
        let pri_a = extension_priority(&a.0, extensions);
        let pri_b = extension_priority(&b.0, extensions);
        pri_a.cmp(&pri_b).then_with(|| a.0.cmp(&b.0))
    });

    // Deduplicate: keep only the first occurrence of each base name
    // (highest priority extension)
    let mut seen_names = std::collections::HashSet::new();
    let mut result = Vec::new();

    for (name_with_ext, content) in files {
        let base_name = strip_extension(&name_with_ext, extensions);
        if seen_names.insert(base_name.clone()) {
            result.push((base_name, content));
        }
    }

    Ok(result)
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
            if let Some(name) = try_parse_file(&path, root, extensions) {
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
                files.push((name, content));
            }
        }
    }

    Ok(())
}

/// Attempts to parse a file path, returning the name with extension if valid.
fn try_parse_file(path: &Path, root: &Path, extensions: &[&str]) -> Option<String> {
    let path_str = path.to_string_lossy();

    // Check if file has a recognized extension
    if !extensions.iter().any(|ext| path_str.ends_with(ext)) {
        return None;
    }

    // Compute relative path from root
    let relative = path.strip_prefix(root).ok()?;
    let name_with_ext = relative
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/");

    Some(name_with_ext)
}

/// Returns the extension priority (lower = higher priority).
fn extension_priority(name: &str, extensions: &[&str]) -> usize {
    for (i, ext) in extensions.iter().enumerate() {
        if name.ends_with(ext) {
            return i;
        }
    }
    usize::MAX
}

/// Strips the extension from a name.
fn strip_extension(name: &str, extensions: &[&str]) -> String {
    for ext in extensions {
        if let Some(base) = name.strip_suffix(ext) {
            return base.to_string();
        }
    }
    name.to_string()
}
