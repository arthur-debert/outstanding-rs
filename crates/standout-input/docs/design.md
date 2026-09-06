# standout-input Design

## Overview

`standout-input` is a standalone crate for declarative input collection in CLI applications. It provides a unified way to acquire user input from multiple sources—CLI arguments, stdin, environment variables, editors, and interactive prompts—with automatic fallback chains.

This is the symmetric counterpart to `standout-pipe`:

```text
standout-pipe:   Handler → Render → [jq/tee/clipboard]   (output flows OUT)
standout-input:  [arg/stdin/editor/prompt] → Handler     (input flows IN)
```

## Goals

1. **Declarative input chains** - Define fallback sequences (arg → stdin → editor) without imperative logic
2. **Pluggable backends** - Support multiple prompt libraries via a common trait
3. **Minimal by default** - Core has ~2 deps; heavy backends are opt-in via features
4. **Standalone value** - Useful for any CLI, not just standout users
5. **Validation integration** - Chain-level and collector-level validation with retry support
6. **Testable** - Handlers receive resolved content; sources can be mocked

## Architecture

```text
┌─────────────────────────────────────────────────────────────────┐
│                    standout-input (core)                         │
│                                                                  │
│  InputCollector<T> trait     InputChain<T> builder               │
│  ArgSource, StdinSource      EnvSource, ClipboardSource          │
│  DefaultSource<T>            Validation hooks                    │
│                                                                  │
│  feature = "simple-prompts"  feature = "editor"                  │
│  ├── SimpleText              └── EditorCollector                 │
│  └── SimpleConfirm               (tempfile + which)              │
│                                                                  │
│  feature = "inquire"         feature = "dialoguer" (future)      │
│  ├── InquireText             ├── DialoguerText                   │
│  ├── InquireConfirm          ├── DialoguerConfirm                │
│  ├── InquireSelect           ├── DialoguerSelect                 │
│  ├── InquireMultiSelect      └── DialoguerEditor                 │
│  ├── InquirePassword                                             │
│  └── InquireEditor                                               │
│                                                                  │
│  feature = "validify"                                            │
│  └── Validify rule integration                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Core Trait

```rust
/// A source that can collect input of type T.
pub trait InputCollector<T>: Send + Sync {
    /// Human-readable name for errors and debugging.
    fn name(&self) -> &'static str;

    /// Can this collector provide input in the current environment?
    ///
    /// Returns false if:
    /// - Interactive collector but no TTY
    /// - Stdin source but stdin is not piped
    /// - Arg source but argument not provided
    fn is_available(&self, matches: &ArgMatches) -> bool;

    /// Attempt to collect input.
    ///
    /// Returns:
    /// - Ok(Some(value)) if input was collected
    /// - Ok(None) if this source should be skipped (try next in chain)
    /// - Err(_) on failure (abort the chain)
    fn collect(&self, matches: &ArgMatches) -> Result<Option<T>, InputError>;

    /// Rebuild this collector over the run's stdin, clipboard and responder.
    /// Returns None when the collector reads none of them.
    fn bind_sources(&self, sources: &InputSources) -> Option<Box<dyn InputCollector<T>>>;

    /// Validate collected value. Called after successful collect().
    /// Default implementation accepts all values.
    fn validate(&self, _value: &T) -> Result<(), String> {
        Ok(())
    }

    /// Can this collector retry after validation failure?
    /// Returns true for interactive collectors (prompts, editor).
    fn can_retry(&self) -> bool {
        false
    }
}
```

## Input Chain

```rust
/// Chain multiple input sources with fallback behavior.
pub struct InputChain<T> {
    sources: Vec<Box<dyn InputCollector<T>>>,
    validators: Vec<Box<dyn Fn(&T) -> Result<(), String> + Send + Sync>>,
    default: Option<T>,
}

impl<T: Clone> InputChain<T> {
    pub fn new() -> Self { ... }

