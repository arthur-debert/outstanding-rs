# Output Piping

`standout-pipe` provides a way to send your CLI's rendered output to external commands. This enables post-processing workflows like filtering with `jq`, logging with `tee`, or copying to the clipboard—without polluting your handler logic.

---

## Why Piping?

Shell commands excel at composition: `ls | grep foo | head -5`. Your CLI's output should participate in this ecosystem, but piping logic doesn't belong in handlers:

- **Separation of concerns**: Handlers produce data, piping is an output concern
- **User choice**: Let users decide what to do with output
- **Testability**: Handler adapters keep returning data through an explicit seam

Standout's piping integrates as a post-output hook, running *after* rendering completes. Your handler and template are unchanged—piping is purely additive.

---

## Three Modes

Piping has three modes, each for different use cases:

| Mode | Returns | Use Case |
| ------ | --------- | ---------- |
| **Passthrough** | Original output | Side effects (logging, clipboard) while still displaying output |
| **Capture** | Command's stdout | Filters (jq, grep, sort) that transform output |
| **Consume** | Empty string | Clipboard-only, no terminal display |

### Passthrough Mode

The output goes to the command's stdin, but your original output is preserved:

```rust
.pipe_to("tee /tmp/output.log")
```

Use this when you want both: display the output *and* send it somewhere else.

### Capture Mode

The command's stdout *becomes* the new output:

```rust
.pipe_through("jq '.items[]'")
```

Use this for filters that transform output. Whatever `jq` prints is what the user sees.

### Consume Mode

The output goes to the command, and nothing is printed:

```rust
.pipe_to_clipboard()  // Uses pbcopy/xclip depending on platform
```

Use this when piping *is* the final destination.

---

## Quick Start

The simplest integration uses the derive macro:

```rust
use standout::cli::Dispatch;

#[derive(Subcommand, Dispatch)]
#[dispatch(handlers = handlers)]
pub enum Commands {
    /// List items, filtered through jq
    #[dispatch(pipe_through = "jq '.items'")]
    List,

    /// Export to clipboard
    #[dispatch(pipe_to_clipboard)]
    Export,
}
```

Or use the builder API for more control:

```rust
let app = App::builder()
    .commands(|g| {
        g.command_with("list", handlers::list, |cfg| {
            cfg.template("list.jinja")
               .pipe_through("jq '.items'")
        })
    })
    .build()?;
```

---

## API Reference

### Macro Attributes

| Attribute | Mode | Example |
| ----------- | ------ | --------- |
| `pipe_to = "cmd"` | Passthrough | `#[dispatch(pipe_to = "tee log.txt")]` |
| `pipe_through = "cmd"` | Capture | `#[dispatch(pipe_through = "jq .data")]` |
| `pipe_to_clipboard` | Consume | `#[dispatch(pipe_to_clipboard)]` |

### Builder Methods

```rust
// Passthrough: run command, return original output
.pipe_to("tee /tmp/output.log")

// Capture: use command's stdout as new output
.pipe_through("jq '.items[]'")

// Clipboard (platform-aware, consume mode)
.pipe_to_clipboard()

// With custom timeout (default is 30 seconds)
.pipe_to_with_timeout("slow-command", Duration::from_secs(120))
.pipe_through_with_timeout("jq .", Duration::from_secs(60))

// Custom PipeTarget implementation
.pipe_with(MyCustomPipe::new())
```

### Low-Level API

For standalone use without the framework:

```rust
use standout_pipe::{SimplePipe, PipeMode, PipeTarget};

// Create a pipe
let pipe = SimplePipe::new("jq '.items'")
    .capture()  // Use command's output
    .with_timeout(Duration::from_secs(30));

// Execute
let output = pipe.pipe("{ \"items\": [1,2,3] }")?;
// output = "[1, 2, 3]"
```

---

## Chaining Pipes

Multiple pipes execute in sequence:

```rust
.pipe_through("jq '.items'")  // First: extract items
.pipe_to("tee /tmp/items.json")  // Second: log to file (passthrough)
```

