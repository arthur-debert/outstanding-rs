//! Help topics: documentation beyond `--help` (guides, tutorials, reference
//! material) accessible via `myapp help <topic>`, either registered inline
//! or loaded from a directory of `.txt`/`.md` files (first non-blank line is
//! the title; filename minus extension is the topic name).
//!
//! ```rust
//! use standout::topics::{Topic, TopicRegistry, TopicType, render_topic};
//!
//! let mut registry = TopicRegistry::new();
//! registry.add_topic(Topic::new("Storage", "Notes live in ~/.notes/", TopicType::Text, Some("storage".into())));
//! let output = render_topic(registry.get_topic("storage").unwrap(), None).unwrap();
//! ```
//!
//! Key types: [`Topic`], [`TopicRegistry`], [`TopicType`], [`render_topic`] /
//! [`render_topics_list`], [`display_with_pager`] (show long content through
//! less/more).

use deunicode::deunicode;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command as ProcessCommand, Stdio};

use console::Style;
use serde::Serialize;

use crate::assets::{TOPICS_LIST_TEMPLATE_NAME, TOPIC_TEMPLATE_NAME};
use crate::cli::help::data::{resolve_name_column, ASSUMED_TERMINAL_WIDTH};
use crate::cli::help::{inline_template_ref, render_via_request};
use crate::{
    default_template_engine, OutputMode, RenderError, TargetProperties, TemplateRef, Theme,
};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TopicType {
    #[default]
    Text,
    Markdown,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Topic {
    pub title: String,
    pub content: String,
    pub topic_type: TopicType,
    pub name: String,
}

impl Topic {
    pub fn new(
        title: impl Into<String>,
        content: impl Into<String>,
        topic_type: TopicType,
        name: Option<String>,
    ) -> Self {
        let title = title.into();
        let name = name.unwrap_or_else(|| Self::generate_slug(&title));

        Self {
            title,
            content: content.into(),
            topic_type,
            name,
        }
    }

    fn generate_slug(title: &str) -> String {
        let transliterated = deunicode(title);
        let mut slug: String = transliterated
            .to_lowercase()
            .replace(' ', "-")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        while slug.contains("--") {
            slug = slug.replace("--", "-");
        }
        slug
    }
}

#[derive(Default, Clone)]
pub struct TopicRegistry {
    topics: HashMap<String, Topic>,
}

impl TopicRegistry {
    pub fn new() -> Self {
        Self {
            topics: HashMap::new(),
        }
    }

    pub fn add_topic(&mut self, topic: Topic) {
        if self.topics.contains_key(&topic.name) {
            panic!(
                "Topic collision: A topic with the name '{}' already exists.",
                topic.name
            );
        }
        self.topics.insert(topic.name.clone(), topic);
    }

    pub fn get_topic(&self, name: &str) -> Option<&Topic> {
        self.topics.get(name)
    }

    pub fn list_topics(&self) -> Vec<&Topic> {
        let mut topics: Vec<&Topic> = self.topics.values().collect();
        topics.sort_by(|a, b| a.name.cmp(&b.name));
        topics
    }

    pub fn add_from_directory(&mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let path = path.as_ref();
        if !path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Directory not found: {}", path.display()),
            ));
        }
        if !path.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Path is not a directory: {}", path.display()),
            ));
        }
        self.load_from_directory(path)
    }

    pub fn add_from_directory_if_exists(&mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let path = path.as_ref();
        if !path.exists() || !path.is_dir() {
            return Ok(());
        }
        self.load_from_directory(path)
    }

    fn load_from_directory(&mut self, path: &Path) -> std::io::Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let topic_type = match extension {
                "txt" => TopicType::Text,
                "md" => TopicType::Markdown,
                _ => continue,
            };

            let content = fs::read_to_string(&path)?;
            let lines: Vec<&str> = content.lines().collect();

            if lines.len() < 2 {
                continue;
            }

            let title_idx = lines.iter().position(|l| !l.trim().is_empty());
            if let Some(idx) = title_idx {
                let title = lines[idx].trim().to_string();

                let content_lines = &lines[idx + 1..];
                let content_start = content_lines
                    .iter()
                    .position(|l| !l.trim().is_empty())
                    .unwrap_or(content_lines.len());

                let body = content_lines[content_start..]
                    .join("\n")
                    .trim_end()
                    .to_string();
                if body.is_empty() {
                    continue;
                }

                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string());

                let topic = Topic::new(title, body, topic_type, name);
                self.add_topic(topic);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct TopicRenderConfig {
    pub topic_template: Option<String>,
    pub list_template: Option<String>,
    pub theme: Option<Theme>,
    pub output_mode: Option<OutputMode>,
}

