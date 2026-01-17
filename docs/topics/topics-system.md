# The Topics System

Outstanding provides a dedicated help topics system because command help (`--help`) is a poor place for conceptual documentation.

Arguments and flags describe *mechanics*, but complex applications need longer-form guides for *concepts*—configuration formats, authentication flows, or troubleshooting. The Topics system integrates these directly into your CLI, accessible via `myapp help <topic>`, keeping users in the terminal.

## What Topics Are For

Command help describes flags and arguments. Topics explain broader concepts:

- Configuration file format
- Authentication setup
- Workflow guides
- Troubleshooting

```bash
myapp help                  # Shows commands + available topics
myapp help auth             # Shows the "auth" topic
myapp help config-format    # Shows the "config-format" topic
myapp help auth --page      # Shows topic in a pager
```

## The Topic Struct

```rust
pub struct Topic {
    pub title: String,        // Display title: "Authentication Setup"
    pub content: String,      // Full content
    pub topic_type: TopicType, // Text or Markdown
    pub name: String,         // URL-safe slug: "authentication-setup"
}

pub enum TopicType {
    Text,
    Markdown,
    Unknown,
}
```

The `name` is a URL-safe slug used in `help <name>`. If not provided, it's auto-generated from the title:

- "Hello World" → `hello-world`
- "Café Setup" → `cafe-setup`

## Adding Topics

### Programmatically

```rust
use outstanding::topics::{Topic, TopicType};

let topic = Topic::new(
    "Configuration Format",
    "The config file uses YAML format...",
    TopicType::Text,
    None,  // Auto-generate name from title
);

App::builder()
    .add_topic(topic)
    .build()?
```

### From a Directory

```rust
App::builder()
    .topics_dir("docs/topics")
    .build()?
```

Outstanding scans the directory for `.txt` and `.md` files. File format:

```text
Configuration Format

The config file uses YAML format.
Place it in ~/.myapp/config.yaml.

Supported keys:
- theme: color theme name
- output: default output mode
```

First non-blank line becomes the title. Everything after becomes content. The filename (without extension) becomes the topic name.

Directory structure:

```text
docs/topics/
  config-format.txt      # Topic name: config-format
  authentication.md      # Topic name: authentication
  getting-started.txt    # Topic name: getting-started
```

## TopicRegistry

`TopicRegistry` stores and retrieves topics:

```rust
let mut registry = TopicRegistry::new();
registry.add_topic(topic1);
registry.add_topic(topic2);

// Retrieve
if let Some(topic) = registry.get_topic("config-format") {
    println!("{}", topic.content);
}

// List all (sorted by name)
for topic in registry.list_topics() {
    println!("{}: {}", topic.name, topic.title);
}
```

Duplicate topic names cause a panic—each name must be unique.

## Help Integration

Topics automatically appear in help output:

```text
myapp help

USAGE
  myapp <COMMAND>

COMMANDS
  list       List items
  add        Add an item
  config     Manage configuration

LEARN MORE
  auth               Authentication Setup
  config-format      Configuration Format
  getting-started    Getting Started
```

The "LEARN MORE" section lists all registered topics. Users run `myapp help <topic-name>` to view the full content.

## Pager Support

For long topics, the `--page` flag displays content through a pager:

```bash
myapp help getting-started --page
```

Outstanding tries pagers in order:

1. `$PAGER` environment variable
2. `less`
3. `more`
4. Falls back to printing directly if none available

## Rendering Topics

For custom topic rendering outside the help system:

```rust
use outstanding::topics::{render_topic, render_topics_list, TopicRenderConfig};

// Render single topic
let output = render_topic(&topic, None)?;

// Render list of all topics
let list = render_topics_list(&registry, "myapp help <topic>", None)?;

// With custom config
let config = TopicRenderConfig {
    theme: Some(my_theme),
    output_mode: Some(OutputMode::Text),
    ..Default::default()
};
let output = render_topic(&topic, Some(config))?;
```

## Topic Templates

Topics are rendered through templates with style tags:

**Single topic template:**

```jinja
[header]{{ title | upper }}[/header]

{{ content }}
```

**Topic list template:**

```jinja
[about]Available Topics[/about]

[header]USAGE[/header]
  [usage]{{ usage }}[/usage]

[header]TOPICS[/header]
{%- for topic in topics %}
  [item]{{ topic.name }}[/item]:{{ topic.padding }}[desc]{{ topic.title }}[/desc]
{%- endfor %}
```

Override via `TopicRenderConfig`:

```rust
let config = TopicRenderConfig {
    topic_template: Some(my_template.into()),
    list_template: Some(my_list_template.into()),
    ..Default::default()
};
```

## Markdown Topics

Topics with `.md` extension or `TopicType::Markdown` can contain Markdown formatting. Outstanding renders Markdown appropriately for the terminal when displaying.

```markdown
# Getting Started

Install the application:

    cargo install myapp

Then create a configuration file...
```

The topic type is inferred from file extension when loading from directories.
