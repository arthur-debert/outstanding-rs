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
- **Testability**: Handler adapters receive already-resolved data through an explicit seam
- **Composability**: Different commands can mix input sources

An `InputChain` runs as a pre-dispatch phase, before your handler executes. The handler receives the resolved value; input acquisition is transparent.

---

## Source Types

Every source implements `InputCollector<T>` and composes into an `InputChain<T>`. See [Backends](backends.md) for the full constructor and feature-flag reference for each one.

### Non-Interactive Sources at a Glance

These work in scripts and CI pipelines:

| Source | Type | Use Case |
| -------- | ------ | ---------- |
| `ArgSource` | `String` | Short content as a CLI argument |
| `FlagSource` | `bool` | A CLI flag, with an optional `.inverted()` |
| `StdinSource` | `String` | Piped content (`cat file \| cmd`) |
| `EnvSource` | `String` | Environment variable |
| `ClipboardSource` | `String` | Pre-filled content from the clipboard |
| `DefaultSource<T>` | `T` | Hardcoded fallback |

### Interactive Sources at a Glance

These require a terminal and are grouped by feature flag:

| Source | Feature | Type | Use Case |
| -------- | --------- | ------ | ---------- |
| `TextPromptSource` | `simple-prompts` (default) | `String` | Short text input |
| `ConfirmPromptSource` | `simple-prompts` (default) | `bool` | Yes/no questions |
| `EditorSource` | `editor` (default) | `String` | Long-form text (commit messages) |
| `InquireText` | `inquire` | `String` | Rich text input with autocomplete |
| `InquireConfirm` | `inquire` | `bool` | Polished yes/no prompt |
| `InquireSelect<T>` | `inquire` | `T` | Pick one from a list |
| `InquireMultiSelect<T>` | `inquire` | `Vec<T>` | Pick many from a list |
| `InquirePassword` | `inquire` | `String` | Hidden text input |
| `InquireEditor` | `inquire` | `String` | Editor with an inquire preview |

---

## Building a Chain

Chain sources in priority order with `InputChain`:

```rust
use standout_input::{InputChain, ArgSource, StdinSource, EditorSource};

let body = InputChain::<String>::new()
    .try_source(ArgSource::new("body"))   // First: try the CLI argument
    .try_source(StdinSource::new())       // Second: try piped stdin
    .try_source(EditorSource::new()       // Third: open the editor
        .extension(".md"))
    .resolve(&matches)?;
```

The chain stops at the first source whose `is_available()` returns `true` and whose `collect()` returns `Some(_)`. This is the `gh pr create` pattern:

- `gh pr create --body "text"` → uses the argument
- `echo "text" | gh pr create` → uses stdin
- `gh pr create` → opens the editor

Add `.default(value)` to fall back to a literal value instead of erroring with `InputError::NoInput` when every source is skipped, and `.validate(f, "message")` to apply a rule regardless of which source produced the value. See [Introduction to Input](../guides/intro-to-input.md) for the full walkthrough.

---

## Wiring a Chain to a Command

Outside the framework, a handler resolves a chain itself, passing the run's `InputSources` so stdin/clipboard/prompt mocks are honored in tests:

```rust
fn create(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Pad> {
    let body = InputChain::<String>::new()
        .try_source(ArgSource::new("body"))
        .try_source(StdinSource::new())
        .try_source(EditorSource::new())
        .resolve_from(matches, ctx.input_sources())?;

    /* business logic ... */
}
```

With the `standout` framework, `CommandConfig::input(name, chain)` registers the same chain to run in pre-dispatch, and the handler reads the resolved value with `ctx.input::<T>(name)` instead of resolving it itself. See [Framework Integration](framework-integration.md) for the full wiring and the `CommandContextInput` trait.

---

## Skipping Interactive Sources

Some commands want a flag like `--no-editor` to skip interactive input entirely. Since chain construction is ordinary Rust, build the chain conditionally instead of adding sources that would prompt:

```rust
let no_editor = matches.get_flag("no-editor");

let mut chain = InputChain::<String>::new()
    .try_source(ArgSource::new("body"))
    .try_source(StdinSource::new());

if !no_editor {
    chain = chain.try_source(EditorSource::new());
}

let body = chain.default(String::new()).resolve(&matches)?;
```

