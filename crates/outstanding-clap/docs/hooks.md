# Handler Hooks

Hooks allow you to run custom code before and after command handlers execute. They are useful for:

- **Logging and metrics**: Record command execution times, log command invocations
- **Clipboard operations**: Copy output to clipboard after command completes
- **Output transformation**: Modify, filter, or enhance output before it's displayed
- **Validation and access control**: Block commands based on conditions
- **Side effects**: Write to files, send notifications, update state

## Hook Points

There are two hook points in the command execution lifecycle:

### Pre-dispatch

Runs **before** the command handler is invoked. Pre-dispatch hooks receive the `CommandContext` and can:

- Perform setup or validation
- Abort execution by returning an error
- Log or record the command being executed

```
Hooks::new().pre_dispatch(|_m, ctx| {
    println!("Running command: {:?}", ctx.command_path);

    // Optionally abort
    if ctx.command_path.contains(&"dangerous".to_string()) {
        return Err(HookError::pre_dispatch("dangerous commands are disabled"));
    }

    Ok(())
})
```

### Post-output

Runs **after** the command handler has executed and output has been rendered. Post-output hooks receive the `CommandContext` and `Output`, and can:

- Inspect or log the output
- Transform the output (modify text, add prefixes, etc.)
- Perform side effects (copy to clipboard, write to file)
- Abort with an error if needed

```
Hooks::new().post_output(|_m, _ctx, output| {
    // Copy text to clipboard (pseudo-code)
    if let Output::Text(ref text) = output {
        clipboard::copy(text)?;
    }

    // Pass through unchanged (or return a modified Output)
    Ok(output)
})
```

## Output Types

The `Output` enum represents the final output from a command:

```rust
pub enum Output {
    Text(String),              // Rendered text output
    Binary(Vec<u8>, String),   // Binary data with suggested filename
    Silent,                    // No output (silent command)
}
```

Post-output hooks can inspect and transform any output type.

## Hook Chaining

Multiple hooks at the same phase are chained and run in registration order:

- **Pre-dispatch**: All hooks run sequentially. If any returns an error, execution stops.
- **Post-output**: Each hook receives the output from the previous hook. Transformations chain together.

```rust
Hooks::new()
    // First: add a prefix
    .post_output(|_m, _ctx, output| {
        if let Output::Text(text) = output {
            Ok(Output::Text(format!("[INFO] {}", text)))
        } else {
            Ok(output)
        }
    })
    // Second: convert to uppercase
    .post_output(|_m, _ctx, output| {
        if let Output::Text(text) = output {
            Ok(Output::Text(text.to_uppercase()))
        } else {
            Ok(output)
        }
    })
    // Third: copy to clipboard
    .post_output(|_m, _ctx, output| {
        if let Output::Text(ref text) = output {
            // clipboard::copy(text)?;
        }
        Ok(output)
    })
```

With input "hello", this produces: `[INFO] HELLO`

## Usage with Declarative API

Register hooks per-command using the `.hooks()` builder method:

```rust
use outstanding_clap::{Outstanding, CommandResult, Hooks, Output, HookError};
use serde::Serialize;

#[derive(Serialize)]
struct ListOutput {
    items: Vec<String>,
}

fn list_handler(_m: &ArgMatches, _ctx: &CommandContext) -> CommandResult<ListOutput> {
    CommandResult::Ok(ListOutput {
        items: vec!["one".into(), "two".into()],
    })
}

Outstanding::builder()
    .command("list", list_handler, "{% for i in items %}{{ i }}\n{% endfor %}")
    .hooks("list", Hooks::new()
        .pre_dispatch(|_m, _ctx| {
            println!("Listing items...");
            Ok(())
        })
        .post_output(copy_to_clipboard))
    .command("export", export_handler, "")
    .hooks("export", Hooks::new()
        .post_output(|_m, _ctx, output| {
            if let Output::Binary(ref bytes, ref filename) = output {
                std::fs::write(filename, bytes)?;
                println!("Wrote {} bytes to {}", bytes.len(), filename);
            }
            Ok(output)
        }))
    .run_and_print(cmd, std::env::args());
```

### Nested Commands

Use dot notation for nested command paths:

```rust
Outstanding::builder()
    .command("config.get", config_get_handler, "{{ value }}")
    .hooks("config.get", Hooks::new()
        .post_output(|_m, _ctx, output| {
            // Mask sensitive values
            if let Output::Text(_) = output {
                Ok(Output::Text("***".into()))
            } else {
                Ok(output)
            }
        }))
```

## Usage with Regular API

For the regular API (manual dispatch), use `Outstanding::run_command()` which automatically applies registered hooks:

```rust
use outstanding_clap::{Outstanding, Hooks, CommandResult, Output};

// Build Outstanding with hooks
let outstanding = Outstanding::builder()
    .hooks("list", Hooks::new()
        .post_output(copy_to_clipboard))
    .build();

// Parse arguments (hooks are NOT applied here)
let matches = outstanding.run_with(cmd);

// Dispatch with hooks applied automatically
match matches.subcommand() {
    Some(("list", sub_m)) => {
        match outstanding.run_command("list", sub_m, |m, ctx| {
            let items = fetch_items();
            CommandResult::Ok(ListOutput { items })
        }, "{% for i in items %}{{ i }}\n{% endfor %}") {
            Ok(output) => {
                // Output has already been processed by hooks
                if let Output::Text(text) = output {
                    println!("{}", text);
                }
            }
            Err(e) => eprintln!("Error: {}", e),
        }
    }
    _ => {}
}
```

## Error Handling

Hook errors abort execution immediately. Use `HookError` to create errors with phase information:

```rust
use outstanding_clap::HookError;

// Pre-dispatch error
Hooks::new().pre_dispatch(|_m, _ctx| {
    Err(HookError::pre_dispatch("access denied"))
})

// Post-output error
Hooks::new().post_output(|_m, _ctx, _output| {
    Err(HookError::post_output("clipboard operation failed"))
})
```

The error message includes the phase where the error occurred:
```
Hook error (pre-dispatch): access denied
Hook error (post-output): clipboard operation failed
```

## Best Practices

1. **Keep hooks focused**: Each hook should do one thing well
2. **Handle all output types**: Check for `Text`, `Binary`, and `Silent` variants
3. **Preserve output when possible**: Return `Ok(output)` unchanged if not transforming
4. **Use pre-dispatch for validation**: Fail fast before the handler runs
5. **Use post-output for side effects**: Logging, clipboard, file writes, etc.
6. **Chain transformations logically**: Order matters for post-output hooks

## API Reference

See the rustdoc for full API documentation:

- [`Hooks`](../outstanding_clap/hooks/struct.Hooks.html) - Hook configuration builder
- [`Output`](../outstanding_clap/hooks/enum.Output.html) - Command output types
- [`HookError`](../outstanding_clap/hooks/struct.HookError.html) - Hook error type
- [`HookPhase`](../outstanding_clap/hooks/enum.HookPhase.html) - Hook execution phases
