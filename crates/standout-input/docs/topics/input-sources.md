# Input Sources

`standout-input` provides a unified way to acquire input before your handler runs. This enables interactive workflows like:

- Opening an editor for commit messages
- Prompting for confirmation ("Delete 5 items?")
- Selecting from a list of options
- Reading piped stdin for scripting
- Pre-filling from clipboard

All without polluting your handler logic.

---

## Why Input Sources?

CLI commands often need content that doesn't fit in command-line arguments. The `gh pr create` pattern is common:

```bash
# Option 1: Inline (awkward for long text)
gh pr create --body "Long description..."

# Option 2: Editor (interactive)
gh pr create --editor

# Option 3: Piped (scriptable)
echo "Description" | gh pr create --body-file -
```

Your CLI should support these patterns, but the logic doesn't belong in handlers:

- **Separation of concerns**: Handlers produce results, input acquisition is a setup concern
- **Testability**: Handlers remain pure functions that receive data
- **Composability**: Different commands can mix input sources

Standout's input system integrates as a pre-handler phase, running *before* your handler executes. Your handler receives resolved content—input acquisition is transparent.

---

## Source Types

Input sources fall into two categories:

### Non-Interactive Sources at a Glance

These work in scripts and CI pipelines:

| Source | Use Case |
| -------- | ---------- |
| **Arg** | Short content as CLI arguments |
| **Stdin** | Piped content (`cat file \| cmd`) |
| **Clipboard** | Pre-filled content from clipboard |
| **Env** | Environment variable |
| **Default** | Hardcoded fallback |

### Interactive Sources at a Glance

These require a TTY and user interaction:

| Source | Use Case | Output Type |
| -------- | ---------- | ------------- |
| **Editor** | Long-form text (commit messages) | `String` |
| **Text** | Short text input ("Enter name:") | `String` |
| **Confirm** | Yes/no questions ("Proceed?") | `bool` |
| **Select** | Pick one from list | `T` |
| **MultiSelect** | Pick many from list | `Vec<T>` |
| **Password** | Hidden text input | `String` |

---

## Non-Interactive Sources

### Arg Source

Read directly from a clap argument:

```rust
InputSource::arg("message")
```

### Stdin Source

Read piped content when stdin is not a TTY:

```rust
InputSource::stdin()
```

Only reads if stdin is actually piped. Returns `None` if stdin is a terminal.

### Clipboard Source

Read from system clipboard:

```rust
InputSource::clipboard()
```

### Env Source

Read from environment variable:

```rust
InputSource::env("MY_APP_TOKEN")
```

---

## Interactive Sources

### Editor Source

Open the user's preferred editor:

```rust
InputSource::editor()
    .initial("# Enter your message\n\n")
    .extension(".md")
    .require_save(true)
```

Use for multi-line content like commit messages or descriptions.

### Text Prompt

Prompt for short text input:

```rust
InputSource::text("Enter your name:")
    .default("Anonymous")
    .placeholder("John Doe")
```

### Confirm Prompt

Ask a yes/no question:

```rust
InputSource::confirm("Delete 5 items?")
    .default(false)  // Default to "no"
```

Returns `bool`. In chains, use with `#[input]` on a `bool` parameter.

### Select Prompt

Pick one from a list:

```rust
InputSource::select("Choose format:")
    .option("json", "JSON output")
    .option("yaml", "YAML output")
    .option("csv", "CSV output")
    .default("json")
```

### Multi-Select Prompt

Pick multiple from a list:

```rust
InputSource::multi_select("Select features:")
    .option("auth", "Authentication")
    .option("logging", "Request logging")
    .option("cache", "Response caching")
```

### Password Prompt

Hidden text input:

```rust
InputSource::password("Enter API token:")
    .confirm("Confirm token:")  // Optional confirmation
```

---

## Quick Start

The simplest integration uses the handler macro:

```rust
use standout_macros::handler;

#[handler]
pub fn create(
    #[input(fallback = "editor")] message: String,
    #[flag] verbose: bool,
) -> Result<CreateResult, Error> {
    // `message` is resolved from: arg → stdin → editor
    Ok(CreateResult { message, verbose })
}
```

