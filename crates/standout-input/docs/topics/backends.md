# Input Backends

`standout-input` provides multiple backend implementations for collecting user input. Each backend is a source that can be composed into input chains. This document covers all available backends in detail and explains how to implement custom sources.

---

## The InputCollector Trait

All input sources implement the `InputCollector<T>` trait:

```rust
pub trait InputCollector<T>: Send + Sync {
    /// Human-readable name for this collector (e.g., "argument", "stdin", "editor").
    fn name(&self) -> &'static str;

    /// Check if this collector can provide input in the current environment.
    /// Return false if stdin isn't piped, no TTY for prompts, etc.
    fn is_available(&self, matches: &ArgMatches) -> bool;

    /// Attempt to collect input.
    /// - Ok(Some(value)) — Input collected successfully
    /// - Ok(None) — No input available, try the next source
    /// - Err(e) — Collection failed, abort the chain
    fn collect(&self, matches: &ArgMatches) -> Result<Option<T>, InputError>;

    /// Validate the collected value. Default accepts all values.
    fn validate(&self, _value: &T) -> Result<(), String> {
        Ok(())
    }

    /// Whether this collector supports retry on validation failure.
    /// Interactive sources (prompts, editor) should return true.
    fn can_retry(&self) -> bool {
        false
    }
}
```

`name()` is not only for humans: the chain turns it into the `InputSourceKind` a handler reads back through `ctx.input_source(...)`, matching `argument`, `flag`, `file`, `stdin`, `environment variable`, `config`, `clipboard`, `editor`, `prompt` and `default`. A name outside that set reports `InputSourceKind::Default`, so a custom source that wants a provenance of its own registers with `try_source_with_kind` instead of `try_source`.

The chain calls `is_available()` first. If it returns `false`, the source is skipped. Otherwise, `collect()` is called. If validation fails and `can_retry()` is `true`, the source is retried (for interactive sources).

---

## Non-Interactive Sources

These sources work in any environment, including CI pipelines and scripts.

### ArgSource

Reads a value from a clap CLI argument.

```rust
use standout_input::ArgSource;

let source = ArgSource::new("message");  // Reads --message or -m
```

**Behavior:**

- `is_available()`: Returns `true` if the argument was provided
- `collect()`: Returns `Some(value)` if present, `None` otherwise
- Type: `String`

### FlagSource

Reads a boolean flag from clap.

```rust
use standout_input::FlagSource;

let source = FlagSource::new("verbose");  // Reads --verbose
let source = FlagSource::new("no-color").inverted();  // --no-color → false
```

**Behavior:**

- `is_available()`: Returns `true` if the flag was provided (set to true)
- `inverted()`: Inverts the logic (flag set → `false`)
- Type: `bool`

### StdinSource

Reads from piped stdin. Skipped when stdin is a terminal.

```rust
use standout_input::StdinSource;

let source = StdinSource::new();
let source = StdinSource::new().trim(false);  // Don't trim whitespace
```

**Behavior:**

- `is_available()`: Returns `true` if stdin is piped (not a terminal)
- `collect()`: Reads all stdin content, returns `None` if empty
- `trim`: Whether to trim leading/trailing whitespace (default: `true`)
- Type: `String`

**Testing:**

```rust
use standout_input::{StdinSource, MockStdin};

let source = StdinSource::with_reader(MockStdin::piped("content"));
let source = StdinSource::with_reader(MockStdin::terminal());  // Simulates no pipe
let source = StdinSource::with_reader(MockStdin::piped_empty());
```

### EnvSource

Reads from an environment variable.

```rust
use standout_input::EnvSource;

let source = EnvSource::new("GITHUB_TOKEN");
```

**Behavior:**

- `is_available()`: Returns `true` if the variable is set and non-empty
- `collect()`: Returns `Some(value)` if set, `None` otherwise
- Type: `String`

**Testing:**

```rust
use standout_input::{EnvSource, MockEnv};

let env = MockEnv::new()
    .with_var("API_KEY", "secret")
    .with_var("DEBUG", "1");

let source = EnvSource::with_reader("API_KEY", env);
```

