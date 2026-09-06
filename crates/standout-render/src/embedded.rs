use std::marker::PhantomData;
use std::path::Path;

use crate::file_loader::{build_embedded_registry, walk_dir};
use crate::style::{parse_theme_content, StylesheetRegistry, STYLESHEET_EXTENSIONS};
use crate::template::{walk_template_dir, TemplateRegistry};
use crate::warnings::WarningBuffer;

fn emit_setup_warning(warnings: Option<&WarningBuffer>, message: impl Into<String>) {
    let message = message.into();
    match warnings {
        Some(buffer) => buffer.push(message),
        None => eprintln!("Warning: {message}"),
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TemplateResource;

#[derive(Debug, Clone, Copy)]
pub struct StylesheetResource;

#[derive(Debug, Clone)]
pub struct EmbeddedSource<R> {
    pub entries: &'static [(&'static str, &'static str)],

    pub source_path: &'static str,

    _marker: PhantomData<R>,
}

impl<R> EmbeddedSource<R> {
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

    pub fn entries(&self) -> &'static [(&'static str, &'static str)] {
        self.entries
    }

    pub fn source_path(&self) -> &'static str {
        self.source_path
    }

    pub fn should_hot_reload(&self) -> bool {
        cfg!(debug_assertions) && std::path::Path::new(self.source_path).exists()
    }
}

pub type EmbeddedTemplates = EmbeddedSource<TemplateResource>;

pub type EmbeddedStyles = EmbeddedSource<StylesheetResource>;

impl EmbeddedTemplates {
    pub fn into_registry(self, warnings: Option<&WarningBuffer>) -> TemplateRegistry {
        if self.should_hot_reload() {
            let files = match walk_template_dir(self.source_path) {
                Ok(files) => files,
                Err(e) => {
                    emit_setup_warning(
                        warnings,
                        format!(
                            "Failed to walk templates directory '{}', using embedded: {}",
                            self.source_path, e
                        ),
                    );
                    return TemplateRegistry::from_embedded_entries(self.entries);
                }
            };

            let mut registry = TemplateRegistry::new();
            if let Err(e) = registry.add_from_files(files) {
                emit_setup_warning(
                    warnings,
                    format!(
                        "Failed to register templates from '{}', using embedded: {}",
                        self.source_path, e
                    ),
                );
                return TemplateRegistry::from_embedded_entries(self.entries);
            }
            registry
        } else {
            TemplateRegistry::from_embedded_entries(self.entries)
        }
    }
}

impl From<EmbeddedTemplates> for TemplateRegistry {
    fn from(source: EmbeddedTemplates) -> Self {
        source.into_registry(None)
    }
}

impl EmbeddedStyles {
    pub fn into_registry(self, warnings: Option<&WarningBuffer>) -> StylesheetRegistry {
        if self.should_hot_reload() {
            let files = match walk_dir(Path::new(self.source_path), STYLESHEET_EXTENSIONS) {
                Ok(files) => files,
                Err(e) => {
                    emit_setup_warning(
                        warnings,
                        format!(
                            "Failed to walk styles directory '{}', using embedded: {}",
                            self.source_path, e
                        ),
                    );
                    return StylesheetRegistry::from_embedded_entries(self.entries)
                        .expect("embedded stylesheets should parse");
                }
            };

            let entries: Vec<(String, String)> = files
                .into_iter()
                .filter_map(|file| match std::fs::read_to_string(&file.path) {
                    Ok(content) => Some((file.name_with_ext, content)),
                    Err(e) => {
                        emit_setup_warning(
                            warnings,
                            format!("Failed to read stylesheet '{}': {}", file.path.display(), e),
                        );
                        None
                    }
                })
                .collect();

            let entries_refs: Vec<(&str, &str)> = entries
                .iter()
                .map(|(n, c)| (n.as_str(), c.as_str()))
                .collect();

            let inline =
                match build_embedded_registry(&entries_refs, STYLESHEET_EXTENSIONS, |content| {
                    parse_theme_content(content)
                }) {
                    Ok(map) => map,
                    Err(e) => {
                        emit_setup_warning(
                            warnings,
                            format!(
                                "Failed to parse stylesheets from '{}', using embedded: {}",
                                self.source_path, e
                            ),
                        );
                        return StylesheetRegistry::from_embedded_entries(self.entries)
                            .expect("embedded stylesheets should parse");
                    }
                };

            let mut registry = StylesheetRegistry::new();
            registry.add_embedded(inline);
            registry
        } else {
            StylesheetRegistry::from_embedded_entries(self.entries)
                .expect("embedded stylesheets should parse")
        }
    }
}

impl From<EmbeddedStyles> for StylesheetRegistry {
    fn from(source: EmbeddedStyles) -> Self {
        source.into_registry(None)
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

        assert!(!source.should_hot_reload());
    }

    #[test]
    fn hot_reload_walk_failure_records_warning_buffer() {
        const CARGO_TOML: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        static ENTRIES: &[(&str, &str)] = &[("ok.jinja", "hi")];
        let source: EmbeddedTemplates = EmbeddedSource::new(ENTRIES, CARGO_TOML);
        if !source.should_hot_reload() {
            return;
        }

        let buffer = WarningBuffer::new();
        let registry = source.into_registry(Some(&buffer));
        let warnings = buffer.take();
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].contains("Failed to walk templates directory"),
            "unexpected warning: {}",
            warnings[0]
        );
        assert_eq!(registry.get_content("ok").unwrap(), "hi");
    }
}
