# Introduction to Input Collection

CLI applications need input from multiple sources: command-line arguments, piped stdin, environment variables, interactive prompts, and editors. Managing these sources with proper fallback logic and validation is tedious and error-prone.

`standout-input` provides a declarative API for input collection with automatic fallback chains. Define where input can come from, and the library handles the rest.

**See Also:**

- [Backends](../topics/backends.md) - Detailed backend options and custom implementations
- [Introduction to Standout](../../../../docs/guides/intro-to-standout.md) - Full framework integration

---

## The Problem

Typical CLI input handling looks like this:

```rust
fn get_message(matches: &ArgMatches) -> Result<String, Error> {
    // Try CLI argument first
    if let Some(msg) = matches.get_one::<String>("message") {
        return Ok(msg.clone());
    }

    // Try stdin if piped
    if !std::io::stdin().is_terminal() {
        let mut buffer = String::new();
        std::io::stdin().read_to_string(&mut buffer)?;
        if !buffer.trim().is_empty() {
            return Ok(buffer.trim().to_string());
        }
    }

    // Try environment variable
    if let Ok(msg) = std::env::var("MY_MESSAGE") {
        return Ok(msg);
    }

    // Fall back to prompting
    print!("Enter message: ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(line.trim().to_string())
}
```

Problems:

- Imperative logic obscures the intended priority
- Hard to test (stdin, environment, terminal detection)
- Duplicated across commands
- Easy to miss edge cases (empty input, whitespace)

---

## The Solution: Input Chains

`standout-input` replaces imperative logic with declarative chains:

```rust
use standout_input::{InputChain, ArgSource, StdinSource, EnvSource, TextPromptSource};

let message = InputChain::<String>::new()
    .try_source(ArgSource::new("message"))      // 1. CLI argument
    .try_source(StdinSource::new())              // 2. Piped stdin
    .try_source(EnvSource::new("MY_MESSAGE"))    // 3. Environment variable
    .try_source(TextPromptSource::new("Enter message: "))  // 4. Interactive prompt
    .resolve(&matches)?;
```

The chain tries each source in order. The first source that provides input wins. If all sources return `None`, the chain returns `InputError::NoInput`.

Benefits:

- **Declarative** — Priority is explicit and readable
- **Testable** — All sources accept mocks for deterministic testing
- **Composable** — Build chains for different commands with shared sources
- **Validated** — Add validation rules that apply to any source

---

## Quick Start

Add `standout-input` to your `Cargo.toml`:

```toml
[dependencies]
standout-input = "8"
```

### Basic Chain

```rust
use standout_input::{InputChain, ArgSource, StdinSource, DefaultSource};
use clap::{Command, Arg};

// Set up clap
let cmd = Command::new("myapp")
    .arg(Arg::new("message").short('m').long("message"));
let matches = cmd.get_matches();

// Build an input chain
let message = InputChain::<String>::new()
    .try_source(ArgSource::new("message"))
    .try_source(StdinSource::new())
    .default("Hello, World!".to_string())
    .resolve(&matches)?;
```

This chain:

1. Checks if `--message` was provided
2. If not, reads from stdin (only if piped, not interactive)
3. Falls back to the default value

### With Validation

Add validation rules that apply regardless of the source:

```rust
let email = InputChain::<String>::new()
    .try_source(ArgSource::new("email"))
    .try_source(TextPromptSource::new("Email: "))
    .validate(|s| s.contains('@'), "Must be a valid email address")
    .validate(|s| s.len() >= 5, "Email too short")
    .resolve(&matches)?;
```

For interactive sources (prompts, editor), validation failures trigger re-prompting. For non-interactive sources (args, stdin), validation failures return an error.

### Knowing the Source

Sometimes you need to know where input came from:

```rust
use standout_input::InputSourceKind;

let result = InputChain::<String>::new()
    .try_source(ArgSource::new("file"))
    .try_source(StdinSource::new())
    .default("default.txt".to_string())
    .resolve_with_source(&matches)?;

match result.source {
    InputSourceKind::Arg => println!("From --file argument"),
    InputSourceKind::Stdin => println!("From piped input"),
    InputSourceKind::Default => println!("Using default"),
    _ => {}
}

let filename = result.value;
```

---

## Available Sources

### Non-Interactive Sources

These sources don't require user interaction and work in CI/scripted environments:

| Source             | Type     | Description                                  |
| ------------------ | -------- | -------------------------------------------- |
| `ArgSource`        | `String` | CLI argument value                           |
| `FlagSource`       | `bool`   | CLI flag (true/false)                        |
| `StdinSource`      | `String` | Piped stdin (skipped if stdin is a terminal) |
| `EnvSource`        | `String` | Environment variable                         |
| `ClipboardSource`  | `String` | System clipboard contents                    |
| `DefaultSource<T>` | `T`      | Fallback value                               |