---

## Direct Use Without a Chain

For commands with input logic too specific for a declarative chain, call the primitives directly. Every interactive source also has a `.prompt()` shortcut that skips the chain and the `&ArgMatches` plumbing (see [Standalone Prompts](../guides/intro-to-input.md#standalone-prompts-no-chain)):

```rust
use standout_input::{read_if_piped, EditorSource};

fn create(matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<Pad> {
    let no_editor = matches.get_flag("no-editor");
    let title_arg = matches.get_one::<String>("title");

    let content = if let Some(piped) = read_if_piped()? {
        // Piped input takes precedence
        piped
    } else if let Some(title) = title_arg {
        if no_editor {
            title.clone()
        } else {
            let body = EditorSource::new()
                .initial_content(format!("# {}\n\n", title))
                .extension(".md")
                .prompt()?;
            format!("{}\n\n{}", title, body)
        }
    } else if no_editor {
        return Err(anyhow!("No content provided. Use --title or pipe input."));
    } else {
        EditorSource::new().prompt()?
    };

    // ... rest of handler
}
```

---

## Editor Detection

Editor detection follows established conventions:

| Priority | Source | Example |
| ---------- | -------- | --------- |
| 1 | `VISUAL` env var | `VISUAL=code` |
| 2 | `EDITOR` env var | `EDITOR=vim` |
| 3 | Platform default | `vim`, `vi`, `nano` (Unix), `notepad` (Windows) |

`EditorSource::is_available()` also requires stdin to be a terminal, so a piped invocation never blocks on an editor.

---

## Clipboard Integration

```rust
use standout_input::ClipboardSource;

let content = InputChain::<String>::new()
    .try_source(ArgSource::new("content"))
    .try_source(ClipboardSource::new())
    .try_source(EditorSource::new())
    .resolve(&matches)?;
```

Platform support:

| Platform | Read Command |
| ---------- | -------------- |
| macOS | `pbpaste` |
| Linux | `xclip -selection clipboard -o` |
| Other | `InputError::ClipboardFailed` — not supported |

---

## Comparison with Output Piping

Input sources and output piping are symmetric but opposite:

| Aspect | Input Sources | Output Piping |
| -------- | --------------- | --------------- |
| Direction | External → Handler | Handler → External |
| Pipeline position | Pre-dispatch | Post-output |
| Interactive | Can be (editor, prompts) | Never |
| Purpose | Acquire content | Transform/route output |

```text
              INPUT SOURCES                    OUTPUT PIPING
              ↓                                ↓
[Arg/Stdin/Editor] → Handler → Render → [jq/tee/clipboard]
```

---

## Error Handling

`InputError` carries the failure reason so a chain-level `?` produces an actionable message:

```text
No editor found. Set VISUAL or EDITOR environment variable.  // InputError::NoEditor
Editor cancelled without saving.                              // InputError::EditorCancelled
Failed to read stdin: <io error>                              // InputError::StdinFailed
Validation failed: Message cannot be empty                    // InputError::ValidationFailed
No input provided and no default available.                   // InputError::NoInput
```

For interactive sources, a validation failure re-prompts instead of returning an error — see [Backends](backends.md) for the retry semantics.

---

## Security Considerations

**Editor execution**: The editor command is resolved from environment variables. Ensure `VISUAL`/`EDITOR` are set by the user, not from untrusted sources.

**Temp file handling**: `EditorSource` writes the initial content to a named temp file and hands it to the editor process; the file is removed when the collector drops it. Content may briefly exist on disk in the system temp directory.

---

## Summary

| Feature | Method |
| --------- | -------- |
| From a CLI argument | `ArgSource::new("name")` |
| From a CLI flag | `FlagSource::new("name")` |
| From piped stdin | `StdinSource::new()` |
| From an environment variable | `EnvSource::new("VAR")` |
| From the clipboard | `ClipboardSource::new()` |
| From the editor | `EditorSource::new()` |
| Fallback value | `.default(value)` |
| Validation | `.validate(f, "error message")` |
| Chain multiple sources | `InputChain::new().try_source(...).try_source(...)` |

For the full constructor reference and feature flags, see [Backends](backends.md). For wiring a chain into a `standout` command, see [Framework Integration](framework-integration.md).
