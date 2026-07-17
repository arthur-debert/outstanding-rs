# Production-shaped application

When an application has behavior worth reusing or testing independently, keep
that behavior in a fully CLI-free library and put every shell concern in the
binary package.

The repository's canonical [`tdoo` worked example](../../crates/todo-example/README.md)
is executable documentation for that layout:

```text
todo-core (library)              tdoo (binary)
------------------------------   ---------------------------------
model and invariants             Clap commands and flags
validation and filtering         Standout App construction
state transitions                handlers as adapters
JSON persistence                 CLI view models
explicit storage path            env lookup, templates, styles
fast core tests                  hooks and TestHarness tests
```

The dependency arrow goes one way:

```text
tdoo  --->  todo-core
```

`todo-core` has no dependency on Clap, Standout, command contexts, environment
variables, view DTOs, templates, styles, or terminal output. Its interface is
the surface used by both the CLI and core tests.

The CLI translates at the seam. A handler maps `--all` to a core `TodoFilter`,
calls the store, then maps domain values to CLI-owned `TodoView` values. That
keeps structured output stable even when the library's persisted model changes.

Read the canonical source rather than copying a second implementation here:

- [`todo-core/src/lib.rs`](../../crates/todo-example/todo-core/src/lib.rs) — the
  small public library interface;
- [`todo-core/src/store.rs`](../../crates/todo-example/todo-core/src/store.rs) —
  behavior, persistence, and fast tests;
- [`tdoo/src/handlers.rs`](../../crates/todo-example/tdoo/src/handlers.rs) — thin
  adapters and direct typed-handler tests;
- [`tdoo/src/app.rs`](../../crates/todo-example/tdoo/src/app.rs) — app assembly,
  shell pipeline features, and focused `TestHarness` tests.

For a five-minute look at dispatch and rendering without the package split, use
the [minimal single-crate example](minimal-single-crate.md).

For a broader review of ownership, rendering, testing, and optional framework
capabilities, use the
[Leveraging Standout implementation quality checklist](leveraging-standout.md).
