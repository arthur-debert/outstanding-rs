# Recover the bootstrap wizard as a bounded architecture generator

## Status

Accepted and implemented by WIZ01.

## Context

Standout's preferred application shape separates reusable behavior from shell
adaptation and presentation. That boundary is easy to describe but costly for
a new user to assemble correctly before they can run a first command.

The WIZ01 epic introduced a bootstrap wizard. The original ADR content was
lost when this path was accidentally replaced with a copy of the draft
Specification, so this record recovers the decisions represented by epic #228
and the shipped implementation.

The first release needed to demonstrate the architecture through real,
compiling output while keeping its questionnaire and generator contract small.
It was not intended to become a general source-code generator, project editor,
or stable template ecosystem.

## Decision

The `standout` package ships an executable named `standout` with a
`new-project` subcommand. It interactively collects one project and one initial
command, resolves those answers into a private validated project model, shows a
plain-language review, and requires explicit confirmation before writing.

The generated project is a two-crate Cargo workspace:

- `<name>lib` owns typed reusable behavior and has no Clap or Standout
  dependency.
- `<name>` owns Clap declarations, Standout assembly, shell-input policy, thin
  handlers, CLI view types, templates, styles, and process execution.

The initial input contract is deliberately bounded:

- value types are string, boolean, and path;
- strings may be required, optional, or repeated;
- booleans use boolean cardinality;
- paths may be required, optional, or repeated;
- required and optional strings may use an ordered combination of argument,
  file-content, and piped-stdin sources;
- repeated strings, booleans, and every path cardinality are argument-only.

File-content input uses a dedicated generated `--<name>-file PATH` option.
Paths remain `PathBuf` values rather than implicitly becoming file contents.
When a string has several sources, the user-provided order defines precedence.

The generated command returns either a small message or record view. Human
output uses a MiniJinja template and semantic CSS; structured output serializes
the same CLI-owned view. Generated tests cover the CLI-free core operation,
typed handler mapping, and the argv-to-output pipeline through `TestHarness`.
A generated README records the selected command syntax and input policy.

Generated manifests use the running wizard's Standout version as a normal
compatible Cargo requirement. The generator refuses non-empty destinations,
writes into a sibling staging directory, and publishes by rename so a failed
write does not expose a partially generated project at the requested path.

## Consequences

Users get a runnable tracer bullet that teaches ownership through code and
tests. The generated structure is intentionally more substantial than a
single-crate quick start, but it scales without moving core behavior out of
CLI modules later.

The supported questionnaire is narrower than Standout itself. New value types,
source combinations, result shapes, existing-project mutation, or a public
generator schema require new decisions and verification rather than being
implied by this ADR.

The internal project model and templates remain implementation details.
Generated projects are owned by their users after creation; the wizard does
not update or regenerate them.