### ClipboardSource

Reads from the system clipboard.

```rust
use standout_input::ClipboardSource;

let source = ClipboardSource::new();
```

**Behavior:**

- `is_available()`: Returns `true` if clipboard has non-empty text content
- `collect()`: Returns clipboard text, `None` if empty
- Platform: Uses `pbpaste` (macOS), `xclip` (Linux)
- Type: `String`

**Testing:**

```rust
use standout_input::{ClipboardSource, MockClipboard};

let source = ClipboardSource::with_reader(MockClipboard::with_content("text"));
let source = ClipboardSource::with_reader(MockClipboard::empty());
```

### ConfigSource

`ConfigSource::new(Option<T>)` carries a value the handler read from its typed
configuration. `None` skips the source; `Some(value)` yields it and reports
`InputSourceKind::Config`. It carries a value rather than a key because the
handler already holds the resolved struct.

### DefaultSource

Provides a fallback value. Always available, always returns its value.

```rust
use standout_input::DefaultSource;

let source = DefaultSource::new("default value".to_string());
let source = DefaultSource::new(42);  // Works with any Clone type
```

**Note:** You can also use `.default(value)` on `InputChain`, which is equivalent to adding a `DefaultSource` at the end.

---

## Editor Backend

**Feature:** `editor` (default)
**Dependencies:** tempfile, which

Opens the user's preferred text editor for multi-line input.

```rust
use standout_input::EditorSource;

let source = EditorSource::new();
```

### Configuration

```rust
let source = EditorSource::new()
    .initial_content("# Enter your message\n\n")  // Pre-populate editor
    .extension(".md")                              // Syntax highlighting
    .require_save(true)                            // Fail if user doesn't save
    .trim(true);                                   // Trim result (default)
```

### Editor Detection

Editors are detected in this order:

1. `$VISUAL` environment variable (supports GUI editors like VS Code)
2. `$EDITOR` environment variable
3. Platform fallbacks: `vim`, `vi`, `nano` on Unix; `notepad` on Windows

### Behavior

- `is_available()`: Returns `true` if an editor is found AND stdin is a terminal
- `collect()`: Opens editor, waits for exit, returns file contents
- `can_retry()`: Returns `true` (validation failures re-open editor)
- Type: `String`

### Testing

```rust
use standout_input::{EditorSource, MockEditorRunner, MockEditorResult};

// Simulate successful edit
let source = EditorSource::with_runner(MockEditorRunner::with_result("user content"));

// Simulate no editor available
let source = EditorSource::with_runner(MockEditorRunner::no_editor());

// Simulate editor failure
let source = EditorSource::with_runner(MockEditorRunner::failure("editor crashed"));

// Simulate closing without saving
let source = EditorSource::with_runner(MockEditorRunner::no_save());
```

### Custom Editor Runner

Implement `EditorRunner` for custom editor behavior:

```rust
pub trait EditorRunner: Send + Sync {
    /// Detect the editor to use. Returns None if no editor is available.
    fn detect_editor(&self) -> Option<String>;

    /// Run the editor on the given file path.
    fn run(&self, editor: &str, path: &Path) -> io::Result<()>;
}
```

---

## Simple Prompts Backend

**Feature:** `simple-prompts` (default)
**Dependencies:** none

Basic terminal prompts without external dependencies.

### TextPromptSource

Simple text input prompt.

```rust
use standout_input::TextPromptSource;

let source = TextPromptSource::new("Enter your name: ");
let source = TextPromptSource::new("Email: ").trim(false);
```

**Behavior:**

- `is_available()`: Returns `true` if stdin is a terminal
- `collect()`: Prints prompt, reads line, returns `None` if empty
- `can_retry()`: Returns `true`
- Type: `String`

**Testing:**

