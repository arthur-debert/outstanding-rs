# Standout Bootstrap Wizard Specification

## Status

Implemented by WIZ01. This document records the shipped first-release scope;
the recovered decision record is [ADR 0001](../adr/0001-wizard.md).

## Overview

Standout makes sophisticated CLI behavior easier to build, but its preferred
architecture differs from the structure many Rust developers initially reach
for. The bootstrap wizard creates a small, runnable Standout project that
demonstrates the intended architecture through one complete command.

The generated project is an architectural tracer bullet, not a finished CLI.
It shows where reusable logic, CLI adaptation, application assembly,
presentation, and tests belong.

## Goals

1. Ask users for a project name, initial command, command inputs, input sources,
   and output shape.
2. Generate a Rust workspace containing:
   - a CLI-free `<name>lib` crate;
   - a `<name>` binary crate using Clap and Standout.
3. Generate one working command from argument parsing through core logic and
   rendered or structured output.
4. Demonstrate Standout input resolution for selected sources such as command
   arguments, piped stdin, and file contents.
5. Generate a MiniJinja template and a minimal semantic CSS theme.
6. Generate tests that demonstrate the intended testing seams:
   - core-library behavior;
   - typed-handler adaptation;
   - full argv-to-output behavior through `TestHarness`.
7. Prove that supported generated projects format, compile, test, and run.

## Non-goals

- Generating a complete production application.
- Adding commands to an existing project.
- Editing, parsing, migrating, or regenerating existing Rust source.
- Maintaining generated files after initial creation.
- Supporting arbitrary project layouts.
- Providing a plugin or third-party template ecosystem.
- Designing persistence, configuration, authentication, logging, or deployment.
- Exposing a stable public generator schema.
- Providing a graphical project editor.
- Generating every Standout capability in the initial project.

## Generated architecture

The default generated workspace is:

```text
<name>/
├── Cargo.toml
└── crates/
    ├── <name>lib/
    │   ├── Cargo.toml
    │   └── src/
    │       └── lib.rs
    └── <name>/
        ├── Cargo.toml
        └── src/
            ├── main.rs
            ├── cli.rs
            ├── handlers.rs
            ├── templates/
            │   └── <command>.jinja
            └── styles/
                └── <name>.css
```

`<name>lib` owns reusable application behavior and must not depend on Clap,
Standout, templates, terminal behavior, environment lookup, or CLI view types.

`<name>` owns all shell-facing behavior: Clap declarations, Standout assembly,
handlers, CLI view types, input-source policy, templates, styles, and process
execution.

For the initial one-command project:

- `main.rs` owns dependency construction, Standout application assembly, and
  process execution;
- `cli.rs` owns Clap declarations;
- `handlers.rs` owns thin adapters and CLI-specific serializable view types.

The wizard does not create `app.rs`, `views.rs`, or `<name>lib/src/api.rs`
until the generated project has enough independent responsibility to justify
those modules.

## Wizard flow

### 1. Project identity

Ask for:

- project name;
- executable name, defaulting to the project name;
- initial command name;
- one-sentence command description.

Validate names before asking detailed command questions.

### 2. Command inputs

For each logical input, ask for:

- name;
- Rust value type from a deliberately small supported set;
- required, optional, repeated, or boolean cardinality;
- allowed sources;
- precedence when more than one source is allowed;
- a core validation demonstrated by the generated example.

Initial source support is narrow:

- named command argument;
- piped stdin;
- file contents.

The wizard asks behavioral questions rather than framework questions. For
example:

> Can `document` come from `--document`, piped stdin, or a file?

It does not ask:

> Should this command use an `InputChain`?

Before generation, summarize the resolved policy in plain language:

> `document` comes from `--document`, then `--file`, then piped stdin. Empty
> contents are rejected before the operation runs.

### 3. Core operation

Ask for:

- operation name;
- whether it returns a message or record;
- a small set of result fields;
- one validation or error case worth demonstrating.

The generated library interface accepts explicit values and dependencies,
return typed results, and contain no CLI concepts.

### 4. Presentation

Generate:

- a CLI-owned serializable view type;
- one MiniJinja template for human output;
- minimal CSS using semantic style names;
- structured output from the same view type without handler branching.

