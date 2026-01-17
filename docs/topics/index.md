# Topics

In-depth documentation for specific Outstanding systems and use cases.

---

## Core Concepts

### [Handler Contract](handler-contract.md)

The interface between your logic and Outstanding. Covers the `Handler` trait, `HandlerResult`, the `Output` enum (`Render`, `Silent`, `Binary`), and `CommandContext`. Essential reading for understanding how handlers return data to be rendered.

### [Rendering System](rendering-system.md)

How Outstanding transforms data into styled terminal output. Covers the two-pass architecture (MiniJinja + BBParser), style tags, themes, template filters, context injection, and structured output modes.

### [Output Modes](output-modes.md)

The `--output` flag and `OutputMode` enum. Covers auto/term/text modes for terminal output, structured modes (JSON, YAML, XML, CSV), file output, and how to access the mode in handlers.

### [Execution Model](execution-model.md)

The request lifecycle from CLI input to rendered output. Covers the pipeline (parsing, dispatch, handler, hooks, rendering), command paths, the hooks system (pre-dispatch, post-dispatch, post-output), and default command behavior.

---

## Configuration

### [App Configuration](app-configuration.md)

The `AppBuilder` API for configuring your application. Covers embedding templates and styles, theme selection, command registration, hooks, context injection, flag customization, and the complete setup workflow.

### [Topics System](topics-system.md)

Adding help topics to your CLI. Covers the `Topic` struct, `TopicRegistry`, loading topics from directories, help integration, pager support, and custom rendering.

---

## Layout

### [Tabular Layout](tabular.md)

Creating aligned, readable output for lists and tables. Covers the `col` filter, `tabular()` and `table()` functions, flexible widths, overflow handling, column styling, borders, and the Rust API.

---

## Standalone Usage

### [Partial Adoption](partial-adoption.md)

Migrating an existing CLI to Outstanding incrementally. Covers using `run` with fallback dispatch, progressive command migration, and full adoption patterns.

### [Render Only](render-only.md)

Using Outstanding's rendering layer without CLI integration. Covers standalone rendering functions, building themes programmatically, template validation, and context injection for non-CLI use cases.