pub fn default_topic_theme() -> Theme {
    Theme::new()
        .add("header", Style::new().bold())
        .add("item", Style::new().bold())
        .add("desc", Style::new())
        .add("usage", Style::new())
        .add("about", Style::new())
}

fn resolve_topic_theme(configured: Option<Theme>) -> Theme {
    match configured {
        Some(theme) => default_topic_theme().merge(theme),
        None => default_topic_theme(),
    }
}

#[derive(Serialize)]
pub(crate) struct TopicData {
    title: String,
    content: String,
}

#[derive(Serialize)]
pub(crate) struct TopicsListData {
    usage: String,
    topics: Vec<TopicListItem>,
    name_width: usize,
}

#[derive(Serialize)]
struct TopicListItem {
    name: String,
    title: String,
}

pub(crate) const DEFAULT_TOPIC_TEMPLATE: &str = include_str!("topic_template.txt");
pub(crate) const DEFAULT_TOPICS_LIST_TEMPLATE: &str = include_str!("topics_list_template.txt");

pub(crate) fn topic_data(topic: &Topic) -> TopicData {
    TopicData {
        title: topic.title.clone(),
        content: topic.content.clone(),
    }
}

pub(crate) fn topics_list_data(
    registry: &TopicRegistry,
    usage_prefix: &str,
    target: &TargetProperties,
) -> TopicsListData {
    let topics = registry.list_topics();
    let topic_items: Vec<TopicListItem> = topics
        .iter()
        .map(|t| TopicListItem {
            name: t.name.clone(),
            title: t.title.clone(),
        })
        .collect();
    let name_width = resolve_name_column(
        &topic_items
            .iter()
            .map(|topic| topic.name.as_str())
            .collect::<Vec<_>>(),
        target.width.unwrap_or(ASSUMED_TERMINAL_WIDTH),
        target.ambiguous_width,
    );
    TopicsListData {
        usage: format!("{} <topic>", usage_prefix),
        topics: topic_items,
        name_width,
    }
}

fn standalone_topic_template(
    configured: Option<&str>,
    default_source: &str,
    named: &str,
    theme: &Theme,
) -> Result<TemplateRef, RenderError> {
    match configured {
        Some(source) => inline_template_ref(source, theme, named),
        None => inline_template_ref(default_source, theme, named),
    }
}

pub fn render_topic(
    topic: &Topic,
    config: Option<TopicRenderConfig>,
) -> Result<String, RenderError> {
    let config = config.unwrap_or_default();
    let theme = resolve_topic_theme(config.theme);
    let template = standalone_topic_template(
        config.topic_template.as_deref(),
        DEFAULT_TOPIC_TEMPLATE,
        TOPIC_TEMPLATE_NAME,
        &theme,
    )?;
    render_via_request(
        &topic_data(topic),
        template,
        theme,
        config.output_mode.unwrap_or(OutputMode::Auto),
        TargetProperties::detect(),
        default_template_engine(),
        None,
        None,
        None,
    )
}

pub fn render_topics_list(
    registry: &TopicRegistry,
    usage_prefix: &str,
    config: Option<TopicRenderConfig>,
) -> Result<String, RenderError> {
    let config = config.unwrap_or_default();
    let theme = resolve_topic_theme(config.theme);
    let template = standalone_topic_template(
        config.list_template.as_deref(),
        DEFAULT_TOPICS_LIST_TEMPLATE,
        TOPICS_LIST_TEMPLATE_NAME,
        &theme,
    )?;
    let target = TargetProperties::detect();
    render_via_request(
        &topics_list_data(registry, usage_prefix, &target),
        template,
        theme,
        config.output_mode.unwrap_or(OutputMode::Auto),
        target,
        default_template_engine(),
        None,
        None,
        None,
    )
}

