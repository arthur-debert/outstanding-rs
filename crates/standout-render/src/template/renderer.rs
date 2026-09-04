//! [`Renderer`] registers templates once (inline, from a directory, or
//! embedded) and reuses them across renders. `add_embedded` shares the
//! inline map, so inline and embedded content sit at the same top priority
//! tier; file-based templates are checked after. In debug builds, file-based
//! templates are re-read from disk on each render (hot reload); embedded
//! content is used otherwise. File-based template names must be unique
//! across all registered directories — a collision is an error, not a
//! silent shadow.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use serde::Serialize;

use super::engine::{MiniJinjaEngine, TemplateEngine};
use super::registry::{walk_template_dir, ResolvedTemplate, TemplateRegistry};
use super::registry_error;
use crate::error::RenderError;
use crate::output::Representation;
use crate::request::{render_request, SharedTemplateEngine, TargetProperties, TemplateRef};
use crate::theme::Theme;
use crate::AmbiguousWidth;
use crate::ColorPolicy;
use crate::EmbeddedTemplates;

pub struct Renderer {
    engine: SharedTemplateEngine,
    registry: TemplateRegistry,
    registry_initialized: bool,
    template_dirs: Vec<std::path::PathBuf>,
    theme: Theme,
    representation: Representation,
    color_policy: ColorPolicy,
    ambiguous_width: AmbiguousWidth,
    icon_mode: Option<crate::IconMode>,
    color_scheme: Option<crate::ColorMode>,
}

impl Renderer {
    pub fn new(theme: Theme) -> Result<Self, RenderError> {
        Self::with_output(theme, Representation::Human)
    }

    pub fn with_output(theme: Theme, representation: Representation) -> Result<Self, RenderError> {
        Self::with_output_and_engine(theme, representation, Box::new(MiniJinjaEngine::new()))
    }

    pub fn with_output_and_engine(
        theme: Theme,
        representation: Representation,
        engine: Box<dyn TemplateEngine>,
    ) -> Result<Self, RenderError> {
        theme
            .validate()
            .map_err(|e| RenderError::StyleError(e.to_string()))?;

        Ok(Self {
            engine: Rc::new(RefCell::new(engine)),
            registry: TemplateRegistry::new(),
            registry_initialized: false,
            template_dirs: Vec::new(),
            theme,
            representation,
            color_policy: ColorPolicy::Auto,
            ambiguous_width: AmbiguousWidth::Narrow,
            icon_mode: None,
            color_scheme: None,
        })
    }

    pub fn set_ambiguous_width(&mut self, policy: AmbiguousWidth) {
        self.ambiguous_width = policy;
    }

    pub fn with_ambiguous_width(mut self, policy: AmbiguousWidth) -> Self {
        self.set_ambiguous_width(policy);
        self
    }

    pub fn set_icon_mode(&mut self, mode: crate::IconMode) {
        self.icon_mode = Some(mode);
    }

    pub fn with_icon_mode(mut self, mode: crate::IconMode) -> Self {
        self.set_icon_mode(mode);
        self
    }

    pub fn set_color_scheme(&mut self, scheme: crate::ColorMode) {
        self.color_scheme = Some(scheme);
    }

    pub fn add_template(&mut self, name: &str, source: &str) -> Result<(), RenderError> {
        self.engine.borrow_mut().add_template(name, source)?;
        self.registry.add_inline(name, source);
        Ok(())
    }

    pub fn add_template_dir<P: AsRef<Path>>(&mut self, path: P) -> Result<(), RenderError> {
        let path = path.as_ref();

        if !path.exists() {
            return Err(RenderError::OperationError(format!(
                "Template directory does not exist: {}",
                path.display()
            )));
        }
        if !path.is_dir() {
            return Err(RenderError::OperationError(format!(
                "Path is not a directory: {}",
                path.display()
            )));
        }

        self.template_dirs.push(path.to_path_buf());
        self.registry_initialized = false;
        Ok(())
    }

    pub fn with_embedded(&mut self, templates: HashMap<String, String>) -> &mut Self {
        self.registry.add_embedded(templates);
        self
    }

    pub fn with_embedded_source(&mut self, source: EmbeddedTemplates) -> &mut Self {
        let template_registry = TemplateRegistry::from(source);

        for name in template_registry.names() {
            if let Ok(content) = template_registry.get_content(name) {
                let _ = self.engine.borrow_mut().add_template(name, &content);
                self.registry.add_inline(name, &content);
            }
        }
        self
    }

    pub fn set_representation(&mut self, representation: Representation) {
        self.representation = representation;
    }

