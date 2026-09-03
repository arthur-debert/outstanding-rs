# standout-input

Declarative input collection for CLI applications.

## Overview

`standout-input` provides a unified way to acquire user input from multiple sources—CLI arguments, stdin, environment variables, editors, and interactive prompts—with automatic fallback chains.

```rust
use standout_input::{InputChain, ArgSource, StdinSource, EditorSource};

// Try argument first, then piped stdin, then open editor
let message = InputChain::<String>::new()
    .try_source(ArgSource::new("message"))
    .try_source(StdinSource::new())
    .try_source(EditorSource::new())
    .resolve(&matches)?;
```

## Features

- **Declarative chains** - Define fallback sequences without imperative logic
- **Pluggable sources** - Arg, stdin, env, clipboard, editor, prompts
- **Validation** - Chain-level and source-level validation with retry support
- **Testable** - All sources accept mock implementations for CI-safe testing
- **Minimal deps** - Core has ~2 dependencies; heavy features are opt-in

## Feature Flags

| Feature          | Default | Dependencies       | Provides                                  |
| ---------------- | ------- | ------------------ | ----------------------------------------- |
| `editor`         | Yes     | tempfile, which    | `EditorSource`                            |
| `simple-prompts` | Yes     | none               | `TextPromptSource`, `ConfirmPromptSource` |
| `inquire`        | No      | inquire (~29 deps) | Rich TUI prompts                          |

### Minimal Build

```toml
[dependencies]
standout-input = { version = "9", default-features = false }
```

### Full Feature Set

```toml
[dependencies]
standout-input = { version = "9", features = ["inquire"] }
```

## Usage

### Basic Chain

```rust
use standout_input::{InputChain, ArgSource, StdinSource, DefaultSource};

let chain = InputChain::<String>::new()
    .try_source(ArgSource::new("message"))
    .try_source(StdinSource::new())
    .default("default message".to_string());

let value = chain.resolve(&matches)?;
```

### With Validation

```rust
let chain = InputChain::<String>::new()
    .try_source(ArgSource::new("email"))
    .try_source(TextPromptSource::new("Email: "))
    .validate(|s| s.contains('@'), "Must be a valid email");
```

### Testing with Mocks

```rust
use standout_input::{StdinSource, MockStdin, EnvSource, MockEnv};

// Simulate piped input
let source = StdinSource::with_reader(MockStdin::piped("test content"));

// Simulate terminal (no piped input)
let source = StdinSource::with_reader(MockStdin::terminal());

// Simulate environment variable
let env = MockEnv::new().with_var("TOKEN", "secret");
let source = EnvSource::with_reader("TOKEN", env);
```

## Available Sources

### Non-Interactive (always available)

| Source             | Type     | Description                                                                      |
| ------------------ | -------- | -------------------------------------------------------------------------------- |
| `ArgSource`        | `String` | CLI argument                                                                     |
| `FlagSource`       | `bool`   | CLI flag                                                                         |
| `StdinSource`      | `String` | Piped stdin (skipped if terminal)                                                |
| `EnvSource`        | `String` | Environment variable                                                             |
| `ClipboardSource`  | `String` | System clipboard                                                                 |
| `ConfigSource<T>`  | `T`      | A value the handler already resolved from its configuration; skipped when `None` |
| `DefaultSource<T>` | `T`      | Fallback value                                                                   |

### Editor (`editor` feature, default)

| Source         | Type     | Description           |
| -------------- | -------- | --------------------- |
| `EditorSource` | `String` | Opens $VISUAL/$EDITOR |

### Simple Prompts (`simple-prompts` feature, default)

| Source                | Type     | Description      |
| --------------------- | -------- | ---------------- |
| `TextPromptSource`    | `String` | Basic text input |
| `ConfirmPromptSource` | `bool`   | Yes/no prompt    |

### Inquire (`inquire` feature)

| Source                  | Type     | Description            |
| ----------------------- | -------- | ---------------------- |
| `InquireText`           | `String` | Text with autocomplete |
| `InquireConfirm`        | `bool`   | Polished yes/no        |
| `InquireSelect<T>`      | `T`      | Single selection       |
| `InquireMultiSelect<T>` | `Vec<T>` | Multiple selection     |
| `InquirePassword`       | `String` | Masked input           |
| `InquireEditor`         | `String` | Editor with preview    |

## Documentation

- [Introduction to Input](docs/guides/intro-to-input.md) - Getting started guide
- [Backends](docs/topics/backends.md) - Detailed backend documentation and custom implementations
- [Answer Sheets](docs/topics/answer-sheets.md) - Editable prose questionnaires with stable identities
- [Derived Questionnaires](../../docs/guides/derived-questionnaires.md) - Typed derives and Standout command integration

## License

MIT