pub fn display_with_pager(content: &str) -> std::io::Result<()> {
    let pagers = get_pager_candidates();

    for pager in pagers {
        if try_pager(&pager, content).is_ok() {
            return Ok(());
        }
    }

    print!("{}", content);
    std::io::stdout().flush()
}

fn get_pager_candidates() -> Vec<String> {
    let mut pagers = Vec::new();

    if let Ok(pager) = std::env::var("PAGER") {
        if !pager.is_empty() {
            pagers.push(pager);
        }
    }

    pagers.push("less".to_string());
    pagers.push("more".to_string());

    pagers
}

fn try_pager(pager: &str, content: &str) -> std::io::Result<()> {
    let mut child = ProcessCommand::new(pager).stdin(Stdio::piped()).spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(content.as_bytes())?;
    }

    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("pager exited with error"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_slug_generation() {
        assert_eq!(Topic::generate_slug("Hello World"), "hello-world");
        assert_eq!(Topic::generate_slug("Testing  123"), "testing-123");
        assert_eq!(Topic::generate_slug("Olá Mundo"), "ola-mundo");
        assert_eq!(Topic::generate_slug("Café"), "cafe");
    }

    #[test]
    fn test_topic_registration() {
        let mut registry = TopicRegistry::new();
        let topic = Topic::new("My Topic", "Content", TopicType::Text, None);
        registry.add_topic(topic);

        assert!(registry.get_topic("my-topic").is_some());
    }

    #[test]
    #[should_panic(expected = "Topic collision")]
    fn test_collision_panic() {
        let mut registry = TopicRegistry::new();
        let t1 = Topic::new(
            "Same",
            "Content 1",
            TopicType::Text,
            Some("same".to_string()),
        );
        let t2 = Topic::new(
            "Same",
            "Content 2",
            TopicType::Text,
            Some("same".to_string()),
        );

        registry.add_topic(t1);
        registry.add_topic(t2);
    }

    #[test]
    fn test_load_from_dir() {
        let dir = tempdir().unwrap();

        let p1 = dir.path().join("intro.txt");
        let mut f1 = File::create(&p1).unwrap();
        writeln!(f1, "Introduction\nThis is the content.").unwrap();

        let p2 = dir.path().join("guide.md");
        let mut f2 = File::create(&p2).unwrap();
        writeln!(f2, "Guide Title\n# Header\nBody").unwrap();

        let p3 = dir.path().join("short.txt");
        let mut f3 = File::create(&p3).unwrap();
        writeln!(f3, "One line only").unwrap();

        let p4 = dir.path().join("empty_body.txt");
        let mut f4 = File::create(&p4).unwrap();
        writeln!(f4, "Just Title\n").unwrap();

        let mut registry = TopicRegistry::new();
        registry.add_from_directory(dir.path()).unwrap();

        assert!(registry.get_topic("intro").is_some());
        assert_eq!(registry.get_topic("intro").unwrap().title, "Introduction");
        assert_eq!(
            registry.get_topic("intro").unwrap().content,
            "This is the content."
        );

        assert!(registry.get_topic("guide").is_some());
        assert_eq!(
            registry.get_topic("guide").unwrap().topic_type,
            TopicType::Markdown
        );

        assert!(registry.get_topic("short").is_none());
        assert!(registry.get_topic("empty_body").is_none());
    }

    #[test]
    fn test_add_from_nonexistent_directory() {
        let mut registry = TopicRegistry::new();
        let result = registry.add_from_directory("/nonexistent/path/that/does/not/exist");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn test_add_from_directory_if_exists_nonexistent() {
        let mut registry = TopicRegistry::new();
        let result = registry.add_from_directory_if_exists("/nonexistent/path");
        assert!(result.is_ok());
        assert_eq!(registry.list_topics().len(), 0);
    }

    #[test]
    #[should_panic(expected = "Topic collision")]
    fn test_directory_collision() {
        let dir1 = tempdir().unwrap();
        let dir2 = tempdir().unwrap();

        let p1 = dir1.path().join("shared.txt");
        let mut f1 = File::create(&p1).unwrap();
        writeln!(f1, "Title 1\nContent 1").unwrap();

        let p2 = dir2.path().join("shared.txt");
        let mut f2 = File::create(&p2).unwrap();
        writeln!(f2, "Title 2\nContent 2").unwrap();

        let mut registry = TopicRegistry::new();
        registry.add_from_directory(dir1.path()).unwrap();
        registry.add_from_directory(dir2.path()).unwrap();
    }

    #[test]
    fn test_render_topic_basic() {
        let topic = Topic::new(
            "Test Topic",
            "This is the content.",
            TopicType::Text,
            Some("test".to_string()),
        );

        let config = TopicRenderConfig {
            output_mode: Some(crate::OutputMode::Text),
            ..Default::default()
        };

        let output = render_topic(&topic, Some(config)).unwrap();
        assert!(output.contains("TEST TOPIC"));
        assert!(output.contains("This is the content."));
    }

    #[test]
    fn test_render_topics_list_basic() {
        let mut registry = TopicRegistry::new();
        registry.add_topic(Topic::new(
            "Storage",
            "Where data lives",
            TopicType::Text,
            None,
        ));
        registry.add_topic(Topic::new(
            "Syntax",
            "Format reference",
            TopicType::Text,
            None,
        ));

        let config = TopicRenderConfig {
            output_mode: Some(crate::OutputMode::Text),
            ..Default::default()
        };

        let output = render_topics_list(&registry, "myapp help", Some(config)).unwrap();
        assert!(output.contains("Available Topics"));
        assert!(output.contains("storage"));
        assert!(output.contains("syntax"));
        assert!(output.contains("myapp help <topic>"));
    }

    #[test]
    fn test_render_topics_list_long_name_keeps_separator() {
        let mut registry = TopicRegistry::new();
        registry.add_topic(Topic::new(
            "A Very Long Topic Name Here",
            "content",
            TopicType::Text,
            None,
        ));
        registry.add_topic(Topic::new("Short", "content", TopicType::Text, None));

        let config = TopicRenderConfig {
            output_mode: Some(crate::OutputMode::Text),
            ..Default::default()
        };

        let output = render_topics_list(&registry, "myapp help", Some(config)).unwrap();

        let long = row_containing(&output, "a-very-long-topic-name-here");
        assert!(
            long.contains("here  A Very Long Topic Name Here"),
            "the longest name must keep the column separator:\n{}",
            output
        );

        let short = row_containing(&output, "short");
        assert_eq!(
            long.find("A Very Long Topic Name Here"),
            short.find("Short"),
            "topic titles must align:\n{}",
            output
        );
    }

    fn row_containing<'a>(output: &'a str, needle: &str) -> &'a str {
        output
            .lines()
            .find(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("no row for {needle} in:\n{output}"))
    }

    #[test]
    fn test_get_pager_candidates_includes_defaults() {
        let candidates = get_pager_candidates();
        assert!(candidates.contains(&"less".to_string()));
        assert!(candidates.contains(&"more".to_string()));
    }

    #[test]
    fn structured_modes_still_print_human_topics() {
        let topic = Topic::new(
            "Storage",
            "Where data lives.",
            TopicType::Text,
            Some("storage".to_string()),
        );
        for mode in [
            crate::OutputMode::Json,
            crate::OutputMode::Yaml,
            crate::OutputMode::Csv,
            crate::OutputMode::Ndjson,
        ] {
            let output = render_topic(
                &topic,
                Some(TopicRenderConfig {
                    output_mode: Some(mode),
                    ..Default::default()
                }),
            )
            .unwrap();
            assert!(
                output.contains("STORAGE") || output.contains("Storage"),
                "{mode:?} must print human topic text, got:\n{output}"
            );
            assert!(
                !output.trim_start().starts_with('{'),
                "{mode:?} must not emit a JSON topic document:\n{output}"
            );
        }
    }

    #[test]
    fn custom_topic_template_unknown_tag_fails_at_construction() {
        let topic = Topic::new("T", "c", TopicType::Text, Some("t".to_string()));
        let err = render_topic(
            &topic,
            Some(TopicRenderConfig {
                topic_template: Some("[nope]x[/nope]".into()),
                output_mode: Some(crate::OutputMode::Text),
                ..Default::default()
            }),
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("nope"), "{msg}");
    }
}