    /// Add any collector to the chain.
    pub fn try_source<C: InputCollector<T> + 'static>(mut self, source: C) -> Self {
        self.sources.push(Box::new(source));
        self
    }

    /// Add a validation rule to the chain.
    pub fn validate<F>(mut self, f: F, error_msg: &str) -> Self
    where F: Fn(&T) -> bool + Send + Sync + 'static { ... }

    /// Use this value if no source provides content.
    pub fn default(mut self, value: T) -> Self {
        self.default = Some(value);
        self
    }

    /// Resolve the chain: try each source in order.
    pub fn resolve(&self, matches: &ArgMatches) -> Result<T, InputError> {
        for source in &self.sources {
            if !source.is_available(matches) {
                continue;
            }

            loop {
                match source.collect(matches)? {
                    Some(value) => {
                        // Source-level validation
                        if let Err(msg) = source.validate(&value) {
                            if source.can_retry() {
                                eprintln!("Invalid: {}", msg);
                                continue;
                            }
                            return Err(InputError::ValidationFailed(msg));
                        }

                        // Chain-level validation
                        for validator in &self.validators {
                            validator(&value)?;
                        }

                        return Ok(value);
                    }
                    None => break, // Try next source
                }
            }
        }

        self.default.clone().ok_or(InputError::NoInput)
    }
}
```

## Built-in Sources (Core)

Always available, no feature flags:

```rust
/// Read from a clap argument.
pub struct ArgSource {
    name: String,
}

/// Read from stdin if piped (not a TTY).
pub struct StdinSource;

/// Read from environment variable.
pub struct EnvSource {
    var_name: String,
}

/// Read from system clipboard.
pub struct ClipboardSource;

/// Provide a default value.
pub struct DefaultSource<T> {
    value: T,
}
```

## Simple Prompts (feature = "simple-prompts")

Minimal prompts using only std::io, no external deps:

```rust
/// Basic text input prompt.
pub struct SimpleText {
    message: String,
    default: Option<String>,
}

/// Basic yes/no confirmation.
pub struct SimpleConfirm {
    message: String,
    default: bool,
}
```

These provide bare-bones functionality for users who don't want inquire's TUI.

## Editor (feature = "editor")

Opens the user's preferred editor:

```rust
pub struct EditorCollector {
    initial: Option<String>,
    extension: Option<String>,
    require_save: bool,
    trim_newlines: bool,
    env_vars: Vec<String>,  // ["VISUAL", "EDITOR"] by default
}

impl EditorCollector {
    pub fn new() -> Self { ... }
    pub fn initial(mut self, content: impl Into<String>) -> Self { ... }
    pub fn extension(mut self, ext: impl Into<String>) -> Self { ... }
    pub fn require_save(mut self, require: bool) -> Self { ... }
    pub fn trim_newlines(mut self, trim: bool) -> Self { ... }
    pub fn env_precedence(mut self, vars: Vec<String>) -> Self { ... }
}
```

Editor detection follows conventions:

1. Check env vars in order (default: `VISUAL`, then `EDITOR`)
2. Fall back to platform defaults (`vim` on Unix, `notepad` on Windows)
3. Search PATH for common editors

## Inquire Backend (feature = "inquire")

Full-featured prompts using the inquire crate:

```rust
/// Rich text input with autocomplete, validation display.
pub struct InquireText { ... }

/// Confirmation with customizable yes/no labels.
pub struct InquireConfirm { ... }

/// Single selection from a list.
pub struct InquireSelect<T> { ... }

/// Multiple selection from a list.
pub struct InquireMultiSelect<T> { ... }

/// Hidden password input.
pub struct InquirePassword { ... }

/// Editor with inquire's two-step UX (press 'e' to open).
pub struct InquireEditor { ... }
```

## Dialoguer Backend (feature = "dialoguer") — Future

Reserved for future implementation. Dialoguer shares the `console` crate with standout-render, making it lightweight for existing standout users.

## Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum InputError {
    #[error("No editor found. Set VISUAL or EDITOR environment variable.")]
    NoEditor,

    #[error("Editor cancelled without saving.")]
    EditorCancelled,

    #[error("Editor failed: {0}")]
    EditorFailed(#[source] std::io::Error),

    #[error("Failed to read stdin: {0}")]
    StdinFailed(#[source] std::io::Error),

    #[error("Failed to read clipboard: {0}")]
    ClipboardFailed(String),

    #[error("Prompt cancelled by user.")]
    PromptCancelled,

    #[error("Prompt failed: {0}")]
    PromptFailed(String),

    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    #[error("No input provided and no default available.")]
    NoInput,
}
```

## Usage Examples

### Basic: Arg with Editor Fallback

```rust
use standout_input::{InputChain, ArgSource, EditorCollector};

let message = InputChain::<String>::new()
    .try_source(ArgSource::new("message"))
    .try_source(EditorCollector::new()
        .initial("# Enter commit message\n\n")
        .extension(".md"))
    .resolve(&matches)?;
```

### The gh pr create Pattern

