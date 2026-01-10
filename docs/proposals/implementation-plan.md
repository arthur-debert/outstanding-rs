# Implementation Plan: Outstanding Declarative API

## Decisions

- **Yaml**: Deferred to future release. Only Json for now.
- **Error type**: `anyhow::Error` for rich error context.
- **Output capture**: Split logic - `run()` returns output, separate method handles printing.

---

## Overview

Build in layers, each self-contained and testable:

1. **Core: OutputMode extensions** - Add Json to OutputMode
2. **Core: Structured output** - Render-or-serialize logic
3. **Clap: Core Types** - CommandContext, CommandResult, RunResult
4. **Clap: Handler trait** - The abstraction for command logic
5. **Clap: Router** - Command registration and dispatch
6. **Clap: Integration** - Wire it all together

---

## Phase 1: Core - OutputMode Extensions

**Goal**: Extend `OutputMode` to include Json.

**Changes** (`crates/outstanding/src/lib.rs`):
```rust
pub enum OutputMode {
    Auto,
    Term,
    Text,
    TermDebug,
    Json,   // NEW
}
```

**Also update**:
- `should_use_color()` - return false for Json
- `is_debug()` - return false for Json
- Add `is_structured()` helper - returns true for Json

**Tests**:
- `OutputMode::Json.should_use_color()` returns false
- `OutputMode::Json.is_structured()` returns true
- Existing tests still pass

**Completion criteria**:
- `cargo test -p outstanding` passes
- New variant exists with correct behavior

---

## Phase 2: Core - Structured Output Helper

**Goal**: Provide a function that either renders a template OR serializes to JSON based on mode.

**New function** (`crates/outstanding/src/lib.rs`):
```rust
/// Renders data using a template, or serializes directly for structured modes.
pub fn render_or_serialize<T: Serialize>(
    template: &str,
    data: &T,
    theme: ThemeChoice<'_>,
    mode: OutputMode,
) -> Result<String, Error> {
    match mode {
        OutputMode::Json => {
            serde_json::to_string_pretty(data)
                .map_err(|e| Error::new(ErrorKind::InvalidOperation, e.to_string()))
        }
        _ => render_with_output(template, data, theme, mode)
    }
}
```

**Dependencies**: `serde_json` already present.

**Tests**:
```rust
#[test]
fn test_render_or_serialize_json() {
    let data = json!({"name": "test", "count": 42});
    let output = render_or_serialize("unused", &data, theme, OutputMode::Json).unwrap();
    assert!(output.contains("\"name\": \"test\""));
}

#[test]
fn test_render_or_serialize_term_uses_template() {
    let data = json!({"name": "test"});
    let output = render_or_serialize("Name: {{ name }}", &data, theme, OutputMode::Text).unwrap();
    assert_eq!(output, "Name: test");
}
```

**Completion criteria**:
- `cargo test -p outstanding` passes
- JSON mode skips template, returns valid JSON
- Template modes still work as before

---

## Phase 3: Clap - Core Types

**Goal**: Define the types needed for command handling.

**New types** (`crates/outstanding-clap/src/handler.rs`):

```rust
/// Context passed to command handlers
pub struct CommandContext {
    pub output_mode: OutputMode,
    pub command_path: Vec<String>,
}

/// Result of a command handler
pub enum CommandResult<T: Serialize> {
    /// Success with data to render
    Ok(T),
    /// Error with context
    Err(anyhow::Error),
    /// Silent exit (no output)
    Silent,
}

/// Result of running the CLI
pub enum RunResult {
    /// A handler processed the command, here's the output
    Handled(String),
    /// No handler matched; here are the matches
    Unhandled(ArgMatches),
}
```

**Dependencies**: Add `anyhow` to Cargo.toml

**Tests**:
- Types can be constructed
- `CommandResult::Ok` holds serializable data
- Basic ergonomics (Debug, Clone where appropriate)

**Completion criteria**:
- Types compile and are exported
- Unit tests for type construction

---

## Phase 4: Clap - Handler Trait

**Goal**: Define the handler abstraction and closure support.

**The trait**:
```rust
pub trait Handler: Send + Sync {
    type Output: Serialize;
    fn handle(&self, matches: &ArgMatches, ctx: &CommandContext) -> CommandResult<Self::Output>;
}
```

**Blanket impl for closures**:
```rust
impl<F, T> Handler for F
where
    F: Fn(&ArgMatches, &CommandContext) -> CommandResult<T> + Send + Sync,
    T: Serialize,
{
    type Output = T;
    fn handle(&self, m: &ArgMatches, ctx: &CommandContext) -> CommandResult<T> {
        (self)(m, ctx)
    }
}
```

**Completion criteria**:
- `Handler` trait is defined and exported
- Both closures and structs can implement it
- Unit tests pass

---

## Phase 5: Clap - Router Builder

**Goal**: Build the registration and dispatch mechanism.

**Builder methods**:
```rust
impl OutstandingBuilder {
    /// Register a command handler with a template
    pub fn command<H, T>(self, path: &str, handler: H, template: &str) -> Self
    where
        H: Handler<Output = T> + 'static,
        T: Serialize + 'static;
}
```

**Dispatch returns output, doesn't print**:
```rust
// Returns RunResult::Handled(output_string) or RunResult::Unhandled(matches)
```

**Completion criteria**:
- Handlers can be registered
- Registered commands dispatch to handlers
- Unregistered commands fall through
- Output is returned, not printed

---

## Phase 6: Clap - Full Integration

**Goal**: Wire everything together, update `--output` flag, polish API.

**Changes**:
- Add `json` to `--output` flag values
- Ensure `render_or_serialize` is used in dispatch
- Add convenience `run_and_print()` that wraps `run()` and prints
- Polish public API exports

**Completion criteria**:
- Full workflow works end-to-end
- JSON mode works
- Existing help system still works
- All tests pass

---

## Summary Table

| Phase | Crate | Deliverable | Key Test |
|-------|-------|-------------|----------|
| 1 | outstanding | OutputMode::Json | `is_structured()` returns true |
| 2 | outstanding | `render_or_serialize()` | JSON mode skips template |
| 3 | outstanding-clap | CommandContext, CommandResult, RunResult | Types construct correctly |
| 4 | outstanding-clap | Handler trait + closure impl | Closures work as handlers |
| 5 | outstanding-clap | Router registration + dispatch | Commands dispatch to handlers |
| 6 | outstanding-clap | Full integration | End-to-end workflow |