    pub fn set_color_policy(&mut self, policy: ColorPolicy) {
        self.color_policy = policy;
    }

    pub fn with_color_policy(mut self, policy: ColorPolicy) -> Self {
        self.set_color_policy(policy);
        self
    }

    pub fn refresh(&mut self) -> Result<(), RenderError> {
        self.initialize_registry()
    }

    fn initialize_registry(&mut self) -> Result<(), RenderError> {
        let mut new_registry = TemplateRegistry::new();
        for name in self.registry.names() {
            if let Ok(ResolvedTemplate::Inline(content)) = self.registry.get(name) {
                new_registry.add_inline(name, &content);
            }
        }

        for dir in &self.template_dirs {
            let files = walk_template_dir(dir).map_err(|e| {
                RenderError::OperationError(format!(
                    "Failed to walk template directory {}: {}",
                    dir.display(),
                    e
                ))
            })?;

            new_registry
                .add_from_files(files)
                .map_err(|e| RenderError::OperationError(e.to_string()))?;
        }

        self.registry = new_registry;
        self.registry_initialized = true;
        Ok(())
    }

    fn ensure_registry_initialized(&mut self) -> Result<(), RenderError> {
        if !self.registry_initialized && !self.template_dirs.is_empty() {
            self.initialize_registry()?;
        }
        Ok(())
    }

    pub fn render<T: Serialize>(&mut self, name: &str, data: &T) -> Result<String, RenderError> {
        self.ensure_registry_initialized()?;
        let is_inline = self
            .registry
            .get(name)
            .is_ok_and(|t| matches!(t, ResolvedTemplate::Inline(_)));
        if (cfg!(debug_assertions) && !is_inline) || !self.engine.borrow().has_template(name) {
            let content = self.get_template_content(name)?;
            self.engine.borrow_mut().add_template(name, &content)?;
        }

        let mut target = TargetProperties::detect();
        target.ambiguous_width = self.ambiguous_width;
        if let Some(icon_mode) = self.icon_mode {
            target.icon_mode = icon_mode;
        }
        if let Some(color_scheme) = self.color_scheme {
            target.color_scheme = color_scheme;
        }
        let request = crate::RenderRequest {
            data: serde_json::to_value(data)?,
            template: TemplateRef::Named(name.to_string()),
            theme: self.theme.clone(),
            format: self.representation,
            color_policy: self.color_policy,
            target,
            engine: self.engine.clone(),
            registry: Some(Rc::new(self.registry.clone())),
            context_registry: None,
            csv_projection: None,
            extras: HashMap::new(),
            warnings: None,
        };
        render_request(&request)
    }

    fn get_template_content(&self, name: &str) -> Result<String, RenderError> {
        let resolved = self.registry.get(name).map_err(registry_error)?;

        match resolved {
            ResolvedTemplate::Inline(content) => Ok(content),
            ResolvedTemplate::File(path) => std::fs::read_to_string(&path).map_err(|e| {
                RenderError::IoError(std::io::Error::other(format!(
                    "Failed to read template {}: {}",
                    path.display(),
                    e
                )))
            }),
        }
    }

