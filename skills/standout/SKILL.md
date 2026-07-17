---
name: standout
description: Build, modify, review, or debug Rust CLI applications that use the Standout framework. Use for Standout handlers and state, app and command wiring, MiniJinja templates and CSS themes, output modes, testing with TestHarness, hooks, input chains, piping, partial adoption, or locating framework ownership.
---

# Standout agent orientation

Treat Standout as the shell adapter around a CLI-free application library:

```text
clap -> pre-dispatch -> handler -> post-dispatch -> render -> post-output -> output
```

Keep reusable application libraries completely free of CLI concerns. The binary
package owns Clap, Standout, handlers, view DTOs, templates, styles, environment
lookup, app construction, output formats, and final writes. Handlers are thin
adapters: translate CLI input into library calls, then translate library results
into serializable CLI view models.

## Invariants

- Do not print, render, or emit ANSI from handlers. Return `Output::Render(data)`, `Output::Silent`, or `Output::Binary { ... }`.
- Do not depend on Clap, Standout, `CommandContext`, `Output`, view DTOs,
  templates, styles, environment lookup, or app construction from a reusable
  library. Keep those in the CLI package.
- Keep durable dependencies in app state and request-scoped values in context extensions.
- Prefer structured output when an agent needs data, text output for stable rendered strings, and terminal-debug output for style-tag inspection.
- Test library behavior through its own interface first. Test adapters through
  typed handler calls; use `TestHarness` for the in-process argv-to-output
  pipeline; spawn a process only for seams the harness cannot model.
- Verify public signatures and integration tests in the checked-out version. Framework documentation can lag API changes.

## Load the task branch

Read every reference whose condition matches the task; each is directly reachable here.

- **Must read [handlers-and-state.md](references/handlers-and-state.md)** before adding, changing, debugging, or unit-testing a handler, its arguments, `Output`, app state, or request extensions.
- **Must read [app-wiring.md](references/app-wiring.md)** before registering commands, using `Dispatch`, configuring templates/themes, running an app, or adding partial adoption/fallback behavior.
- **Must read [rendering-and-output.md](references/rendering-and-output.md)** before changing templates, CSS/themes, style tags, output modes, structured serialization, or output-file behavior.
- **Must read [testing.md](references/testing.md)** before writing or reviewing Standout tests, choosing a test level, using `TestHarness`, or diagnosing test interference.
- **Must read [hooks-input-and-piping.md](references/hooks-input-and-piping.md)** before adding hooks, declarative inputs, prompts, stdin/clipboard sources, or output pipes.
- **Must read [project-map-and-gotchas.md](references/project-map-and-gotchas.md)** when locating ownership, choosing a crate/doc/example, upgrading copied code, or resolving an API mismatch.
- **Must read [implementation-quality-checklist.md](references/implementation-quality-checklist.md)** before reviewing how completely or effectively an application leverages Standout.

Use `crates/todo-example/todo-core/` as the canonical CLI-free library and
`crates/todo-example/tdoo/` as the canonical binary-only Standout CLI. For
framework documentation work rather than application code, use the
`writing-standout-docs` skill.
