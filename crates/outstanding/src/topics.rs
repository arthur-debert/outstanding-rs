use deunicode::deunicode;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command as ProcessCommand, Stdio};

use console::Style;
use serde::Serialize;

use crate::{render_with_output, Error, OutputMode, Theme};

/// Fixed width for the name column in topic listings.
const NAME_COLUMN_WIDTH: usize = 14;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopicType {
    Text,
    Markdown,
    Unknown,
}

impl Default for TopicType {
    fn default() -> Self {
        Self::Text
    }
}

#[derive(Debug, Clone)]
pub struct Topic {
    pub title: String,
    pub content: String,
    pub topic_type: TopicType,
    pub name: String,
}

impl Topic {
    /// Creates a new topic.
    /// If name is None, it is generated from the title.
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
        // Collapse consecutive dashes
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

    /// Adds a topic to the registry.
    /// Panics if a topic with the same name already exists.
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

    /// Adds topics from files in the specified directory.
    /// Only .txt and .md files are processed.
    /// Empty files or files with only one line are ignored.
    /// Returns an error if the path does not exist or is not a directory.
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

    /// Adds topics from files in the specified directory if it exists.
    /// Silently ignores non-existent paths.
    /// Only .txt and .md files are processed.
    /// Empty files or files with only one line are ignored.
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

            // Skip empty or single-line files
            if lines.len() < 2 {
                continue;
            }

