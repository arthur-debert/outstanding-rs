# Handler Hooks

Hooks allow you to run custom code before and after command handlers execute. They are useful for:

- **Logging and metrics**: Record command execution times, log command invocations
- **Clipboard operations**: Copy output to clipboard after command completes
- **Output transformation**: Modify, filter, or enhance output before it's displayed
- **Data validation and enrichment**: Validate or modify handler data before rendering
- **Validation and access control**: Block commands based on conditions
- **Side effects**: Write to files, send notifications, update state

## Hook Points

There are three hook points in the command execution lifecycle:

### Pre-dispatch

Runs **before** the command handler is invoked. Pre-dispatch hooks receive the `CommandContext` and can:

- Perform setup or validation
- Abort execution by returning an error
- Log or record the command being executed

```rust
Hooks::new().pre_dispatch(|_m, ctx| {
    println!("Running command: {:?}", ctx.command_path);

    // Optionally abort
    if ctx.command_path.contains(&"dangerous".to_string()) {
        return Err(HookError::pre_dispatch("dangerous commands are disabled"));
    }

    Ok(())
})
```

### Post-dispatch

Runs **after** the command handler has executed but **before** the output is rendered. Post-dispatch hooks receive the raw handler result as a `serde_json::Value`, and can:

- Inspect or validate the data
- Add or modify fields before rendering
- Abort with an error if data is invalid

```rust
use serde_json::json;

Hooks::new().post_dispatch(|_m, _ctx, mut data| {
    // Add metadata before rendering
    if let Some(obj) = data.as_object_mut() {
        obj.insert("timestamp".into(), json!(chrono::Utc::now().to_rfc3339()));
        obj.insert("processed".into(), json!(true));
    }
    Ok(data)
})
```

Post-dispatch hooks are ideal for:

- **Data enrichment**: Add computed fields, timestamps, metadata
- **Validation**: Check data meets requirements before rendering
- **Filtering**: Remove or redact sensitive fields
- **Normalization**: Ensure consistent data structure

### Post-output

Runs **after** the command handler has executed and output has been rendered. Post-output hooks receive the `CommandContext` and `Output`, and can:

- Inspect or log the output
- Transform the output (modify text, add prefixes, etc.)
- Perform side effects (copy to clipboard, write to file)
- Abort with an error if needed

```rust
Hooks::new().post_output(|_m, _ctx, output| {
    // Copy text to clipboard (pseudo-code)
    if let Output::Text(ref text) = output {
        clipboard::copy(text)?;
    }

    // Pass through unchanged (or return a modified Output)
    Ok(output)
})
```

## Execution Flow

The hooks execute in a specific order during command dispatch:

```
┌─────────────────────────────────────────────────────────────┐
│                    Command Dispatch                          │
├─────────────────────────────────────────────────────────────┤
│  1. Pre-dispatch hooks                                       │
│     ↓ (can abort)                                           │
│  2. Handler executes → returns data                         │
│     ↓                                                        │
│  3. Post-dispatch hooks (receive raw serde_json::Value)     │
│     ↓ (can modify data or abort)                            │
│  4. Render data using template                              │
│     ↓                                                        │
│  5. Post-output hooks (receive rendered Output)             │
│     ↓ (can modify output or abort)                          │
│  6. Return final output                                      │
└─────────────────────────────────────────────────────────────┘
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
- **Post-dispatch**: Each hook receives the data from the previous hook. Transformations chain together.
- **Post-output**: Each hook receives the output from the previous hook. Transformations chain together.

```rust
use serde_json::json;

Hooks::new()
    // Pre-dispatch: validate before running
    .pre_dispatch(|_m, ctx| {
        println!("Running: {:?}", ctx.command_path);
        Ok(())
    })
    // Post-dispatch: enrich data before rendering
    .post_dispatch(|_m, _ctx, mut data| {
        if let Some(obj) = data.as_object_mut() {
            obj.insert("enriched".into(), json!(true));
        }
        Ok(data)
    })
    // Post-output: transform rendered text
    .post_output(|_m, _ctx, output| {
        if let Output::Text(text) = output {
            Ok(Output::Text(format!("[INFO] {}", text)))
        } else {
            Ok(output)
        }
    })
    // Post-output: copy to clipboard
    .post_output(|_m, _ctx, output| {
        if let Output::Text(ref text) = output {
            // clipboard::copy(text)?;
        }
        Ok(output)
    })
```

## Usage with Declarative API

Register hooks per-command using the `.hooks()` builder method:

```rust
use outstanding_clap::{Outstanding, CommandResult, Hooks, Output, HookError};
use serde::Serialize;
use serde_json::json;

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
        .post_dispatch(|_m, _ctx, mut data| {
            // Add item count before rendering
            if let Some(items) = data.get("items").and_then(|v| v.as_array()) {
                if let Some(obj) = data.as_object_mut() {
                    obj.insert("count".into(), json!(items.len()));
                }
            }
            Ok(data)
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
        .post_dispatch(|_m, _ctx, mut data| {
            // Redact sensitive values in the data
            if let Some(obj) = data.as_object_mut() {
                if let Some(v) = obj.get_mut("value") {
                    *v = json!("***");
                }
            }
            Ok(data)
        }))
```

## Usage with Regular API

For the regular API (manual dispatch), use `Outstanding::run_command()` which automatically applies registered hooks:

```rust
use outstanding_clap::{Outstanding, Hooks, CommandResult, Output};

// Build Outstanding with hooks
let outstanding = Outstanding::builder()
    .hooks("list", Hooks::new()
        .post_dispatch(|_m, _ctx, data| {
            // Validate data before rendering
            Ok(data)
        })
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

// Post-dispatch error
Hooks::new().post_dispatch(|_m, _ctx, data| {
    if data.get("items").and_then(|v| v.as_array()).map(|a| a.is_empty()) == Some(true) {
        return Err(HookError::post_dispatch("no items to display"));
    }
    Ok(data)
})

// Post-output error
Hooks::new().post_output(|_m, _ctx, _output| {
    Err(HookError::post_output("clipboard operation failed"))
})
```

The error message includes the phase where the error occurred:
```
Hook error (pre-dispatch): access denied
Hook error (post-dispatch): no items to display
Hook error (post-output): clipboard operation failed
```

## Best Practices

1. **Keep hooks focused**: Each hook should do one thing well
2. **Handle all output types**: Check for `Text`, `Binary`, and `Silent` variants in post-output hooks
3. **Preserve data/output when possible**: Return `Ok(data)` or `Ok(output)` unchanged if not transforming
4. **Use pre-dispatch for validation**: Fail fast before the handler runs
5. **Use post-dispatch for data manipulation**: Enrich, validate, or filter data before rendering
6. **Use post-output for side effects**: Logging, clipboard, file writes, etc.
7. **Chain transformations logically**: Order matters for all hook types

## API Reference

See the rustdoc for full API documentation:

- [`Hooks`](../outstanding_clap/hooks/struct.Hooks.html) - Hook configuration builder
- [`Output`](../outstanding_clap/hooks/enum.Output.html) - Command output types
- [`HookError`](../outstanding_clap/hooks/struct.HookError.html) - Hook error type
- [`HookPhase`](../outstanding_clap/hooks/enum.HookPhase.html) - Hook execution phases
