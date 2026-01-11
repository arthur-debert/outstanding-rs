# Render Context Injection

## Summary

Add a general-purpose mechanism for injecting additional context objects into templates beyond the handler's serialized data. This enables templates to access utilities, formatters, and runtime-computed values that cannot be represented as JSON.

## Motivation

### The Problem

Currently, templates only have access to data passed from command handlers:

```
Handler → Serialize → JSON Value → Template
```

This works well for data, but falls short for:

1. **Non-serializable utilities**: `TableFormatter` instances that need terminal width
2. **Runtime values**: Terminal dimensions, TTY detection, timestamps
3. **Shared configuration**: User preferences, environment settings
4. **Callable utilities**: Formatters, converters, validators

### Concrete Example: Table Formatting

Users requested declarative table formatting in templates:

```jinja
{% for commit in commits %}
{{ tables.log.row([commit.hash, commit.message]) }}
{% endfor %}
```

This requires injecting a `TableFormatter` object into the template context. The formatter needs:
- The `TableSpec` definition (column widths, alignment, etc.)
- Terminal width (known only at render time)

Without context injection, users must either:
- Repeat column widths in every `col()` filter call
- Pre-format data in handlers (mixing presentation with logic)
- Accept that Fill columns don't work (no terminal width)

### Generalization

Table formatting is one use case. The same mechanism supports:

- **Terminal info**: `{{ terminal.width }}`, `{{ terminal.is_tty }}`
- **Environment**: `{{ env.HOME }}`, `{{ env.cwd }}`
- **User preferences**: Date formats, timezone, locale
- **App-wide state**: Shared configuration, feature flags
- **Utilities**: Custom formatters, validators callable from templates

A general context injection mechanism is more valuable than special-casing tables.

## Design

### Core Concepts

1. **Static Context**: Objects created once at builder time
2. **Dynamic Context**: Factories called at render time with `RenderContext`

### RenderContext

Information available to dynamic context factories:

```rust
pub struct RenderContext<'a> {
    /// Output mode (term, text, json, etc.)
    pub output_mode: OutputMode,

    /// Terminal width if available (None if not a TTY or unknown)
    pub terminal_width: Option<usize>,

    /// Command path being executed (e.g., ["config", "get"])
    pub command_path: &'a [String],

    /// Parsed CLI arguments
    pub matches: &'a ArgMatches,

    /// Theme being used
    pub theme: &'a Theme,

    /// Handler's output data (serialized to JSON)
    pub data: &'a serde_json::Value,
}
```

### ContextProvider Trait

```rust
/// Trait for types that can provide context objects at render time.
pub trait ContextProvider: Send + Sync {
    /// Provide a context object given the render context.
    fn provide(&self, ctx: &RenderContext) -> Arc<dyn Object>;
}

// Blanket implementation for closures
impl<F, O> ContextProvider for F
where
    F: Fn(&RenderContext) -> O + Send + Sync,
    O: Object + 'static,
{
    fn provide(&self, ctx: &RenderContext) -> Arc<dyn Object> {
        Arc::new((self)(ctx))
    }
}
```

### Builder API

```rust
Outstanding::builder()
    // Static context: created once
    .context("app", AppInfo { version: "1.0.0" })

    // Dynamic context: factory called per-render
    .context_fn("terminal", |ctx| TerminalInfo {
        width: ctx.terminal_width.unwrap_or(80),
        is_tty: ctx.output_mode == OutputMode::Term,
    })

    // Dynamic context using handler data
    .context_fn("tables", |ctx| {
        TablesRegistry::new(ctx.terminal_width.unwrap_or(80))
            .add("log", log_table_spec())
    })

    .command("log", handler, template)
```

### Template Usage

```jinja
{# Access static context #}
Version: {{ app.version }}

{# Access dynamic terminal info #}
{% if terminal.is_tty %}
  Width: {{ terminal.width }}
{% endif %}

{# Use table formatter #}
{% for commit in commits %}
{{ tables.log.row([commit.hash, commit.author, commit.message]) }}
{% endfor %}
```

## Implementation Plan

### Phase 1: Core Support (outstanding crate)

1. Define `RenderContext` struct in new `context` module
2. Define `ContextProvider` trait with blanket impl for closures
3. Add `render_with_context` function that accepts context extensions
4. Implement `Object` trait for `TableFormatter`

### Phase 2: Clap Integration (outstanding-clap crate)

1. Add `context()` and `context_fn()` methods to `OutstandingBuilder`
2. Store context entries (static or dynamic) in builder
3. Modify dispatch to:
   - Build `RenderContext` from matches, output mode, terminal width
   - Invoke dynamic providers to get context objects
   - Pass context extensions to render function
4. Auto-inject `terminal_width` into `RenderContext`

### Phase 3: Table Integration

1. Create `TablesContext` wrapper implementing `Object`
2. Add `row()` method callable from templates
3. Document table usage with context injection

## Migration

This is a purely additive change. Existing code continues to work unchanged.

## Alternatives Considered

### 1. Table-specific registry

```rust
.table("log", TableSpec::builder()...)
```

Rejected: Too narrow. Same mechanism needed for other utilities.

### 2. Extend handler return type

```rust
struct Output<T> {
    data: T,
    context: HashMap<String, Box<dyn Object>>,
}
```

Rejected: Mixes handler concerns with presentation concerns.

### 3. Global context only

Only support static context, no dynamic factories.

Rejected: Prevents runtime-dependent values like terminal width.

## Open Questions

1. **Should context override data fields?** If both data and context have `foo`, which wins?
   - Proposal: Data wins (context is supplementary)

2. **Should context be typed or stringly-typed?**
   - Proposal: Stringly-typed (`HashMap<String, ...>`) for flexibility

3. **Thread safety requirements?**
   - Proposal: `Send + Sync` required for all context objects

## References

- MiniJinja `Object` trait: https://docs.rs/minijinja/latest/minijinja/value/trait.Object.html
- Original table formatting discussion: (this conversation)