```rust
use standout_input::{InputChain, ArgSource, StdinSource, EditorCollector};

// arg → stdin → editor
let body = InputChain::<String>::new()
    .try_source(ArgSource::new("body"))
    .try_source(StdinSource)
    .try_source(EditorCollector::new()
        .initial("# PR Description\n\n"))
    .validate(|s| !s.trim().is_empty(), "Body cannot be empty")
    .resolve(&matches)?;
```

### Interactive Confirmation

```rust
use standout_input::{InputChain, FlagSource, InquireConfirm};

// -y flag skips prompt
let proceed = InputChain::<bool>::new()
    .try_source(FlagSource::new("yes").inverted())  // -y means true
    .try_source(InquireConfirm::new("Delete 5 items?").default(false))
    .default(false)
    .resolve(&matches)?;
```

### Selection with Validation

```rust
use standout_input::{InputChain, ArgSource, InquireSelect};

#[derive(Clone, Debug)]
enum Format { Json, Yaml, Csv }

let format = InputChain::<Format>::new()
    .try_source(ArgSource::new("format").parse())
    .try_source(InquireSelect::new("Output format:")
        .option(Format::Json, "JSON - machine readable")
        .option(Format::Yaml, "YAML - human readable")
        .option(Format::Csv, "CSV - spreadsheet compatible"))
    .default(Format::Json)
    .resolve(&matches)?;
```

### Direct Library Use (Complex Logic)

For commands with intricate input logic, use primitives directly:

```rust
use standout_input::{editor, stdin, clipboard};

fn create_handler(matches: &ArgMatches) -> Result<Pad> {
    let no_editor = matches.get_flag("no-editor");
    let title_arg = matches.get_one::<String>("title");

    let content = if let Some(piped) = stdin::read_if_piped()? {
        piped
    } else if let Some(title) = title_arg {
        if no_editor {
            title.clone()
        } else {
            let body = editor::edit(EditorConfig::new()
                .initial(&format!("# {}\n\n", title)))?
                .unwrap_or_default();
            format!("{}\n\n{}", title, body)
        }
    } else if no_editor {
        return Err(anyhow!("No content provided"));
    } else {
        let initial = clipboard::read().unwrap_or_default();
        editor::edit(EditorConfig::new().initial(&initial))?
            .ok_or_else(|| anyhow!("Editor cancelled"))?
    };

    // ... create pad
}
```

## Cargo.toml

```toml
[package]
name = "standout-input"
version = "1.0.0"
edition = "2021"
description = "Declarative input collection for CLI applications"
license = "MIT"
keywords = ["cli", "input", "prompt", "editor", "terminal"]
categories = ["command-line-interface"]
repository = "https://github.com/arthur-debert/standout"

[features]
default = ["editor", "simple-prompts"]
editor = ["dep:tempfile", "dep:which"]
simple-prompts = []
inquire = ["dep:inquire"]
# dialoguer = ["dep:dialoguer"]  # Future
# validify = ["dep:validify"]    # Future

[dependencies]
thiserror = "2"
clap = { version = "4", default-features = false }

# Optional: editor support
tempfile = { version = "3", optional = true }
which = { version = "7", optional = true }

# Optional: inquire prompts
inquire = { version = "0.7", optional = true }

# Future
# dialoguer = { version = "0.11", optional = true }
# validify = { version = "...", optional = true }
```

## Dependency Analysis

| Configuration | Unique Deps | Use Case |
| --------------- | ------------- | ---------- |
| `default-features = false` | ~2 | Minimal: just arg/stdin/env |
| `features = ["editor"]` | ~16 | + Editor (tempfile, which) |
| `features = ["simple-prompts"]` | ~2 | + Basic TTY prompts |
| `features = ["inquire"]` | ~29 | + Rich TUI prompts |
| `features = ["dialoguer"]` | ~8 new* | + dialoguer (*shares console with standout) |

## Standout Integration

When used with the standout framework:

### Builder API

```rust
let app = App::builder()
    .command_with("create", handlers::create, |cfg| {
        cfg.template_name("create")
           .input("body", InputChain::<String>::new()
               .try_source(ArgSource::new("body"))
               .try_source(StdinSource)
               .try_source(EditorCollector::new()))
    })
    .build()?;
```

### Handler Macro (Future)

```rust
#[handler]
pub fn create(
    #[input(fallback = "editor")] body: String,
    #[flag] verbose: bool,
) -> Result<CreateResult, Error> {
    // body is resolved before handler runs
}
```

## Future Considerations

- `feature = "dialoguer"`: Dialoguer adapter (shares console with standout-render)
- `feature = "validify"`: Deep validation integration
- `#[input]` attribute for handler macro
