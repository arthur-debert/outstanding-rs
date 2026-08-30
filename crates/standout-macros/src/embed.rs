use proc_macro2::TokenStream;
use quote::quote;
use std::path::{Path, PathBuf};
use syn::LitStr;

pub const TEMPLATE_EXTENSIONS: &[&str] = &[".jinja", ".jinja2", ".j2", ".txt"];

pub const STYLESHEET_EXTENSIONS: &[&str] = &[".css", ".yaml", ".yml"];

pub fn embed_templates_impl(input: LitStr) -> TokenStream {
    let source_path = input.value();
    let dir_path = resolve_path(&source_path);

    let files = match collect_files(&dir_path, TEMPLATE_EXTENSIONS) {
        Ok(files) => files,
        Err(e) => {
            return syn::Error::new(input.span(), e).to_compile_error();
        }
    };

    let absolute_path = dir_path.to_string_lossy().to_string();

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
            ::standout::EmbeddedSource::<::standout::TemplateResource>::new(
                ENTRIES,
                #absolute_path,
            )
        }
    }
}

pub fn embed_styles_impl(input: LitStr) -> TokenStream {
    let source_path = input.value();
    let dir_path = resolve_path(&source_path);

    let files = match collect_files(&dir_path, STYLESHEET_EXTENSIONS) {
        Ok(files) => files,
        Err(e) => {
            return syn::Error::new(input.span(), e).to_compile_error();
        }
    };

    let absolute_path = dir_path.to_string_lossy().to_string();

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
            ::standout::EmbeddedSource::<::standout::StylesheetResource>::new(
                ENTRIES,
                #absolute_path,
            )
        }
    }
}

fn resolve_path(path: &str) -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR should be set during compilation");
    Path::new(&manifest_dir).join(path)
}

fn collect_files(dir: &Path, extensions: &[&str]) -> Result<Vec<(String, String)>, String> {
    if !dir.exists() {
        return Err(format!("Directory not found: {}", dir.display()));
    }
    if !dir.is_dir() {
        return Err(format!("Path is not a directory: {}", dir.display()));
    }

    let mut files = Vec::new();
    collect_files_recursive(dir, dir, extensions, &mut files)?;

    files.sort_by(|a, b| a.0.cmp(&b.0));

    Ok(files)
}

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

            if extensions.iter().any(|ext| path_str.ends_with(ext)) {
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
        assert_eq!(files[0].0, "config.yaml");
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