    pub fn template_count(&self) -> usize {
        self.registry.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use console::Style;
    use serde::Serialize;
    use std::io::Write;
    use tempfile::TempDir;

    #[derive(Serialize)]
    struct SimpleData {
        message: String,
    }

    #[test]
    fn test_renderer_add_and_render() {
        let theme = Theme::new().add("ok", Style::new().green());
        let mut renderer = Renderer::with_output(theme, Representation::Human)
            .unwrap()
            .with_color_policy(ColorPolicy::Never);

        renderer
            .add_template("test", r#"[ok]{{ message }}[/ok]"#)
            .unwrap();

        let output = renderer
            .render(
                "test",
                &SimpleData {
                    message: "hi".into(),
                },
            )
            .unwrap();
        assert_eq!(output, "hi");
    }

    #[test]
    fn test_renderer_unknown_template_error() {
        let theme = Theme::new();
        let mut renderer = Renderer::with_output(theme, Representation::Human)
            .unwrap()
            .with_color_policy(ColorPolicy::Never);

        let result = renderer.render(
            "nonexistent",
            &SimpleData {
                message: "x".into(),
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_renderer_multiple_templates() {
        let theme = Theme::new()
            .add("a", Style::new().red())
            .add("b", Style::new().blue());

        let mut renderer = Renderer::with_output(theme, Representation::Human)
            .unwrap()
            .with_color_policy(ColorPolicy::Never);
        renderer
            .add_template("tmpl_a", r#"A: [a]{{ message }}[/a]"#)
            .unwrap();
        renderer
            .add_template("tmpl_b", r#"B: [b]{{ message }}[/b]"#)
            .unwrap();

        let data = SimpleData {
            message: "test".into(),
        };

        assert_eq!(renderer.render("tmpl_a", &data).unwrap(), "A: test");
        assert_eq!(renderer.render("tmpl_b", &data).unwrap(), "B: test");
    }

    #[test]
    fn test_renderer_fails_with_invalid_theme() {
        let theme = Theme::new().add("orphan", "missing");
        let result = Renderer::new(theme);
        assert!(result.is_err());
    }

    #[test]
    fn test_renderer_succeeds_with_valid_aliases() {
        let theme = Theme::new()
            .add("base", Style::new().bold())
            .add("alias", "base");

        let result = Renderer::new(theme);
        assert!(result.is_ok());
    }

    fn create_template_file(dir: &Path, relative_path: &str, content: &str) {
        let full_path = dir.join(relative_path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut file = std::fs::File::create(&full_path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn test_renderer_add_template_dir() {
        let temp_dir = TempDir::new().unwrap();
        create_template_file(temp_dir.path(), "config.jinja", "Config: {{ value }}");

        let mut renderer = Renderer::new(Theme::new()).unwrap();
        renderer.add_template_dir(temp_dir.path()).unwrap();

        #[derive(Serialize)]
        struct Data {
            value: String,
        }

        let output = renderer
            .render(
                "config",
                &Data {
                    value: "test".into(),
                },
            )
            .unwrap();
        assert_eq!(output, "Config: test");
    }

    #[test]
    fn test_renderer_nested_template_dir() {
        let temp_dir = TempDir::new().unwrap();
        create_template_file(temp_dir.path(), "todos/list.jinja", "List: {{ count }}");
        create_template_file(temp_dir.path(), "todos/detail.jinja", "Detail: {{ id }}");

        let mut renderer = Renderer::new(Theme::new()).unwrap();
        renderer.add_template_dir(temp_dir.path()).unwrap();

        #[derive(Serialize)]
        struct ListData {
            count: usize,
        }

        #[derive(Serialize)]
        struct DetailData {
            id: usize,
        }

        let list_output = renderer
            .render("todos/list", &ListData { count: 5 })
            .unwrap();
        assert_eq!(list_output, "List: 5");

        let detail_output = renderer
            .render("todos/detail", &DetailData { id: 42 })
            .unwrap();
        assert_eq!(detail_output, "Detail: 42");
    }

    #[test]
    fn test_renderer_template_with_extension() {
        let temp_dir = TempDir::new().unwrap();
        create_template_file(temp_dir.path(), "config.jinja", "Content");

        let mut renderer = Renderer::new(Theme::new()).unwrap();
        renderer.add_template_dir(temp_dir.path()).unwrap();

        #[derive(Serialize)]
        struct Empty {}

        assert!(renderer.render("config", &Empty {}).is_ok());
        assert!(renderer.render("config.jinja", &Empty {}).is_ok());
    }

    #[test]
    fn test_renderer_inline_shadows_file() {
        let temp_dir = TempDir::new().unwrap();
        create_template_file(temp_dir.path(), "config.jinja", "From file");

        let mut renderer = Renderer::new(Theme::new()).unwrap();
        renderer.add_template_dir(temp_dir.path()).unwrap();

        renderer.add_template("config", "From inline").unwrap();

        #[derive(Serialize)]
        struct Empty {}

        let output = renderer.render("config", &Empty {}).unwrap();
        assert_eq!(output, "From inline");
    }

    #[test]
    fn test_renderer_nonexistent_dir_error() {
        let mut renderer = Renderer::new(Theme::new()).unwrap();
        let result = renderer.add_template_dir("/nonexistent/path/that/does/not/exist");
        assert!(result.is_err());
    }

    #[test]
    fn test_renderer_hot_reload() {
        let temp_dir = TempDir::new().unwrap();
        create_template_file(temp_dir.path(), "hot.jinja", "Version 1");

        let mut renderer = Renderer::new(Theme::new()).unwrap();
        renderer.add_template_dir(temp_dir.path()).unwrap();

        #[derive(Serialize)]
        struct Empty {}

        let output1 = renderer.render("hot", &Empty {}).unwrap();
        assert_eq!(output1, "Version 1");

        create_template_file(temp_dir.path(), "hot.jinja", "Version 2");

        let output2 = renderer.render("hot", &Empty {}).unwrap();
        assert_eq!(output2, "Version 2");
    }

    #[test]
    fn test_renderer_extension_priority() {
        let temp_dir = TempDir::new().unwrap();
        create_template_file(temp_dir.path(), "config.j2", "From j2");
        create_template_file(temp_dir.path(), "config.jinja", "From jinja");

        let mut renderer = Renderer::new(Theme::new()).unwrap();
        renderer.add_template_dir(temp_dir.path()).unwrap();

        #[derive(Serialize)]
        struct Empty {}

        let output = renderer.render("config", &Empty {}).unwrap();
        assert_eq!(output, "From jinja");
    }

    #[test]
    fn test_renderer_unknown_name_reads_kind_once() {
        let mut renderer = Renderer::new(Theme::new()).unwrap();
        let err = renderer.render("nosuch", &()).unwrap_err();
        assert_eq!(err.to_string(), "template not found: nosuch");
    }

    #[test]
    fn test_renderer_with_embedded() {
        let mut renderer = Renderer::new(Theme::new()).unwrap();

        let mut embedded = HashMap::new();
        embedded.insert("embedded".to_string(), "Embedded: {{ val }}".to_string());
        renderer.with_embedded(embedded);

        #[derive(Serialize)]
        struct Data {
            val: String,
        }

        let output = renderer
            .render("embedded", &Data { val: "ok".into() })
            .unwrap();
        assert_eq!(output, "Embedded: ok");
    }

    #[test]
    fn test_renderer_set_color_policy() {
        use console::Style;

        let theme = Theme::new().add("highlight", Style::new().green().force_styling(true));
        let mut renderer = Renderer::with_output(theme, Representation::Human)
            .unwrap()
            .with_color_policy(ColorPolicy::Always);
        renderer
            .add_template("test", "[highlight]hello[/highlight]")
            .unwrap();

        #[derive(Serialize)]
        struct Empty {}

        let term_output = renderer.render("test", &Empty {}).unwrap();
        assert!(
            term_output.contains("\x1b["),
            "Expected ANSI codes under an always color policy, got: {:?}",
            term_output
        );

        renderer.set_color_policy(ColorPolicy::Never);
        let text_output = renderer.render("test", &Empty {}).unwrap();
        assert_eq!(
            text_output, "hello",
            "Expected plain text under a never color policy"
        );
    }

    #[test]
    fn test_renderer_with_embedded_source() {
        use crate::{EmbeddedSource, TemplateResource};

        static ENTRIES: &[(&str, &str)] = &[
            ("greeting.jinja", "Hello, {{ name }}!"),
            ("_partial.jinja", "PARTIAL"),
            (
                "with_include.jinja",
                "Before {% include '_partial' %} After",
            ),
        ];
        let source: EmbeddedSource<TemplateResource> =
            EmbeddedSource::new(ENTRIES, "/nonexistent/path");

        let mut renderer = Renderer::new(Theme::new()).unwrap();
        renderer.with_embedded_source(source);

        #[derive(Serialize)]
        struct Data {
            name: String,
        }

        let output = renderer
            .render(
                "greeting",
                &Data {
                    name: "World".into(),
                },
            )
            .unwrap();
        assert_eq!(output, "Hello, World!");

        let output2 = renderer
            .render(
                "greeting.jinja",
                &Data {
                    name: "Test".into(),
                },
            )
            .unwrap();
        assert_eq!(output2, "Hello, Test!");

        #[derive(Serialize)]
        struct Empty {}
        let output3 = renderer.render("with_include", &Empty {}).unwrap();
        assert_eq!(output3, "Before PARTIAL After");
    }
    #[test]
    fn test_renderer_with_custom_engine() {
        use std::collections::HashMap;

        struct MockEngine {
            templates: HashMap<String, String>,
        }

        impl TemplateEngine for MockEngine {
            fn add_template(&mut self, name: &str, source: &str) -> Result<(), RenderError> {
                self.templates.insert(name.to_string(), source.to_string());
                Ok(())
            }

            fn has_template(&self, name: &str) -> bool {
                self.templates.contains_key(name)
            }

            fn render_template(
                &self,
                source: &str,
                data: &serde_json::Value,
            ) -> Result<String, RenderError> {
                Ok(format!("Mock Render: {} data={}", source, data))
            }

            fn render_named(
                &self,
                name: &str,
                data: &serde_json::Value,
            ) -> Result<String, RenderError> {
                if let Some(src) = self.templates.get(name) {
                    Ok(format!("Mock Named: {} data={}", src, data))
                } else {
                    Err(RenderError::TemplateNotFound(name.to_string()))
                }
            }

            fn render_with_context(
                &self,
                template: &str,
                data: &serde_json::Value,
                _context: HashMap<String, serde_json::Value>,
            ) -> Result<String, RenderError> {
                self.render_template(template, data)
            }

            fn supports_includes(&self) -> bool {
                false
            }
            fn supports_filters(&self) -> bool {
                false
            }
            fn supports_control_flow(&self) -> bool {
                false
            }
        }

        let engine = Box::new(MockEngine {
            templates: HashMap::new(),
        });
        let mut renderer =
            Renderer::with_output_and_engine(Theme::new(), Representation::Human, engine)
                .unwrap()
                .with_color_policy(ColorPolicy::Never);

        renderer.add_template("test", "content").unwrap();

        #[derive(Serialize)]
        struct Data {
            val: i32,
        }

        let output = renderer.render("test", &Data { val: 42 }).unwrap();
        assert_eq!(output, "Mock Named: content data={\"val\":42}");
    }

    #[test]
    fn test_renderer_with_simple_engine() {
        use crate::template::SimpleEngine;

        let engine = Box::new(SimpleEngine::new());
        let mut renderer =
            Renderer::with_output_and_engine(Theme::new(), Representation::Human, engine)
                .unwrap()
                .with_color_policy(ColorPolicy::Never);

        renderer.add_template("welcome", "Hello, {name}!").unwrap();

        #[derive(Serialize)]
        struct User {
            name: String,
        }

        let output = renderer
            .render(
                "welcome",
                &User {
                    name: "Standout".into(),
                },
            )
            .unwrap();
        assert_eq!(output, "Hello, Standout!");
    }

    #[test]
    fn test_renderer_with_icons() {
        use crate::IconDefinition;

        let theme = Theme::new().add_icon(
            "check",
            IconDefinition::new("[ok]").with_nerdfont("\u{f00c}"),
        );

        let mut renderer = Renderer::with_output(theme, Representation::Human)
            .unwrap()
            .with_color_policy(ColorPolicy::Never);
        renderer
            .add_template("test", "{{ icons.check }} {{ message }}")
            .unwrap();

        let output = renderer
            .render(
                "test",
                &SimpleData {
                    message: "done".into(),
                },
            )
            .unwrap();
        assert_eq!(output, "[ok] done");
    }

    #[test]
    fn test_renderer_with_icons_nerdfont() {
        use crate::{IconDefinition, IconMode};

        let theme = Theme::new().add_icon(
            "check",
            IconDefinition::new("[ok]").with_nerdfont("\u{f00c}"),
        );

        let mut renderer = Renderer::with_output(theme, Representation::Human)
            .unwrap()
            .with_icon_mode(IconMode::NerdFont);
        renderer
            .add_template("test", "{{ icons.check }} {{ message }}")
            .unwrap();

        let output = renderer
            .render(
                "test",
                &SimpleData {
                    message: "done".into(),
                },
            )
            .unwrap();
        assert_eq!(output, "\u{f00c} done");
    }

    #[test]
    fn test_renderer_without_icons() {
        let theme = Theme::new().add("ok", Style::new().green());
        let mut renderer = Renderer::with_output(theme, Representation::Human)
            .unwrap()
            .with_color_policy(ColorPolicy::Never);
        renderer
            .add_template("test", "[ok]{{ message }}[/ok]")
            .unwrap();

        let output = renderer
            .render(
                "test",
                &SimpleData {
                    message: "hi".into(),
                },
            )
            .unwrap();
        assert_eq!(output, "hi");
    }

    #[test]
    fn renderer_applies_width_policy_to_filters_and_tables() {
        let template = "{{ '↦≈Δ' | display_width }}|{% set t = tabular([{\"width\": 7}], width=7) %}{{ t.row(['↦≈Δ']) }}";

        let mut narrow = Renderer::with_output(Theme::new(), Representation::Human)
            .unwrap()
            .with_color_policy(ColorPolicy::Never);
        narrow.add_template("width", template).unwrap();
        assert_eq!(
            narrow.render("width", &serde_json::json!({})).unwrap(),
            "3|↦≈Δ    "
        );

        let mut wide = Renderer::with_output(Theme::new(), Representation::Human)
            .unwrap()
            .with_ambiguous_width(AmbiguousWidth::Wide);
        wide.add_template("width", template).unwrap();
        assert_eq!(
            wide.render("width", &serde_json::json!({})).unwrap(),
            "5|↦≈Δ  "
        );
    }
}
