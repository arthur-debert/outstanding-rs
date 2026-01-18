# Standout Design Guidelines

> **TL;DR**: Standout prioritizes **configuration safety** and **testability** over initial ease of writing. We use the type system to make invalid states unrepresentable and property-based testing to verify the massive configuration space.

## 1. Motivation: The Complexity Trap

Standout is designed to be flexible. It supports:
- **8 Output Modes** (Auto, Term, Text, TermDebug, Json, Yaml, Xml, Csv)
- **2 Handler Modes** (ThreadSafe, Local)
- **3 Template Sources** (Embedded, File, None)
- **4 Style Sources** (Embedded, File, Programmatic, None)
- **2 Help Systems** (Standard, Topics)

This creates a combinatorial explosion of **1,152+ possible configurations**.

In early versions, we relied on manual testing and "happy path" assumptions. This led to fragility where a feature would work in `Term` mode but break in `Json` mode, or work in `App` but panic in `LocalApp`.

Trust in the system requires that we cannot rely on manual verification. We must design for mathematical correctness.

## 2. Core Pillars

To manage this complexity, we adhere to three core pillars:

### I. Configuration Safety (Type-Driven Design)

**Principle**: Invalid configurations must fail to compile or fail explicitly at build time. They must never panic at runtime.

- **Rule**: No `unwrap()` or `expect()` on user configuration. All builder methods must return `Result<Self, SetupError>`.
- **Rule**: No silent fallbacks for optional fields. If a template is required but missing, fail safely, do not silently output nothing.
- **Rule**: Use "Type Detection" not "Option checking".
    - *Bad*: `if let Some(t) = self.templates { ... }`
    - *Good*: `AppBuilder<HasTemplates>` vs `AppBuilder<NoTemplates>` (Typestate pattern where applicable).

### II. Structural Unification

**Principle**: Logic should be defined once, generically.

- **Rule**: Avoid `App` vs `LocalApp` code duplication. Use `App<M: HandlerMode>`.
- **Rule**: If you find yourself copying a method "just to change one type", refactor into a trait or generic struct.
- **Rule**: The rendering core (`standout` without `clap`) must remain pure and oblivious to CLI concerns.

### III. Comprehensive Testing (Matrix & Property)

**Principle**: We cannot write tests for every case manually. We generate them.

- **Rule**: Every new configuration option must be added to the `Arbitrary` generation strategy in `tests/property_rendering.rs`.
- **Rule**: Use **Property-Based Testing** (`proptest`) to verify invariants across the entire configuration matrix (e.g., "Rendering never panics", "JSON output is always valid JSON").
- **Rule**: Use **Snapshot Testing** (`insta`) to catch visual regressions in terminal output.

## 3. Architecture Guide

### The Generic App Pattern

The core abstraction of the CLI layer is:

```rust
pub struct App<M: HandlerMode> {
    builder: AppBuilder<M>,
    // ... shared state
}
```

- `M: HandlerMode` allows us to abstract over `Send + Sync` (ThreadSafe) vs `!Send` (Local) handlers without duplicating the dispatch logic.
- Dispatch logic lives in `core` or generic `impl` blocks.

### Error Handling

- **Setup Phase** (`App::builder()`): Returns `Result<Self, SetupError>`.
    - Errors: `Io`, `Template`, `DuplicateCommand`.
- **Runtime Phase** (`dispatch()`): Returns `RunResult`.
    - Errors from handlers are propagated via `HandlerResult` (`anyhow::Error` or similar).

## 4. PR Evaluation Checklist

When proposing changes, evaluate against this checklist:

- [ ] **Complexity**: Does this add a new configuration dimension?
    - If yes: Have you added it to the `proptest` strategy?
- [ ] **Safety**: Does this introduce any `unwrap()` or `expect()`?
    - If yes: Can it be replaced by `Result` propagation?
- [ ] **Duplication**: Did you copy logic from `App` to `LocalApp` (or vice versa)?
    - If yes: Stop. Refactor to a shared generic implementation.
- [ ] **Verification**: Did you include a snapshot test for the UI output?

## 5. Development Workflow

1.  **Plan**: Draft an `implementation_plan.md` using the Design Guidelines.
2.  **Safety First**: Implement types and builders before logic.
3.  **Test**: Add property tests if changing core dispatch.
4.  **Verify**: Run `cargo test --features clap` and check snapshots.