### Interactive Sources (Feature-Gated)

These require a terminal and are feature-gated to control dependencies:

**`simple-prompts` feature (default, no dependencies):**

| Source                | Type     | Description                |
| --------------------- | -------- | -------------------------- |
| `TextPromptSource`    | `String` | Basic text input prompt    |
| `ConfirmPromptSource` | `bool`   | Yes/no confirmation prompt |

**`editor` feature (default, adds tempfile + which):**

| Source         | Type     | Description                                |
| -------------- | -------- | ------------------------------------------ |
| `EditorSource` | `String` | Opens $VISUAL/$EDITOR for multi-line input |

**`inquire` feature (optional, adds inquire crate):**

| Source                  | Type     | Description                        |
| ----------------------- | -------- | ---------------------------------- |
| `InquireText`           | `String` | Rich text input with autocomplete  |
| `InquireConfirm`        | `bool`   | Polished yes/no prompt             |
| `InquireSelect<T>`      | `T`      | Single selection with arrow keys   |
| `InquireMultiSelect<T>` | `Vec<T>` | Multiple selection with checkboxes |
| `InquirePassword`       | `String` | Masked password input              |
| `InquireEditor`         | `String` | Editor with preview                |

See [Backends](../topics/backends.md) for full documentation on each source.

---

## Standalone Prompts (No Chain)

Chains shine for CLI commands that need fallback between sources. For interactive flows that drive standout themselves — wizards, REPLs, setup helpers — every interactive source has a `.prompt()` shortcut that skips the chain machinery and the `&ArgMatches` plumbing entirely:

```rust
use standout_input::{InquireConfirm, InquireSelect, InquireText};

let pack: String = InquireText::new("Pack name:")
    .help("a-z0-9-")
    .prompt()?;

let env: String = InquireSelect::new("Environment:", vec!["dev", "staging", "prod"])
    .prompt()?
    .to_string();

let proceed: bool = InquireConfirm::new("Continue?")
    .default(true)
    .prompt()?;
```

`prompt()` returns `Result<T, InputError>` directly — no `Option` to unwrap. Stdin not being a TTY *or* an empty submission both map to [`InputError::NoInput`], so a re-ask loop is just `match` on the error. User cancellation is reported as a backend-specific variant (`PromptCancelled` for prompts, `EditorCancelled` for editors); see the [Interactive Flows topic](../topics/interactive-flows.md) for the full table.

Available on every interactive source:

| Source                                                              | Returns                                |
| ------------------------------------------------------------------- | -------------------------------------- |
| `TextPromptSource`, `ConfirmPromptSource`                           | `Result<String, _>`, `Result<bool, _>` |
| `EditorSource`                                                      | `Result<String, _>`                    |
| `InquireText`, `InquireConfirm`, `InquirePassword`, `InquireEditor` | as above                               |
| `InquireSelect<T>`, `InquireMultiSelect<T>`                         | `Result<T, _>`, `Result<Vec<T>, _>`    |

The `InputCollector` impls are unchanged — these sources still work in chains exactly as before. See [Interactive Flows](../topics/interactive-flows.md) for a full wizard walkthrough that pairs `.prompt()` with standout's renderer.

---

## Common Patterns

### The `gh pr create` Pattern

Many CLI tools follow this pattern for body text:

```rust
// arg → stdin → editor → default
let body = InputChain::<String>::new()
    .try_source(ArgSource::new("body"))
    .try_source(StdinSource::new())
    .try_source(EditorSource::new().extension(".md"))
    .default(String::new())
    .resolve(&matches)?;
```

### Confirmation with `--yes` Flag

Skip prompts in scripts with a flag override:

```rust
let confirmed = InputChain::<bool>::new()
    .try_source(FlagSource::new("yes"))
    .try_source(ConfirmPromptSource::new("Proceed?").default(false))
    .resolve(&matches)?;
```

Running with `--yes` returns `true` immediately. Without the flag, the user is prompted.

### API Token with Environment Fallback

```rust
let token = InputChain::<String>::new()
    .try_source(ArgSource::new("token"))
    .try_source(EnvSource::new("GITHUB_TOKEN"))
    .try_source(InquirePassword::new("GitHub token:"))
    .resolve(&matches)?;
```

### Clipboard Prefill

For tools like paste managers:

```rust
let content = InputChain::<String>::new()
    .try_source(ArgSource::new("content"))
    .try_source(StdinSource::new())
    .try_source(ClipboardSource::new())
    .try_source(EditorSource::new())
    .resolve(&matches)?;
```

---

## Testing

All sources accept mock implementations, enabling deterministic tests without actual terminal I/O, environment variables, or clipboard access.

### Mocking Stdin

