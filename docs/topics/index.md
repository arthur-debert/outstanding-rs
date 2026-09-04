# Topics

In-depth documentation for specific Standout systems and use cases.

---

## Framework Configuration

### [App Configuration](app-configuration.md)

The `AppBuilder` API for configuring your application. Covers embedding templates and styles, theme selection, command registration, hooks, context injection, flag customization, and the complete setup workflow.

### [Configuration Files](config-files.md)

Settings from files, environment and flags through clapfig. Covers the precedence ladder, the `config` command, the `[term]` section, a flag above a configured value, config errors, and how tests stay hermetic.

### [Output Modes](output-modes.md)

The `--output` flag, the `Representation` enum, the separate style decision and where the bytes go. Covers the human page, the structured encodings (JSON, YAML, CSV, NDJSON), `--color`, file output, the pager, and why the representation stays out of handlers.

### [Incremental Commands](incremental-commands.md)

A command that produces its result while it runs. Covers the `Results` channel and `emit`, the `.event` template, what each encoding builds from the events and the summary, and what a failure part-way through leaves behind.

### [Topics System](topics-system.md)

Adding help topics to your CLI. Covers the `Topic` struct, `TopicRegistry`, loading topics from directories, help integration, and custom rendering.

---

## Crate Documentation

For detailed documentation on the underlying libraries, see:

### Rendering (standout-render)

- [Introduction to Rendering](../crates/render/guides/intro-to-rendering.md) — Templates, themes, output modes
- [Introduction to Tabular](../crates/render/guides/intro-to-tabular.md) — Column layouts and tables
- [Styling System](../crates/render/topics/styling-system.md) — Themes, adaptive styles, CSS syntax
- [Templating](../crates/render/topics/templating.md) — MiniJinja, style tags, processing modes
- [File System Resources](../crates/render/topics/file-system-resources.md) — Hot reload, registries, embedding

### Dispatch (standout-dispatch)

- [Introduction to Dispatch](../crates/dispatch/guides/intro-to-dispatch.md) — Handlers, hooks, testing
- [Handler Contract](../crates/dispatch/topics/handler-contract.md) — Handler traits, Output enum
- [Execution Model](../crates/dispatch/topics/execution-model.md) — Pipeline, hooks, command routing
- [Partial Adoption](../crates/dispatch/topics/partial-adoption.md) — Incremental migration strategies