            // Title is first non-blank line
            let title_idx = lines.iter().position(|l| !l.trim().is_empty());
            if let Some(idx) = title_idx {
                let title = lines[idx].trim().to_string();

                // Content starts after title, skipping any leading blank lines
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

                // Name is filename sans extension
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

// ============================================================================
// TOPIC RENDERING
// ============================================================================

/// Configuration for topic rendering.
#[derive(Debug, Clone, Default)]
pub struct TopicRenderConfig {
    /// Custom template string for single topic. If None, uses built-in template.
    pub topic_template: Option<String>,
    /// Custom template string for topic list. If None, uses built-in template.
    pub list_template: Option<String>,
    /// Custom theme. If None, uses the default topic theme.
    pub theme: Option<Theme>,
    /// Output mode. If None, uses Auto (auto-detects).
    pub output_mode: Option<OutputMode>,
}

/// Returns the default theme for topic rendering.
pub fn default_topic_theme() -> Theme {
    Theme::new()
        .add("header", Style::new().bold())
        .add("item", Style::new().bold())
        .add("desc", Style::new())
        .add("usage", Style::new())
        .add("about", Style::new())
}

#[derive(Serialize)]
struct TopicData {
    title: String,
    content: String,
}

#[derive(Serialize)]
struct TopicsListData {
    usage: String,
    topics: Vec<TopicListItem>,
}

#[derive(Serialize)]
struct TopicListItem {
    name: String,
    title: String,
    padding: String,
}

/// Renders a single topic using outstanding templating.
///
/// # Example
///
/// ```rust
/// use outstanding::topics::{Topic, TopicType, render_topic, TopicRenderConfig};
///
/// let topic = Topic::new(
///     "Storage",
///     "Notes are stored in ~/.notes/\n\nEach note is a separate file.",
///     TopicType::Text,
///     Some("storage".to_string()),
/// );
///
/// let output = render_topic(&topic, None).unwrap();
/// println!("{}", output);
/// ```
pub fn render_topic(topic: &Topic, config: Option<TopicRenderConfig>) -> Result<String, Error> {
    let config = config.unwrap_or_default();
    let template = config
        .topic_template
        .as_deref()
        .unwrap_or(include_str!("topic_template.txt"));

    let theme = config.theme.unwrap_or_else(default_topic_theme);
    let mode = config.output_mode.unwrap_or(OutputMode::Auto);

    let data = TopicData {
        title: topic.title.clone(),
        content: topic.content.clone(),
    };

    render_with_output(template, &data, &theme, mode)
}

/// Renders a list of all available topics.
///
/// # Arguments
///
/// * `registry` - The topic registry containing all topics
/// * `usage_prefix` - The command prefix for usage display (e.g., "myapp help")
/// * `config` - Optional rendering configuration
///
/// # Example
///
/// ```rust
/// use outstanding::topics::{TopicRegistry, Topic, TopicType, render_topics_list};
///
/// let mut registry = TopicRegistry::new();
/// registry.add_topic(Topic::new("Storage", "Where data is stored", TopicType::Text, None));
/// registry.add_topic(Topic::new("Syntax", "Note syntax reference", TopicType::Text, None));
///
/// let output = render_topics_list(&registry, "myapp help", None).unwrap();
/// println!("{}", output);
/// ```
pub fn render_topics_list(
    registry: &TopicRegistry,
    usage_prefix: &str,
    config: Option<TopicRenderConfig>,
) -> Result<String, Error> {
    let config = config.unwrap_or_default();
    let template = config
        .list_template
        .as_deref()
        .unwrap_or(include_str!("topics_list_template.txt"));

    let theme = config.theme.unwrap_or_else(default_topic_theme);
    let mode = config.output_mode.unwrap_or(OutputMode::Auto);

    let topics = registry.list_topics();

    let topic_items: Vec<TopicListItem> = topics
        .iter()
        .map(|t| {
            // +1 accounts for the colon added in the template
            let pad = NAME_COLUMN_WIDTH.saturating_sub(t.name.len() + 1);
            TopicListItem {
                name: t.name.clone(),
                title: t.title.clone(),
                padding: " ".repeat(pad),
            }
        })
        .collect();

    let data = TopicsListData {
        usage: format!("{} <topic>", usage_prefix),
        topics: topic_items,
    };

    render_with_output(template, &data, &theme, mode)
}

// ============================================================================
// PAGER SUPPORT
// ============================================================================

/// Displays content through a pager.
///
/// Tries pagers in this order:
/// 1. `$PAGER` environment variable
/// 2. `less`
/// 3. `more`
///
/// If all pagers fail, falls back to printing directly to stdout.
///
/// # Example
///
/// ```rust,no_run
/// use outstanding::topics::display_with_pager;
///
/// let long_content = "Line 1\nLine 2\n...";
/// display_with_pager(long_content).unwrap();
/// ```
pub fn display_with_pager(content: &str) -> std::io::Result<()> {
    let pagers = get_pager_candidates();

    for pager in pagers {
        if try_pager(&pager, content).is_ok() {
            return Ok(());
        }
    }

    // Fallback: print directly
    print!("{}", content);
    std::io::stdout().flush()
}

/// Returns the list of pager candidates to try.
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

/// Attempts to run content through a specific pager.
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
        assert_eq!(Topic::generate_slug("Testing  123"), "testing-123"); // Consecutive dashes are collapsed
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

        // Good file
        let p1 = dir.path().join("intro.txt");
        let mut f1 = File::create(&p1).unwrap();
        writeln!(f1, "Introduction\nThis is the content.").unwrap();

        // Markdown file
        let p2 = dir.path().join("guide.md");
        let mut f2 = File::create(&p2).unwrap();
        writeln!(f2, "Guide Title\n# Header\nBody").unwrap();

        // Too short
        let p3 = dir.path().join("short.txt");
        let mut f3 = File::create(&p3).unwrap();
        writeln!(f3, "One line only").unwrap();

        // Empty body (title found but no content after)
        let p4 = dir.path().join("empty_body.txt");
        let mut f4 = File::create(&p4).unwrap();
        writeln!(f4, "Just Title\n").unwrap(); // Trim might make body empty if only newline

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
        // Should succeed silently for non-existent directory
        let result = registry.add_from_directory_if_exists("/nonexistent/path");
        assert!(result.is_ok());
        assert_eq!(registry.list_topics().len(), 0);
    }

    #[test]
    #[should_panic(expected = "Topic collision")]
    fn test_directory_collision() {
        let dir1 = tempdir().unwrap();
        let dir2 = tempdir().unwrap();

        // Same filename in both directories
        let p1 = dir1.path().join("shared.txt");
        let mut f1 = File::create(&p1).unwrap();
        writeln!(f1, "Title 1\nContent 1").unwrap();

        let p2 = dir2.path().join("shared.txt");
        let mut f2 = File::create(&p2).unwrap();
        writeln!(f2, "Title 2\nContent 2").unwrap();

        let mut registry = TopicRegistry::new();
        registry.add_from_directory(dir1.path()).unwrap();
        registry.add_from_directory(dir2.path()).unwrap(); // Should panic
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
    fn test_get_pager_candidates_default() {
        std::env::remove_var("PAGER");
        let candidates = get_pager_candidates();
        assert_eq!(candidates, vec!["less", "more"]);
    }

    #[test]
    fn test_get_pager_candidates_with_env() {
        std::env::set_var("PAGER", "bat");
        let candidates = get_pager_candidates();
        assert_eq!(candidates[0], "bat");
        assert_eq!(candidates[1], "less");
        std::env::remove_var("PAGER");
    }
}