```rust
use standout_input::{TextPromptSource, MockTerminal};

let source = TextPromptSource::with_terminal("Name: ", MockTerminal::with_response("Alice"));

// Multiple responses for retry testing
let terminal = MockTerminal::with_responses(["", "Bob"]);  // Empty first, then "Bob"
let source = TextPromptSource::with_terminal("Name: ", terminal);

// Simulate EOF (Ctrl+D)
let source = TextPromptSource::with_terminal("Name: ", MockTerminal::eof());
```

### ConfirmPromptSource

Yes/no confirmation prompt.

```rust
use standout_input::ConfirmPromptSource;

let source = ConfirmPromptSource::new("Proceed?");
let source = ConfirmPromptSource::new("Delete all?").default(false);
```

**Behavior:**

- `is_available()`: Returns `true` if stdin is a terminal
- `collect()`: Prints prompt with `[y/n]`, `[Y/n]`, or `[y/N]` suffix based on default
- Accepts: `y`, `yes`, `Y`, `YES` → `true`; `n`, `no`, `N`, `NO` → `false`
- Invalid input returns `ValidationFailed` error (triggers retry)
- Empty input uses default if set, otherwise returns `None`
- `can_retry()`: Returns `true`
- Type: `bool`

**Testing:**

```rust
use standout_input::{ConfirmPromptSource, MockTerminal};

let source = ConfirmPromptSource::with_terminal("OK?", MockTerminal::with_response("y"));
let source = ConfirmPromptSource::with_terminal("OK?", MockTerminal::with_response("no"));
```

### Custom Terminal IO

Implement `TerminalIO` for custom terminal behavior:

```rust
pub trait TerminalIO: Send + Sync {
    /// Check if stdin is a terminal.
    fn is_terminal(&self) -> bool;

    /// Write a prompt to stdout.
    fn write_prompt(&self, prompt: &str) -> io::Result<()>;

    /// Read a line from stdin.
    fn read_line(&self) -> io::Result<String>;
}
```

---

## Inquire Backend

**Feature:** `inquire`
**Dependencies:** inquire crate (~29 dependencies)

Rich TUI prompts with arrow-key navigation, autocomplete, and visual feedback.

### InquireText

Text input with autocomplete and help messages.

```rust
use standout_input::InquireText;

let source = InquireText::new("What is your name?")
    .default("Anonymous")
    .placeholder("Your name...")
    .help("Enter your full name");
```

### InquireConfirm

Polished yes/no prompt.

```rust
use standout_input::InquireConfirm;

let source = InquireConfirm::new("Proceed with deployment?")
    .default(false)
    .help("This will deploy to production");
```

### InquireSelect

Single selection from a list with arrow-key navigation.

```rust
use standout_input::InquireSelect;

let source = InquireSelect::new("Choose environment:", vec![
    "development",
    "staging",
    "production",
])
.help("Use arrow keys to select")
.page_size(5);
```

**Type:** Returns the selected item's type (`T`)

### InquireMultiSelect

Multiple selection with checkboxes.

```rust
use standout_input::InquireMultiSelect;

let source = InquireMultiSelect::new("Select features:", vec![
    "logging",
    "metrics",
    "tracing",
    "profiling",
])
.help("Space to toggle, Enter to confirm")
.min_selections(1)
.max_selections(3)
.page_size(10);
```

**Type:** Returns `Vec<T>` of selected items

### InquirePassword

Secure password input with masking.

```rust
use standout_input::InquirePassword;

let source = InquirePassword::new("API token:")
    .help("Your token won't be displayed")
    .masked()                                    // Show asterisks (default)
    .with_confirmation("Confirm token:");       // Require confirmation

// Display modes
let source = InquirePassword::new("Password:").hidden();  // No characters shown
let source = InquirePassword::new("Password:").full();    // Show password as typed
```

### InquireEditor

Editor with preview in the terminal.

```rust
use standout_input::InquireEditor;

let source = InquireEditor::new("Enter commit message:")
    .help("Press Enter to open editor")
    .extension(".md")
    .predefined_text("# Summary\n\n# Details\n");
```

### Testing Inquire Sources

