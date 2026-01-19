# Partial Adoption

One of the key benefits of `standout-dispatch` is that you don't need to adopt it all at once. You can migrate one command at a time, keeping existing code alongside dispatch-managed commands.

---

## The Problem with All-or-Nothing Frameworks

Many CLI frameworks require a complete rewrite:

- All commands must use the framework's patterns
- Existing code can't coexist with framework code
- Migration is a massive undertaking
- Risk is concentrated in a single change

`standout-dispatch` is designed differently. It's a library, not a framework—you call it, it doesn't call you.

---

## Strategy: Migrate One Command at a Time

### Step 1: Identify a Good Starting Command

Pick a command that:
- Is self-contained (few dependencies on other commands)
- Has clear inputs and outputs
- Would benefit from structured output (JSON, etc.)
- Has existing tests you can update

### Step 2: Create the Handler

Convert the command's logic to a handler:

```rust
// Before: mixed logic and output
fn list_command(matches: &ArgMatches) {
    let items = storage::list().unwrap();
    for item in items {
        println!("{}: {}", item.id, item.name);
    }
}

// After: handler returns data
fn list_handler(_m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<Vec<Item>> {
    let items = storage::list()?;
    Ok(Output::Render(items))
}
```

### Step 3: Set Up Dispatch for That Command

```rust
use standout_dispatch::{FnHandler, from_fn, extract_command_path, path_to_string};

fn main() {
    let cmd = build_clap_command();  // Your existing clap definition
    let matches = cmd.get_matches();
    let path = extract_command_path(&matches);

    // Dispatch-managed command
    if path_to_string(&path) == "list" {
        let handler = FnHandler::new(list_handler);
        let render = from_fn(|data, _| Ok(serde_json::to_string_pretty(data)?));

        let ctx = CommandContext { command_path: path };
        if let Ok(Output::Render(data)) = handler.handle(&matches, &ctx) {
            let json = serde_json::to_value(&data).unwrap();
            println!("{}", render(&json, "list").unwrap());
        }
        return;
    }

    // Fall back to existing code for other commands
    match matches.subcommand() {
        Some(("add", sub)) => add_command(sub),
        Some(("delete", sub)) => delete_command(sub),
        _ => {}
    }
}
```

### Step 4: Repeat

Migrate one command at a time. Each migration:
- Is a small, reviewable change
- Can be tested independently
- Doesn't affect other commands
- Is easy to roll back if needed

---

## Coexistence Patterns

### Pattern 1: Check Path First

```rust
let path = extract_command_path(&matches);

// Dispatch-managed commands
let dispatch_commands = ["list", "show", "export"];
if dispatch_commands.contains(&path_to_string(&path).as_str()) {
    dispatch_command(&matches, &path);
    return;
}

// Legacy commands
legacy_dispatch(&matches);
```

### Pattern 2: Try Dispatch, Fall Back

```rust
if let Some(result) = try_dispatch(&matches) {
    handle_dispatch_result(result);
} else {
    // Not a dispatch-managed command
    legacy_dispatch(&matches);
}
```

### Pattern 3: Wrapper Function

```rust
fn run_command(matches: &ArgMatches) {
    let path = extract_command_path(matches);

    match path_to_string(&path).as_str() {
        // New dispatch-based handlers
        "list" => run_with_dispatch(list_handler, matches, &path),
        "show" => run_with_dispatch(show_handler, matches, &path),

        // Legacy handlers (unchanged)
        "add" => add_command(get_deepest_matches(matches)),
        "delete" => delete_command(get_deepest_matches(matches)),

        _ => eprintln!("Unknown command"),
    }
}

fn run_with_dispatch<T: Serialize>(
    handler: impl Fn(&ArgMatches, &CommandContext) -> HandlerResult<T>,
    matches: &ArgMatches,
    path: &[String],
) {
    let ctx = CommandContext { command_path: path.to_vec() };
    match handler(matches, &ctx) {
        Ok(Output::Render(data)) => {
            let json = serde_json::to_value(&data).unwrap();
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
        }
        Ok(Output::Silent) => {}
        Ok(Output::Binary { data, filename }) => {
            std::fs::write(&filename, &data).unwrap();
        }
        Err(e) => eprintln!("Error: {}", e),
    }
}
```

---

## Benefits During Migration

### Immediate Benefits per Command

Each migrated command gains:

1. **Structured output** — JSON/YAML support
2. **Testable logic** — Handler is a pure function
3. **Error handling** — `?` operator, proper error types
4. **Hook points** — Add logging, auth without touching handler

### Progressive Enhancement

As you migrate more commands:

1. **Shared hooks** — Apply auth check to all migrated commands
2. **Consistent output** — Same renderer for all commands
3. **Unified error handling** — Errors formatted consistently

---

## Migration Checklist

For each command:

- [ ] Create data types (`#[derive(Serialize)]`)
- [ ] Write handler function
- [ ] Add to dispatch routing
- [ ] Update tests to test handler directly
- [ ] Verify existing behavior unchanged
- [ ] Document the migration

---

## Example: Full Migration

Before (monolithic):

```rust
fn main() {
    let matches = build_cli().get_matches();

    match matches.subcommand() {
        Some(("list", sub)) => list_command(sub),
        Some(("add", sub)) => add_command(sub),
        Some(("delete", sub)) => delete_command(sub),
        Some(("export", sub)) => export_command(sub),
        _ => {}
    }
}
```

After (gradual migration):

```rust
fn main() {
    let matches = build_cli().get_matches();
    let path = extract_command_path(&matches);

    // Dispatch-managed (migrated)
    if let Some(result) = dispatch_if_managed(&matches, &path) {
        return;
    }

    // Legacy (not yet migrated)
    match matches.subcommand() {
        Some(("add", sub)) => add_command(sub),
        Some(("delete", sub)) => delete_command(sub),
        _ => {}
    }
}

fn dispatch_if_managed(matches: &ArgMatches, path: &[String]) -> Option<()> {
    let ctx = CommandContext { command_path: path.to_vec() };
    let render = from_fn(|data, _| Ok(serde_json::to_string_pretty(data)?));

    let result = match path_to_string(path).as_str() {
        "list" => list_handler(matches, &ctx),
        "export" => export_handler(matches, &ctx),
        _ => return None,  // Not managed by dispatch
    };

    match result {
        Ok(Output::Render(data)) => {
            let json = serde_json::to_value(&data).ok()?;
            println!("{}", render(&json, "").ok()?);
        }
        Ok(Output::Silent) => {}
        Ok(Output::Binary { data, filename }) => {
            std::fs::write(&filename, &data).ok()?;
        }
        Err(e) => eprintln!("Error: {}", e),
    }

    Some(())
}
```

---

## Summary

Partial adoption lets you:

1. **Start small** — Migrate one command at a time
2. **Reduce risk** — Each migration is independent
3. **Maintain velocity** — Keep shipping while migrating
4. **Validate benefits** — See the value before full commitment

The goal is pragmatic improvement, not architectural purity. Migrate what benefits most, leave what works alone.
