# Declarative Command Dispatch

**Status:** Proposal
**Author:** Claude
**Date:** 2026-01-12

## Motivation

CLI applications built with clap typically follow a pattern:

1. Define argument structures using clap's derive macros
2. Parse input with `clap::Parser`
3. Write a manual dispatch tree matching commands to handler functions

Step 3 is repetitive boilerplate. Worse, handler functions end up with CLI-aware signatures that manually extract arguments from `ArgMatches`:

```rust
fn migrate(matches: &ArgMatches, ctx: &CommandContext) -> CommandResult<Value> {
    let db = matches.get_one::<String>("database").unwrap();
    let host = matches.get_one::<String>("host").unwrap_or(&"localhost".into());
    let dry_run = matches.get_flag("dry-run");

    // Finally, actual business logic...
}
```

This is error-prone (typos in string keys, missing unwraps), verbose, and couples business logic to CLI infrastructure.

**The ideal**: handlers with natural function signatures that receive typed, validated arguments:

```rust
fn migrate(args: MigrateArgs) -> Result<MigrateOutput, Error> {
    // Pure business logic - no CLI awareness
}
```

## Goals

1. **Natural handler signatures** - Handlers receive typed structs, not raw `ArgMatches`
2. **Leverage clap's derive macros** - Don't reinvent argument parsing; build on `#[derive(Args)]`
3. **Progressive disclosure** - Simple cases are simple; power users get escape hatches
4. **Type-safe dispatch** - Compile-time verification of command paths and handler types
5. **Composable** - Works with existing hook system (pre_dispatch, post_dispatch, post_output)
6. **Convention over configuration** - Templates resolve from command paths by default

## Assumptions

1. **Users have a working clap CLI** - We integrate with clap, not replace it
2. **Clap derive is the norm** - Most users define `#[derive(Args)]` structs for their commands
3. **Handlers are pure functions** - Business logic shouldn't depend on CLI infrastructure
4. **Serializable output** - All handler output must implement `Serialize` for template rendering
5. **Single dispatch target** - Each command maps to exactly one handler function

## Non-Goals

- Replacing clap's argument parsing
- Async handler support (can be added later)
- Runtime command discovery (compile-time only)

---

## Design Overview

### The Three-Layer Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│ Layer 3: Attribute Macro (Future)                               │
│   #[outstanding::handler]                                        │
│   Auto-generates Args struct from function signature             │
├─────────────────────────────────────────────────────────────────┤
│ Layer 2: Derive Macro Integration (Future)                      │
│   #[derive(Dispatch)] on clap enums                              │
│   Generates routing from subcommand tree to handlers            │
├─────────────────────────────────────────────────────────────────┤
│ Layer 1: Args-Aware Handler Trait (Foundation) ← START HERE     │
│   IntoHandler trait + typed builder methods                      │
│   Enables typed argument extraction                             │
├─────────────────────────────────────────────────────────────────┤
│ Layer 0: Current System (Unchanged)                             │
│   Handler trait + dispatch! macro + hooks                        │
│   For power users who need raw ArgMatches access                │
└─────────────────────────────────────────────────────────────────┘
```

Each layer builds on the one below. Macros (Layer 2, 3) generate code that uses the builder API (Layer 1).

---

## Layer 1: Handler Shapes

We support multiple handler "shapes" (function signatures), all converging to the same internal machinery:

### Supported Signatures

```rust
// Shape A: Full control (current behavior, escape hatch)
fn(&ArgMatches, &CommandContext) -> CommandResult<T>

// Shape B: Typed args with context
fn(Args, &CommandContext) -> CommandResult<T>

// Shape C: Typed args, no context (most common)
fn(Args) -> CommandResult<T>

// Shape D: No args, no context (e.g., version command)
fn() -> CommandResult<T>
```

Additionally, for simpler cases, we support `Result<T, E>` return types that auto-convert:

```rust
// Shape C with Result (auto-converts to CommandResult)
fn(Args) -> Result<T, E>  where E: Into<anyhow::Error>