```rust
use standout_input::{StdinSource, MockStdin};

// Simulate piped input
let source = StdinSource::with_reader(MockStdin::piped("test content"));

// Simulate interactive terminal (no piped input)
let source = StdinSource::with_reader(MockStdin::terminal());
```

### Mocking Environment Variables

```rust
use standout_input::{EnvSource, MockEnv};

let env = MockEnv::new()
    .with_var("API_KEY", "secret123")
    .with_var("DEBUG", "true");

let source = EnvSource::with_reader("API_KEY", env);
```

### Mocking Clipboard

```rust
use standout_input::{ClipboardSource, MockClipboard};

let source = ClipboardSource::with_reader(MockClipboard::with_content("clipboard text"));
let source = ClipboardSource::with_reader(MockClipboard::empty());
```

### Mocking Prompts

```rust
use standout_input::{TextPromptSource, MockTerminal};

// Simulate user typing "Alice" and pressing Enter
let source = TextPromptSource::with_terminal("Name: ", MockTerminal::with_response("Alice"));

// Simulate multiple responses for retry scenarios
let terminal = MockTerminal::with_responses(["invalid", "valid@email.com"]);
```

### Mocking Editor

```rust
use standout_input::{EditorSource, MockEditorRunner};

// Simulate editor returning content
let source = EditorSource::with_runner(MockEditorRunner::with_result("user input"));

// Simulate no editor available
let source = EditorSource::with_runner(MockEditorRunner::no_editor());
```

### Full Integration Test

```rust
use standout_input::{InputChain, ArgSource, StdinSource, EnvSource, MockStdin, MockEnv};
use clap::{Command, Arg};

#[test]
fn test_input_priority() {
    let cmd = Command::new("test")
        .arg(Arg::new("token").long("token"));

    // Test: env var is used when arg is not provided
    let matches = cmd.clone().get_matches_from(["test"]);

    let chain = InputChain::<String>::new()
        .try_source(ArgSource::new("token"))
        .try_source(StdinSource::with_reader(MockStdin::terminal()))
        .try_source(EnvSource::with_reader("TOKEN",
            MockEnv::new().with_var("TOKEN", "from-env")));

    let result = chain.resolve(&matches).unwrap();
    assert_eq!(result, "from-env");

    // Test: arg overrides env var
    let matches = cmd.get_matches_from(["test", "--token", "from-arg"]);

    let chain = InputChain::<String>::new()
        .try_source(ArgSource::new("token"))
        .try_source(EnvSource::with_reader("TOKEN",
            MockEnv::new().with_var("TOKEN", "from-env")));

    let result = chain.resolve(&matches).unwrap();
    assert_eq!(result, "from-arg");
}
```

---

## Feature Flags

`standout-input` uses feature flags to control dependencies:

| Feature          | Default | Dependencies       | Provides                                  |
| ---------------- | ------- | ------------------ | ----------------------------------------- |
| `editor`         | Yes     | tempfile, which    | `EditorSource`                            |
| `simple-prompts` | Yes     | none               | `TextPromptSource`, `ConfirmPromptSource` |
| `inquire`        | No      | inquire (~29 deps) | Rich TUI prompts                          |

### Minimal Dependencies

For the smallest footprint:

```toml
[dependencies]
standout-input = { version = "0.1", default-features = false }
```

This gives you only non-interactive sources (~2 dependencies).

### Full Feature Set

```toml
[dependencies]
standout-input = { version = "0.1", features = ["inquire"] }
```

---

## Standalone vs. Standout Framework

`standout-input` works as a standalone library with any clap-based CLI:

```rust
// Standalone usage
use standout_input::{InputChain, ArgSource, StdinSource};

let message = InputChain::<String>::new()
    .try_source(ArgSource::new("message"))
    .try_source(StdinSource::new())
    .resolve(&matches)?;
```

When using the full Standout framework, input chains integrate with the dispatch system:

```rust
// With Standout framework
use standout::cli::App;
use standout_input::{InputChain, ArgSource, EditorSource};

App::builder()
    .commands(|g| {
        g.command_with("create", handlers::create, |cfg| {
            cfg.input("body", InputChain::<String>::new()
                .try_source(ArgSource::new("body"))
                .try_source(EditorSource::new()))
        })
    })?
    .build()?;
```

---

## Summary

`standout-input` transforms CLI input handling from imperative spaghetti into declarative chains:

1. **Declarative priority** — Source order is explicit in the chain definition
2. **Testable** — All sources accept mocks for deterministic testing
3. **Feature-gated** — Control dependencies with feature flags
4. **Validated** — Chain-level validation with retry support for interactive sources
5. **Composable** — Build reusable source configurations

For detailed information on specific backends, including how to implement custom sources, see [Backends](../topics/backends.md).
