# Leveraging Standout: Value and Implementation Quality Checklist

Standout is most valuable when it creates a testable seam between application
behavior and shell presentation. Use this guide to review an implementation,
decide which framework capabilities add value, and name missing framework
support without pushing presentation concerns back into handlers.

## Evaluation classes

Classify every finding before deciding whether it needs work:

- **Invariant** — a property the application should preserve wherever Standout
  owns a command. Violating it weakens the architecture or public behavior.
- **Applicable capability** — a Standout feature that is valuable only when the
  application's needs call for it. Not using it is not automatically a defect.
- **Framework gap** — a desirable behavior that Standout does not currently
  integrate. Record the gap instead of hiding a custom workaround in a handler.

The checklist uses **I**, **A**, and **G** for those classes.

## Ownership and dispatch

- **I — Keep reusable behavior CLI-free.** Domain rules, state transitions,
  validation, persistence, and reusable services should expose ordinary Rust
  interfaces. A separate core crate is the recommended production shape, not a
  requirement enforced by Standout. Small applications may keep the same seam
  inside one crate.
- **I — Let the binary own the shell.** Clap types, Standout app construction,
  handlers, CLI view DTOs, templates, styles, environment lookup, and final
  output belong in the CLI package. Handlers are adapters: translate parsed
  arguments, call the core, and return serializable data.
- **I — Integrate with Clap.** Standout does not replace Clap. Clap remains the
  command and argument parser; Standout adds dispatch, rendering, output modes,
  hooks, and test seams around the resulting command model.
- **A — Prefer declarative dispatch when conventions fit.**
  `#[derive(Dispatch)]` maps Clap variants to handlers and can attach templates,
  hooks, nested dispatch, and pipes. Use explicit `command_with` registration
  when a command needs configuration that is clearer in builder code.

The production-shaped [`todo-core` + `tdoo` example](production-shaped-example.md)
shows this ownership boundary in executable code. The
[dispatch guide](../crates/dispatch/guides/intro-to-dispatch.md) covers the
pipeline in detail.

## Output and rendering

- **I — Return data, not presentation.** A normal handler returns
  `Output::Render(data)`, `Output::Silent`, or `Output::Binary { ... }`. It
  should not print, emit ANSI, render a template, or branch on output mode.
- **I — Let the framework own the write for reported artifacts.** When a command
  produces a file *and* a report, return `Output::Artifact(...)` with owned bytes,
  an opt-in suggested destination, and the report. Standout selects the
  destination, writes, and renders the report afterwards with a receipt — the
  application never opens the file or words a success it hasn't earned.
- **I — Keep one data contract across modes.** `auto`, `term`, and `text` render
  the MiniJinja template and transform semantic style tags. `term-debug` keeps
  the tags visible. Structured `json`, `yaml`, `xml`, and `csv` modes serialize
  handler data directly and bypass templates, including template-injected
  context.
- **A — Use MiniJinja and semantic CSS.** Put layout in file-backed MiniJinja
  templates and appearance in CSS classes such as `.title` or `.warning`.
  Define adaptive overrides with `@media (prefers-color-scheme: light)` and
  `@media (prefers-color-scheme: dark)`; Standout resolves the matching style
  variant for the terminal background.
- **A — Embed resources and keep the debug loop short.** `embed_templates!` and
  `embed_styles!` provide release-safe assets. Hot reload specifically means
  that debug file-backed resources are re-read on render, so source edits can
  be observed without rebuilding while the original paths remain available.
- **A — Use tabular layout for column relationships.** Reach for the `col`
  filter and tabular specifications when alignment, width allocation,
  truncation, or nested columns matter; do not hand-pad strings in handlers.
- **I — Use the right render diagnostic.** `--output term-debug` shows where
  style tags were placed, but it preserves known and unknown tags alike and
  does not validate that the tags exist. Use `validate_template` in development
  or tests to detect missing style definitions. At render time an unresolved
  tag degrades to unstyled text — never a `?` marker — and, run through
  `App::run`, the tag is named in a stderr warning. A useful signal, but no
  substitute for validation.
- **G — Do not imply integrated per-command CSV projection.** Normal app
  dispatch automatically flattens serializable data for CSV. The direct
  `render_auto_with_spec` API can render CSV with a `FlatDataSpec`, but arbitrary
  per-command projection through normal `App` dispatch is not currently
  integrated. Prefer a stable CLI-owned view DTO, or record the missing app
  configuration seam.

See [Output Modes](../topics/output-modes.md),
[Templating](../crates/render/topics/templating.md),
[Styling System](../crates/render/topics/styling-system.md),
[File System Resources](../crates/render/topics/file-system-resources.md), and
[Introduction to Tabular](../crates/render/guides/intro-to-tabular.md) for the
focused APIs and trade-offs.

## Testing pyramid

- **I — Core tests:** exercise validation, filtering, state transitions, and
  persistence through the CLI-free Rust interface.
- **I — Adapter tests:** call the typed handler function directly and assert the
  CLI-to-core mapping plus returned view DTO.
- **I — Pipeline tests:** use `standout-test::TestHarness` for Clap parsing,
  dispatch, hooks, inputs, templates, structured modes, and controlled process
  seams. Mark harness tests serial because those seams are process-global.
- **A — Process tests:** reserve a small end-to-end layer for real PTYs,
  signals, subprocess behavior, and build or link boundaries the harness cannot
  model.

This ordering keeps most failures close to their owner while still proving the
user-visible shell contract. See [Testing](../topics/testing.md) for the full
boundary matrix.

## Optional application capabilities

These are **applicable capabilities**, not baseline requirements:

- **Hooks** place cross-command validation, request-context injection,
  serialized-data transformation, or post-output observation at an explicit
  pipeline phase.
- **Declarative input chains** resolve values from arguments, environment,
  stdin, clipboard, defaults, editors, or prompts with one validation path.
- **Pipes** send or transform rendered text after output; binary and silent
  results pass through unchanged.
- **Partial adoption** lets an existing Clap application delegate selected
  commands to Standout and retain its legacy path for unmatched commands.

Adopt each capability only when it removes application-owned orchestration or
improves a test seam. If the framework cannot express the required behavior,
classify it as a framework gap and keep the workaround isolated in the CLI
layer.

## Review outcome

A high-quality implementation has no unresolved invariant violations, uses the
applicable capabilities that materially simplify its shell boundary, and names
framework gaps explicitly. That is a stronger signal than counting how many
Standout features an application uses.