// Shape D with Result
fn() -> Result<T, E>
```

### The `IntoHandler` Trait

A single trait converts any handler shape into our internal dispatch machinery:

```rust
/// Marker types for handler shapes (enables multiple blanket impls)
pub mod handler_shape {
    pub struct Raw;           // (&ArgMatches, &CommandContext)
    pub struct ArgsCtx<A>(std::marker::PhantomData<A>);  // (A, &CommandContext)
    pub struct ArgsOnly<A>(std::marker::PhantomData<A>); // (A,)
    pub struct NoArgs;        // ()
}

/// Trait for converting any handler shape into a dispatch function.
pub trait IntoHandler<Shape>: Send + Sync + 'static {
    /// Convert this handler into a type-erased dispatch function.
    fn into_dispatch_fn(self, template_resolver: TemplateResolver) -> DispatchFn;
}
```

### Blanket Implementations

```rust
// Shape A: fn(&ArgMatches, &CommandContext) -> CommandResult<T>
impl<F, T> IntoHandler<handler_shape::Raw> for F
where
    F: Fn(&ArgMatches, &CommandContext) -> CommandResult<T> + Send + Sync + 'static,
    T: Serialize + 'static,
{
    fn into_dispatch_fn(self, resolver: TemplateResolver) -> DispatchFn {
        Arc::new(move |matches, ctx, hooks| {
            let result = (self)(matches, ctx);
            resolver.render(result, hooks)
        })
    }
}

// Shape B: fn(A, &CommandContext) -> CommandResult<T>
impl<F, A, T> IntoHandler<handler_shape::ArgsCtx<A>> for F
where
    F: Fn(A, &CommandContext) -> CommandResult<T> + Send + Sync + 'static,
    A: clap::FromArgMatches + Clone + Send + Sync + 'static,
    T: Serialize + 'static,
{
    fn into_dispatch_fn(self, resolver: TemplateResolver) -> DispatchFn {
        Arc::new(move |matches, ctx, hooks| {
            let args = A::from_arg_matches(matches)
                .map_err(|e| ExtractError::args(ctx.command_path.join("."), e))?;
            let result = (self)(args, ctx);
            resolver.render(result, hooks)
        })
    }
}

// Shape C: fn(A) -> CommandResult<T>  (context-free)
impl<F, A, T> IntoHandler<handler_shape::ArgsOnly<A>> for F
where
    F: Fn(A) -> CommandResult<T> + Send + Sync + 'static,
    A: clap::FromArgMatches + Clone + Send + Sync + 'static,
    T: Serialize + 'static,
{
    fn into_dispatch_fn(self, resolver: TemplateResolver) -> DispatchFn {
        Arc::new(move |matches, ctx, hooks| {
            let args = A::from_arg_matches(matches)
                .map_err(|e| ExtractError::args(ctx.command_path.join("."), e))?;
            let result = (self)(args);
            resolver.render(result, hooks)
        })
    }
}

// Shape D: fn() -> CommandResult<T>  (no args)
impl<F, T> IntoHandler<handler_shape::NoArgs> for F
where
    F: Fn() -> CommandResult<T> + Send + Sync + 'static,
    T: Serialize + 'static,
{
    fn into_dispatch_fn(self, resolver: TemplateResolver) -> DispatchFn {
        Arc::new(move |_matches, _ctx, hooks| {
            let result = (self)();
            resolver.render(result, hooks)
        })
    }
}
```

---

## Return Type Flexibility

### The `IntoCommandResult` Trait

Support both explicit `CommandResult<T>` and simple `Result<T, E>`:

```rust
/// Trait for types that can be converted to CommandResult.
pub trait IntoCommandResult<T: Serialize> {
    fn into_command_result(self) -> CommandResult<T>;
}

