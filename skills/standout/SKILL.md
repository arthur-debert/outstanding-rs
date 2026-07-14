---
name: standout
description: Build, modify, review, or debug Rust CLI applications that use the Standout framework. Use for Standout handlers and state, app and command wiring, MiniJinja templates and CSS themes, output modes, testing with TestHarness, hooks, input chains, piping, partial adoption, or locating framework ownership.
---

# Standout agent orientation

Treat Standout as a boundary between command logic and shell presentation:

```text
clap -> pre-dispatch -> handler -> post-dispatch -> render -> post-output -> output
```

Keep handlers focused on application logic and return serializable data. Let Standout own dispatch, templates, styles, output formats, and final writes. This separation is the test seam: inspect returned data directly, then test only the necessary integration surface.

## Invariants

- Do not print, render, or emit ANSI from handlers. Return `Output::Render(data)`, `Output::Silent`, or `Output::Binary { ... }`.
- Keep durable dependencies in app state and request-scoped values in context extensions.
- Prefer structured output when an agent needs data, text output for stable rendered strings, and terminal-debug output for style-tag inspection.
- Prefer direct handler tests; use `TestHarness` for the in-process argv-to-output pipeline; spawn a process only for boundaries the harness cannot model.
- Verify public signatures and integration tests in the checked-out version. Framework documentation can lag API changes.

## Load the task branch

Read every reference whose condition matches the task; each is directly reachable here.

- **Must read [handlers-and-state.md](references/handlers-and-state.md)** before adding, changing, debugging, or unit-testing a handler, its arguments, `Output`, app state, or request extensions.
- **Must read [app-wiring.md](references/app-wiring.md)** before registering commands, using `Dispatch`, configuring templates/themes, running an app, or adding partial adoption/fallback behavior.
- **Must read [rendering-and-output.md](references/rendering-and-output.md)** before changing templates, CSS/themes, style tags, output modes, structured serialization, or output-file behavior.
- **Must read [testing.md](references/testing.md)** before writing or reviewing Standout tests, choosing a test level, using `TestHarness`, or diagnosing test interference.
- **Must read [hooks-input-and-piping.md](references/hooks-input-and-piping.md)** before adding hooks, declarative inputs, prompts, stdin/clipboard sources, or output pipes.
- **Must read [project-map-and-gotchas.md](references/project-map-and-gotchas.md)** when locating ownership, choosing a crate/doc/example, upgrading copied code, or resolving an API mismatch.

Use `crates/todo-example/` as the worked application when available. For framework documentation work rather than application code, use the `writing-standout-docs` skill.
