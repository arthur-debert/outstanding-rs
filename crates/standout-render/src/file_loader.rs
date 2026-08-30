//! File-based resource loading for templates and stylesheets, with a
//! release mode that embeds everything into the binary at compile time.
//!
//! Resources are referenced by a resolution name: the file's path relative
//! to a registered root, without extension (`templates/report/summary.jinja`
//! resolves as `"report/summary"`). Extensions are matched against a
//! priority-ordered list; when several files share a base name, the
//! extension earlier in the list wins for extensionless lookups. Lookups
//! are also extension-agnostic in the other direction: an unrecognized
//! suffix on a lookup name is stripped and retried against the base name,
//! so `get("config.j2")` can resolve to a file registered as `"config"`.
//!
//! [`FileRegistry`] holds entries as either [`LoadedEntry::File`] (dev mode:
//! re-read and transformed from disk on every access, so edits are visible
//! immediately) or [`LoadedEntry::Embedded`] (release mode: content baked in
//! at compile time via the `standout_macros` embedding macros). Embedded
//! entries always shadow file-based ones of the same name.
//!
//! A name resolving to files in two different registered directories is a
//! configuration error and panics with [`LoadError::Collision`]; the same
//! name at different extensions within one directory is resolved by
//! priority instead.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedFile {
    pub name: String,
    pub name_with_ext: String,
    pub path: PathBuf,
    pub source_dir: PathBuf,
}

impl LoadedFile {
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

    pub fn extension_priority(&self, extensions: &[&str]) -> usize {
        extension_priority(&self.name_with_ext, extensions)
    }
}

pub fn extension_priority(name: &str, extensions: &[&str]) -> usize {
    for (i, ext) in extensions.iter().enumerate() {
        if name.ends_with(ext) {
            return i;
        }
    }
    usize::MAX
}

pub fn strip_extension(name: &str, extensions: &[&str]) -> String {
    for ext in extensions {
        if let Some(base) = name.strip_suffix(ext) {
            return base.to_string();
        }
    }
    name.to_string()
}

pub fn resolve_in_map<'a, V>(
    map: &'a HashMap<String, V>,
    name: &str,
    extensions: &[&str],
) -> Option<&'a V> {
    if let Some(value) = map.get(name) {
        return Some(value);
    }
    let base_name = strip_extension(name, extensions);
    if base_name != name {
        map.get(base_name.as_str())
    } else {
        None
    }
}

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

    let mut sorted: Vec<_> = entries.iter().collect();
    sorted.sort_by_key(|(name, _)| extension_priority(name, extensions));

    let mut seen_base_names = std::collections::HashSet::new();

    for (name_with_ext, content) in sorted {
        let value = transform(content)?;
        let base_name = strip_extension(name_with_ext, extensions);

        registry.insert((*name_with_ext).to_string(), value.clone());

        if seen_base_names.insert(base_name.clone()) {
            registry.insert(base_name, value);
        }
    }

    Ok(registry)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadedEntry<T> {
    File(PathBuf),
    Embedded(T),
}