// CommandResult passes through unchanged
impl<T: Serialize> IntoCommandResult<T> for CommandResult<T> {
    fn into_command_result(self) -> CommandResult<T> {
        self
    }
}

// Result<T, E> converts automatically
impl<T: Serialize, E: Into<anyhow::Error>> IntoCommandResult<T> for Result<T, E> {
    fn into_command_result(self) -> CommandResult<T> {
        match self {
            Ok(data) => CommandResult::Ok(data),
            Err(e) => CommandResult::Err(e.into()),
        }
    }
}

// Raw T for infallible handlers (optional - may be too magical)
impl<T: Serialize> IntoCommandResult<T> for T {
    fn into_command_result(self) -> CommandResult<T> {
        CommandResult::Ok(self)
    }
}
```

This allows handlers to use whichever return type is most natural:

```rust
// Explicit control (can return Silent, Archive, etc.)
fn export(args: ExportArgs) -> CommandResult<ExportOutput> {
    if args.format == "pdf" {
        CommandResult::Archive(pdf_bytes, "export.pdf".into())
    } else {
        CommandResult::Ok(ExportOutput { ... })
    }
}

// Simple Result (most common)
fn migrate(args: MigrateArgs) -> Result<MigrateOutput, MigrateError> {
    Ok(MigrateOutput { success: true })
}

// Infallible (optional)
fn version() -> VersionOutput {
    VersionOutput { version: "1.0.0".into() }
}
```

---

## Error Handling

### The `ExtractError` Type

Extraction failures include context about what failed and where:

```rust
use thiserror::Error;

/// Error during argument extraction.
#[derive(Debug, Error)]
pub enum ExtractError {
    /// Clap failed to parse/extract arguments
    #[error("Failed to extract arguments for '{command}': {message}")]
    Extraction {
        command: String,
        message: String,
        #[source]
        source: Option<clap::Error>,
    },
}

impl ExtractError {
    /// Create from a clap error with command context.
    pub fn args(command: impl Into<String>, error: clap::Error) -> Self {
        Self::Extraction {
            command: command.into(),
            message: error.to_string(),
            source: Some(error),
        }
    }
}
```

---

## Builder API

### Explicit Methods (Recommended for Phase 1)

```rust
impl OutstandingBuilder {
    /// Register handler with raw ArgMatches access (escape hatch)
    pub fn command<F, T>(self, path: &str, handler: F, template: &str) -> Self
    where
        F: Fn(&ArgMatches, &CommandContext) -> CommandResult<T> + Send + Sync + 'static,
        T: Serialize + 'static;

    /// Register handler with typed Args and context
    pub fn handler_with_context<A, F, R, T>(self, path: &str, handler: F) -> Self
    where
        A: FromArgMatches + Clone + Send + Sync + 'static,
        F: Fn(A, &CommandContext) -> R + Send + Sync + 'static,
        R: IntoCommandResult<T>,
        T: Serialize + 'static;

    /// Register context-free handler with typed Args (MOST COMMON)
    pub fn handler<A, F, R, T>(self, path: &str, handler: F) -> Self
    where
        A: FromArgMatches + Clone + Send + Sync + 'static,
        F: Fn(A) -> R + Send + Sync + 'static,
        R: IntoCommandResult<T>,
        T: Serialize + 'static;
    /// Register a handler function.
    ///
    /// The handler function's arguments are automatically extracted from the
    /// command context using the `FromContext` trait. Its return type is
    /// converted to `CommandResult` using `IntoCommandResult`.
    ///
    /// This method supports various handler signatures, including:
    /// - `fn(Args) -> Result<T, E>`
    /// - `fn(Args, CommandContext) -> Result<T, E>`
    /// - `fn() -> Result<T, E>`
    /// - `fn(&ArgMatches) -> Result<T, E>` (escape hatch)
    ///
    /// Where `Args` implements `clap::FromArgMatches` and `E` implements `Into<anyhow::Error>`.
    pub fn handler<H, T>(self, path: &str, handler: H) -> Self
    where
        H: IntoHandler<T> + Send + Sync + 'static,
        T: 'static, // Placeholder for the tuple of extractors
    {
        // ... internal implementation using IntoHandler ...
        unimplemented!()
    }