Order matters: the second pipe receives the first pipe's output.

---

## Platform-Specific Clipboard

`pipe_to_clipboard()` automatically selects the right command:

| Platform | Command |
| ---------- | --------- |
| macOS | `pbcopy` |
| Linux | `xclip -selection clipboard` |
| Other | Error (use `pipe_to` with explicit command) |

If the platform isn't supported, the hook returns an error. Use `pipe_to("your-clipboard-cmd")` for unsupported platforms.

---

## Error Handling

Pipe errors propagate as hook errors:

```rust
// Command failed
// Error: Command `jq` failed with status 1

// Timeout
// Error: Command `slow-process` timed out after 30s
```

The error includes the command name for debugging when multiple pipes are chained.

---

## Custom Pipe Targets

Implement `PipeTarget` for custom processing:

```rust
use standout_pipe::{PipeTarget, PipeError};

struct UppercasePipe;

impl PipeTarget for UppercasePipe {
    fn pipe(&self, input: &str) -> Result<String, PipeError> {
        Ok(input.to_uppercase())
    }
}

// Use it
.pipe_with(UppercasePipe)
```

This is useful for transformations that don't need a shell command.

---

## ANSI Code Handling

**Piped content is always plain text.** This matches standard shell behavior where `command | other_command` receives unformatted output because stdout is not a TTY.

When you pipe output:

- The piped content has all ANSI escape codes stripped automatically
- Terminal display still shows rich formatting (colors, bold, etc.)
- Clipboard operations receive clean, pasteable text

```rust
// Your template has styled output
cfg.template("[bold]{{ title }}[/bold]: [green]{{ count }}[/green]")
   .pipe_through("jq .")

// Terminal sees: "\x1b[1mReport\x1b[0m: \x1b[32m42\x1b[0m" (formatted)
// jq receives:   "Report: 42" (plain text)
```

This is implemented using the framework's two-pass rendering:

1. Template engine produces output with `[style]...[/style]` tags
2. `apply_style_tags` is called twice: once with ANSI codes for terminal, once stripped for piping

**Custom pipe targets** also receive plain text via the `PipeTarget::pipe(&self, input: &str)` method.

---

## Limitations

**Text output only**: Piping only operates on `RenderedOutput::Text`. Binary and silent outputs pass through unchanged.

**Memory buffering**: The entire output is buffered in memory before and after piping. For multi-megabyte outputs, consider streaming alternatives.

**Shell execution**: Commands run through `sh -c` (Unix) or `cmd /C` (Windows). Be careful when constructing commands from untrusted input—see Security below.

---

## Security Considerations

Commands are passed to the shell, so constructing them from user input requires care:

```rust
// DANGEROUS if user_input is untrusted:
.pipe_through(&format!("grep {}", user_input))

// User could pass: "; rm -rf /"

// SAFE: use fixed commands
.pipe_through("grep pattern")

// Or validate/sanitize input first
```

This is a general shell injection concern, not specific to standout-pipe. If you need to pass user input to commands, sanitize it or use a command that accepts arguments safely.

---

## Integration with Hooks

Piping runs as a post-output hook, after all rendering is complete:

```text
Handler → Post-dispatch → Render → Post-output (piping here) → Final Output
```

You can combine piping with other post-output hooks:

```rust
.post_output(add_footer)  // Runs first
.pipe_through("jq .")     // Receives footer-added output
```

> **Tip:** For details on the full pipeline, see [Execution Model](../../standout-dispatch/docs/topics/execution-model.md).

---

## Summary

| Feature | Method/Attribute |
| --------- | ------------------ |
| Log while displaying | `pipe_to("tee file")` |
| Filter output | `pipe_through("jq .data")` |
| Copy to clipboard | `pipe_to_clipboard` |
| Custom timeout | `pipe_to_with_timeout(cmd, duration)` |
| Custom logic | `pipe_with(impl PipeTarget)` |
| Chain pipes | Call multiple methods |