Or use the builder API for more control:

```rust
let app = App::builder()
    .command_with("create", handlers::create, |cfg| {
        cfg.template("create.jinja")
           .input("message", InputSource::chain()
               .try_arg("message")
               .try_stdin()
               .fallback_editor(EditorConfig::new()
                   .initial("# Enter message")
                   .extension(".md")))
    })
    .build()?;
```

---

## Input Chains

Chain multiple sources with fallback behavior:

```rust
InputSource::chain()
    .try_arg("body")           // First: try CLI arg
    .try_stdin()               // Second: try piped stdin
    .fallback_editor(config)   // Third: open editor
```

The chain stops at the first source that provides content. This enables the `gh pr create` pattern:

- `gh pr create --body "text"` → uses arg
- `echo "text" | gh pr create` → uses stdin
- `gh pr create` → opens editor

### Chain with Skip Flag

Some commands want `--no-editor` to skip interactive input:

```rust
InputSource::chain()
    .try_arg("body")
    .try_stdin()
    .fallback_editor_unless("no-editor", config)
    .default("")  // If --no-editor and no other source, use empty
```

---

## API Reference

### Macro Attributes

| Attribute | Behavior |
| ----------- | ---------- |
| `#[input]` | Resolve from arg of same name |
| `#[input(fallback = "editor")]` | Arg → stdin → editor chain |
| `#[input(fallback = "stdin")]` | Arg → stdin chain |
| `#[input(source = "editor")]` | Editor only |

### Builder Methods

```rust
// Single sources
InputSource::arg("name")      // From CLI argument
InputSource::stdin()          // From piped stdin
InputSource::editor()         // Always open editor
InputSource::clipboard()      // From system clipboard

// Editor configuration
InputSource::editor()
    .initial("prefilled content")
    .extension(".md")          // For syntax highlighting
    .require_save(true)        // Abort if user doesn't save
    .trim_newlines(true)       // Strip trailing newlines

// Chains
InputSource::chain()
    .try_arg("message")
    .try_stdin()
    .fallback_editor(config)
    .default("fallback value")

// With validation
InputSource::chain()
    .try_arg("message")
    .validate(|s| !s.is_empty(), "Message cannot be empty")
```

### Low-Level API

For standalone use without the framework:

```rust
use standout_input::{Editor, detect_editor, read_stdin_if_piped};

// Detect preferred editor
let editor = detect_editor()?;  // Checks: VISUAL, EDITOR, then fallbacks

// Read stdin only if piped
let piped: Option<String> = read_stdin_if_piped()?;

// Open editor with content
let content = Editor::new()
    .executable(&editor)
    .initial("# Enter message\n")
    .extension(".md")
    .edit()?;  // Returns Option<String>, None if user aborted
```

---

## Editor Detection

Editor detection follows established conventions:

| Priority | Source | Example |
| ---------- | -------- | --------- |
| 1 | `VISUAL` env var | `VISUAL=code` |
| 2 | `EDITOR` env var | `EDITOR=vim` |
| 3 | Platform default | `vim` (Unix), `notepad` (Windows) |

For apps that want custom precedence (like `gh` with `GH_EDITOR`):

```rust
let editor = detect_editor_with_precedence(&[
    "GH_EDITOR",    // App-specific first
    "VISUAL",
    "EDITOR",
])?;
```

---

## Integration with Handlers

Resolved input is injected into `CommandContext.extensions`:

```rust
// Framework resolves input before handler runs
// Handler receives it via #[input] attribute or ctx.extensions

#[handler]
pub fn create(
    #[input(fallback = "editor")] body: String,
    #[ctx] ctx: &CommandContext,
) -> Result<Pad, Error> {
    // `body` is already resolved
    // Can also access: ctx.extensions.get::<ResolvedInput<"body">>()
}
```

For complex cases that need the resolution metadata:

```rust
fn create(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Pad> {
    let input = ctx.extensions.get_required::<ResolvedInput>()?;

    match input.source {
        InputSourceKind::Arg => log::debug!("Got body from --body arg"),
        InputSourceKind::Stdin => log::debug!("Got body from piped stdin"),
        InputSourceKind::Editor => log::debug!("Got body from editor"),
    }

    let body = input.content;
    // ...
}
```

---

## Direct Use in Handlers

For commands with complex input logic (like padz's "smart create"), use the library directly:

```rust
use standout_input::{Editor, read_stdin_if_piped, read_clipboard};

fn create(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Pad> {
    let no_editor = matches.get_flag("no-editor");
    let title_arg = matches.get_one::<String>("title");

    let content = if let Some(piped) = read_stdin_if_piped()? {
        // Piped input takes precedence
        piped
    } else if let Some(title) = title_arg {
        if no_editor {
            // Title only, no body
            title.clone()
        } else {
            // Title provided, open editor for body
            let body = Editor::new()
                .initial(&format!("# {}\n\n", title))
                .extension(".md")
                .edit()?
                .unwrap_or_default();
            format!("{}\n\n{}", title, body)
        }
    } else if no_editor {
        // No input and no editor - error
        return Err(anyhow!("No content provided. Use --title or pipe input."));
    } else {
        // No args - prefill from clipboard, open editor
        let clipboard = read_clipboard().unwrap_or_default();
        Editor::new()
            .initial(&clipboard)
            .edit()?
            .ok_or_else(|| anyhow!("Editor cancelled"))?
    };

    // ... rest of handler
}
```

This gives full control while still using standardized primitives.

---

## Clipboard Integration

Read from system clipboard as an input source:

```rust
// As part of a chain
InputSource::chain()
    .try_arg("content")
    .try_clipboard()
    .fallback_editor(config)

// Or for prefilling editor
let initial = read_clipboard().unwrap_or_default();
Editor::new().initial(&initial).edit()?
```

Platform support:

| Platform | Read Command |
| ---------- | -------------- |
| macOS | `pbpaste` |
| Linux | `xclip -selection clipboard -o` |
| Windows | PowerShell `Get-Clipboard` |

---

## Comparison with Output Piping

Input sources and output piping are symmetric but opposite:

| Aspect | Input Sources | Output Piping |
| -------- | --------------- | --------------- |
| Direction | External → Handler | Handler → External |
| Pipeline position | Pre-handler | Post-output |
| Interactive | Can be (editor) | Never |
| Purpose | Acquire content | Transform/route output |

```text
              INPUT SOURCES                    OUTPUT PIPING
              ↓                                ↓
[Arg/Stdin/Editor] → Handler → Render → [jq/tee/clipboard]
```

---

## Error Handling

Input errors are returned before handler execution:

```rust
// Editor not found
// Error: No editor found. Set VISUAL or EDITOR environment variable.

// User cancelled editor (with require_save)
// Error: Editor cancelled without saving.

// Stdin read failed
// Error: Failed to read from stdin: <io error>

// Validation failed
// Error: Input validation failed: Message cannot be empty
```

---

## Security Considerations

**Editor execution**: The editor command is resolved from environment variables. Ensure `VISUAL`/`EDITOR` are set by the user, not from untrusted sources.

**Temp file handling**: Editor content is written to a temp file. The file is deleted after reading. Content may briefly exist on disk.

```rust
// Files are created in system temp directory with random names
// e.g., /tmp/standout-input-a7b3c9.md
```

---

## Summary

| Feature | Method/Attribute |
| --------- | ------------------ |
| From CLI arg | `InputSource::arg("name")` |
| From piped stdin | `InputSource::stdin()` |
| From editor | `InputSource::editor()` |
| From clipboard | `InputSource::clipboard()` |
| Chain with fallback | `InputSource::chain().try_arg().fallback_editor()` |
| Prefill editor | `.initial("content")` |
| File extension | `.extension(".md")` |
| Require save | `.require_save(true)` |
| Validation | `.validate(fn, "error message")` |
