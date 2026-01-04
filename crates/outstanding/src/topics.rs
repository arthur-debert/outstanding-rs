use deunicode::deunicode;
use std::collections::HashMap;
use std::path::Path;
use std::fs;

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
    pub fn new(title: impl Into<String>, content: impl Into<String>, topic_type: TopicType, name: Option<String>) -> Self {
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
        transliterated
            .to_lowercase()
            .replace(' ', "-")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect()
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
            panic!("Topic collision: A topic with the name '{}' already exists.", topic.name);
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
                
                // Content is everything from the next line onwards
                // We join back with explicit newlines
                let body = lines[idx + 1..].join("\n").trim().to_string();
                if body.is_empty() {
                    continue;
                }
                
                // Name is filename sans extension
                let name = path.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string());

                // If name is from file, we use it directly? 
                // "The name is the file name (sans extension). The title is the first non blank line..."
                // "If the name is not set, we will generate it from title..." -> This applies to manual creation?
                // Re-reading user request: "For templates: the name is the file name (sans extension)..."
                // So we pass the filename as the name.
                
                let topic = Topic::new(title, body, topic_type, name);
                self.add_topic(topic);
            }
        }
        Ok(())
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
        assert_eq!(Topic::generate_slug("Testing  123"), "testing--123"); // Simple replace creates double dash, maybe user wants dedupe? 
        // "spaces to dashes" -> strictly interpreted.
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
        let t1 = Topic::new("Same", "Content 1", TopicType::Text, Some("same".to_string()));
        let t2 = Topic::new("Same", "Content 2", TopicType::Text, Some("same".to_string()));
        
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
        assert_eq!(registry.get_topic("intro").unwrap().content, "This is the content.");

        assert!(registry.get_topic("guide").is_some());
        assert_eq!(registry.get_topic("guide").unwrap().topic_type, TopicType::Markdown);

        assert!(registry.get_topic("short").is_none());
        assert!(registry.get_topic("empty_body").is_none());
    }
}