### 5. Review and confirmation

Before writing files, show:

- the destination;
- generated tree;
- command syntax;
- input-source precedence;
- core operation;
- output shape;
- tests that will be generated.

Generation requires explicit confirmation after this review.

## Internal project model

The questionnaire resolves into one validated internal project model
before any files are rendered.

Conceptually:

```text
Wizard answers → ProjectSpec → validation → GeneratedFiles
```

`ProjectSpec` is a private implementation detail. It isolates
interactive prompting from validation and file rendering, enabling deterministic
tests without simulating a terminal.

The first version does not promise that this model is serializable, stable, or
accepted as user-authored configuration.

## Generated tests

Every generated project must include:

### Core test

Calls `<name>lib` directly and proves:

- valid input produces the expected typed result;
- the selected validation case produces a core error.

### Typed-handler test

Calls the preserved typed handler and proves:

- resolved CLI input is mapped into the core call;
- the core result is mapped into the expected CLI view;
- the handler returns data instead of printing or rendering.

### Pipeline test

Uses `standout_test::TestHarness` and proves:

- argv reaches the registered command;
- the selected input source is resolved;
- human output uses the generated template;
- JSON output is valid and has the expected shape.

Harness tests must follow Standout's serialization requirements for
process-global test seams.

## Generator verification

The generator's own integration suite must create projects in temporary
directories and verify the emitted artifacts rather than only snapshotting file
contents.

For every supported configuration represented in the test matrix, the suite
must run:

```text
cargo fmt --check
cargo check --workspace
cargo test --workspace
```

It must also execute the generated binary and assert:

- representative argument input;
- piped input when selected;
- file-content input when selected;
- source precedence where multiple sources are selected;
- human-rendered text;
- parseable structured JSON;
- non-zero failure and a useful diagnostic for invalid input.

At least one end-to-end case must exercise the richest supported configuration
through a real generated binary. Fixtures or snapshots alone are insufficient
release evidence.

## Configuration test matrix

The initial matrix covers:

1. required argument to rendered record;
2. optional flag behavior;
3. piped stdin;
4. file contents;
5. multiple input sources with defined precedence;
6. human text and JSON output;
7. invalid or missing input.

The matrix covers supported behaviors without attempting every
combinatorial permutation.

## File-system behavior

- Refuse to overwrite a non-empty destination.
- Do not partially replace an existing project.
- Validate the full project model before writing.
- Generate into a sibling staging directory, clean it up on failure, and
  publish it to the requested destination by rename.
- Never execute generated code until after the user has confirmed generation.

## Acceptance criteria

- [x] A user can describe a project and one command through the wizard.
- [x] The generated workspace contains `<name>lib` and `<name>` crates with the
      stated ownership split.
- [x] The generated CLI-free crate has no Clap or Standout dependency.
- [x] The generated handler is a thin adapter returning a CLI-owned view.
- [x] The generated project contains a template and semantic CSS.
- [x] Selected argument, piped-input, and file-content policies work as
      specified.
- [x] Generated core, handler, and pipeline tests pass.
- [x] Supported generated configurations format, compile, and test.
- [x] A real generated binary succeeds for representative input paths.
- [x] JSON output parses and preserves the declared view shape.
- [x] Invalid input produces a non-zero result and useful diagnostic.
- [x] Existing destinations are not overwritten.
- [x] The documentation clearly presents the result as a starting architecture,
      not a finished CLI.

## Resolved first-release contract

1. The package exposes the wizard as `standout new-project`.
2. Inputs support string, boolean, and path values. Strings support required,
   optional, and repeated cardinality; booleans use boolean cardinality; paths
   support required, optional, and repeated cardinality. Required and optional
   strings may select ordered argument, file-content, and piped-stdin sources.
   Repeated strings, booleans, and paths are argument-only.
3. File-content sources use a dedicated generated `--<name>-file PATH` option.
   Path-typed inputs are named arguments and remain paths.
4. Generated manifests use the running wizard's Standout version as a normal
   compatible Cargo requirement.
5. Each generated binary crate includes a short README documenting its
   architecture, command syntax, source policy, and verification commands.