Inquire prompts are interactive and require a real terminal. For testing, use the simpler backends or test at the integration level with `MockTerminal` equivalents.

---

## Implementing Custom Sources

Create custom sources by implementing `InputCollector<T>`:

```rust
use standout_input::{InputCollector, InputError};
use clap::ArgMatches;

/// Read from a configuration file.
struct ConfigFileSource {
    key: String,
    path: PathBuf,
}

impl ConfigFileSource {
    pub fn new(key: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            key: key.into(),
            path: path.into(),
        }
    }
}

impl InputCollector<String> for ConfigFileSource {
    fn name(&self) -> &'static str {
        "config file"
    }

    fn is_available(&self, _matches: &ArgMatches) -> bool {
        self.path.exists()
    }

    fn collect(&self, _matches: &ArgMatches) -> Result<Option<String>, InputError> {
        let content = std::fs::read_to_string(&self.path)
            .map_err(|e| InputError::PromptFailed(e.to_string()))?;

        // Parse as TOML and extract key
        let config: toml::Value = toml::from_str(&content)
            .map_err(|e| InputError::PromptFailed(e.to_string()))?;

        match config.get(&self.key) {
            Some(toml::Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(InputError::ValidationFailed(
                format!("Config key '{}' is not a string", self.key)
            )),
            None => Ok(None),
        }
    }
}

// Usage
let source = ConfigFileSource::new("api_key", "~/.myapp/config.toml");
```

### Making Sources Testable

Use the generic pattern to inject mock implementations:

```rust
use std::sync::Arc;

pub trait ConfigReader: Send + Sync {
    fn read(&self, key: &str) -> Result<Option<String>, InputError>;
    fn exists(&self) -> bool;
}

pub struct ConfigFileSource<R: ConfigReader = RealConfigReader> {
    reader: Arc<R>,
    key: String,
}

impl ConfigFileSource<RealConfigReader> {
    pub fn new(key: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            reader: Arc::new(RealConfigReader::new(path)),
            key: key.into(),
        }
    }
}

impl<R: ConfigReader> ConfigFileSource<R> {
    pub fn with_reader(key: impl Into<String>, reader: R) -> Self {
        Self {
            reader: Arc::new(reader),
            key: key.into(),
        }
    }
}

// Mock for testing
pub struct MockConfigReader {
    values: HashMap<String, String>,
}

impl MockConfigReader {
    pub fn new() -> Self {
        Self { values: HashMap::new() }
    }

    pub fn with_value(mut self, key: &str, value: &str) -> Self {
        self.values.insert(key.to_string(), value.to_string());
        self
    }
}

impl ConfigReader for MockConfigReader {
    fn read(&self, key: &str) -> Result<Option<String>, InputError> {
        Ok(self.values.get(key).cloned())
    }

    fn exists(&self) -> bool {
        true
    }
}

// Test
#[test]
fn test_config_source() {
    let reader = MockConfigReader::new().with_value("token", "secret123");
    let source = ConfigFileSource::with_reader("token", reader);

    let result = source.collect(&empty_matches()).unwrap();
    assert_eq!(result, Some("secret123".to_string()));
}
```

---

## Summary

| Backend | Feature | Dependencies | Sources |
| --------- | --------- | -------------- | --------- |
| Core | always | clap, thiserror | ArgSource, FlagSource, StdinSource, EnvSource, ClipboardSource, DefaultSource |
| Editor | `editor` | tempfile, which | EditorSource |
| Simple Prompts | `simple-prompts` | none | TextPromptSource, ConfirmPromptSource |
| Inquire | `inquire` | inquire | InquireText, InquireConfirm, InquireSelect, InquireMultiSelect, InquirePassword, InquireEditor |

All sources follow the same pattern:

1. Implement `InputCollector<T>`
2. Accept a mock via `with_reader()` or `with_runner()`
3. Return `Ok(None)` to pass to the next source in the chain
4. Return `Ok(Some(value))` when input is collected
5. Return `Err(...)` to abort the chain with an error
