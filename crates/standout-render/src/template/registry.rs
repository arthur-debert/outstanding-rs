//! Template registry for file-based and inline templates.
//!
//! [`TemplateRegistry`] is a thin wrapper around
//! [`FileRegistry<String>`](crate::file_loader::FileRegistry) that adds
//! template-specific resolution on top of the generic file-loading
//! infrastructure. It resolves names against four tiers, in priority order:
//! inline templates, `add_from_files` file templates, directory-registered
//! templates, then framework (`standout/`-namespaced) defaults — so user
//! templates always override framework ones.
//!
//! Recognized extensions, highest priority first: [`TEMPLATE_EXTENSIONS`]
//! (`.jinja`, `.jinja2`, `.j2`, `.stpl`, `.txt`). A name may be looked up
//! with or without an extension, and even with the "wrong" recognized
//! extension (e.g. `"list.j2"` resolves to a file registered as
//! `list.jinja`) — the extension is stripped and the base name is retried.
//!
//! Collisions are strict by design, to catch configuration mistakes early
//! rather than pick an arbitrary winner: two directories producing the same
//! resolved name is an error, but the same directory offering multiple
//! extensions for one base name is not (the higher-priority extension wins
//! silently).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::file_loader::{
    self, build_embedded_registry, resolve_in_map, FileRegistry, FileRegistryConfig, LoadError,
    LoadedEntry, LoadedFile,
};

pub const TEMPLATE_EXTENSIONS: &[&str] = &[".jinja", ".jinja2", ".j2", ".stpl", ".txt"];

static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateFile {
    pub name: String,
    pub name_with_ext: String,
    pub absolute_path: PathBuf,
    pub source_dir: PathBuf,
}

impl TemplateFile {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedTemplate {
    Inline(String),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    Collision {
        name: String,
        existing_path: PathBuf,
        existing_dir: PathBuf,
        conflicting_path: PathBuf,
        conflicting_dir: PathBuf,
    },

    NotFound {
        name: String,
    },