#[derive(Clone)]
pub struct FileRegistryConfig<T> {
    pub extensions: &'static [&'static str],
    pub transform: fn(&str) -> Result<T, LoadError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    DirectoryNotFound {
        path: PathBuf,
    },
    Io {
        path: PathBuf,
        message: String,
    },
    NotFound {
        name: String,
    },
    Collision {
        name: String,
        existing_path: PathBuf,
        existing_dir: PathBuf,
        conflicting_path: PathBuf,
        conflicting_dir: PathBuf,
    },
    Transform {
        name: String,
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

#[derive(Clone)]
pub struct FileRegistry<T> {
    config: FileRegistryConfig<T>,
    dirs: Vec<PathBuf>,
    entries: HashMap<String, LoadedEntry<T>>,
    sources: HashMap<String, (PathBuf, PathBuf)>,
    initialized: bool,
}

impl<T: Clone> FileRegistry<T> {
    pub fn new(config: FileRegistryConfig<T>) -> Self {
        Self {
            config,
            dirs: Vec::new(),
            entries: HashMap::new(),
            sources: HashMap::new(),
            initialized: false,
        }
    }

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

    pub fn add_embedded(&mut self, name: &str, content: T) {
        self.entries
            .insert(name.to_string(), LoadedEntry::Embedded(content));
    }

    pub fn refresh(&mut self) -> Result<(), LoadError> {
        let mut all_files = Vec::new();
        for dir in &self.dirs {
            let files = walk_dir(dir, self.config.extensions)?;
            all_files.extend(files);
        }

        self.entries
            .retain(|_, v| matches!(v, LoadedEntry::Embedded(_)));
        self.sources.clear();

        all_files.sort_by_key(|f| f.extension_priority(self.config.extensions));

        for file in all_files {
            let entry = LoadedEntry::File(file.path.clone());

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
                if !self.entries.contains_key(&file.name_with_ext) {
                    self.entries.insert(file.name_with_ext.clone(), entry);
                }
                continue;
            }

            self.sources.insert(
                file.name.clone(),
                (file.path.clone(), file.source_dir.clone()),
            );

            if !self.entries.contains_key(&file.name) {
                self.entries.insert(file.name.clone(), entry.clone());
            }

            if !self.entries.contains_key(&file.name_with_ext) {
                self.entries.insert(file.name_with_ext.clone(), entry);
            }
        }

        self.initialized = true;
        Ok(())
    }

    fn ensure_initialized(&mut self) -> Result<(), LoadError> {
        if !self.initialized && !self.dirs.is_empty() {
            self.refresh()?;
        }
        Ok(())
    }

    pub fn get(&mut self, name: &str) -> Result<T, LoadError> {
        self.ensure_initialized()?;

        if self.entries.contains_key(name) {
            return self.get_by_key(name);
        }

        let base_name = strip_extension(name, self.config.extensions);
        if base_name != name && self.entries.contains_key(base_name.as_str()) {
            return self.get_by_key(&base_name);
        }

        Err(LoadError::NotFound {
            name: name.to_string(),
        })
    }

    fn get_by_key(&self, key: &str) -> Result<T, LoadError> {
        match self.entries.get(key) {
            Some(LoadedEntry::Embedded(content)) => Ok(content.clone()),
            Some(LoadedEntry::File(path)) => {
                let content = std::fs::read_to_string(path).map_err(|e| LoadError::Io {
                    path: path.clone(),
                    message: e.to_string(),
                })?;
                (self.config.transform)(&content).map_err(|e| {
                    if let LoadError::Transform { message, .. } = e {
                        LoadError::Transform {
                            name: key.to_string(),
                            message,
                        }
                    } else {
                        e
                    }
                })
            }
            None => Err(LoadError::NotFound {
                name: key.to_string(),
            }),
        }
    }

    pub fn get_entry(&self, name: &str) -> Option<&LoadedEntry<T>> {
        if let Some(entry) = self.entries.get(name) {
            return Some(entry);
        }
        let base_name = strip_extension(name, self.config.extensions);
        if base_name != name {
            self.entries.get(base_name.as_str())
        } else {
            None
        }
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(|s| s.as_str())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.sources.clear();
        self.initialized = false;
    }

    pub fn dirs(&self) -> &[PathBuf] {
        &self.dirs
    }
}

pub fn walk_dir(root: &Path, extensions: &[&str]) -> Result<Vec<LoadedFile>, LoadError> {
    let root_canonical = root.canonicalize().map_err(|e| LoadError::Io {
        path: root.to_path_buf(),
        message: e.to_string(),
    })?;

    let mut files = Vec::new();
    walk_dir_recursive(&root_canonical, &root_canonical, extensions, &mut files)?;
    Ok(files)
}

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