    // Shape C with Result (auto-converts to CommandResult)
    // fn(Args) -> Result<T, E>  where E: Into<anyhow::Error>

    // Shape D with Result
    // fn() -> Result<T, E>
}
```

---

## Macro Syntax Enhancement

The `dispatch!` macro extends to support typed handlers with full inference:

```rust
dispatch! {
    db: {
        // Fully inferred! (Arg type deduced from function signature)
        migrate => migrate,

        // With config block
        backup => {
            handler: backup,
            template: "backup.j2",
            pre_dispatch: validate_auth,
        },
    },

    // Simple handler (no args)
    version => version,
}
```

The macro expands to builder method calls using the `IntoHandler` trait:

```rust
|builder: GroupBuilder| {
    builder
        .handler("migrate", migrate)
        .handler_with_config("backup", backup, |cfg| {
            cfg.template("backup.j2").pre_dispatch(validate_auth)
        })
        .handler("version", version)
}
```

---

## Complete Example

### Before (Current System)

```rust
use clap::{Command, Arg, ArgMatches};
use outstanding_clap::{Outstanding, CommandResult, CommandContext, dispatch};
use serde::Serialize;
use serde_json::json;

fn migrate(matches: &ArgMatches, _ctx: &CommandContext) -> CommandResult<serde_json::Value> {
    // Manual extraction - verbose and error-prone
    let db = matches.get_one::<String>("database").unwrap();
    let host = matches.get_one::<String>("host").unwrap_or(&"localhost".to_string());
    let dry_run = matches.get_flag("dry-run");

    if dry_run {
        return CommandResult::Ok(json!({ "success": true, "tables": 0 }));
    }

    CommandResult::Ok(json!({ "success": true, "tables": 42 }))
}

fn main() {
    let cmd = Command::new("myapp")
        .subcommand(Command::new("db")
            .subcommand(Command::new("migrate")
                .arg(Arg::new("database").required(true))
                .arg(Arg::new("host").long("host").default_value("localhost"))
                .arg(Arg::new("dry-run").long("dry-run").action(clap::ArgAction::SetTrue))));

    Outstanding::builder()
        .template_dir("templates")
        .commands(dispatch! {
            db: {
                migrate => migrate,
            },
        })
        .run_and_print(cmd, std::env::args());
}
```

### After (Proposed Design)

```rust
use clap::{Args, Parser, Subcommand};
use outstanding_clap::{Outstanding, dispatch, CommandContext};
use serde::Serialize;
use anyhow::Result; // Standard error handling!

// Clap handles extraction - type-safe and self-documenting
#[derive(Args)]  // No Clone needed!
struct MigrateArgs {
    /// Database name
    database: String,

    /// Host address
    #[arg(long, default_value = "localhost")]
    host: String,

    /// Perform dry run without changes
    #[arg(long)]
    dry_run: bool,
}

#[derive(Serialize)]
struct MigrateOutput {
    success: bool,
    tables: usize,
}

// Clean handler - pure business logic!
// Return standard Result (handled by IntoCommandResult)
// Context is optional - only requested if needed
fn migrate(args: MigrateArgs, _ctx: CommandContext) -> Result<MigrateOutput> {
    if args.dry_run {
        return Ok(MigrateOutput { success: true, tables: 0 });
    }

    // Use args.database, args.host naturally
    Ok(MigrateOutput { success: true, tables: 42 })
}

