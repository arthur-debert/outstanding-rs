# Guides

Step-by-step walkthroughs covering principles, rationale, and features.

## Framework Guides

- **[Introduction to Standout](./intro-to-standout.md)** — Adopting Standout in a working CLI. Start here.
- **[TLDR Quick Start](./tldr-intro-to-standout.md)** — Fast-paced intro for experienced developers.
- **[Introduction to Testing](./intro-to-testing.md)** — Testing Standout CLIs end-to-end, in-process, with full environment control.
- **[Leveraging Standout: Value and Implementation Quality Checklist](./leveraging-standout.md)** — Review ownership, rendering, testing, optional capabilities, and framework gaps.

## Crate Guides

For detailed guides on the underlying libraries:

### Rendering (standout-render)

- **[Introduction to Rendering](../crates/render/guides/intro-to-rendering.md)** — Creating polished terminal output with templates and styles.
- **[Introduction to Tabular](../crates/render/guides/intro-to-tabular.md)** — Building aligned, readable tabular layouts.

### Dispatch (standout-dispatch)

- **[Introduction to Dispatch](../crates/dispatch/guides/intro-to-dispatch.md)** — The execution pattern with handlers and hooks.

## Where to Start

If you're new to Standout, begin with [Introduction to Standout](./intro-to-standout.md). It walks through adopting Standout in an existing CLI, step by step.

For a quick overview without the explanations, see the [TLDR Quick Start](./tldr-intro-to-standout.md).

Once you have the framework in place, [Introduction to Testing](./intro-to-testing.md) shows how the architecture plus the `standout-test` harness largely replaces slow, brittle subprocess-based CLI tests with fast in-process ones.

Use the [implementation quality checklist](./leveraging-standout.md) to review
whether an application preserves Standout's invariants, applies relevant
capabilities, and identifies framework gaps explicitly.

If you want to use the crates independently (without the full framework), start with the crate-specific guides above.