    ReadError {
        path: PathBuf,
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

fn template_config() -> FileRegistryConfig<String> {
    FileRegistryConfig {
        extensions: TEMPLATE_EXTENSIONS,
        transform: |content| Ok(content.to_string()),
    }
}

#[derive(Clone)]
pub struct TemplateRegistry {
    inner: FileRegistry<String>,
    inline: HashMap<String, String>,
    files: HashMap<String, PathBuf>,
    sources: HashMap<String, (PathBuf, PathBuf)>,
    framework: HashMap<String, String>,
    // Clones keep the same id; a mutated copy is distinguished by `generation`,
    // not by allocation address.
    id: u64,
    // Identity plus this value is the load-into-engine cache key. Sibling
    // clones must not share a revision after they diverge.
    generation: u64,
}

impl Default for TemplateRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateRegistry {
    pub fn new() -> Self {
        Self {
            inner: FileRegistry::new(template_config()),
            inline: HashMap::new(),
            files: HashMap::new(),
            sources: HashMap::new(),
            framework: HashMap::new(),
            id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            generation: 0,
        }
    }

    fn bump_generation(&mut self) {
        self.generation = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_inline(&mut self, name: impl Into<String>, content: impl Into<String>) {
        self.inline.insert(name.into(), content.into());
        self.bump_generation();
    }

    pub fn add_template_dir<P: AsRef<Path>>(&mut self, path: P) -> Result<(), RegistryError> {
        self.inner.add_dir(path).map_err(RegistryError::from)?;
        self.bump_generation();
        Ok(())
    }

    pub fn add_from_files(&mut self, files: Vec<TemplateFile>) -> Result<(), RegistryError> {
        let mut sorted_files = files;
        sorted_files.sort_by_key(|f| f.extension_priority());
        // Bump first: a collision can return after partial inserts, and those
        // still change resolution for the names that landed.
        self.bump_generation();

        for file in sorted_files {
            if let Some((existing_path, existing_dir)) = self.sources.get(&file.name) {
                if existing_dir != &file.source_dir {
                    return Err(RegistryError::Collision {
                        name: file.name.clone(),
                        existing_path: existing_path.clone(),
                        existing_dir: existing_dir.clone(),
                        conflicting_path: file.absolute_path.clone(),
                        conflicting_dir: file.source_dir.clone(),
                    });
                }
                continue;
            }

            self.sources.insert(
                file.name.clone(),
                (file.absolute_path.clone(), file.source_dir.clone()),
            );

            self.files
                .insert(file.name.clone(), file.absolute_path.clone());

            self.files
                .insert(file.name_with_ext.clone(), file.absolute_path);
        }

        Ok(())
    }

    pub fn add_embedded(&mut self, templates: HashMap<String, String>) {
        for (name, content) in templates {
            self.inline.insert(name, content);
        }
        self.bump_generation();
    }

    pub fn add_framework(&mut self, name: impl Into<String>, content: impl Into<String>) {
        self.framework.insert(name.into(), content.into());
        self.bump_generation();
    }

    pub fn add_framework_entries(&mut self, entries: &[(&str, &str)]) {
        let framework: HashMap<String, String> =
            build_embedded_registry(entries, TEMPLATE_EXTENSIONS, |content| {
                Ok::<_, std::convert::Infallible>(content.to_string())
            })
            .unwrap();

        for (name, content) in framework {
            self.framework.insert(name, content);
        }
        self.bump_generation();
    }

    pub fn clear_framework(&mut self) {
        self.framework.clear();
        self.bump_generation();
    }

    pub fn from_embedded_entries(entries: &[(&str, &str)]) -> Self {
        let mut registry = Self::new();

        let inline: HashMap<String, String> =
            build_embedded_registry(entries, TEMPLATE_EXTENSIONS, |content| {
                Ok::<_, std::convert::Infallible>(content.to_string())
            })
            .unwrap();

        registry.inline = inline;
        registry
    }

    pub fn get(&self, name: &str) -> Result<ResolvedTemplate, RegistryError> {
        if let Some(content) = resolve_in_map(&self.inline, name, TEMPLATE_EXTENSIONS) {
            return Ok(ResolvedTemplate::Inline(content.clone()));
        }

        if let Some(path) = resolve_in_map(&self.files, name, TEMPLATE_EXTENSIONS) {
            return Ok(ResolvedTemplate::File(path.clone()));
        }

        if let Some(entry) = self.inner.get_entry(name) {
            return Ok(ResolvedTemplate::from(entry));
        }

        if let Some(content) = resolve_in_map(&self.framework, name, TEMPLATE_EXTENSIONS) {
            return Ok(ResolvedTemplate::Inline(content.clone()));
        }

        Err(RegistryError::NotFound {
            name: name.to_string(),
        })
    }

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

    pub fn refresh(&mut self) -> Result<(), RegistryError> {
        self.inner.refresh().map_err(RegistryError::from)?;
        self.bump_generation();
        Ok(())
    }

    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    // ADR-0019 hot reload
    pub(crate) fn has_file_sources(&self) -> bool {
        !self.files.is_empty() || !self.inner.dirs().is_empty()
    }

    pub fn len(&self) -> usize {
        self.inline.len() + self.files.len() + self.inner.len() + self.framework.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inline.is_empty()
            && self.files.is_empty()
            && self.inner.is_empty()
            && self.framework.is_empty()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.inline
            .keys()
            .map(|s| s.as_str())
            .chain(self.files.keys().map(|s| s.as_str()))
            .chain(self.inner.names())
            .chain(self.framework.keys().map(|s| s.as_str()))
    }

    pub fn clear(&mut self) {
        self.inline.clear();
        self.files.clear();
        self.sources.clear();
        self.inner.clear();
        self.framework.clear();
        self.bump_generation();
    }

    pub fn has_framework_templates(&self) -> bool {
        !self.framework.is_empty()
    }

    pub fn has_application_templates(&self) -> bool {
        !self.inline.is_empty() || !self.files.is_empty() || !self.inner.is_empty()
    }

    pub fn framework_names(&self) -> impl Iterator<Item = &str> {
        self.framework.keys().map(|s| s.as_str())
    }
}

pub fn walk_template_dir(root: impl AsRef<Path>) -> Result<Vec<TemplateFile>, std::io::Error> {
    let files = file_loader::walk_dir(root.as_ref(), TEMPLATE_EXTENSIONS)
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    Ok(files.into_iter().map(TemplateFile::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_file_extension_priority() {
        let jinja = TemplateFile::new("config", "config.jinja", "/a/config.jinja", "/a");
        let jinja2 = TemplateFile::new("config", "config.jinja2", "/a/config.jinja2", "/a");
        let j2 = TemplateFile::new("config", "config.j2", "/a/config.j2", "/a");
        let stpl = TemplateFile::new("config", "config.stpl", "/a/config.stpl", "/a");
        let txt = TemplateFile::new("config", "config.txt", "/a/config.txt", "/a");
        let unknown = TemplateFile::new("config", "config.xyz", "/a/config.xyz", "/a");

        assert_eq!(jinja.extension_priority(), 0);
        assert_eq!(jinja2.extension_priority(), 1);
        assert_eq!(j2.extension_priority(), 2);
        assert_eq!(stpl.extension_priority(), 3);
        assert_eq!(txt.extension_priority(), 4);
        assert_eq!(unknown.extension_priority(), usize::MAX);
    }

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

        assert_eq!(registry.len(), 4);

        assert!(registry.get("config").is_ok());
        assert!(registry.get("todos/list").is_ok());

        assert!(registry.get("config.jinja").is_ok());
        assert!(registry.get("todos/list.jinja").is_ok());
    }

    #[test]
    fn test_registry_extension_priority() {
        let mut registry = TemplateRegistry::new();

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

        let files = vec![TemplateFile::new(
            "config",
            "config.jinja",
            "/templates/config.jinja",
            "/templates",
        )];
        registry.add_from_files(files).unwrap();

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

    #[test]
    fn refresh_assigns_a_new_generation() {
        let mut registry = TemplateRegistry::new();
        assert_eq!(registry.generation(), 0);
        registry.refresh().unwrap();
        let first = registry.generation();
        assert_ne!(first, 0);
        registry.refresh().unwrap();
        assert_ne!(registry.generation(), first);
    }

    #[test]
    fn in_memory_mutations_assign_distinct_generations() {
        let mut registry = TemplateRegistry::new();
        assert_eq!(registry.generation(), 0);
        let mut seen = std::collections::HashSet::from([0]);
        registry.add_inline("a", "1");
        assert!(seen.insert(registry.generation()));
        registry.add_framework("standout/x", "x");
        assert!(seen.insert(registry.generation()));
        registry.add_embedded(HashMap::from([("b".into(), "2".into())]));
        assert!(seen.insert(registry.generation()));
        registry.add_framework_entries(&[("standout/y.jinja", "y")]);
        assert!(seen.insert(registry.generation()));
        registry.clear_framework();
        assert!(seen.insert(registry.generation()));
        registry.clear();
        assert!(seen.insert(registry.generation()));
    }

    #[test]
    fn new_registries_have_distinct_ids_and_clones_keep_theirs() {
        let mut a = TemplateRegistry::new();
        let b = TemplateRegistry::new();
        assert_ne!(a.id(), b.id());
        a.add_inline("x", "1");
        let mut cloned = a.clone();
        assert_eq!(a.id(), cloned.id());
        assert_eq!(a.generation(), cloned.generation());
        cloned.add_inline("x", "2");
        assert_eq!(a.id(), cloned.id());
        assert_ne!(a.generation(), cloned.generation());
        assert_eq!(a.get_content("x").unwrap(), "1");
    }

    #[test]
    fn sibling_clones_mutated_once_each_get_distinct_generations() {
        let parent = TemplateRegistry::new();
        let mut first = parent.clone();
        let mut second = parent.clone();
        first.add_inline("x", "one");
        second.add_inline("x", "two");
        assert_eq!(first.id(), second.id());
        assert_ne!(first.generation(), second.generation());
        assert_eq!(first.get_content("x").unwrap(), "one");
        assert_eq!(second.get_content("x").unwrap(), "two");
    }

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

    #[test]
    fn test_from_embedded_entries_single() {
        let entries: &[(&str, &str)] = &[("hello.jinja", "Hello {{ name }}")];
        let registry = TemplateRegistry::from_embedded_entries(entries);

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
        let entries: &[(&str, &str)] = &[
            ("config.txt", "txt content"),
            ("config.jinja", "jinja content"),
        ];
        let registry = TemplateRegistry::from_embedded_entries(entries);

        let content = registry.get_content("config").unwrap();
        assert_eq!(content, "jinja content");

        assert_eq!(registry.get_content("config.txt").unwrap(), "txt content");
        assert_eq!(
            registry.get_content("config.jinja").unwrap(),
            "jinja content"
        );
    }

    #[test]
    fn test_from_embedded_entries_extension_priority_reverse_order() {
        let entries: &[(&str, &str)] = &[
            ("config.jinja", "jinja content"),
            ("config.txt", "txt content"),
        ];
        let registry = TemplateRegistry::from_embedded_entries(entries);

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
        let entries: &[(&str, &str)] = &[
            ("main.jinja", "Start {% include '_partial' %} End"),
            ("_partial.jinja", "PARTIAL_CONTENT"),
        ];
        let registry = TemplateRegistry::from_embedded_entries(entries);

        let mut env = crate::template::new_environment();
        for name in registry.names() {
            if let Ok(content) = registry.get_content(name) {
                env.add_template_owned(name.to_string(), content).unwrap();
            }
        }

        let tmpl = env.get_template("main").unwrap();
        let output = tmpl.render(()).unwrap();
        assert_eq!(output, "Start PARTIAL_CONTENT End");
    }

    #[test]
    fn test_extensionless_includes_with_extension_syntax() {
        let entries: &[(&str, &str)] = &[
            ("main.jinja", "Start {% include '_partial.jinja' %} End"),
            ("_partial.jinja", "PARTIAL_CONTENT"),
        ];
        let registry = TemplateRegistry::from_embedded_entries(entries);

        let mut env = crate::template::new_environment();
        for name in registry.names() {
            if let Ok(content) = registry.get_content(name) {
                env.add_template_owned(name.to_string(), content).unwrap();
            }
        }

        let tmpl = env.get_template("main").unwrap();
        let output = tmpl.render(()).unwrap();
        assert_eq!(output, "Start PARTIAL_CONTENT End");
    }

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

        registry.add_framework("config", "framework content");

        registry.add_inline("config", "inline content");

        let content = registry.get_content("config").unwrap();
        assert_eq!(content, "inline content");
    }

    #[test]
    fn test_framework_user_can_override() {
        let mut registry = TemplateRegistry::new();

        registry.add_framework("standout/list-view", "framework default");

        registry.add_inline("standout/list-view", "user override");

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

    #[test]
    fn test_inline_cross_extension_lookup() {
        let entries: &[(&str, &str)] = &[("list.jinja", "List content")];
        let registry = TemplateRegistry::from_embedded_entries(entries);

        let content = registry.get_content("list.j2").unwrap();
        assert_eq!(content, "List content");
    }

    #[test]
    fn test_inline_cross_extension_nested_path() {
        let entries: &[(&str, &str)] = &[("todos/list.jinja", "Todos")];
        let registry = TemplateRegistry::from_embedded_entries(entries);

        assert_eq!(registry.get_content("todos/list.j2").unwrap(), "Todos");
        assert_eq!(registry.get_content("todos/list.stpl").unwrap(), "Todos");
        assert_eq!(registry.get_content("todos/list").unwrap(), "Todos");
    }

    #[test]
    fn test_framework_cross_extension_lookup() {
        let mut registry = TemplateRegistry::new();
        let entries: &[(&str, &str)] = &[("standout/list-view.jinja", "Framework view")];
        registry.add_framework_entries(entries);

        let content = registry.get_content("standout/list-view.j2").unwrap();
        assert_eq!(content, "Framework view");
    }

    #[test]
    fn test_files_cross_extension_lookup() {
        let mut registry = TemplateRegistry::new();
        let files = vec![TemplateFile::new(
            "config",
            "config.jinja",
            "/templates/config.jinja",
            "/templates",
        )];
        registry.add_from_files(files).unwrap();

        assert!(registry.get("config.j2").is_ok());
        assert!(registry.get("config.stpl").is_ok());
        assert!(registry.get("config.txt").is_ok());
    }
}