fn main() {
    Outstanding::builder()
        .template_dir("templates")
        .commands(dispatch! {
            db: {
                // Type inference handles the rest
                migrate => migrate,
            },
        })
        .run_and_print(cmd, std::env::args());
}
```

---

## Architecture: The Extractor Pattern

Instead of hardcoded handler shapes, we use an **Extractor Pattern** (similar to Axum). This allows for infinite extensibility.

### The `FromContext` Trait

```rust
pub trait FromContext: Sized {
    type Error: Into<anyhow::Error>;
    fn from_context(matches: &ArgMatches, ctx: &CommandContext) -> Result<Self, Self::Error>;
}
```

### Supported Extractors

1.  **`T` (Typed Args)**: Any type implementing `clap::FromArgMatches`.
2.  **`CommandContext`**: The framework context.
3.  **`&ArgMatches`**: Raw escape hatch.
4.  **`Option<T>`**: Optional extractors.

This architecture allows future extensions, such as injecting database connections or user sessions, simply by implementing `FromContext`.

### The `IntoHandler` Trait

```rust
pub trait IntoHandler<Args>: Send + Sync + 'static {
    type Output: Serialize;
    fn call(&self, matches: &ArgMatches, ctx: &CommandContext) -> CommandResult<Self::Output>;
}

// Blanket impl for any function (A, B) -> R where A, B are Extractors
impl<F, A, B, R> IntoHandler<(A, B)> for F
where
    F: Fn(A, B) -> R + Send + Sync + 'static,
    A: FromContext,
    B: FromContext,
    R: IntoCommandResult,
{ /* ... */ }
```

---

## Evolution: Derive Macros (Layer 2)

For users who use clap's derive pattern with enum-based subcommands:

```rust
use clap::{Parser, Subcommand, Args};
use outstanding_clap::Dispatch;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Dispatch)]  // Add Dispatch derive
enum Commands {
    /// Database operations
    Db(DbCommands),
}

#[derive(Subcommand, Dispatch)]
enum DbCommands {
    /// Run migrations
    #[dispatch(handler = db::migrate)]
    Migrate(MigrateArgs),

    /// Backup database
    #[dispatch(handler = db::backup, template = "backup.j2")]
    Backup(BackupArgs),
}
```

---

## Implementation Plan

### Phase 1: Core Traits (Foundation)

1.  **Define `FromContext` Trait**: The core extractor interface.
2.  **Implement Extractors**: For `FromArgMatches`, `CommandContext`.
3.  **Define `IntoCommandResult`**: Support `Result<T, E>` and raw `T`.
4.  **Refactor `Handler`**: Move to the generic `IntoHandler` based on extractors.
5.  **Update Builder**: Make `handler()` methods generic over `IntoHandler`.

### Phase 2: Macro Updates

1.  **Simplify `dispatch!`**: Remove need for explicit generic annotations (rely on inference).
2.  **Update Groups**: Ensure group builder supports the new generic handlers.

### Phase 3: Derived Dispatch (Future)

1.  Create `outstanding-macros` crate.
2.  Implement `#[derive(Dispatch)]`.

---

## Design Decisions Summary

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Handler Architecture** | **Extractor Pattern** | Infinite extensibility (inject DB, Config, etc.) vs fixed shapes. |
| **Argument Types** | `FromArgMatches` | No `Clone` needed. Efficient zero-copy where possible. |
| **Return Type** | `IntoCommandResult` | Support `anyhow::Result`, raw output, or `CommandResult`. |
| **Registration API** | Generic Inference | `migrate => migrate` is cleaner than `migrate => migrate::<Args>`. |
| **Context** | Optional (Extractor) | Only pay for what you use. |

---

## Open Questions

1.  **Async Support?**: Currently synchronous. If handlers need to be async in the future, `IntoHandler` will need to return `Future`. (Deferred for now).
2.  **Naming**: `handler()` is simple and clear.

---

## References

-   [Axum Extractors](https://docs.rs/axum/latest/axum/extract/index.html)
-   [Clap Derive Tutorial](https://docs.rs/clap/latest/clap/_derive/)
```