fn try_parse_file(path: &Path, root: &Path, extensions: &[&str]) -> Option<LoadedFile> {
    let path_str = path.to_string_lossy();

    let extension = extensions.iter().find(|ext| path_str.ends_with(*ext))?;

    let relative = path.strip_prefix(root).ok()?;
    let relative_str = relative.to_string_lossy();

    let name_with_ext = relative_str.replace(std::path::MAIN_SEPARATOR, "/");

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

        let content = registry.get("config").unwrap();
        assert_eq!(content, "From tmpl");

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

        registry.refresh().unwrap();
    }

    #[test]
    fn test_registry_embedded_shadows_file() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "config.tmpl", "From file");

        let mut registry = FileRegistry::new(string_config());
        registry.add_dir(temp_dir.path()).unwrap();
        registry.add_embedded("config", "From embedded".to_string());

        let content = registry.get("config").unwrap();
        assert_eq!(content, "From embedded");
    }

    #[test]
    fn test_registry_hot_reload() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "hot.tmpl", "Version 1");

        let mut registry = FileRegistry::new(string_config());
        registry.add_dir(temp_dir.path()).unwrap();

        assert_eq!(registry.get("hot").unwrap(), "Version 1");

        create_file(temp_dir.path(), "hot.tmpl", "Version 2");

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

        create_file(temp_dir.path(), "second.tmpl", "Second content");

        registry.refresh().unwrap();

        assert!(registry.get("second").is_ok());
        assert_eq!(registry.get("second").unwrap(), "Second content");
    }

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

    #[test]
    fn test_resolve_in_map_exact_match() {
        let mut map = HashMap::new();
        map.insert("config".to_string(), "base");
        map.insert("config.tmpl".to_string(), "with ext");

        let extensions = &[".tmpl", ".j2"];

        assert_eq!(resolve_in_map(&map, "config", extensions), Some(&"base"));
        assert_eq!(
            resolve_in_map(&map, "config.tmpl", extensions),
            Some(&"with ext")
        );
    }

    #[test]
    fn test_resolve_in_map_fallback_to_base_name() {
        let mut map = HashMap::new();
        map.insert("config".to_string(), "content");

        let extensions = &[".tmpl", ".j2"];

        assert_eq!(
            resolve_in_map(&map, "config.j2", extensions),
            Some(&"content")
        );
        assert_eq!(
            resolve_in_map(&map, "config.tmpl", extensions),
            Some(&"content")
        );
    }

    #[test]
    fn test_resolve_in_map_unknown_extension_no_fallback() {
        let mut map = HashMap::new();
        map.insert("config".to_string(), "content");

        let extensions = &[".tmpl", ".j2"];

        assert_eq!(resolve_in_map(&map, "config.txt", extensions), None);
    }

    #[test]
    fn test_resolve_in_map_no_match() {
        let map: HashMap<String, &str> = HashMap::new();
        let extensions = &[".tmpl"];

        assert_eq!(resolve_in_map(&map, "missing", extensions), None);
        assert_eq!(resolve_in_map(&map, "missing.tmpl", extensions), None);
    }

    #[test]
    fn test_registry_get_cross_extension_fallback() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "config.tmpl", "Template content");

        let mut registry = FileRegistry::new(string_config());
        registry.add_dir(temp_dir.path()).unwrap();

        assert_eq!(registry.get("config.j2").unwrap(), "Template content");
        assert_eq!(registry.get("config.jinja2").unwrap(), "Template content");
        assert_eq!(registry.get("config").unwrap(), "Template content");
        assert_eq!(registry.get("config.tmpl").unwrap(), "Template content");
    }

    #[test]
    fn test_registry_get_entry_cross_extension_fallback() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "list.tmpl", "List content");

        let mut registry = FileRegistry::new(string_config());
        registry.add_dir(temp_dir.path()).unwrap();
        registry.refresh().unwrap();

        assert!(registry.get_entry("list.j2").is_some());
        assert!(registry.get_entry("list.jinja2").is_some());
        assert!(registry.get_entry("list").is_some());
        assert!(registry.get_entry("list.tmpl").is_some());
    }

    #[test]
    fn test_registry_get_cross_extension_nested_path() {
        let temp_dir = TempDir::new().unwrap();
        create_file(temp_dir.path(), "todos/list.tmpl", "Todos list");

        let mut registry = FileRegistry::new(string_config());
        registry.add_dir(temp_dir.path()).unwrap();

        assert_eq!(registry.get("todos/list.j2").unwrap(), "Todos list");
        assert_eq!(registry.get("todos/list").unwrap(), "Todos list");
    }
}
